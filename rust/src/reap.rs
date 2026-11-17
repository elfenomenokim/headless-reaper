use crate::{
    detect::{self, Group},
    proc::{self, Proc},
};
use std::{
    collections::BTreeMap,
    fs, io, thread,
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Result {
    pub targets: usize,
    pub sigterm: Vec<i32>,
    pub sigkill: Vec<i32>,
    pub survivors: Vec<i32>,
    pub already_gone: Vec<i32>,
    pub denied: Vec<i32>,
}
#[cfg(unix)]
fn signal(pid: i32, sig: i32) -> io::Result<()> {
    if unsafe { libc::kill(pid, sig) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn signal(_: i32, _: i32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "unsupported platform",
    ))
}
fn alive(pid: i32) -> bool {
    signal(pid, 0)
        .map(|_| true)
        .unwrap_or_else(|e| e.raw_os_error() != Some(3))
}
fn same(targets: &BTreeMap<i32, Proc>, procs: Vec<Proc>) -> Vec<i32> {
    let current: BTreeMap<i32, Proc> = procs.into_iter().map(|p| (p.pid, p)).collect();
    targets
        .iter()
        .filter(|(pid, old)| {
            current
                .get(pid)
                .is_some_and(|p| !p.zombie && (p.started - old.started).abs() <= 2.0)
        })
        .map(|(pid, _)| *pid)
        .collect()
}
fn wait(pids: &[i32], seconds: f64) {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
    while Instant::now() < deadline && pids.iter().any(|&pid| alive(pid)) {
        thread::sleep(Duration::from_millis(200));
    }
}
pub fn reap(groups: &[Group], grace: f64) -> io::Result<Result> {
    let targets: BTreeMap<i32, Proc> = groups
        .iter()
        .flat_map(|g| g.members.iter())
        .filter(|p| p.pid > 1 && p.pid != std::process::id() as i32)
        .map(|p| (p.pid, p.clone()))
        .collect();
    let mut r = Result {
        targets: targets.len(),
        ..Result::default()
    };
    if targets.is_empty() {
        return Ok(r);
    }
    let before = same(&targets, proc::list()?);
    r.already_gone = targets
        .keys()
        .filter(|p| !before.contains(p))
        .copied()
        .collect();
    for pid in before {
        match signal(pid, 15) {
            Ok(()) => r.sigterm.push(pid),
            Err(e) if e.raw_os_error() == Some(3) => r.already_gone.push(pid),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => r.denied.push(pid),
            Err(e) => return Err(e),
        }
    }
    wait(&r.sigterm, grace);
    let survivors = same(&targets, proc::list()?);
    for &pid in &survivors {
        match signal(pid, 9) {
            Ok(()) => r.sigkill.push(pid),
            Err(e) if e.raw_os_error() == Some(3) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                if !r.denied.contains(&pid) {
                    r.denied.push(pid)
                }
            }
            Err(e) => return Err(e),
        }
    }
    if !survivors.is_empty() {
        wait(&survivors, 3.0);
        r.survivors = same(&targets, proc::list()?);
    }
    Ok(r)
}
pub type Cleanup = (Vec<String>, Vec<(String, String)>);
pub fn cleanup(groups: &[Group]) -> io::Result<Cleanup> {
    let procs = proc::list()?;
    let reaped: Vec<i32> = groups.iter().flat_map(Group::pids).collect();
    let (mut removed, mut kept) = (Vec::new(), Vec::new());
    for g in groups {
        let Some(path) = &g.profile else { continue };
        let p = std::path::Path::new(path);
        let refusal = if !p.is_dir() {
            Some("directory is gone".into())
        } else if p.is_symlink() {
            Some("path is a symlink".into())
        } else if !detect::is_temp(path) {
            Some("not under a temp root".into())
        } else if detect::is_default(path) {
            Some("default user profile".into())
        } else {
            let live = procs.iter().find(|proc| {
                !reaped.contains(&proc.pid)
                    && !proc.zombie
                    && detect::is_browser(proc)
                    && (detect::profile(proc)
                        .is_some_and(|s| detect::real(&s) == detect::real(path))
                        || proc.argv.join(" ").contains(path)
                        || proc
                            .argv
                            .join(" ")
                            .contains(detect::real(path).to_string_lossy().as_ref()))
            });
            if let Some(proc) = live {
                Some(format!("a live browser still uses it (pid {})", proc.pid))
            } else {
                fs::read_link(p.join("SingletonLock"))
                    .ok()
                    .and_then(|p| {
                        p.to_string_lossy()
                            .rsplit('-')
                            .next()
                            .and_then(|s| s.parse::<i32>().ok())
                    })
                    .filter(|&pid| pid > 1 && alive(pid))
                    .map(|pid| format!("profile lock held by live process {pid}"))
            }
        };
        if let Some(reason) = refusal {
            kept.push((path.clone(), reason));
            continue;
        }
        match fs::remove_dir_all(p) {
            Ok(()) => removed.push(path.clone()),
            Err(e) => kept.push((
                path.clone(),
                format!(
                    "delete failed: {}",
                    e.to_string()
                        .split(" (os error")
                        .next()
                        .unwrap_or("unknown error")
                ),
            )),
        }
    }
    Ok((removed, kept))
}
