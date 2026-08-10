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
    results.print();
    if !results.held() {
        std::process::exit(1);
    }
    Ok(())
}

struct Asked {
    playbook: String,
    model: String,
}

/// **The model is a parameter, never a constant** — it is the operator's call,
/// for cost.
fn arguments() -> Result<Asked> {
    let mut playbook = None;
    let mut model = DEFAULT_MODEL.to_string();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--playbook" => playbook = args.next(),
            "--model" => model = args.next().context("--model needs a name")?,
            other => anyhow::bail!("unknown argument {other:?} — see --playbook and --model"),
        }
    }
    Ok(Asked {
        playbook: playbook.context("--playbook is required: this crate authors none")?,
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
