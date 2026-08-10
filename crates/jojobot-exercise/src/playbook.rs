//! **The playbook is INPUT, and it is somebody's English.**
//!
//! A run is a playbook driven against a room. This crate authors none: it reads
//! one, and it reads as little of it as it can get away with, because the
//! moment a playbook has to be written to suit a parser it has stopped being
//! the thing a person can read and started being a fixture format.
//!
//! **The reader was written against the document, not the other way round.**
//! The three things a playbook hands over are the ordered phases, one block of
//! prose per phase addressed to the model, and a per-phase session marker. Each
//! of them is already how somebody writes the document for a human reader:
//!
//! * a phase is a `## Phase N — name` heading, so every other heading in the
//!   file is for whoever maintains the suite and is sent nowhere;
//! * the block addressed to the model is the block quote, which is how a
//!   document already marks words meant for somebody else;
//! * the marker is the sentence that says `**Session: fresh.**` or
//!   `**Session: continues …**`, which a maintainer would write regardless.
//!
//! Nothing else is syntax, and a playbook stays a markdown document. **How many
//! sessions a run uses is the playbook's business** — a phase that must meet
//! jojobot with no memory of the last one says so, and this decides nothing.

use anyhow::{Context, Result};

/// What a heading looks like when it starts a phase. Every other heading in the
/// document is prose for a maintainer.
const PHASE: &str = "## Phase ";

/// The line that carries the session marker, and the word that means a phase
/// starts from nothing. Anything else on that line — `continues phase 7` —
/// means it carries the last one on.
const MARKER: &str = "**Session:";
const FRESH: &str = "fresh";

/// One beat of a run: what to say, and whether to say it to a session that
/// remembers the phase before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    /// What this phase is called — the heading, whole, so a report and the
    /// document name a phase the same way.
    pub name: String,
    /// The block addressed to the model, with the quoting taken off.
    pub prompt: String,
    /// Whether this phase starts a session of its own.
    pub fresh_session: bool,
}

/// An ordered list of phases, and where they were read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playbook {
    pub source: String,
    pub phases: Vec<Phase>,
}

impl Playbook {
    /// Read one off disk.
    pub fn read(path: &std::path::Path) -> Result<Playbook> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the playbook at {}", path.display()))?;
        Playbook::parse(&path.display().to_string(), &text)
    }

    /// **The whole grammar.**
    pub fn parse(source: &str, text: &str) -> Result<Playbook> {
        let mut phases: Vec<Phase> = Vec::new();
        let mut inside = false;
        for line in text.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                // A heading that is not a phase ENDS the phase before it, so
                // the maintainer's prose after the last phase reaches nobody.
                inside = line.starts_with(PHASE);
                if inside {
                    phases.push(Phase {
                        name: heading.trim().to_string(),
                        prompt: String::new(),
                        fresh_session: false,
                    });
                }
                continue;
            }
            if !inside {
                continue;
            }
            let Some(current) = phases.last_mut() else {
                continue;
            };
            if let Some(said) = line.trim_start().strip_prefix(MARKER) {
                current.fresh_session = said.to_lowercase().contains(FRESH);
                continue;
            }
            // The block quote is the part addressed to the model. Everything
            // else under a phase heading is for whoever maintains the suite.
            if let Some(quoted) = quoted(line) {
                current.prompt.push_str(quoted);
                current.prompt.push('\n');
            }
        }
        for phase in &mut phases {
            phase.prompt = phase.prompt.trim().to_string();
        }
        anyhow::ensure!(
            !phases.is_empty(),
            "{source}: no phases — a phase is a heading beginning {PHASE:?}",
        );
        if let Some(silent) = phases.iter().find(|p| p.prompt.is_empty()) {
            anyhow::bail!(
                "{source}: the phase {:?} has no block addressed to the model",
                silent.name,
            );
        }
        Ok(Playbook {
            source: source.to_string(),
            phases,
        })
    }
}

/// The text of a quoted line, or nothing when the line is not quoted. A bare
/// `>` is a blank line inside the block rather than the end of it.
fn quoted(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The trivial fixture, and it is deliberately not a suite.** What a real
    /// playbook says is authored somewhere else; these cases are about the
    /// reading, so the words are nonsense on purpose. A fixture that looked
    /// like the real thing would invite somebody to keep the two in step, and
    /// there would be two playbooks.
    const TRIVIAL: &str = "\
# A document

Prose for whoever maintains this.

## What this file gives the harness

This heading is not a phase and none of it is said to anybody.

## Phase 1 — the first thing

**Session: fresh.** A note for a maintainer.

> Ask about the first thing.
>
> Ask about it in two paragraphs.

A note under the block, which is nobody's prompt.

## Phase 2 — the second thing

**Session: continues phase 1.**

> Ask about the second thing.

## The report

Prose after the last phase, for a maintainer.
";

    #[test]
    fn a_playbook_is_its_phases_in_order() {
        let read = Playbook::parse("trivial", TRIVIAL).expect("it parses");
        assert_eq!(
            read.phases
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Phase 1 — the first thing", "Phase 2 — the second thing"],
            "only the phase headings are phases, in the order they were written",
        );
    }

    /// **Only the block addressed to the model is sent.** Everything else under
    /// a phase heading is a maintainer's, and so is every heading that is not a
    /// phase — including the ones after the last phase, which is the case a
    /// reader that simply ran to the end of the file would get wrong.
    #[test]
    fn a_phase_sends_the_block_addressed_to_the_model_and_nothing_around_it() {
        let read = Playbook::parse("trivial", TRIVIAL).expect("it parses");
        assert_eq!(
            read.phases[0].prompt, "Ask about the first thing.\n\nAsk about it in two paragraphs.",
            "the quoting comes off and the blank line inside the block survives",
        );
        for phase in &read.phases {
            assert!(
                !phase.prompt.contains("maintainer") && !phase.prompt.contains("harness"),
                "a maintainer's prose reached the model in {:?}: {:?}",
                phase.name,
                phase.prompt,
            );
        }
    }

    /// The marker, both ways in the same case: a check that only ever saw the
    /// fresh phase would pass on a build where every phase starts fresh.
    #[test]
    fn a_phase_says_whether_it_starts_a_session_of_its_own() {
        let read = Playbook::parse("trivial", TRIVIAL).expect("it parses");
        assert!(
            read.phases[0].fresh_session,
            "a phase marked fresh starts its own session",
        );
        assert!(
            !read.phases[1].fresh_session,
            "a phase that continues one carries it on",
        );
    }

    /// A playbook that says nothing to the model is a mistake somebody wants
    /// told, not a run that costs money and asserts nothing.
    #[test]
    fn a_playbook_with_nothing_to_say_is_refused() {
        let no_phases = Playbook::parse("trivial", "## Not a phase\n\n> Words.\n");
        assert!(
            no_phases.is_err(),
            "a document with no phase was accepted: {no_phases:?}",
        );

        let silent = Playbook::parse("trivial", "## Phase 1 — quiet\n\n**Session: fresh.**\n");
        assert!(
            silent.is_err(),
            "a phase with no block for the model was accepted: {silent:?}",
        );

        // The positive both rest on: the same reader accepts a playbook that
        // does say something, so the refusals above are about the emptiness.
        assert!(Playbook::parse("trivial", TRIVIAL).is_ok());
    }
}
