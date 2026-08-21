//! **One run, end to end** — furnish a room, drive the shipped agent through a
//! playbook, read the room back, and say what held.
//!
//! What this module does NOT do is judge. There is no rubric, no score and no
//! verdict on whether jojobot did well: it reports which expectations held and
//! prints what the agent said, and the reading is the operator's.

use anyhow::{Context, Result};
use serde_json::json;

use crate::agent::{Agent, Conversation};
use crate::playbook::Playbook;
use crate::room::Room;
use crate::surface::{Seed, Surface};

/// What one expectation had to say about the room afterwards.
pub struct Outcome {
    pub name: String,
    pub held: bool,
    /// What was actually found — the sentence a reader needs when it did not
    /// hold, and a receipt when it did.
    pub saying: String,
}

/// **A claim about the room's state after the run.**
///
/// Over state, never over prose: a real run is non-deterministic, and an
/// assertion on wording is either too tight to pass or too loose to mean
/// anything. What a phase leaves behind — an entity of the right kind, a fact
/// on the most specific subject, an edge, a provenance that defaulted the
/// cautious way — is stable, and it is what a later session would have to find.
///
/// The test for whether one is worth writing: does it measure a **visible
/// external side effect**? Not that the agent said the right thing — that it
/// left something, and that the something is observable through the surface.
#[async_trait::async_trait]
pub trait Expectation: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self, seen: &Observed<'_>) -> Outcome;
}

/// **What one phase left, read at the boundary either side of it.**
///
/// Some claims are about the end state and some are about a change: the phase
/// whose whole point is that polling a box moves nothing cannot be checked
/// against the finished room, because later phases post and process mail on
/// purpose. So a run records what it can see at each boundary, and an
/// expectation asks across the one it cares about.
pub struct Boundary {
    /// The phase this was taken before.
    pub before: String,
    /// Every mailbox as the board reports it, with nothing taken.
    pub mail: String,
    /// The inventory and everything the index can see.
    pub world: String,
    /// How many runs the door offers this identity here.
    pub runs_offered: usize,
    /// **What the door offers this identity here.** A session's record is
    /// readable only on the boot path — there is no verb that reads one by id —
    /// so a claim about a run has to be taken while the run is still offered.
    /// A boot that resumes nothing writes nothing, so taking this costs the
    /// room no state.
    pub board: String,
}

/// The room after the run, and what it looked like at each phase boundary.
pub struct Observed<'a> {
    pub room: &'a Surface,
    pub boundaries: &'a [Boundary],
}

impl Observed<'_> {
    /// The readings either side of the phase whose name begins with `phase`,
    /// or nothing when the run has no such boundary — which is a failure to
    /// report rather than one to swallow.
    pub fn across(&self, phase: &str) -> Option<(&Boundary, &Boundary)> {
        let at = self
            .boundaries
            .iter()
            .position(|b| b.before.starts_with(phase))?;
        Some((self.boundaries.get(at)?, self.boundaries.get(at + 1)?))
    }
}

/// What one phase printed.
pub struct Said {
    pub phase: String,
    pub prompt: String,
    pub output: String,
    /// Whether the invocation itself came back clean.
    pub ran: bool,
    /// Whether this phase was told to carry the conversation before it on.
    pub continuing: bool,
}

/// Everything one run produced.
pub struct Results {
    pub playbook: String,
    pub model: String,
    pub outcomes: Vec<Outcome>,
    pub transcript: Vec<Said>,
    /// **Phases nobody wrote an expectation for.** Named in the results, so a
    /// phase with no check reads as uncovered and never as passed.
    pub uncovered: Vec<String>,
    /// What the door offered at each phase boundary, oldest first.
    pub boundaries: Vec<Boundary>,
    /// What the room held before the agent touched it, and after.
    pub before: String,
    pub after: String,
}

