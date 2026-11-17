use crate::{detect::Group, reap::Result};
const VERSION: &str = env!("CARGO_PKG_VERSION");
pub fn bytes(count: u64) -> String {
    let mut v = count as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v < 1024.0 || unit == "TB" {
            return if unit == "B" {
                format!("{count} B")
            } else {
                format!("{} {unit}", format!("{v:.1}").trim_end_matches(".0"))
            };
        }
        v /= 1024.0;
    }
    unreachable!()
}
pub fn duration(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (d, h, m, s) = (
        total / 86400,
        total % 86400 / 3600,
        total % 3600 / 60,
        total % 60,
    );
    if d > 0 {
        if h > 0 {
            format!("{d}d {h}h")
        } else {
            format!("{d}d")
        }
    } else if h > 0 {
        if m > 0 {
            format!("{h}h {m}m")
        } else {
            format!("{h}h")
        }
    } else if m > 0 {
        if s > 0 {
            format!("{m}m {s}s")
        } else {
            format!("{m}m")
        }
    } else {
        format!("{s}s")
    }
}
fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.into()
    } else {
        many.into()
    }
}
fn pids(pids: &[i32]) -> String {
    pids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
pub fn render(
    groups: &[Group],
    scanned: usize,
    now: f64,
    min_age: f64,
    paths: &[String],
    result: Option<&Result>,
    removed: &[String],
    kept: &[(String, String)],
) -> String {
    let mut scope = Vec::new();
    if min_age > 0.0 {
        scope.push(format!("min age {}", duration(min_age)))
    }
    if !paths.is_empty() {
        scope.push(format!("paths {}", paths.join(", ")))
    }
    let suffix = format!(
        " ({scanned} processes scanned{})",
        if scope.is_empty() {
            String::new()
        } else {
            format!(", {}", scope.join(", "))
        }
    );
    if groups.is_empty() {
        return format!("headless-reaper {VERSION}: no orphaned headless browsers{suffix}.");
    }
    let processes = groups.iter().map(|g| g.members.len()).sum::<usize>();
    let mut lines = vec![
        format!(
            "headless-reaper {VERSION}: {} orphaned {}, {processes} {}, {} RSS{suffix}",
            groups.len(),
            plural(groups.len(), "group", "groups"),
            plural(processes, "process", "processes"),
            bytes(groups.iter().map(Group::rss).sum())
        ),
        String::new(),
    ];
    let headers = ["ROOT", "AGE", "RSS", "PIDS", "PROFILE"];
    let rows: Vec<[String; 5]> = groups
        .iter()
        .map(|g| {
            [
                g.root.pid.to_string(),
                duration(g.age(now)),
                bytes(g.rss()),
                g.members.len().to_string(),
                g.profile
                    .clone()
                    .unwrap_or_else(|| "(no profile switch)".into()),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..5)
        .map(|i| {
            rows.iter()
                .map(|r| r[i].chars().count())
                .max()
                .unwrap_or(0)
                .max(headers[i].len())
        })
        .collect();
    let row = |r: &[String; 5]| {
        (0..5)
            .map(|i| format!("{}{}", r[i], " ".repeat(widths[i] - r[i].chars().count())))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_owned()
    };
    lines.push(row(&headers.map(str::to_owned)));
    for (g, r) in groups.iter().zip(rows.iter()) {
        lines.push(row(r));
        lines.push(format!(
            "{}why: {}; {}",
            " ".repeat(widths[0] + 2),
            g.signals.join(", "),
            g.reason
        ));
    }
    lines.push(String::new());
    if let Some(r) = result {
        lines.push(format!(
            "reaped {} {}: sent SIGTERM to {} of {} processes, SIGKILL to {}.",
            groups.len(),
            plural(groups.len(), "group", "groups"),
            r.sigterm.len(),
            r.targets,
            r.sigkill.len()
        ));
        if !r.already_gone.is_empty() {
            lines.push(format!(
                "already gone before the signal: {} processes.",
                r.already_gone.len()
            ));
        }
        if !r.denied.is_empty() {
            lines.push(format!(
                "not permitted: {} (run as the owner of that process)",
                pids(&r.denied)
            ));
        }
        if !r.survivors.is_empty() {
            lines.push(format!(
                "still running after SIGKILL: {} (check permissions, then kill them yourself)",
                pids(&r.survivors)
            ));
        }
        for p in removed {
            lines.push(format!("removed temp profile: {p}"));
        }
        for (p, why) in kept {
            lines.push(format!("kept profile: {p} ({why})"));
        }
    } else {
        lines.push(
            "dry run: nothing was killed. Rerun with --kill to terminate these groups.".into(),
        );
    }
    lines.join("\n")
}
