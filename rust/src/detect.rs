use crate::proc::Proc;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    path::{Component, Path, PathBuf},
};

pub fn absolute(text: &str) -> PathBuf {
    let expanded = if text == "~" || text.starts_with("~/") {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(text.strip_prefix("~/").unwrap_or(""))
    } else {
        PathBuf::from(text)
    };
    let path = if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir().unwrap_or_default().join(expanded)
    };
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            _ => out.push(part),
        }
    }
    out
}
// canonicalize() alone fails for a profile that has just disappeared. Resolve
// its existing ancestor too, preserving /tmp -> /private/tmp equivalence.
pub fn real(text: &str) -> PathBuf {
    let path = absolute(text);
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    let mut tail = Vec::new();
    let mut parent = path.as_path();
    while let Some(name) = parent.file_name() {
        tail.push(name.to_owned());
        parent = parent.parent().unwrap_or(Path::new("/"));
        if let Ok(mut p) = parent.canonicalize() {
            for name in tail.iter().rev() {
                p.push(name)
            }
            return p;
        }
    }
    path
}
pub fn under(path: &str, prefix: &str) -> bool {
    real(path).starts_with(real(prefix))
}
pub fn is_temp(path: &str) -> bool {
    [
        "/tmp",
        "/var/tmp",
        "/private/tmp",
        "/private/var/tmp",
        "/dev/shm",
    ]
    .iter()
    .any(|p| under(path, p))
        || real(path).starts_with(real(&env::temp_dir().to_string_lossy()))
        || {
            let p = real(path).to_string_lossy().into_owned();
            p.contains("/var/folders/") && p.contains("/T/")
        }
}
pub fn is_default(path: &str) -> bool {
    let home = env::var("HOME").unwrap_or_default();
    let roots: &[&str] = if cfg!(target_os = "macos") {
        &[
            "Library/Application Support/Google/Chrome",
            "Library/Application Support/Google/Chrome Beta",
            "Library/Application Support/Google/Chrome Dev",
            "Library/Application Support/Chromium",
            "Library/Application Support/Microsoft Edge",
            "Library/Application Support/BraveSoftware/Brave-Browser",
            "Library/Application Support/Firefox",
        ]
    } else {
        &[
            ".config/google-chrome",
            ".config/google-chrome-beta",
            ".config/chromium",
            ".config/microsoft-edge",
            ".config/BraveSoftware/Brave-Browser",
            ".mozilla/firefox",
        ]
    };
    roots.iter().any(|p| under(path, &format!("{home}/{p}")))
}
pub fn is_automation(path: &str) -> bool {
    if is_temp(path) {
        return true;
    }
    let p = real(path).to_string_lossy().to_lowercase();
    let name = p.rsplit('/').next().unwrap_or("");
    let profile_marker = |prefix: &str, trailing_separator: bool| -> bool {
        if let Some(rest) = name.strip_prefix(prefix) {
            for (i, _) in rest.match_indices("profile") {
                if rest[..i]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && (!trailing_separator || rest[i + 7..].starts_with(['-', '_']))
                {
                    return true;
                }
            }
        }
        false
    };
    name.starts_with("scoped_dir")
        || name.starts_with(".org.chromium.chromium.")
        || profile_marker("playwright_", true)
        || profile_marker("puppeteer_dev_", false)
        || ["/ms-playwright/", "/chrome-for-testing/", "/puppeteer/"]
            .iter()
            .any(|m| p.contains(m))
}
pub fn is_browser(p: &Proc) -> bool {
    let n = p.name().to_lowercase();
    let names = [
        "chrome",
        "chrome.exe",
        "google chrome",
        "google-chrome",
        "google-chrome-stable",
        "google-chrome-beta",
        "google-chrome-unstable",
        "chromium",
        "chromium-browser",
        "chromium-browser-stable",
        "chromium.exe",
        "chrome-headless-shell",
        "chrome-headless-shell.exe",
        "headless_shell",
        "headless-shell",
        "msedge",
        "msedge.exe",
        "microsoft edge",
        "microsoft-edge",
        "microsoft-edge-stable",
        "firefox",
        "firefox-bin",
        "firefox.exe",
        "nightly",
        "pw_run.sh",
        "playwright",
        "minibrowser",
    ];
    names.contains(&n.as_str())
        || ([
            "ms-playwright",
            "chrome-for-testing",
            "puppeteer",
            "playwright",
        ]
        .iter()
        .any(|m| p.exe.to_lowercase().contains(m))
            && [
                "chrome",
                "chromium",
                "headless",
                "firefox",
                "nightly",
                "pw_",
                "playwright",
                "minibrowser",
                "webkit",
            ]
            .iter()
            .any(|s| n.starts_with(s)))
}
fn helper(p: &Proc) -> bool {
    [
        "helper",
        "crashpad_handler",
        "webprocess",
        "networkprocess",
        "gpuprocess",
    ]
    .iter()
    .any(|m| p.name().to_lowercase().contains(m))
        || p.argv.iter().any(|a| {
            let a = a.to_lowercase();
            a.starts_with("--type=") || a == "-contentproc" || a == "--contentproc"
        })
}
fn option(p: &Proc, names: &[&str]) -> Option<String> {
    for (i, a) in p.argv.iter().enumerate() {
        let (key, value) = a
            .split_once('=')
            .map(|(k, v)| (k, Some(v)))
            .unwrap_or((a, None));
        if names.contains(&key) {
            let value = value.or_else(|| {
                p.argv
                    .get(i + 1)
                    .map(String::as_str)
                    .filter(|s| !s.starts_with('-'))
            })?;
            return if value.is_empty() {
                None
            } else {
                Some(value.to_owned())
            };
        }
    }
    None
}
pub fn profile(p: &Proc) -> Option<String> {
    option(p, &["--user-data-dir", "-user-data-dir"])
        .or_else(|| option(p, &["-profile", "--profile"]))
        .map(|p| absolute(&p).to_string_lossy().into_owned())
}
#[derive(Clone)]
pub struct Group {
    pub root: Proc,
    pub members: Vec<Proc>,
    pub profile: Option<String>,
    pub signals: Vec<String>,
    pub reason: String,
}
impl Group {
    pub fn pids(&self) -> Vec<i32> {
        self.members.iter().map(|p| p.pid).collect()
    }
    pub fn rss(&self) -> u64 {
        self.members.iter().map(|p| p.rss).sum()
    }
    pub fn age(&self, now: f64) -> f64 {
        now - self
            .members
            .iter()
            .map(|p| p.started)
            .fold(f64::INFINITY, f64::min)
    }
}
pub fn groups(procs: &[Proc], now: f64, min_age: f64, paths: &[String]) -> Vec<Group> {
    let by_pid: HashMap<i32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<i32, Vec<&Proc>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p)
    }
    let mut roots: BTreeMap<PathBuf, Group> = BTreeMap::new();
    for p in procs {
        if !is_browser(p) || helper(p) {
            continue;
        }
        let profile = profile(p);
        if profile.as_ref().is_some_and(|s| is_default(s)) {
            continue;
        }
        let mut signals = Vec::new();
        if p.argv.iter().any(|s| {
            let s = s.to_lowercase();
            s == "--headless"
                || s == "-headless"
                || s.starts_with("--headless=")
                || s.starts_with("-headless=")
        }) {
            signals.push("headless".into())
        }
        let automation = profile.as_ref().is_some_and(|s| is_automation(s));
        if automation {
            signals.push(
                if is_temp(profile.as_ref().unwrap()) {
                    "temp profile"
                } else {
                    "automation profile"
                }
                .into(),
            )
        }
        if automation
            && p.argv
                .iter()
                .any(|s| s.to_lowercase().starts_with("--remote-debugging-"))
        {
            signals.push("remote debugging".into())
        }
        if signals.is_empty() {
            continue;
        }
        let parent = by_pid.get(&p.ppid);
        let reason = if p.ppid == 1 {
            format!(
                "launcher gone (reparented to {} 1)",
                parent.map(|p| p.name()).unwrap_or("init")
            )
        } else if let Some(parent) = parent {
            if ["launchd", "init", "systemd"].contains(&parent.name().to_lowercase().as_str()) {
                format!(
                    "launcher gone (reparented to {} {})",
                    parent.name(),
                    parent.pid
                )
            } else {
                continue;
            }
        } else {
            format!("launcher gone (parent pid {} exited)", p.ppid)
        };
        let key = profile
            .as_ref()
            .map(|s| real(s))
            .unwrap_or_else(|| PathBuf::from(format!("pid:{}", p.pid)));
        if roots.get(&key).is_none_or(|g| p.started < g.root.started) {
            roots.insert(
                key,
                Group {
                    root: p.clone(),
                    members: Vec::new(),
                    profile,
                    signals,
                    reason,
                },
            );
        }
    }
    let mut out = Vec::new();
    for (key, mut g) in roots {
        let mut seen = HashSet::new();
        let mut stack = vec![g.root.pid];
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().map(|p| p.pid))
            }
        }
        for p in procs {
            if (is_browser(p) || helper(p)) && profile(p).is_some_and(|s| real(&s) == key) {
                seen.insert(p.pid);
            }
        }
        g.members = procs
            .iter()
            .filter(|p| seen.contains(&p.pid))
            .cloned()
            .collect();
        g.members.sort_by_key(|p| p.pid);
        if g.age(now) < min_age
            || (!paths.is_empty()
                && !g
                    .profile
                    .as_ref()
                    .is_some_and(|s| paths.iter().any(|prefix| under(s, prefix))))
        {
            continue;
        }
        out.push(g);
    }
    out.sort_by(|a, b| {
        b.age(now)
            .total_cmp(&a.age(now))
            .then(a.root.pid.cmp(&b.root.pid))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn browser(pid: i32, ppid: i32, profile: &str, extra: &[&str]) -> Proc {
        let mut argv = vec!["chromium".into(), format!("--user-data-dir={profile}")];
        argv.extend(extra.iter().map(|s| s.to_string()));
        Proc {
            pid,
            ppid,
            rss: 4096,
            started: 1.0,
            zombie: false,
            exe: "chromium".into(),
            argv,
        }
    }
    #[test]
    fn only_orphaned_automation_roots() {
        let orphan = browser(100, 1, "/tmp/reaper-fixture/prof", &["--headless"]);
        let live = browser(101, 200, "/tmp/reaper-fixture/live", &["--headless"]);
        // Even an inaccessible parent's PID must preserve the live-parent guard.
        let parent = Proc {
            pid: 200,
            ppid: -1,
            rss: 0,
            started: 0.0,
            zombie: false,
            exe: String::new(),
            argv: vec![],
        };
        let default = if cfg!(target_os = "macos") {
            "Library/Application Support/Google/Chrome"
        } else {
            ".config/google-chrome"
        };
        let default = format!("{}/{}", env::var("HOME").unwrap(), default);
        let personal = browser(102, 1, &default, &["--headless"]);
        let visible = browser(
            103,
            1,
            "/opt/personal-profile",
            &["--remote-debugging-port=9222"],
        );
        let mut helper = browser(104, 100, "/tmp/reaper-fixture/prof", &["--type=renderer"]);
        helper.exe = "chromium".into();
        let procs = vec![orphan, live, parent, personal, visible, helper];
        let found = groups(&procs, 100.0, 0.0, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].root.pid, 100);
        assert_eq!(found[0].pids(), vec![100, 104]);
        assert!(groups(&procs, 100.0, 100.0, &[]).is_empty());
        assert!(groups(&procs, 100.0, 0.0, &["/tmp/reaper-fixture/pro".into()]).is_empty());
    }
    #[test]
    fn profile_argument_boundaries() {
        let p = browser(100, 1, "/tmp/reaper fixture/profile", &["--headless=new"]);
        assert_eq!(profile(&p).as_deref(), Some("/tmp/reaper fixture/profile"));
        assert!(is_automation("/opt/playwright_firefoxdev_profile-abc"));
        assert!(is_automation("/opt/puppeteer_dev_chrome_profile-abc"));
        assert!(!is_automation("/opt/playwright_personal"));
        assert!(!under("/tmp/reaper-fixture-other", "/tmp/reaper-fixture"));
    }
}