impl Results {
    /// **Did anything happen at all?**
    ///
    /// The guard that stands in front of every expectation. A room starts
    /// nearly empty, so "the wrong thing is not there" holds by default on a
    /// run where the agent did nothing — because it was confused, because the
    /// CLI was not there, because the playbook said nothing. Those runs must
    /// not be able to report a pass, and no individual expectation can notice
    /// them: each one is asking about its own claim, not about whether the run
    /// happened.
    ///
    /// It is asked of the ROOM rather than of the transcript, because a
    /// transcript is the agent's prose and this tier does not assert over
    /// prose. A run that left no visible external side effect left nothing to
    /// measure.
    pub fn the_room_changed(&self) -> bool {
        self.before != self.after
    }

    /// **Phases that were told to carry a conversation on and did not.**
    ///
    /// Two signals, both structural, neither a reading of the agent's prose.
    ///
    /// The first is the invocation itself: a CLI asked to resume a conversation
    /// it cannot find comes back non-zero, and that is free to see.
    ///
    /// The second is the board. A phase that continues already holds a session
    /// and journals into it; one that lost its memory boots again, and a run it
    /// had no reason to start appears. **This is the signal that catches a
    /// SILENT degrade** — a resume that quietly became a fresh conversation
    /// produces a transcript a reader would pass, and phases 9 to 11 exist to
    /// measure exactly the continuity it destroyed.
    pub fn lost_continuity(&self) -> Vec<String> {
        let mut lost = Vec::new();
        for (at, said) in self.transcript.iter().enumerate() {
            if !said.continuing {
                continue;
            }
            if !said.ran {
                lost.push(format!("{} — the agent could not carry it on", said.phase));
                continue;
            }
            let (Some(before), Some(after)) =
                (self.boundaries.get(at), self.boundaries.get(at + 1))
            else {
                continue;
            };
            if after.runs_offered > before.runs_offered {
                lost.push(format!(
                    "{} — it was told to continue and started a run of its own instead ({} then \
                     {})",
                    said.phase, before.runs_offered, after.runs_offered,
                ));
            }
        }
        lost
    }

    /// **Steps the agent was asked and did not answer.**
    ///
    /// A phase is a numbered list of steps and each one asks for something
    /// reported. A run that answers step 53 and writes "48–53: PASS" has
    /// reported a verdict on five steps nobody took, and the transcript cannot
    /// be read to settle what happened in them.
    ///
    /// **Structural, not a reading of the prose.** It asks whether the answer
    /// names the step at all — the number the playbook itself uses — and never
    /// what the answer says about it. A rephrasing passes; a silence does not.
    ///
    /// **A harness failure rather than a product verdict.** Nothing here says
    /// jojobot behaved badly: it says the run did not measure what it was sent
    /// to measure, which is the more expensive failure of the two because it
    /// reads as a pass.
    pub fn steps_unanswered(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for said in &self.transcript {
            if !said.ran {
                continue;
            }
            let unanswered: Vec<String> = steps_of(&said.prompt)
                .into_iter()
                .filter(|step| !names_step(&said.output, step))
                .collect();
            if !unanswered.is_empty() {
                missing.push(format!(
                    "{} — asked {} steps and answered none of {}",
                    said.phase,
                    steps_of(&said.prompt).len(),
                    unanswered.join(", "),
                ));
            }
        }
        missing
    }

    /// Everything held, something happened, and no phase quietly lost its
    /// memory on the way.
    pub fn held(&self) -> bool {
        self.the_room_changed()
            && self.lost_continuity().is_empty()
            && self.steps_unanswered().is_empty()
            && !self.outcomes.is_empty()
            && self.outcomes.iter().all(|outcome| outcome.held)
    }

    /// **The transcript first, then the results.** When something did not hold
    /// the next question is always what the agent reached for, so it is shown
    /// rather than kept for a rerun that costs money again.
    pub fn print(&self) {
        print!("{}", self.rendered());
    }

