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

/// **The day this sitting happens on**, written beside the session marker.
///
/// A run acting out a year is fiction inside the test: nothing in jojobot
/// learns about elapsed time, and there is no clock to fool. **The surface
/// takes the day from the caller and stamps real today only when a caller
/// sends none**, so the fiction holds exactly as far as each sitting carries
/// its own date into the calls it makes.
///
/// It is written where the session marker is because it is the same kind of
/// fact about the sitting — what a maintainer would put there anyway.
const DAY: &str = "**Day:";

/// **The marker that says a phase is one a person has to read.**
///
/// Some things a demanding run measures cannot be locked, and that is not a
/// gap: a run asserts over what the model LEFT IN THE STORE and never over what
/// it said, because an assertion over words is a text match against a model's
/// phrasing and it would weaken every result the run ever produced.
///
/// **A question about something nobody ever recorded is the clearest case.**
/// The right answer is that jojobot does not know, and a confident invention
/// that records nothing leaves an identical store.
///
/// So the format owes that phase not a lock but PROMINENCE — the run puts it
/// where a reader lands rather than leaving it in the middle of a long
/// transcript.
const READ_THIS: &str = "**Read this.**";

/// **A line that ends one delivery and starts the next**, inside a phase.
///
/// A phase used to arrive as one message, so a step asking what the agent
/// EXPECTS was read alongside the step that says what happens — the answer
/// printed under its own question. The phases that turn on a prediction are
/// exactly the ones that measure what the surface let somebody believe, and
/// they could not record a wrong one.
///
/// **An explicit marker rather than prose the parser sniffs for.** Which step
/// must be answered before the next is read is the author's decision, and a
/// reader of the document can see where the breaks are.
const BREAK: &str = "---- answer before reading on ----";

/// One beat of a run: what to say, and whether to say it to a session that
/// remembers the phase before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    /// What this phase is called — the heading, whole, so a report and the
    /// document name a phase the same way.
    pub name: String,
    /// The block addressed to the model, with the quoting taken off — every
    /// delivery of it, joined, which is what a report shows and what the steps
    /// are counted off.
    pub prompt: String,
    /// **What the model is told, in order, one message each.**
    ///
    /// Usually one. A phase that asks for a prediction before it reveals the
    /// answer is delivered in two or more, and the model answers each before
    /// the next arrives — see [`BREAK`].
    pub deliveries: Vec<String>,
    /// Whether this phase starts a session of its own.
    pub fresh_session: bool,
    /// **Whether a person has to read this phase.** What it measures lives in
    /// the answer rather than in the room, so nothing asserts over it and the
    /// run surfaces it instead.
    pub read_this: bool,
    /// **The day this sitting claims**, when the document says one.
    ///
    /// `None` is a sitting that names no day, which is every room written
    /// before the year existed — and those are stamped with real today by the
    /// surface, exactly as they always were.
    pub day: Option<String>,
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
                        deliveries: Vec::new(),
                        fresh_session: false,
                        read_this: false,
                        day: None,
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
                // The two markers sit on one line as often as not, so the day
                // is looked for here as well as on a line of its own.
                current.day = current.day.take().or_else(|| day_in(said));
                current.read_this |= said.contains(READ_THIS);
                continue;
            }
            if let Some(said) = line.trim_start().strip_prefix(DAY) {
                current.day = day_in(said);
                continue;
            }
            if line.contains(READ_THIS) {
                current.read_this = true;
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
            // **Split where the author put a break, and nowhere else.** A phase
            // with none is one delivery, which is every phase that reveals
            // nothing to a step above it.
            phase.deliveries = phase
                .prompt
                .split(BREAK)
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect();
            // The joined text is what a report shows and what the step count
            // reads, so the marker itself does not survive into either.
            phase.prompt = phase.deliveries.join("\n");
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

/// **The day a marker line names**, in the one shape a date is written here.
///
/// Read out rather than pattern-matched loosely: a line saying `**Day: soon.**`
/// names no day, and a sitting whose date could not be read must come back
/// `None` so the run says so rather than inventing one.
fn day_in(said: &str) -> Option<String> {
    said.split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .find(|part| part.len() == 10 && part.split('-').count() == 3 && part.starts_with("20"))
        .map(str::to_string)
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

    /// **A sitting claims the day it happens on, and one that names none says
    /// so.**
    ///
    /// The year is fiction inside the test and jojobot stamps real today when a
    /// caller sends no date, so a sitting that cannot state its day is a
    /// sitting whose fiction silently collapses.
    ///
    /// **Both halves in one case**: the day is read off the line, and a phase
    /// that names no day comes back with none rather than with something
    /// invented. Without the second, a reader that returned a date for
    /// everything would pass.
    #[test]
    fn a_phase_carries_the_day_it_claims_and_none_when_it_names_none() {
        let read = Playbook::parse(
            "trivial",
            "## Phase 1 — the opening\n\
             **Session: fresh.** **Day: 2026-03-04.**\n\
             \n\
             > what the model is told\n\
             \n\
             ## Phase 2 — no day at all\n\
             **Session: fresh.**\n\
             \n\
             > and this one\n",
        )
        .expect("the playbook reads");
        assert_eq!(read.phases[0].day.as_deref(), Some("2026-03-04"));
        assert_eq!(
            read.phases[1].day, None,
            "a sitting that names no day invents none",
        );
    }

    /// **A day nobody can read comes back as none.**
    ///
    /// ⚠️ **The number matters.** A first version of this used `**Day: soon.**`
    /// and passed against a reader that accepted any token at all — because
    /// "soon" carries no digits, so a loose reader found nothing there either.
    /// **It was watching nothing.** A marker that carries a number which is not
    /// a date is what tells a real reader from a shrug.
    #[test]
    fn a_marker_carrying_something_that_is_not_a_date_carries_no_day() {
        for vague in ["**Day: soon.**", "**Day: 3.**", "**Day: 2026-3.**"] {
            let read = Playbook::parse(
                "trivial",
                &format!("## Phase 1 — vague\n**Session: fresh.** {vague}\n\n> told\n"),
            )
            .expect("the playbook reads");
            assert_eq!(
                read.phases[0].day, None,
                "{vague:?} names no day, so the sitting claims none",
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
