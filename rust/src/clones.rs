use crate::report;
use serde_json::{json, Value};

pub struct CloneReport {
    supported: bool,
    dry_run: bool,
    root: Option<String>,
    min_age: u64,
    candidates: Vec<Value>,
    kept: Vec<Value>,
    removed: Vec<String>,
    apparent: u64,
    allocated: u64,
    free_change: Option<i64>,
    message: String,
}
impl CloneReport {
    pub fn failed(&self) -> bool {
        self.supported
            && (!self.message.is_empty()
                || self
                    .kept
                    .iter()
                    .any(|k| k["reason"] == "clone deletion failed"))
    }
    pub fn value(&self) -> Value {
        json!({"supported":self.supported,"dry_run":self.dry_run,"root":self.root,
            "min_age_seconds":self.min_age,"candidates":self.candidates,"kept":self.kept,
            "removed":self.removed,"apparent_bytes":self.apparent,"allocated_bytes":self.allocated,
            "reclaimable_bytes_estimate":null,"free_space_change_bytes":self.free_change,"message":self.message})
    }
    pub fn render(&self) -> String {
        if !self.supported {
            return "code-sign clones: nothing to do on this OS.".into();
        }
        if !self.message.is_empty() {
            return format!("code-sign clones: {}.", self.message);
        }
        let mut text=format!("code-sign clones: {} eligible, {} kept, {} apparent; reclaimable space: unknown (shared APFS blocks).",self.candidates.len(),self.kept.len(),report::bytes(self.apparent));
        if self.dry_run {
            text.push_str(
                "\ndry run: no code-sign clones were removed. Rerun with --kill --clean-clones.",
            );
        } else {
            text.push_str(&format!(
                "\nremoved {} code-sign clones.",
                self.removed.len()
            ));
            if let Some(n) = self.free_change {
                text.push_str(&format!(
                    " filesystem free-space change: {}{}.",
                    if n < 0 { "-" } else { "+" },
                    report::bytes(n.unsigned_abs())
                ));
            }
        }
        text
    }
}
pub fn clean(kill: bool, min_age: f64) -> CloneReport {
    #[allow(unused_mut)]
    let mut result = CloneReport {
        supported: cfg!(target_os = "macos"),
        dry_run: !kill,
        root: None,
        min_age: min_age as u64,
        candidates: vec![],
        kept: vec![],
        removed: vec![],
        apparent: 0,
        allocated: 0,
        free_change: None,
        message: if cfg!(target_os = "macos") {
            String::new()
        } else {
            "nothing to do on this OS".into()
        },
    };
    #[cfg(target_os = "macos")]
    if let Err(e) = macos::scan(&mut result, kill, min_age) {
        result.message = format!("clone scan unavailable: {e}");
    }
    result
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::{detect, proc};
    use std::os::macos::fs::MetadataExt as MacMetadataExt;
    use std::{
        collections::HashSet,
        ffi::{CStr, CString},
        fs, io, mem,
        os::unix::fs::MetadataExt,
        path::{Path, PathBuf},
        process::Command,
        ptr,
    };
    fn err(text: &str) -> io::Error {
        io::Error::other(text)
    }
    fn parent(name: &str) -> bool {
        name.to_lowercase()
            .strip_suffix(".code_sign_clone")
            .is_some_and(|n| {
                [
                    "com.google.chrome",
                    "org.chromium.chromium",
                    "com.microsoft.edgemac",
                    "com.google.chrome.for_testing",
                    "com.google.chrome.fortesting",
                ]
                .contains(&n)
            })
    }
    fn root() -> io::Result<PathBuf> {
        let output = Command::new("getconf")
            .arg("DARWIN_USER_TEMP_DIR")
            .output()?;
        if !output.status.success() {
            return Err(err("user X temp directory is unavailable"));
        }
        let temp = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).canonicalize()?;
        let x = temp
            .parent()
            .ok_or_else(|| err("user X temp directory is unavailable"))?
            .join("X");
        if !x.is_dir() || x.is_symlink() {
            return Err(err("user X temp directory is unavailable"));
        }
        Ok(x)
    }
    // Exact executable paths use libproc. lsof supplies open AND mapped file
    // paths because SIP denies some native FD-path queries for same-user apps.
    fn open_paths() -> io::Result<HashSet<PathBuf>> {
        let mut paths = HashSet::new();
        unsafe {
            let bytes = libc::proc_listpids(1, 0, ptr::null_mut(), 0);
            if bytes <= 0 {
                return Err(err("cannot enumerate processes for clone cleanup"));
            }
            let mut pids = vec![0i32; bytes as usize / 4 + 1024];
            let count =
                libc::proc_listpids(1, 0, pids.as_mut_ptr().cast(), (pids.len() * 4) as i32);
            if count <= 0 {
                return Err(err("cannot enumerate processes for clone cleanup"));
            }
            for &pid in &pids[..count as usize / 4] {
                if pid <= 0 {
                    continue;
                }
                let mut buf = [0u8; 4096];
                if libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) > 0 {
                    paths.insert(detect::real(
                        &CStr::from_ptr(buf.as_ptr().cast()).to_string_lossy(),
                    ));
                }
            }
        }
        let output = Command::new("lsof").args(["-nP", "-Fpn"]).output()?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(err("open-file scan failed; preserving code-sign clones"));
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.starts_with("n/") {
                paths.insert(detect::real(&line[1..]));
            }
        }
        Ok(paths)
    }
    fn identities(paths: &HashSet<PathBuf>) -> HashSet<(u64, u64)> {
        paths
            .iter()
            .filter_map(|p| p.metadata().ok().map(|s| (s.dev(), s.ino())))
            .collect()
    }
    fn size(path: &Path, opened: &HashSet<(u64, u64)>) -> io::Result<(u64, u64, bool)> {
        let (mut apparent, mut allocated) = (0, 0);
        let mut used = false;
        let mut stack = vec![path.to_path_buf()];
        while let Some(path) = stack.pop() {
            let meta = path.symlink_metadata()?;
            used |= opened.contains(&(meta.dev(), meta.ino()));
            apparent += meta.len();
            allocated += meta.blocks() * 512;
            if meta.is_dir() && !meta.is_symlink() {
                for entry in fs::read_dir(path)? {
                    stack.push(entry?.path());
                }
            }
        }
        Ok((apparent, allocated, used))
    }
    fn free(path: &Path) -> io::Result<i64> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| err("invalid X temp directory"))?;
        let mut stat: libc::statvfs = unsafe { mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((stat.f_bavail as u64 * stat.f_frsize) as i64)
    }
    fn used(path: &Path, opened: &HashSet<PathBuf>) -> bool {
        opened.iter().any(|p| p.starts_with(path))
    }
    fn keep(r: &mut CloneReport, path: &Path, reason: &str) {
        r.kept
            .push(json!({"path":path.to_string_lossy(),"reason":reason}));
    }
    fn age(meta: &fs::Metadata, now: f64) -> f64 {
        now - meta.st_birthtime() as f64 - meta.st_birthtime_nsec() as f64 / 1e9
    }
    pub fn scan(r: &mut CloneReport, kill: bool, min_age: f64) -> io::Result<()> {
        let root = root()?;
        r.root = Some(root.to_string_lossy().into_owned());
        let mut paths = Vec::new();
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if !parent(&path.file_name().unwrap_or_default().to_string_lossy())
                || !path.is_dir()
                || path.is_symlink()
            {
                continue;
            }
            for entry in fs::read_dir(path)? {
                let path = entry?.path();
                if path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("code_sign_clone.")
                {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        if paths.is_empty() {
            return Ok(());
        }
        let opened = open_paths()?;
        let file_ids = identities(&opened);
        let now = proc::now();
        for path in paths {
            let Ok(meta) = path.symlink_metadata() else {
                keep(r, &path, "clone directory is unreadable or changed");
                continue;
            };
            if !meta.is_dir()
                || meta.is_symlink()
                || path.canonicalize().ok().as_ref() != Some(&path)
            {
                keep(r, &path, "not a plain clone directory");
                continue;
            }
            let age = age(&meta, now);
            if age < min_age {
                keep(r, &path, "younger than min age");
                continue;
            }
            if used(&path, &opened) {
                keep(r, &path, "used by a running process");
                continue;
            }
            match size(&path, &file_ids) {
                Ok((_, _, true)) => keep(r, &path, "used by a running process"),
                Ok((apparent, allocated, false)) => {
                    r.candidates.push(json!({"path":path.to_string_lossy(),"age_seconds":age.max(0.0) as u64,"apparent_bytes":apparent,"allocated_bytes":allocated}));
                    r.apparent += apparent;
                    r.allocated += allocated;
                }
                Err(_) => keep(r, &path, "clone directory is unreadable or changed"),
            }
        }
        if kill && !r.candidates.is_empty() {
            let before = free(&root)?;
            let opened = open_paths()?;
            let file_ids = identities(&opened);
            for item in r.candidates.clone() {
                let path = PathBuf::from(item["path"].as_str().unwrap());
                let Ok(meta) = path.symlink_metadata() else {
                    keep(r, &path, "clone directory changed");
                    continue;
                };
                if path.is_symlink()
                    || path.parent().is_some_and(Path::is_symlink)
                    || path.canonicalize().ok().as_ref() != Some(&path)
                {
                    keep(r, &path, "clone directory changed");
                } else if age(&meta, proc::now()) < min_age {
                    keep(r, &path, "younger than min age");
                } else if used(&path, &opened)
                    || size(&path, &file_ids)
                        .map(|(_, _, used)| used)
                        .unwrap_or(true)
                {
                    keep(r, &path, "used by a running process");
                } else {
                    match fs::remove_dir_all(&path) {
                        Ok(()) => r.removed.push(path.to_string_lossy().into_owned()),
                        Err(_) => keep(r, &path, "clone deletion failed"),
                    }
                }
            }
            r.free_change = Some(free(&root)? - before);
        }
        Ok(())
    }
}