    /// **Keep the run where somebody can read it after the process is gone.**
    ///
    /// A paid run's most valuable output is the part no assertion touches —
    /// what the model reached for, what it did not find, what it concluded.
    /// That exists only here, and stdout is where it stopped existing.
    ///
    /// **The same rendering as [`print`](Self::print), so the two cannot
    /// drift**: a second renderer for the file is how the read copy comes to
    /// say something the printed one does not.
    ///
    /// **Whole, from the beginning.** A capture that keeps the end of a
    /// transcript has thrown away the part where the model was working out
    /// what it could do — which is the part being judged.
    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.rendered())
    }

    /// The whole run as text: transcript, then results.
    ///
    /// **One rendering, two destinations.** Read by [`print`](Self::print) for
    /// the person watching and by [`write_to`](Self::write_to) for the person
    /// reading afterwards.
    pub fn rendered(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(out, "playbook: {}", self.playbook);
        let _ = writeln!(out, "model:    {}", self.model);
        let _ = writeln!(
            out,
            "\n── transcript ──────────────────────────────────────────────"
        );
        // **An empty transcript says so.** Otherwise a run that produced
        // nothing and a capture that never ran read the same — an empty
        // section and an empty file, and nobody can tell which they are
        // holding.
        if self.transcript.is_empty() {
            let _ = writeln!(
                out,
                "\nTHE RUN PRODUCED NO TRANSCRIPT: no phase produced any output. This is the \
                 run's own answer, not a capture that failed — a file that stops here was \
                 written by a run that had nothing to say."
            );
        }
        for said in &self.transcript {
            let _ = writeln!(
                out,
                "\n▸ {}\n  said: {}\n{}",
                said.phase, said.prompt, said.output
            );
        }
        // **What the door offered, phase by phase.** Collected since this
        // harness existed and rendered nowhere, so the one thing a reader
        // wants from a long run — is jojobot handing over MORE as the story
        // accumulates, or less — was thrown away with the process.
        //
        // Rendered whole rather than diffed: a diff is a summary, and what to
        // make of the change is the reader's.
        if !self.boundaries.is_empty() {
            let _ = writeln!(
                out,
                "\n── what the door offered, phase by phase ───────────────────"
            );
            for boundary in &self.boundaries {
                let _ = writeln!(
                    out,
                    "\n▸ before {}\n  runs offered: {}\n  world: {}\n  mail:  {}\n  door:  {}",
                    boundary.before,
                    boundary.runs_offered,
                    boundary.world,
                    boundary.mail,
                    boundary.board,
                );
            }
        }
        let _ = writeln!(
            out,
            "\n── results ─────────────────────────────────────────────────"
        );
        if !self.the_room_changed() {
            let _ = writeln!(
                out,
                "\nFAILED: the room is exactly as it was furnished, so the agent left no visible \
                 side effect and nothing below means anything — an unchanged room satisfies every \
                 negative assertion by itself."
            );
        }
        for outcome in &self.outcomes {
            let mark = if outcome.held { "held" } else { "FAILED" };
            let _ = writeln!(out, "  [{mark}] {} — {}", outcome.name, outcome.saying);
        }
        for lost in self.lost_continuity() {
            let _ = writeln!(
                out,
                "  [ FAILED ] {lost}. A phase that lost its memory still produces a readable \
                 transcript, and the phases that measure continuity are meaningless without it."
            );
        }
        for unanswered in self.steps_unanswered() {
            let _ = writeln!(
                out,
                "  [ HARNESS ] {unanswered}. The run did not measure what it was sent to \
                 measure, which reads as a pass unless it is said here."
            );
        }
        for phase in &self.uncovered {
            let _ = writeln!(
                out,
                "  [ no check ] {phase} — what this phase measures lives in the agent's answer, \
                 so a person reads it above. It is NOT passed."
            );
        }
        out
    }
}

/// **Where a run goes when nobody said.**
///
/// Named for the playbook it ran and the moment it ran, under one directory, so
/// runs accumulate rather than overwrite each other — a run's history is the
/// point of keeping them, and a fixed name would leave only the last one.
pub fn kept_beside(playbook: &str) -> std::path::PathBuf {
    let stem = std::path::Path::new(playbook)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "run".to_string());
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::path::PathBuf::from("transcripts").join(format!("{stem}-{at}.md"))
}

/// **The step numbers a phase asks about**, read off the playbook's own list.
///
/// A step is a line whose first token is a number and a dot, which is the shape
/// the document writes them in. Nothing here interprets what the step asks.
fn steps_of(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (number, rest) = line.split_once('.')?;
            (!number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
                && rest.starts_with(' '))
            .then(|| number.to_string())
        })
        .collect()
}

