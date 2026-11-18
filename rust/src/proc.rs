use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub rss: u64,
    pub started: f64,
    pub zombie: bool,
    pub exe: String,
    pub argv: Vec<String>,
    /// Working directory when it could be read; `None` when it cannot be
    /// determined. A process enumerated but not fully readable stays in the
    /// snapshot with the unread fields empty, so an inaccessible launcher is
    /// never mistaken for a missing parent.
    pub cwd: Option<String>,
}
impl Proc {
    pub fn name(&self) -> &str {
        self.exe.rsplit('/').next().unwrap_or(&self.exe)
    }
}
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(target_os = "linux")]
pub fn list() -> io::Result<Vec<Proc>> {
    use std::fs;
    let now = now();
    let uptime: f64 = fs::read_to_string("/proc/uptime")?
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0.0);
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    let mut out = Vec::new();
    for ent in fs::read_dir("/proc")?.flatten() {
        let Ok(pid) = ent.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let base = ent.path();
        let stat = match fs::read_to_string(base.join("stat")) {
            Ok(stat) => stat,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue, // exited mid-scan
            Err(_) => {
                // alive but unreadable: keep an unknown entry so a child does not
                // mistake its live launcher for a missing parent
                out.push(Proc {
                    pid,
                    ppid: -1,
                    rss: 0,
                    started: 0.0,
                    zombie: false,
                    exe: String::new(),
                    argv: Vec::new(),
                    cwd: None,
                });
                continue;
            }
        };
        let Some(end) = stat.rfind(')') else { continue };
        let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
        if fields.len() < 22 {
            continue;
        }
        let (Ok(ppid), Ok(start), Ok(rss)) = (
            fields[1].parse(),
            fields[19].parse::<f64>(),
            fields[21].parse::<i64>(),
        ) else {
            continue;
        };
        let cwd = fs::read_link(base.join("cwd"))
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        let raw = match fs::read(base.join("cmdline")) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue, // exited mid-scan
            Err(_) => {
                // argv unreadable (permissions): keep the process with unknown
                // argv; it can still serve as a live parent
                let exe = fs::read_link(base.join("exe"))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.push(Proc {
                    pid,
                    ppid,
                    rss: rss.max(0) as u64 * page,
                    started: now - (uptime - start / ticks).max(0.0),
                    zombie: fields[0] == "Z",
                    exe,
                    argv: Vec::new(),
                    cwd,
                });
                continue;
            }
        };
        let mut argv: Vec<String> = raw
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        if argv.is_empty() {
            continue;
        }
        if argv.len() == 1 && argv[0].contains(' ') {
            argv = argv[0].split_whitespace().map(str::to_owned).collect();
        }
        let exe = fs::read_link(base.join("exe"))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| argv[0].clone());
        out.push(Proc {
            pid,
            ppid,
            rss: rss.max(0) as u64 * page,
            started: now - (uptime - start / ticks).max(0.0),
            zombie: fields[0] == "Z",
            exe,
            argv,
            cwd,
        });
    }
    Ok(out)
}

/// Parse `lsof -a -d cwd -Fn`: `p<pid>` lines followed by `n<path>`.
/// The leading `/` of the path belongs to the path, not the field tag.
fn parse_lsof_cwd(text: &str) -> std::collections::HashMap<i32, String> {
    let mut cwd = std::collections::HashMap::new();
    let mut current: Option<i32> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            current = rest.parse().ok();
        } else if let Some(path) = line.strip_prefix('n') {
            if let Some(pid) = current {
                cwd.insert(pid, path.to_owned());
            }
        }
    }
    cwd
}

/// Best-effort cwd per pid (same user only); empty when lsof is unavailable.
#[cfg(target_os = "macos")]
fn macos_cwd() -> std::collections::HashMap<i32, String> {
    let output = std::process::Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            parse_lsof_cwd(&String::from_utf8_lossy(&o.stdout))
        }
        _ => std::collections::HashMap::new(),
    }
}

