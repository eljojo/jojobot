//! `jojobot-exercise` — drive the shipped agent through a playbook against a
//! room built from nothing, and say what held.
//!
//! **This costs money.** It is not reachable from `cargo test` and `make check`
//! does not run it: the machinery below it is a library with free tests, and
//! this binary is what somebody invokes on purpose.
//!
//!     jojobot-exercise --playbook <path> [--model <name>]
//!
//! It exits non-zero when an expectation does not hold, because it is a test.
//! **Auth is the agent CLI's own** — a person is already logged in to it, and
//! nothing here reads, checks or holds a key.

use anyhow::{Context, Result};

use jojobot_exercise::agent::{Agent, DEFAULT_MODEL};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::run;

#[tokio::main]
async fn main() -> Result<()> {
    let asked = arguments()?;
    let playbook = Playbook::read(std::path::Path::new(&asked.playbook))?;
    let agent = Agent::new(&asked.model);
    let expectations = expectations_for(&playbook)?;
    let seed = jojobot_exercise::expectations::seed_for(&playbook.source)?;

    let results = run::go(&playbook, &agent, &seed, &expectations).await?;
    results.print(Some(&jojobot_exercise::calls::beside(&asked.transcript)));
    // **The run is kept, and where it went is said.** A paid run's most
    // valuable output is the part no expectation touches — what the model
    // reached for, what it did not find, what it concluded — and stdout is
    // where that stopped existing. **Written before the exit below**, so a run
    // that failed its expectations is the one most worth reading and is not
    // the one thrown away.
    match results.write_to(&asked.transcript) {
        Ok(()) => println!("\ntranscript: {}", asked.transcript.display()),
        // Not fatal, and loud. The run happened and was billed; losing the
        // file is worth saying and is not worth pretending the run did not
        // hold.
        Err(e) => eprintln!(
            "\nTRANSCRIPT NOT WRITTEN to {}: {e}. The run above is on stdout and nowhere else.",
            asked.transcript.display(),
        ),
    }
    if !results.held() {
        std::process::exit(1);
    }
    Ok(())
}

struct Asked {
    playbook: String,
    model: String,
    /// Where the run is kept for somebody to read afterwards.
    transcript: std::path::PathBuf,
}

/// **The model is a parameter, never a constant** — it is the operator's call,
/// for cost.
fn arguments() -> Result<Asked> {
    let mut playbook = None;
    let mut model = DEFAULT_MODEL.to_string();
    let mut transcript = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--playbook" => playbook = args.next(),
            "--model" => model = args.next().context("--model needs a name")?,
            "--transcript" => {
                transcript = Some(std::path::PathBuf::from(
                    args.next().context("--transcript needs a path")?,
                ))
            }
            other => anyhow::bail!(
                "unknown argument {other:?} — see --playbook, --model and --transcript"
            ),
        }
    }
    let playbook = playbook.context("--playbook is required: this crate authors none")?;
    Ok(Asked {
        transcript: transcript.unwrap_or_else(|| run::kept_beside(&playbook)),
        playbook,
        model,
    })
}

/// **The expectations for a playbook, and a loud nothing when there are none.**
///
/// They are Rust, keyed to the playbook by name: an assertion over store state
/// reads the surface, branches and reports, and every attempt to say that in
/// English ends as a small language somebody has to learn. The playbook stays
/// prose and grows no syntax.
///
/// A run with no expectations is a bill with no answer at the end of it, so an
/// unknown playbook stops here — before a room is built and before anything is
/// billed — rather than running and reporting a pass over an empty list.
fn expectations_for(playbook: &Playbook) -> Result<Vec<Box<dyn run::Expectation>>> {
    jojobot_exercise::expectations::for_playbook(&playbook.source).ok_or_else(|| {
        anyhow::anyhow!(
            "no expectations are registered for {} — this crate ships the machinery and authors \
             no playbook, so what must be true of the room after one runs is written beside it, \
             here, in Rust. Nothing was started and nothing was billed.",
            playbook.source,
        )
    })
}
