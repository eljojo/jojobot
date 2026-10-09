//! **Read back the history `recording` wrote.**
//!
//! Every file it reads was written whole by this crate, so the reader asks only
//! for the shapes the writer produces: a flat JSON object per line. A file it
//! cannot read is no data, never a pass: the run or job it belongs to is left
//! out and the rest is read.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// **One value of a flat JSON object.**
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

/// **A flat JSON object** (strings, numbers, booleans and null, no nesting) as
/// the writer spells it. `None` for anything else.
pub(crate) fn parse_flat(line: &str) -> Option<BTreeMap<String, Json>> {
    let mut chars = line.trim().chars().peekable();
    let mut fields = BTreeMap::new();
    if chars.next()? != '{' {
        return None;
    }
    fn skip_space(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    }
    fn string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
        if chars.next()? != '"' {
            return None;
        }
        let mut out = String::new();
        loop {
            match chars.next()? {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let code: String = (0..4).filter_map(|_| chars.next()).collect();
                        out.push(char::from_u32(u32::from_str_radix(&code, 16).ok()?)?);
                    }
                    _ => return None,
                },
                c => out.push(c),
            }
        }
    }
    skip_space(&mut chars);
    if chars.peek() == Some(&'}') {
        return Some(fields);
    }
    loop {
        skip_space(&mut chars);
        let key = string(&mut chars)?;
        skip_space(&mut chars);
        if chars.next()? != ':' {
            return None;
        }
        skip_space(&mut chars);
        let value = if chars.peek() == Some(&'"') {
            Json::Str(string(&mut chars)?)
        } else {
            let mut word = String::new();
            while chars.peek().is_some_and(|c| !matches!(c, ',' | '}')) {
                word.push(chars.next()?);
            }
            match word.trim() {
                "null" => Json::Null,
                "true" => Json::Bool(true),
                "false" => Json::Bool(false),
                number => Json::Num(number.parse().ok()?),
            }
        };
        fields.insert(key, value);
        skip_space(&mut chars);
        match chars.next()? {
            ',' => continue,
            '}' => return Some(fields),
            _ => return None,
        }
    }
}

fn text(fields: &BTreeMap<String, Json>, key: &str) -> Option<String> {
    match fields.get(key)? {
        Json::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn number(fields: &BTreeMap<String, Json>, key: &str) -> Option<f64> {
    match fields.get(key)? {
        Json::Num(n) => Some(*n),
        _ => None,
    }
}

/// **One job as one run recorded it.**
#[derive(Debug, Clone, PartialEq)]
pub struct JobRun {
    pub run_id: String,
    /// The commit that run was at, when it said.
    pub head: Option<String>,
    /// The machine's load when that run began, when it could be read.
    pub load_average: Option<f64>,
    /// How many CPUs that run had, when it said: a run recorded before the
    /// field existed has none.
    pub cpus: Option<u64>,
    /// Whether the job's `cargo test` exited zero.
    pub ok: bool,
    pub duration_ms: Option<u64>,
    /// The tests in the job that failed, as the run printed them.
    pub failed: Vec<String>,
}

/// **A judgement of a red**, as `bar call` wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub test: String,
    /// `flake` or `defect`.
    pub call: String,
    pub run_id: Option<String>,
    pub head: Option<String>,
    /// When it was made; a later call on the same red replaces an earlier one.
    pub at: String,
}

/// **What the history directory holds for one level of run.**
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    /// Per job name, its runs oldest first.
    pub jobs: BTreeMap<String, Vec<JobRun>>,
    pub calls: Vec<RecordedCall>,
}