/// **Does this answer name that step?** The number, as a number rather than as
/// part of a longer one — `5` must not be satisfied by `53`.
///
/// A range counts: an answer that writes `48-53` has named both ends, and the
/// steps between them are what this cannot see. That is deliberate — the check
/// is for a step nobody mentioned at all, and a run that summarised a range is
/// caught by the ends it did not write rather than by parsing its arithmetic.
fn names_step(output: &str, step: &str) -> bool {
    let mut from = 0;
    while let Some(at) = output[from..].find(step) {
        let at = from + at;
        let before = output[..at].chars().next_back();
        let after = output[at + step.len()..].chars().next();
        let bounded = !before.is_some_and(|c| c.is_ascii_digit())
            && !after.is_some_and(|c| c.is_ascii_digit());
        if bounded {
            return true;
        }
        from = at + step.len();
    }
    false
}

/// **The whole run.**
pub async fn go(
    playbook: &Playbook,
    agent: &Agent,
    seed: &Seed,
    expectations: &[Box<dyn Expectation>],
) -> Result<Results> {
    anyhow::ensure!(
        !expectations.is_empty(),
        "no expectations are registered for {} — a run that asserts nothing is a bill with no \
         answer at the end of it",
        playbook.source,
    );

    let room = Room::open(&crate::room::server_binary()?).await?;
    let surface = Surface::connect(room.endpoint()).await?;
    seed.furnish(&surface)
        .await
        .context("furnishing the room")?;
    let mut boundaries = vec![boundary(&surface, "Phase 1").await];
    let before = boundaries[0].world.clone();

    let mut transcript = Vec::new();
    let mut conversation = Conversation::fresh();
    for (at, phase) in playbook.phases.iter().enumerate() {
        // **The playbook decides how many sessions a run uses.** The first
        // phase starts one; after that, a phase carries the last conversation
        // on unless it says it must meet jojobot with no memory of it.
        conversation = if at == 0 || phase.fresh_session {
            Conversation::fresh()
        } else {
            conversation.carried()
        };
        // **Delivered as the phase says, one message at a time.** A phase that
        // asks what the agent EXPECTS before it says what happens is split
        // there, so the answer is given before the reveal arrives — those are
        // the steps that measure what the surface let somebody believe, and
        // arriving together made a wrong prediction impossible to record.
        let mut output = String::new();
        let mut ran = true;
        for delivery in &phase.deliveries {
            let worked = agent
                .work(room.endpoint(), &conversation, delivery)
                .await
                .with_context(|| format!("driving the phase {:?}", phase.name))?;
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&worked.output);
            ran &= worked.ran;
            // Every delivery after the first carries the one before it on, or
            // the answer to the prediction is not in the room when the reveal
            // lands.
            conversation = conversation.carried();
        }
        transcript.push(Said {
            phase: phase.name.clone(),
            prompt: phase.prompt.clone(),
            output,
            ran,
            continuing: !(at == 0 || phase.fresh_session),
        });
        // Taken after every phase and named for the one that comes next, so a
        // claim about a change has both sides of its boundary.
        let next = playbook
            .phases
            .get(at + 1)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "the end".to_string());
        boundaries.push(boundary(&surface, &next).await);
    }

    let after = boundaries
        .last()
        .map(|b| b.world.clone())
        .unwrap_or_default();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let mut outcomes = Vec::new();
    for expectation in expectations {
        outcomes.push(expectation.check(&seen).await);
    }

    // **A phase with no expectation is named, not omitted.** An expectation is
    // keyed to a phase by the `Phase N` its name opens with, so a phase no
    // check mentions is one nobody is asserting anything about.
    let named: Vec<&str> = expectations.iter().map(|e| e.name()).collect();
    let uncovered = playbook
        .phases
        .iter()
        .filter(|phase| !phase_is_covered(&phase.name, &named))
        .map(|phase| phase.name.clone())
        .collect();

    let results = Results {
        playbook: playbook.source.clone(),
        model: agent.model().to_string(),
        outcomes,
        transcript,
        uncovered,
        boundaries,
        before,
        after,
    };
    surface.finish().await;
    Ok(results)
}

