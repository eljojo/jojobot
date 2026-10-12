//! **Send a paid run's recorded calls again to this build, and say which
//! answers changed class.** No model runs and nothing is billed.
//!
//!     replay [--stream <path>] [--room <name>] [--candidate <label>]
//!            [--baselines <dir>] [--reports <dir>]
//!
//! With `--stream`, that file. With `--room` and no stream, the newest
//! `<room>-<stamp>.jsonl` in the baselines directory. With neither, the newest
//! stream of each room present there. The baselines directory is
//! `REPLAY_BASELINES`, else `jojobot-replay/baselines` under the state
//! directory; reports go to `REPLAY_REPORTS`, else `jojobot-replay/reports`
//! there. Neither is under the repository: both quote answer bodies from a run.
//!
//! The candidate is the server `JOJOBOT_BIN` names, else the one built beside
//! this binary. **It reports and exits zero whatever differs**: an intended fix
//! changes a class too. It exits non-zero only when a replay could not run.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use jojobot_exercise::room::server_binary;
use jojobot_exercise::{expectations, replay};

#[derive(Default)]
struct Asked {
    stream: Option<PathBuf>,
    room: Option<String>,
    candidate: Option<String>,
    baselines: Option<PathBuf>,
    reports: Option<PathBuf>,
}

fn arguments(mut args: impl Iterator<Item = String>) -> Result<Asked> {
    let mut asked = Asked::default();
    while let Some(argument) = args.next() {
        let mut value = |name: &str| args.next().with_context(|| format!("{name} needs a value"));
        match argument.as_str() {
            "--stream" => asked.stream = Some(PathBuf::from(value("--stream")?)),
            "--room" => asked.room = Some(value("--room")?),
            "--candidate" => asked.candidate = Some(value("--candidate")?),
            "--baselines" => asked.baselines = Some(PathBuf::from(value("--baselines")?)),
            "--reports" => asked.reports = Some(PathBuf::from(value("--reports")?)),
            other => anyhow::bail!(
                "unknown argument {other:?} — see --stream, --room, --candidate, --baselines and --reports"
            ),
        }
    }
    Ok(asked)
}

/// A directory named by the environment, else one under the replay's own state
/// directory.
fn directory(variable: &str, beneath: &str) -> Result<PathBuf> {
    if let Some(named) = std::env::var_os(variable).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(named));
    }
    let root = replay::state_root_from(
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
    .with_context(|| format!("neither {variable} nor a home directory is set"))?;
    Ok(root.join(beneath))
}

/// The short head of the checkout this runs in, for a report's name.
fn short_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|head| !head.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let asked = arguments(std::env::args().skip(1))?;
    let baselines = match &asked.baselines {
        Some(dir) => dir.clone(),
        None => directory("REPLAY_BASELINES", "baselines")?,
    };
    let reports = match &asked.reports {
        Some(dir) => dir.clone(),
        None => directory("REPLAY_REPORTS", "reports")?,
    };
    let candidate = asked
        .candidate
        .clone()
        .or_else(|| std::env::var("CANDIDATE").ok().filter(|c| !c.is_empty()))
        .unwrap_or_else(short_head);

    let streams: Vec<PathBuf> = match (&asked.stream, &asked.room) {
        (Some(stream), _) => vec![stream.clone()],
        (None, Some(room)) => {
            vec![replay::newest_of(&baselines, room)?.with_context(|| {
                format!("{} holds no stream of room {room}", baselines.display())
            })?]
        }
        (None, None) => replay::newest_per_room(&baselines)?,
    };
    anyhow::ensure!(
        !streams.is_empty(),
        "{} holds no stream to replay",
        baselines.display()
    );
    for stream in streams {
        replay_one(&stream, asked.room.as_deref(), &candidate, &reports).await?;
    }
    Ok(())
}

async fn replay_one(
    stream: &Path,
    room_named: Option<&str>,
    candidate: &str,
    reports: &Path,
) -> Result<()> {
    let room_name = match room_named {
        Some(room) => room.to_string(),
        None => replay::room_of(stream).with_context(|| {
            format!(
                "{} is not named <room>-<stamp>.jsonl, so its room is not known — name it with --room",
                stream.display()
            )
        })?,
    };
    let document = format!("rooms/{room_name}.md");
    anyhow::ensure!(
        expectations::shipped_rooms().any(|shipped| shipped == document),
        "no room document {document} is shipped — the rooms are: {}",
        expectations::shipped_rooms().collect::<Vec<_>>().join(", ")
    );
    let text = std::fs::read_to_string(stream)
        .with_context(|| format!("reading the stream {}", stream.display()))?;
    let sittings = replay::read_stream(&text);
    anyhow::ensure!(
        sittings.iter().any(|sitting| !sitting.calls.is_empty()),
        "{} holds no jojobot call to replay",
        stream.display()
    );

    // The same three steps a paid run takes before the model arrives: a room on
    // the shipped server, furnished from the room's own document.
    let seed = expectations::seed_for(&document)?;
    // The server acts out the day the replayed run was made on, never the wall
    // clock; a log that records no day is refused here, naming the run.
    let (room, surface, day) = replay::open_for_run(&server_binary()?, &text, stream).await?;
    // **Read back from the server, not trusted from the request**: the report
    // names the day the server says it acts out.
    let reported = surface.stated_day().await;
    anyhow::ensure!(
        reported.as_deref() == Some(day.day.as_str()),
        "the replay of {} asked its server to act out {}, and the server reports {}",
        stream.display(),
        day.day,
        reported
            .as_deref()
            .unwrap_or("no stated day (the wall clock)"),
    );
    seed.furnish(&surface)
        .await
        .context("furnishing the room")?;
    let results = replay::replay(&sittings, &surface).await;
    surface.finish().await;
    drop(room);

    let stem = stream
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("stream");
    let report = replay::render(
        &room_name,
        &format!("{stem}.jsonl"),
        candidate,
        &day,
        &replay::gaps(&text, &sittings, &results),
        &results,
    );
    print!("{report}");
    std::fs::create_dir_all(reports).with_context(|| format!("making {}", reports.display()))?;
    let kept = reports.join(format!("{stem}-{candidate}.txt"));
    std::fs::write(&kept, &report).with_context(|| format!("writing {}", kept.display()))?;
    println!("\nreport kept: {}", kept.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strings: &[&str]) -> impl Iterator<Item = String> {
        strings
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn every_argument_is_read_and_an_unknown_one_is_refused() {
        let asked = arguments(args(&[
            "--stream",
            "a/vault-1.jsonl",
            "--room",
            "vault",
            "--candidate",
            "abc",
            "--baselines",
            "b",
            "--reports",
            "r",
        ]))
        .expect("reads");
        assert_eq!(asked.stream, Some(PathBuf::from("a/vault-1.jsonl")));
        assert_eq!(asked.room.as_deref(), Some("vault"));
        assert_eq!(asked.candidate.as_deref(), Some("abc"));
        assert_eq!(asked.baselines, Some(PathBuf::from("b")));
        assert_eq!(asked.reports, Some(PathBuf::from("r")));
        assert!(arguments(args(&["--nope"])).is_err());
        assert!(arguments(args(&["--stream"])).is_err());
    }
}
