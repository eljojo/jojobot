//! **CI runner occupancy, summed over the forge's per-job task records.**
//!
//! The forge lists one record per job it ran, with the time the job started and
//! the time it last changed. A job's occupancy is the gap between the two. A job
//! that ran again is a second record, and a cancelled job still held a runner, so
//! both count. The run's wall time is never used: jobs of one run run side by
//! side, and the cost is the runner time they hold together.
//!
//! The bar reads the list from a file and reaches no forge. A workflow step
//! fetches the public list and passes the file.

use std::collections::BTreeSet;

use serde_json::Value;

const WEEK: i64 = 7 * 24 * 3600;
/// The states in which a job held a runner from its start to its last change.
const HELD: [&str; 3] = ["success", "failure", "cancelled"];

#[derive(Default)]
struct Window {
    seconds: i64,
    jobs: usize,
    runs: BTreeSet<i64>,
    pull_request_seconds: i64,
    pull_request_runs: BTreeSet<i64>,
}

impl Window {
    fn add(&mut self, run: i64, pull_request: bool, seconds: i64) {
        self.seconds += seconds;
        self.jobs += 1;
        self.runs.insert(run);
        if pull_request {
            self.pull_request_seconds += seconds;
            self.pull_request_runs.insert(run);
        }
    }

    fn describe(&self) -> String {
        let plural = |n: usize| if n == 1 { "run" } else { "runs" };
        if self.jobs == 0 {
            return "none".into();
        }
        let a_run = |seconds: i64, runs: usize| (2 * seconds + runs as i64) / (2 * runs as i64);
        let pull_requests = if self.pull_request_runs.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{} s a run",
                a_run(self.pull_request_seconds, self.pull_request_runs.len())
            )
        };
        format!(
            "{} s over {} jobs in {} {} ({} s a run; pull requests {pull_requests})",
            self.seconds,
            self.jobs,
            self.runs.len(),
            plural(self.runs.len()),
            a_run(self.seconds, self.runs.len()),
        )
    }
}

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Seconds since the epoch of an RFC 3339 time such as `2026-10-28T04:06:38-04:00`.
fn epoch(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once('T')?;
    let mut parts = date.split('-');
    let (year, month, day) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    let clock = rest.get(..8)?;
    let mut hms = clock.split(':');
    let (hour, minute, second): (i64, i64, i64) = (
        hms.next()?.parse().ok()?,
        hms.next()?.parse().ok()?,
        hms.next()?.parse().ok()?,
    );
    let zone = rest
        .get(8..)?
        .trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "Z" => 0,
        zone => {
            let sign = match zone.get(..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let (h, m) = zone.get(1..)?.split_once(':')?;
            sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60)
        }
    };
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// **The weekly line**, from the text of the forge's task list. The two weeks end
/// at the newest record in the list, so the same list gives the same line on any
/// day. A list the line cannot be read from is an error and not an empty week.
pub fn weekly_line(text: &str) -> Result<String, String> {
    let list: Value = serde_json::from_str(text).map_err(|e| format!("the task list: {e}"))?;
    let records = list
        .get("workflow_runs")
        .and_then(Value::as_array)
        .ok_or("the task list holds no workflow_runs")?;
    let mut held = Vec::new();
    for record in records {
        let status = record.get("status").and_then(Value::as_str).unwrap_or("");
        if !HELD.contains(&status) {
            continue;
        }
        let time = |key: &str| {
            record
                .get(key)
                .and_then(Value::as_str)
                .and_then(epoch)
                .ok_or_else(|| format!("a task record has no readable {key}"))
        };
        let (start, end) = (time("run_started_at")?, time("updated_at")?);
        let run = record
            .get("run_number")
            .and_then(Value::as_i64)
            .ok_or("a task record has no run_number")?;
        let pull_request = record.get("event").and_then(Value::as_str) == Some("pull_request");
        held.push((end, run, pull_request, (end - start).max(0)));
    }
    let Some(newest) = held.iter().map(|(end, ..)| *end).max() else {
        return Ok("ci runner occupancy: none in the task list".into());
    };
    let (mut this, mut last) = (Window::default(), Window::default());
    for (end, run, pull_request, seconds) in held {
        if end > newest - WEEK {
            this.add(run, pull_request, seconds);
        } else if end > newest - 2 * WEEK {
            last.add(run, pull_request, seconds);
        }
    }
    Ok(format!(
        "ci runner occupancy: this week {}; last week {}; summed job time from each job's start \
         to its last change, retries and cancellations counted, not wall time; the weeks end at \
         the newest record",
        this.describe(),
        last.describe(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_to_the_same_second_in_any_zone() {
        let utc = epoch("2026-10-28T08:06:38Z").unwrap();
        assert_eq!(epoch("2026-10-28T04:06:38-04:00"), Some(utc));
        assert_eq!(epoch("2026-10-28T09:36:38.250+01:30"), Some(utc));
        assert_eq!(epoch("2026-10-28"), None);
        assert_eq!(epoch("2026-10-28T04:06:38"), None);
    }

    #[test]
    fn a_job_is_its_own_gap_and_a_skipped_one_holds_nothing() {
        let list = r#"{"workflow_runs":[
          {"status":"success","run_number":1,"event":"push",
           "run_started_at":"2026-10-28T10:00:00Z","updated_at":"2026-10-28T10:01:40Z"},
          {"status":"skipped","run_number":1,"event":"push",
           "run_started_at":"2026-10-28T10:00:00Z","updated_at":"2026-10-28T10:50:00Z"}]}"#;
        let line = weekly_line(list).unwrap();
        assert!(
            line.contains("this week 100 s over 1 jobs in 1 run"),
            "{line}"
        );
    }

    #[test]
    fn a_list_the_line_cannot_be_read_from_is_refused() {
        assert!(weekly_line("not json").is_err());
        assert!(weekly_line("{}").is_err());
        let unreadable = r#"{"workflow_runs":[{"status":"success","run_number":1}]}"#;
        assert!(weekly_line(unreadable).is_err());
    }
}