/// Whether any of `expectations` is keyed to this phase.
///
/// **Key against key, never a prefix of one.** The keys are numbered and the
/// numbers run past nine, so `Phase 1` is the front of `Phase 10`: a match on
/// the front of the name lends the later phase's check to the earlier one, and
/// the earlier phase drops out of the uncovered list while nothing asserts
/// anything about it.
fn phase_is_covered(phase: &str, expectations: &[&str]) -> bool {
    let key = phase_key(phase);
    expectations.iter().any(|name| phase_key(name) == key)
}

/// The `Phase N` a phase or an expectation is keyed by — everything before the
/// dash that separates the number from the title.
fn phase_key(name: &str) -> &str {
    name.split('\u{2014}').next().unwrap_or("").trim()
}

/// **What the room holds, read through the served surface.**
///
/// Deliberately coarse: the world half is not an assertion, it is the evidence
/// that a run happened at all. Two reads a session would make — the inventory,
/// and everything the index can see — so a run that wrote a fact but no entity
/// still moves it. The mail half is read apart because the claims about mail
/// are claims about what did NOT move.
async fn boundary(room: &Surface, before: &str) -> Boundary {
    let entities = room.call("list_entities", json!({})).await;
    let everything = room
        .call("search", json!({"query": "*", "limit": 200}))
        .await;
    let mail = room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let board = room
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let runs_offered = serde_json::from_str::<serde_json::Value>(&board)
        .ok()
        .and_then(|b| b["session"]["choices"].as_array().map(Vec::len))
        .unwrap_or(0);
    Boundary {
        runs_offered,
        before: before.to_string(),
        mail,
        world: format!("{entities}\n{everything}"),
        board,
    }
}

/// **What a room holds before anybody drives anything.**
///
/// Its own function because it is asserted on its own, without an agent: the
/// claim that a run meets the instance as it ships is one a free test can hold,
/// and it is the claim a future session would break by seeding a charter to
/// make a failing run go green.
pub async fn starting_identity(room: &Surface) -> Result<serde_json::Value> {
    room.must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
}

#[cfg(test)]
mod tests {
    use super::{Results, Said, phase_is_covered};

    /// A run with two phases and nothing else — the smallest thing that has a
    /// beginning and an end.
    fn ran(phases: &[&str]) -> Results {
        Results {
            playbook: "rooms/whatever.md".into(),
            model: "some-model".into(),
            outcomes: Vec::new(),
            transcript: phases
                .iter()
                .map(|phase| Said {
                    phase: (*phase).to_string(),
                    prompt: format!("what {phase} was asked"),
                    output: format!("what {phase} answered"),
                    ran: true,
                    continuing: false,
                })
                .collect(),
            uncovered: Vec::new(),
            boundaries: Vec::new(),
            before: String::new(),
            after: String::new(),
        }
    }

