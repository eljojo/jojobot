//! **Where a run of the bar leaves what its tests did.**
//!
//! Every run is a directory of its own under the history directory, so two bars
//! running at once on one machine never write the same file and need no lock.
//! `run.json` is written when the run begins and has no end until the run
//! finishes. Each job writes one `<job>.jsonl` when it finishes, under another
//! name first and renamed into place, so what a reader finds under a job's name
//! is all of it. A bar that is killed leaves a header with no end and the jobs
//! that finished, and a reader counts a job with no file as no data, never as a
//! pass.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::history::{JobRecord, Verdict};

/// **The variable that names the history directory.** An empty value counts as
/// unset.
pub const HISTORY_DIR_VARIABLE: &str = "BAR_HISTORY_DIR";

/// The history directory under a home directory, when nothing names one.
const DEFAULT_UNDER_HOME: &str = ".local/state/jojobot-bar";

/// **Where the history lives**: the directory the variable names, else the
/// default under the home directory, else nowhere.
pub fn history_root(variable: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    fn named(value: Option<&str>) -> Option<&str> {
        value.filter(|v| !v.trim().is_empty())
    }
    match (named(variable), named(home)) {
        (Some(dir), _) => Some(PathBuf::from(dir)),
        (None, Some(home)) => Some(Path::new(home).join(DEFAULT_UNDER_HOME)),
        (None, None) => None,
    }
}

/// **What a run says about itself when it begins.**
#[derive(Debug, Clone, PartialEq)]
pub struct RunHeader {
    /// Also the name of the run's directory.
    pub run_id: String,
    /// Which target ran: `check` or `rooms`.
    pub level: String,
    /// The name of the directory the bar ran in.
    pub checkout: String,
    /// The commit the checkout was at, when git said.
    pub head: Option<String>,
    /// Whether the tree had uncommitted changes, when git said. A dirty tree's
    /// head is not the code that ran.
    pub dirty: Option<bool>,
    pub started_at: String,
    /// How many test binaries ran at once, which decides how long each took.
    pub bar_jobs: usize,
    /// The machine's one-minute load when the run began, when it could be read.
    pub load_average: Option<f64>,
    /// How many CPUs the machine gave the bar, which turns a load into a share:
    /// a load of 16 on 16 CPUs is a machine with room, and a load of 16 on 4 is not.
    pub cpus: Option<usize>,
}

impl RunHeader {
    fn to_json(&self, ended_at: Option<&str>) -> String {
        let mut fields = vec![
            ("run_id", json_string(&self.run_id)),
            ("level", json_string(&self.level)),
            ("checkout", json_string(&self.checkout)),
            (
                "head",
                self.head.as_deref().map_or("null".into(), json_string),
            ),
            ("dirty", self.dirty.map_or("null".into(), |d| d.to_string())),
            ("started_at", json_string(&self.started_at)),
            ("bar_jobs", self.bar_jobs.to_string()),
            (
                "load_average",
                self.load_average.map_or("null".into(), |l| l.to_string()),
            ),
            ("cpus", self.cpus.map_or("null".into(), |n| n.to_string())),
        ];
        if let Some(ended) = ended_at {
            fields.push(("ended_at", json_string(ended)));
        }
        json_object(&fields)
    }
}

/// **One run's directory.**
#[derive(Debug)]
pub struct Run {
    dir: PathBuf,
    header: RunHeader,
}

impl Run {
    /// **Make the run's directory and write its header, with no end.**
    pub fn begin(root: &Path, header: RunHeader) -> io::Result<Run> {
        let dir = root.join(&header.run_id);
        fs::create_dir_all(&dir)?;
        write_whole(&dir, "run.json", &header.to_json(None))?;
        Ok(Run { dir, header })
    }

    /// **Write a finished job's file**: its own line, then one line per test.
    /// `ok` is whether the job's `cargo test` exited zero.
    pub fn record_job(&self, job: &[String], ok: bool, record: &JobRecord) -> io::Result<()> {
        let name = job_name(job);
        let mut text = json_object(&[
            ("kind", json_string("job")),
            ("job", json_string(&name)),
            ("ok", ok.to_string()),
            (
                "duration_ms",
                record
                    .duration_ms
                    .map_or("null".into(), |ms| ms.to_string()),
            ),
        ]);
        text.push('\n');
        for test in &record.tests {
            text.push_str(&json_object(&[
                ("kind", json_string("test")),
                ("job", json_string(&name)),
                ("test", json_string(&test.test)),
                ("verdict", json_string(verdict_word(test.verdict))),
                (
                    "duration_ms",
                    test.duration_ms.map_or("null".into(), |ms| ms.to_string()),
                ),
            ]));
            text.push('\n');
        }
        write_whole(&self.dir, &format!("{name}.jsonl"), &text)
    }

