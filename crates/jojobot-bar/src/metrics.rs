//! Timing only: a failure to record samples never changes the bar's verdict.
//! The sample writer owns the closed label sets; raw job/test names stay local.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn record(mut value: serde_json::Value) {
    let Some(file) = std::env::var_os("JOJOBOT_BUILD_METRICS_FILE") else {
        return;
    };
    value["checkout"] = std::env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .into();
    value["ended"] = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
        .into();
    let written = (|| -> std::io::Result<()> {
        let mut out = OpenOptions::new().create(true).append(true).open(file)?;
        out.write_all(format!("{value}\n").as_bytes())
    })();
    if written.is_err() {
        eprintln!("bar: build metrics not recorded; verdict kept");
    }
}

pub fn phase(args: &[&str], ok: bool, took: Duration) {
    let step = match args.first().copied() {
        Some("fmt") => "fmt",
        Some("build") => "build",
        Some("clippy") => "lint",
        Some("test") if args.contains(&"--no-run") => "test_compile",
        Some("test") if args.contains(&"--list") => "test_list",
        Some("test") if args.contains(&"--doc") => "doc",
        Some("test") => "test",
        _ => "other",
    };
    record(serde_json::json!({"kind":"step", "step":step,
        "outcome":if ok {"success"} else {"failure"}, "seconds":took.as_secs_f64()}));
}

pub fn test_binary(job: &[String], ok: bool, text: &str, took: Duration) {
    if std::env::var_os("JOJOBOT_BUILD_METRICS_FILE").is_none() {
        return;
    }
    record(
        serde_json::json!({"kind":if job == ["--doc"] {"step"} else {"test_binary"},
        "step":if job == ["--doc"] {"doc"} else {"test"}, "job":crate::recording::job_name(job),
        "outcome":if ok {"success"} else {"failure"}, "seconds":took.as_secs_f64()}),
    );
    for test in crate::parse_job(text).tests {
        if let Some(ms) = test.duration_ms {
            record(
                serde_json::json!({"kind":"test", "step":"test", "job":crate::recording::job_name(job),
                "test":test.test, "outcome":if test.verdict == crate::Verdict::Failed {"failure"} else {"success"},
                "seconds":ms as f64 / 1000.0}),
            );
        }
    }
}