    /// **The run outlives the process, whole and from the beginning.**
    ///
    /// The valuable half of a paid run is the part no assertion touches — what
    /// the model reached for and what it concluded — and it existed only on
    /// stdout.
    ///
    /// **The FIRST phase is asserted as hard as the last.** A capture that
    /// keeps the end of a transcript looks right at a glance and has thrown
    /// away the part where the model was working out what it could do.
    #[test]
    fn a_run_is_readable_in_full_after_the_process_is_gone() {
        let dir = std::env::temp_dir().join(format!("exercise-transcript-{}", std::process::id()));
        let path = dir.join("run.md");
        ran(&["Phase 1 — the opening", "Phase 2 — the close"])
            .write_to(&path)
            .expect("the transcript is written");

        let read = std::fs::read_to_string(&path).expect("and reads back");
        assert!(
            read.contains("Phase 1 — the opening")
                && read.contains("what Phase 1 — the opening answered"),
            "the beginning of the run is in the file: {read}",
        );
        assert!(
            read.contains("Phase 2 — the close"),
            "…and so is the end: {read}",
        );
        assert!(
            read.contains("some-model"),
            "…and which model produced it, because a run nobody can attribute is not evidence",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A run that produced nothing says so, rather than leaving an empty
    /// file.**
    ///
    /// The failure this exists for: an empty capture and a capture that never
    /// ran are the same bytes, and a person holding one cannot tell which it
    /// is. **Both halves in one case** — the run's own nothing is stated, and
    /// the file is not empty, which is what makes the two distinguishable.
    #[test]
    fn a_run_that_produced_nothing_says_so_and_an_empty_file_means_the_capture_failed() {
        let empty = ran(&[]);
        let rendered = empty.rendered();
        assert!(
            rendered.contains("THE RUN PRODUCED NO TRANSCRIPT"),
            "a run with nothing to say says that in its own words: {rendered}",
        );
        assert!(
            !rendered.trim().is_empty(),
            "…so an empty file can only mean the capture never wrote, which is a different fault",
        );
    }

    /// **What the door offered at each boundary is in the run**, because that
    /// is how a long run shows jojobot handing over more context as the story
    /// accumulates, or less.
    ///
    /// It was collected and rendered nowhere, so it died with the process.
    #[test]
    fn what_the_door_offered_at_each_phase_is_in_the_kept_run() {
        let mut run = ran(&["Phase 1 — the opening"]);
        run.boundaries = vec![super::Boundary {
            before: "Phase 2 — the close".into(),
            world: "what the index could see".into(),
            mail: "what the board reported".into(),
            runs_offered: 2,
            board: "the runs the door offered".into(),
        }];
        let rendered = run.rendered();
        assert!(
            rendered.contains("Phase 2 — the close")
                && rendered.contains("what the index could see")
                && rendered.contains("what the board reported")
                && rendered.contains("the runs the door offered"),
            "the boundary is in the run a person reads: {rendered}",
        );
    }

    /// **A run with no boundaries renders no boundary section**, so an empty
    /// heading never reads as a door that offered nothing.
    #[test]
    fn a_run_with_no_boundaries_does_not_render_an_empty_section() {
        assert!(
            !ran(&["Phase 1 — the only one"])
                .rendered()
                .contains("what the door offered"),
            "an empty section would read as a door that offered nothing",
        );
    }

    /// **A run is named for the playbook it ran**, so a directory of them is
    /// readable without opening any.
    ///
    /// **And two runs of one playbook do not collide.** A fixed name leaves
    /// only the last run, which is the opposite of keeping them.
    #[test]
    fn a_kept_run_is_named_for_its_playbook_and_does_not_overwrite_the_last_one() {
        let path = super::kept_beside("crates/jojobot-exercise/rooms/ledger.md");
        let named = path.to_string_lossy().to_string();
        assert!(
            named.starts_with("transcripts/") && named.contains("ledger"),
            "a run is filed under one directory, named for its playbook: {named}",
        );
        assert_ne!(
            named, "transcripts/ledger.md",
            "…and carries what tells two runs of it apart",
        );
    }

    /// **What is written and what is shown are one rendering.**
    ///
    /// A second renderer for the file is how the copy somebody reads afterwards
    /// comes to say something the printed one did not.
    #[test]
    fn the_written_run_and_the_shown_run_are_the_same_text() {
        let run = ran(&["Phase 1 — the only one"]);
        let dir = std::env::temp_dir().join(format!("exercise-same-{}", std::process::id()));
        let path = dir.join("run.md");
        run.write_to(&path).expect("written");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read back"),
            run.rendered(),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A phase is covered by the check written for THAT phase.**
    ///
    /// The keys are numbered and the numbers run into double figures, so `Phase
    /// 1` is a prefix of `Phase 10`: a match on the front of the name hands
    /// phase 1 the check written for phase 10, and phase 1 then never reaches
    /// the uncovered list even though nothing asserts anything about it. Both
    /// directions in one case, because a predicate that answered `false` to
    /// everything would pass the half above on its own.
    #[test]
    fn a_longer_phase_number_does_not_cover_the_one_it_begins_with() {
        assert!(
            !phase_is_covered(
                "Phase 1 \u{2014} the door, cold",
                &["Phase 10 \u{2014} what an earlier run left was picked up"],
            ),
            "the phase 10 check was counted as covering phase 1",
        );
        assert!(
            phase_is_covered(
                "Phase 7 \u{2014} mail, without taking on work",
                &["Phase 7 \u{2014} polling a box moved nothing in it"],
            ),
            "a phase its own check names was reported as uncovered",
        );
    }
}