#[cfg(target_os = "macos")]
pub fn list() -> io::Result<Vec<Proc>> {
    use std::{ffi::CStr, mem, ptr};
    // libproc supplies stable start times and RSS; KERN_PROCARGS2 preserves argv
    // boundaries rather than reconstructing switches from a space-joined ps line.
    unsafe {
        let bytes = libc::proc_listpids(1 /* PROC_ALL_PIDS */, 0, ptr::null_mut(), 0);
        if bytes <= 0 {
            return Err(io::Error::last_os_error());
        }
        let mut pids = vec![0i32; bytes as usize / mem::size_of::<i32>() + 1024];
        let count = libc::proc_listpids(
            1, /* PROC_ALL_PIDS */
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * 4) as i32,
        );
        if count <= 0 {
            return Err(io::Error::last_os_error());
        }
        let mut argmax = 0i32;
        let mut len = mem::size_of_val(&argmax);
        let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
        if libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut argmax as *mut i32).cast(),
            &mut len,
            ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut out = Vec::new();
        let mut args = vec![0u8; argmax.max(4096) as usize];
        for &pid in &pids[..count as usize / 4] {
            if pid <= 0 {
                continue;
            }
            let mut bsd: libc::proc_bsdinfo = mem::zeroed();
            let size = mem::size_of_val(&bsd) as i32;
            if libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut bsd as *mut libc::proc_bsdinfo).cast(),
                size,
            ) != size
            {
                // Keep an inaccessible PID in the snapshot. Omitting it could
                // misclassify its child's live launcher as a missing parent.
                out.push(Proc {
                    pid,
                    ppid: -1,
                    rss: 0,
                    started: 0.0,
                    zombie: false,
                    exe: if pid == 1 {
                        "launchd".into()
                    } else {
                        String::new()
                    },
                    argv: Vec::new(),
                    cwd: None,
                });
                continue;
            }
            let mut task: libc::proc_taskinfo = mem::zeroed();
            let tsize = mem::size_of_val(&task) as i32;
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTASKINFO,
                0,
                (&mut task as *mut libc::proc_taskinfo).cast(),
                tsize,
            );
            let mut path = [0u8; 4096];
            let n = libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32);
            let mut exe = if n > 0 {
                CStr::from_ptr(path.as_ptr().cast())
                    .to_string_lossy()
                    .into_owned()
            } else {
                CStr::from_ptr(bsd.pbi_comm.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            };
            let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
            let mut len = args.len();
            let mut argv = Vec::new();
            if libc::sysctl(
                mib.as_mut_ptr(),
                3,
                args.as_mut_ptr().cast(),
                &mut len,
                ptr::null_mut(),
                0,
            ) == 0
                && len >= 4
            {
                let argc = i32::from_ne_bytes(args[..4].try_into().unwrap()).max(0) as usize;
                let raw = &args[4..len];
                if let Some(end) = raw.iter().position(|&b| b == 0) {
                    // The executable path is followed by padding, then argc NUL strings.
                    if exe.is_empty() {
                        exe = String::from_utf8_lossy(&raw[..end]).into_owned();
                    }
                    let mut pos = end;
                    while pos < raw.len() && raw[pos] == 0 {
                        pos += 1;
                    }
                    for _ in 0..argc {
                        if pos >= raw.len() {
                            break;
                        }
                        let end = raw[pos..]
                            .iter()
                            .position(|&b| b == 0)
                            .map(|n| pos + n)
                            .unwrap_or(raw.len());
                        argv.push(String::from_utf8_lossy(&raw[pos..end]).into_owned());
                        pos = end + 1;
                    }
                }
            }
            if argv.is_empty() {
                argv.push(exe.clone());
            }
            out.push(Proc {
                pid,
                ppid: bsd.pbi_ppid as i32,
                rss: task.pti_resident_size,
                started: bsd.pbi_start_tvsec as f64 + bsd.pbi_start_tvusec as f64 / 1e6,
                zombie: bsd.pbi_status == 5,
                exe,
                argv,
                cwd: None,
            });
        }
        let cwd = macos_cwd();
        for p in &mut out {
            p.cwd = cwd.get(&p.pid).cloned();
        }
        Ok(out)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn list() -> io::Result<Vec<Proc>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "no process backend for '{}'; this release supports macOS and Linux",
            platform()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_cwd_parsing_keeps_the_leading_slash() {
        let parsed = parse_lsof_cwd("p1\nfcwd\nn/\np123\nn/home/user\np999\n");
        assert_eq!(parsed.get(&1).map(String::as_str), Some("/"));
        assert_eq!(parsed.get(&123).map(String::as_str), Some("/home/user"));
        assert!(!parsed.contains_key(&999));
    }
}
pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else {
        std::env::consts::OS
    }
}
