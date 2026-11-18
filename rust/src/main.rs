mod clones;
mod detect;
mod proc;
mod reap;
mod report;
use serde_json::{json, Value};
use std::{env, io};
const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = include_str!("help.txt");
#[derive(Default)]
struct Args {
    kill: bool,
    json: bool,
    remove: bool,
    clean_clones: bool,
    all_leftovers: bool,
    min_age: Option<String>,
    grace: Option<String>,
    paths: Vec<String>,
}
fn duration(text: &str, option: &str) -> Result<f64, String> {
    let raw = text.trim();
    let (number, multiplier) = match raw.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => (
            &raw[..raw.len() - 1],
            match c.to_ascii_lowercase() {
                's' => 1.0,
                'm' => 60.0,
                'h' => 3600.0,
                'd' => 86400.0,
                _ => 0.0,
            },
        ),
        _ => (raw, 1.0),
    };
    let number = number.trim_end();
    let parts: Vec<&str> = number.split('.').collect();
    let valid = parts.len() <= 2
        && parts
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
    if valid && multiplier > 0.0 {
        if let Ok(n) = number.parse::<f64>() {
            if (n * multiplier).is_finite() {
                return Ok(n * multiplier);
            }
        }
    }
    Err(format!(
        "{option}: expected a duration such as 30s, 10m, 2h or 1d, got {}",
        py_repr(text)
    ))
}
fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let body = text
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace(quote, &format!("\\{quote}"));
    format!("{quote}{body}{quote}")
}
fn parse(arguments: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut unknown = Vec::new();
    let mut i = 0;
    while i < arguments.len() {
        let token = &arguments[i];
        let (key, value) = token
            .split_once('=')
            .map(|(k, v)| (k, Some(v.to_owned())))
            .unwrap_or((token.as_str(), None));
        let options = [
            "--kill",
            "--clean-clones",
            "--all-leftovers",
            "--json",
            "--remove-profiles",
            "--min-age",
            "--grace",
            "--path",
            "--version",
            "--help",
        ];
        let key = if key.starts_with("--") && key.len() > 2 {
            options
                .iter()
                .find(|s| s.starts_with(key))
                .copied()
                .unwrap_or(key)
        } else {
            key
        };
        if [
            "--kill",
            "--json",
            "--remove-profiles",
            "--clean-clones",
            "--all-leftovers",
        ]
        .contains(&key)
        {
            if let Some(v) = value {
                return Err(format!(
                    "argument {key}: ignored explicit argument {}",
                    py_repr(&v)
                ));
            }
            match key {
                "--kill" => args.kill = true,
                "--json" => args.json = true,
                "--clean-clones" => args.clean_clones = true,
                "--all-leftovers" => args.all_leftovers = true,
                _ => args.remove = true,
            }
        } else if ["--min-age", "--grace", "--path"].contains(&key) {
            let value = match value {
                Some(v) => v,
                None => {
                    i += 1;
                    arguments
                        .get(i)
                        .filter(|s| {
                            !s.starts_with('-')
                                || s.strip_prefix('-').is_some_and(|n| {
                                    let parts: Vec<&str> = n.split('.').collect();
                                    (parts.len() == 1
                                        && !parts[0].is_empty()
                                        && parts[0].chars().all(|c| c.is_ascii_digit()))
                                        || (parts.len() == 2
                                            && parts[0].chars().all(|c| c.is_ascii_digit())
                                            && !parts[1].is_empty()
                                            && parts[1].chars().all(|c| c.is_ascii_digit()))
                                })
                        })
                        .cloned()
                        .ok_or_else(|| format!("argument {key}: expected one argument"))?
                }
            };
            match key {
                "--min-age" => args.min_age = Some(value),
                "--grace" => args.grace = Some(value),
                _ => args.paths.push(value),
            }
        } else if key == "--version" {
            if let Some(v) = value {
                return Err(format!(
                    "argument --version: ignored explicit argument {}",
                    py_repr(&v)
                ));
            }
            println!("headless-reaper {VERSION}");
            std::process::exit(0);
        } else if key == "--help" || key == "-h" {
            if let Some(v) = value {
                return Err(format!(
                    "argument -h/--help: ignored explicit argument {}",
                    py_repr(&v)
                ));
            }
            print!("{HELP}");
            std::process::exit(0);
        } else {
            unknown.push(token.to_owned());
        }
        i += 1;
    }
    if !unknown.is_empty() {
        return Err(format!("unrecognized arguments: {}", unknown.join(" ")));
    }
    Ok(args)
}
fn fail(message: &str, code: i32, json: bool) -> i32 {
    if json {
        println!("{}", json!({"ok":false,"error":message}));
    } else {
        eprintln!("headless-reaper: {message}");
    }
    code
}
fn io_fail(error: io::Error, json: bool) -> i32 {
    let unsupported = error.kind() == io::ErrorKind::Unsupported;
    fail(
        &if unsupported {
            error.to_string()
        } else {
            format!("cannot read the process table: {error}")
        },
        if unsupported { 2 } else { 1 },
        json,
    )
}
fn run() -> i32 {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let json_mode = arguments.iter().any(|a| a == "--json");
    let mut args = match parse(&arguments) {
        Ok(a) => a,
        Err(e) => return fail(&e, 2, json_mode),
    };
    if args.all_leftovers {
        args.kill = true;
        args.clean_clones = true;
        args.remove = true;
    }
    let min_age = match duration(
        &args.min_age.unwrap_or_else(|| {
            env::var("HEADLESS_REAPER_MIN_AGE")
                .unwrap_or_else(|_| if args.clean_clones { "10m" } else { "0" }.into())
        }),
        "--min-age",
    ) {
        Ok(n) => n,
        Err(e) => return fail(&e, 2, args.json),
    };
    let grace = match duration(
        &args
            .grace
            .unwrap_or_else(|| env::var("HEADLESS_REAPER_GRACE").unwrap_or_else(|_| "10s".into())),
        "--grace",
    ) {
        Ok(n) => n,
        Err(e) => return fail(&e, 2, args.json),
    };
    let paths = if args.paths.is_empty() {
        env::var_os("HEADLESS_REAPER_PATH")
            .map(|s| {
                env::split_paths(&s)
                    .map(|p| p.to_string_lossy().trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        args.paths
    };
    if args.remove && !args.kill {
        return fail("--remove-profiles requires --kill", 2, args.json);
    }
    let procs = if args.clean_clones && !args.kill {
        Vec::new()
    } else {
        match proc::list() {
            Ok(p) => p,
            Err(e) if args.clean_clones && e.kind() == io::ErrorKind::Unsupported => {
                return fail(
                    &format!("{e}; code-sign clones: nothing to do on this OS"),
                    2,
                    args.json,
                )
            }
            Err(e) => return io_fail(e, args.json),
        }
    };
    let now = proc::now();
    let groups = detect::groups(&procs, now, min_age, &paths);
    let result = if args.kill {
        match reap::reap(&groups, grace) {
            Ok(r) => Some(r),
            Err(e) => return io_fail(e, args.json),
        }
    } else {
        None
    };
    let (removed, kept) = if args.remove {
        let r = result.as_ref().unwrap();
        let clean: Vec<_> = groups
            .iter()
            .filter(|g| {
                !g.pids()
                    .iter()
                    .any(|p| r.survivors.contains(p) || r.denied.contains(p))
            })
            .cloned()
            .collect();
        match reap::cleanup(&clean) {
            Ok(r) => r,
            Err(e) => return io_fail(e, args.json),
        }
    } else {
        (Vec::new(), Vec::new())
    };
    let clones = if args.clean_clones {
        Some(clones::clean(args.kill, min_age))
    } else {
        None
    };
    let clone_failed = clones.as_ref().is_some_and(|c| c.failed());
    if args.json {
        let gs:Vec<Value>=groups.iter().map(|g| json!({"root_pid":g.root.pid,"pids":g.pids(),"processes":g.members.len(),"profile":g.profile,"signals":g.signals,"reason":g.reason,"age_seconds":g.age(now).max(0.0) as u64,"age":report::duration(g.age(now)),"rss_bytes":g.rss(),"rss":report::bytes(g.rss())})).collect();
        let r=result.as_ref().map(|r| json!({"targets":r.targets,"sigterm":r.sigterm,"sigkill":r.sigkill,"survivors":r.survivors,"already_gone":r.already_gone,"denied":r.denied,"removed_profiles":removed,"kept_profiles":kept.iter().map(|(p,why)| json!({"path":p,"reason":why})).collect::<Vec<_>>()}));
        let mut payload = json!({"ok":!clone_failed,"version":VERSION,"platform":proc::platform(),"dry_run":!args.kill,"scanned":procs.len(),"min_age_seconds":min_age as u64,"paths":paths,"totals":{"groups":groups.len(),"processes":groups.iter().map(|g|g.members.len()).sum::<usize>(),"rss_bytes":groups.iter().map(|g|g.rss()).sum::<u64>()},"groups":gs,"reap":r});
        if let Some(c) = &clones {
            payload["clones"] = c.value();
        }
        println!("{payload}");
    } else {
        if !args.clean_clones || args.kill {
            println!(
                "{}",
                report::render(
                    &groups,
                    procs.len(),
                    now,
                    min_age,
                    &paths,
                    result.as_ref(),
                    &removed,
                    &kept
                )
            );
        }
        if let Some(c) = &clones {
            println!("{}", c.render());
        }
    }
    if clone_failed || result.is_some_and(|r| !r.survivors.is_empty()) {
        1
    } else {
        0
    }
}
fn main() {
    std::process::exit(run());
}