/// **Read the runs of the levels asked for** (`check`, `report` or `rooms`) and
/// every call. A directory that does not exist is an empty history.
pub fn read_history(root: &Path, levels: &[&str]) -> History {
    let mut history = History::default();
    let Ok(entries) = fs::read_dir(root) else {
        return history;
    };
    let mut runs: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    runs.sort();
    for dir in runs {
        if dir.file_name().is_some_and(|name| name == "calls") || !dir.is_dir() {
            continue;
        }
        let Some(header) = fs::read_to_string(dir.join("run.json"))
            .ok()
            .and_then(|text| parse_flat(&text))
        else {
            continue;
        };
        if !text(&header, "level").is_some_and(|level| levels.contains(&level.as_str())) {
            continue;
        }
        let Some(run_id) = text(&header, "run_id") else {
            continue;
        };
        let head = text(&header, "head");
        let load_average = number(&header, "load_average");
        let cpus = number(&header, "cpus").map(|n| n as u64);
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten().map(|f| f.path()) {
            if file.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(body) = fs::read_to_string(&file) else {
                continue;
            };
            let mut lines = body.lines().filter_map(parse_flat);
            let Some(job) = lines
                .next()
                .filter(|j| text(j, "kind").as_deref() == Some("job"))
            else {
                continue;
            };
            let (Some(name), Some(Json::Bool(ok))) = (text(&job, "job"), job.get("ok")) else {
                continue;
            };
            let failed = lines
                .filter(|l| text(l, "verdict").as_deref() == Some("failed"))
                .filter_map(|l| text(&l, "test"))
                .collect();
            history.jobs.entry(name).or_default().push(JobRun {
                run_id: run_id.clone(),
                head: head.clone(),
                load_average,
                cpus,
                ok: *ok,
                duration_ms: number(&job, "duration_ms").map(|ms| ms as u64),
                failed,
            });
        }
    }
    if let Ok(files) = fs::read_dir(root.join("calls")) {
        let mut files: Vec<_> = files.flatten().map(|f| f.path()).collect();
        files.sort();
        for file in files {
            let Some(call) = fs::read_to_string(&file)
                .ok()
                .and_then(|body| parse_flat(&body))
            else {
                continue;
            };
            if let (Some(test), Some(word), Some(at)) =
                (text(&call, "test"), text(&call, "call"), text(&call, "at"))
            {
                history.calls.push(RecordedCall {
                    test,
                    call: word,
                    run_id: text(&call, "run_id"),
                    head: text(&call, "head"),
                    at,
                });
            }
        }
    }
    history
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::{Call, write_call};
    use crate::history::{JobRecord, TestRecord, Verdict};
    use crate::recording::{Run, RunHeader};
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("jojobot-bar-reading-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn header(id: &str, level: &str, load: Option<f64>) -> RunHeader {
        RunHeader {
            run_id: id.into(),
            level: level.into(),
            checkout: "c".into(),
            head: Some(format!("head-{id}")),
            dirty: Some(false),
            started_at: "2026-10-08T22:13:20Z".into(),
            bar_jobs: 8,
            load_average: load,
            cpus: Some(4),
        }
    }

    fn record(failed: &[&str], ms: Option<u64>) -> JobRecord {
        let mut tests = vec![TestRecord {
            test: "t::passes".into(),
            verdict: Verdict::Ok,
            duration_ms: None,
        }];
        tests.push(TestRecord {
            test: "t::skipped".into(),
            verdict: Verdict::Ignored,
            duration_ms: None,
        });
        tests.extend(failed.iter().map(|name| TestRecord {
            test: name.to_string(),
            verdict: Verdict::Failed,
            duration_ms: None,
        }));
        JobRecord {
            tests,
            duration_ms: ms,
        }
    }

    /// **What the writer wrote is what the reader reads back**, run by run, in
    /// the order the runs began, with the level asked for and no other.
    #[test]
    fn what_the_writer_wrote_is_read_back_in_order_and_by_level() {
        let root = scratch("roundtrip");
        let alpha = ["--test".to_string(), "alpha".to_string()];
        let first =
            Run::begin(&root, header("20261001000000-c-1", "check", Some(2.5))).expect("begin");
        first
            .record_job(&alpha, true, &record(&[], Some(1_000)))
            .expect("job");
        let second = Run::begin(&root, header("20261002000000-c-2", "check", None)).expect("begin");
        second
            .record_job(&alpha, false, &record(&["t::breaks"], Some(2_000)))
            .expect("job");
        let rooms = Run::begin(&root, header("20261003000000-c-3", "rooms", None)).expect("begin");
        rooms
            .record_job(&alpha, true, &record(&[], Some(3_000)))
            .expect("job");
        write_call(
            &root,
            &Call {
                test: "t::breaks".into(),
                call: "flake".into(),
                by: "bot:alpha".into(),
                run_id: Some("20261002000000-c-2".into()),
                head: None,
                note: None,
            },
            "2026-10-08T23:00:00Z",
            7,
        )
        .expect("call");

        let history = read_history(&root, &["check"]);
        let runs = &history.jobs["test-alpha"];
        assert_eq!(runs.len(), 2, "the rooms run is another level: {runs:?}");
        assert_eq!(runs[0].run_id, "20261001000000-c-1");
        assert_eq!(runs[0].load_average, Some(2.5));
        assert_eq!(runs[0].cpus, Some(4));
        assert_eq!(runs[0].duration_ms, Some(1_000));
        assert!(runs[0].ok && runs[0].failed.is_empty());
        assert_eq!(runs[0].head.as_deref(), Some("head-20261001000000-c-1"));
        assert_eq!(runs[1].run_id, "20261002000000-c-2");
        assert_eq!(runs[1].load_average, None);
        assert!(!runs[1].ok);
        assert_eq!(
            runs[1].failed,
            ["t::breaks"],
            "only the failed test is named"
        );
        assert_eq!(history.calls.len(), 1);
        assert_eq!(history.calls[0].call, "flake");
        assert_eq!(
            history.calls[0].run_id.as_deref(),
            Some("20261002000000-c-2")
        );
        assert_eq!(read_history(&root, &["rooms"]).jobs["test-alpha"].len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    /// **A file the reader cannot read is no data, and the rest is read.** A
    /// run with no header, a job file with no job line and a call that is not an
    /// object are left out; a root that does not exist is an empty history.
    #[test]
    fn a_file_that_cannot_be_read_is_left_out_and_the_rest_is_read() {
        let root = scratch("damaged");
        let lib = ["--lib".to_string()];
        let good = Run::begin(&root, header("20261001000000-c-1", "check", None)).expect("begin");
        good.record_job(&lib, true, &record(&[], Some(5)))
            .expect("job");
        fs::create_dir_all(root.join("20261002000000-c-2")).expect("a run with no header");
        let broken = Run::begin(&root, header("20261003000000-c-3", "check", None)).expect("begin");
        broken
            .record_job(&lib, true, &record(&[], Some(5)))
            .expect("job");
        fs::write(root.join("20261003000000-c-3/doc.jsonl"), "not json\n").expect("junk");
        fs::create_dir_all(root.join("calls")).expect("calls");
        fs::write(root.join("calls/junk.json"), "[1,2]").expect("junk call");

        let history = read_history(&root, &["check"]);
        assert_eq!(history.jobs["lib"].len(), 2, "both lib files are whole");
        assert!(
            !history.jobs.contains_key("doc"),
            "the junk job file is no data"
        );
        assert!(history.calls.is_empty());
        assert_eq!(
            read_history(&root.join("absent"), &["check"]),
            History::default()
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// **The flat reader takes what the writer escapes, and nothing nested.**
    #[test]
    fn the_flat_reader_takes_the_escapes_and_refuses_what_is_not_flat() {
        let read = parse_flat(
            "{\"a\":\"x\\\"y\\\\z\\n\\u0001\",\"b\":12,\"c\":true,\"d\":null,\"e\":1.5}",
        )
        .expect("an object");
        assert_eq!(read["a"], Json::Str("x\"y\\z\n\u{1}".into()));
        assert_eq!(read["b"], Json::Num(12.0));
        assert_eq!(read["c"], Json::Bool(true));
        assert_eq!(read["d"], Json::Null);
        assert_eq!(read["e"], Json::Num(1.5));
        assert_eq!(parse_flat("{}"), Some(BTreeMap::new()));
        for bad in [
            "[1]",
            "{\"a\":}",
            "{\"a\":1",
            "{\"a\" 1}",
            "",
            "{\"a\":\"\\q\"}",
        ] {
            assert_eq!(parse_flat(bad), None, "{bad:?}");
        }
    }
}