    /// **Say when the run ended**, by writing the header again with its end.
    pub fn finish(&self, ended_at: &str) -> io::Result<()> {
        write_whole(&self.dir, "run.json", &self.header.to_json(Some(ended_at)))
    }
}

/// **A job's name in a file name**: `--lib` is `lib`, `--test alpha` is
/// `test-alpha`, and `-p alpha --lib` is `lib-alpha`: the package comes last.
pub fn job_name(job: &[String]) -> String {
    let (package, rest) = match job {
        [flag, package, rest @ ..] if flag == "-p" => (Some(package), rest),
        _ => (None, job),
    };
    rest.iter()
        .chain(package)
        .map(|part| part.trim_start_matches('-'))
        .collect::<Vec<_>>()
        .join("-")
}

fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Ok => "ok",
        Verdict::Failed => "failed",
        Verdict::Ignored => "ignored",
    }
}

/// **Write a file so that it exists whole or not at all**: under a temporary
/// name in the same directory, then renamed over the real one.
pub(crate) fn write_whole(dir: &Path, name: &str, text: &str) -> io::Result<()> {
    let temporary = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, dir.join(name)).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

pub(crate) fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An object from keys and values already written as JSON, in the order given.
pub(crate) fn json_object(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string(key)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// **A moment as `YYYY-MM-DDTHH:MM:SSZ`**, from seconds since the epoch.
pub fn iso_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // The civil date from a day count (days since 1970-01-01), by the standard
    // era arithmetic: 400-year eras of 146,097 days, March as the first month.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// **Seconds since the epoch from `YYYY-MM-DDTHH:MM:SSZ`**, the inverse of
/// [`iso_utc`]. Any other spelling is `None`.
pub fn parse_iso_utc(moment: &str) -> Option<u64> {
    let (date, time) = moment.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hours: u64 = time.next()?.parse().ok()?;
    let minutes: u64 = time.next()?.parse().ok()?;
    let seconds: u64 = time.next()?.parse().ok()?;
    if date.next().is_some()
        || time.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hours > 23
        || minutes > 59
        || seconds > 59
    {
        return None;
    }
    // The day count from the civil date, by the same era arithmetic as
    // `iso_utc` run the other way: March is the first month of the shifted year.
    let shifted_year = year - i64::from(month <= 2);
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year.rem_euclid(400);
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days)
        .ok()
        .map(|days| days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

/// **A run's name**: when it began, the checkout it ran in and the process.
pub fn run_id(started_at: &str, checkout: &str, pid: u32) -> String {
    let compact: String = started_at.chars().filter(char::is_ascii_digit).collect();
    let checkout: String = checkout
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{compact}-{checkout}-{pid}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{TestRecord, Verdict};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(crate::scratch::name(&format!(
            "jojobot-bar-recording-{name}"
        )));

        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn header() -> RunHeader {
        RunHeader {
            run_id: "20231114T221320Z-jojobot-7".into(),
            level: "check".into(),
            checkout: "jojobot".into(),
            head: Some("abc123".into()),
            dirty: Some(false),
            started_at: "2023-11-14T22:13:20Z".into(),
            bar_jobs: 8,
            load_average: Some(1.5),
            cpus: Some(16),
        }
    }

    /// **The directory comes from the variable, else from the home directory,
    /// else nowhere, and an empty value is no value.**
    #[test]
    fn the_history_directory_is_the_variable_then_home_then_nowhere() {
        assert_eq!(
            history_root(Some("/data/h"), Some("/home/x")),
            Some(PathBuf::from("/data/h"))
        );
        assert_eq!(
            history_root(None, Some("/home/x")),
            Some(PathBuf::from("/home/x/.local/state/jojobot-bar"))
        );
        assert_eq!(
            history_root(Some(""), Some("/home/x")),
            Some(PathBuf::from("/home/x/.local/state/jojobot-bar")),
            "an empty variable is unset"
        );
        assert_eq!(history_root(Some("  "), Some("")), None);
        assert_eq!(history_root(None, None), None);
    }

    /// **A moment is written in UTC to the second.** The first value is the
    /// epoch; the others are days that cross a leap day and a century.
    #[test]
    fn a_moment_is_written_in_utc_to_the_second() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_utc(1_709_164_800), "2024-02-29T00:00:00Z", "a leap day");
        assert_eq!(
            iso_utc(951_782_400),
            "2000-02-29T00:00:00Z",
            "a century leap day"
        );
        assert_eq!(
            iso_utc(4_107_542_399),
            "2100-02-28T23:59:59Z",
            "2100 has no leap day"
        );
    }

    /// **A moment read back is the moment written.** The same days as above,
    /// and a spelling that is not a moment reads as none.
    #[test]
    fn a_moment_written_reads_back_the_same_and_a_bad_spelling_reads_as_none() {
        for seconds in [0, 1_700_000_000, 1_709_164_800, 951_782_400, 4_107_542_399] {
            assert_eq!(parse_iso_utc(&iso_utc(seconds)), Some(seconds));
        }
        for bad in [
            "",
            "2026-10-08",
            "2026-10-08T10:00:00",
            "2026-13-08T10:00:00Z",
            "2026-10-08T24:00:00Z",
            "2026-10-08T10:00:00:00Z",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(parse_iso_utc(bad), None, "{bad}");
        }
    }

    /// **A job's file name says which job it is, and a run's name says when,
    /// where and by which process.** The stored spellings, pinned.
    #[test]
    fn the_names_that_reach_the_disk_are_pinned() {
        assert_eq!(job_name(&["--lib".into()]), "lib");
        assert_eq!(job_name(&["--test".into(), "alpha".into()]), "test-alpha");
        assert_eq!(
            job_name(&["-p".into(), "jojobot-domain".into(), "--lib".into()]),
            "lib-jojobot-domain"
        );
        assert_eq!(
            run_id("2023-11-14T22:13:20Z", "jojobot 6/x", 42),
            "20231114221320-jojobot_6_x-42"
        );
    }

    /// **The header and the job file are written as these exact lines.** The
    /// spelling of a stored key is declared nowhere else, so this is the pin.
    #[test]
    fn the_stored_lines_are_pinned() {
        let root = scratch("pinned");
        let run = Run::begin(&root, header()).expect("begin");
        let dir = root.join("20231114T221320Z-jojobot-7");
        assert_eq!(
            fs::read_to_string(dir.join("run.json")).expect("header"),
            "{\"run_id\":\"20231114T221320Z-jojobot-7\",\"level\":\"check\",\"checkout\":\"jojobot\",\
             \"head\":\"abc123\",\"dirty\":false,\"started_at\":\"2023-11-14T22:13:20Z\",\
             \"bar_jobs\":8,\"load_average\":1.5,\"cpus\":16}",
            "a run that has not ended says nothing of its end"
        );
        run.finish("2023-11-14T22:14:00Z").expect("finish");
        assert!(
            fs::read_to_string(dir.join("run.json"))
                .expect("header")
                .ends_with(",\"ended_at\":\"2023-11-14T22:14:00Z\"}"),
        );
        let record = JobRecord {
            tests: vec![
                TestRecord {
                    test: "a::b \"quoted\"".into(),
                    verdict: Verdict::Failed,
                    duration_ms: Some(7),
                },
                TestRecord {
                    test: "a::c".into(),
                    verdict: Verdict::Ignored,
                    duration_ms: None,
                },
            ],
            duration_ms: Some(1250),
        };
        run.record_job(&["--test".into(), "alpha".into()], false, &record)
            .expect("job");
        assert_eq!(
            fs::read_to_string(dir.join("test-alpha.jsonl")).expect("job file"),
            "{\"kind\":\"job\",\"job\":\"test-alpha\",\"ok\":false,\"duration_ms\":1250}\n\
             {\"kind\":\"test\",\"job\":\"test-alpha\",\"test\":\"a::b \\\"quoted\\\"\",\
             \"verdict\":\"failed\",\"duration_ms\":7}\n\
             {\"kind\":\"test\",\"job\":\"test-alpha\",\"test\":\"a::c\",\
             \"verdict\":\"ignored\",\"duration_ms\":null}\n"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// **A file is replaced whole**: writing a job twice leaves the second, and
    /// no temporary name is left behind.
    #[test]
    fn a_file_is_replaced_whole_and_leaves_no_temporary_name() {
        let root = scratch("whole");
        let run = Run::begin(&root, header()).expect("begin");
        let dir = root.join("20231114T221320Z-jojobot-7");
        let one = JobRecord {
            tests: vec![],
            duration_ms: Some(1),
        };
        let two = JobRecord {
            tests: vec![],
            duration_ms: Some(2),
        };
        run.record_job(&["--lib".into()], true, &one)
            .expect("first");
        run.record_job(&["--lib".into()], true, &two)
            .expect("second");
        assert!(
            fs::read_to_string(dir.join("lib.jsonl"))
                .expect("file")
                .contains("\"duration_ms\":2")
        );
        let mut names: Vec<String> = fs::read_dir(&dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["lib.jsonl", "run.json"]);
        let _ = fs::remove_dir_all(&root);
    }
}
