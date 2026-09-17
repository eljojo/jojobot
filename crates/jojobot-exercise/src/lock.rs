//! **A lock: what must be true of the room after a phase, written beside the
//! phase it belongs to.**
//!
//! A lock is a **query** and an **assertion** and a **sentence**, and the three
//! are deliberately different things:
//!
//! * the **query** is jojobot's own vocabulary — the verb and the arguments a
//!   session would send, or the name of a view. Nothing is translated;
//! * the **assertion** is this format's own, and it is three words that do not
//!   branch. Pushing it into jojobot would grow a test framework inside a query
//!   surface, which is worse than the cost it saves;
//! * the **sentence** is authored prose and is never generated. *The pump says
//!   `sent` for where the job has got to* is worth more than a boolean because
//!   somebody wrote it about this room.
//!
//! ```text
//! recall  {"kind": "rhythm", "overdue": {"as_of": "2026-06-14"}}
//! carries thing:floor-pump
//! say     the pump was not offered as due, so months passed and nothing noticed
//! ```
//!
//! # 🚨 The rule this format exists to hold
//!
//! **A lock whose only assertion is `lacks` is refused, at parse time.**
//!
//! A negative alone passes on an empty answer — a store that lost everything
//! satisfies *the wrong thing is not there* perfectly. It is the rule this
//! project repeats more than any other and the one a tired author breaks, so
//! the parser holds it rather than whoever is awake.
//!
//! # 🚨 A lock carries no session, and that is what a lock IS
//!
//! **A lock's query goes to the room with no `sid`**, so it can ask only the
//! verbs that need no identity. `read_mailbox` opens the box of whoever is
//! asking, and a lock is nobody.
//!
//! **This is deliberate rather than a gap to close.** A lock observes the state
//! a phase left behind. The moment it carries an identity it becomes a
//! participant, and what it can see starts to depend on whose identity the
//! harness pretended to be — which is the harness coaching the occupant by
//! another route.
//!
//! **The identity-free path is usually there.** To assert about mail, read the
//! board rather than a box:
//!
//! ```text
//! search  {"query": "*", "include_mail": true, "limit": 200}
//! carries the subject the room posted
//! lacks   "state":"new"
//! say     the brief is still sitting new in the box, so nobody took delivery
//! ```
//!
//! **A state that no identity-free verb can reach is a finding about the
//! surface**, not a gap in the harness. Report it rather than working around
//! it.
//!
//! # ⚠️ Name the key, always
//!
//! An assertion is a substring of the answer as text, so **`carries 35` matches
//! the digits inside a timestamp** and holds for a reason that has nothing to
//! do with what it claims. Write `carries "cost":"35"`. The lock is then
//! checking the value under the key it means, which is also the stronger claim.
//!
//! # ⚠️ One lock per object
//!
//! A substring says a value is somewhere in the answer. **It cannot say which
//! object is holding it.** A read of every thing plus `carries paid` holds on a
//! room where the job that said `paid` was painted over and another job
//! supplied the word. Select the one object — `recall {"subject": …}` — when
//! the claim is about that object.
//!
//! Correlating two fields inside one answer would make this a query language,
//! which is what the assertion vocabulary refuses to become.
//!
//! # The hatch, and it is counted
//!
//! A lock may name a Rust check instead of a query. **The run reports how many
//! did and which**, because an escape nobody counts is an escape everybody
//! takes — and each one is a specific thing jojobot's query surface cannot say,
//! which is a finding rather than a footnote.

use anyhow::{Context, Result, bail};

/// What a lock asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asks {
    /// A verb and its arguments, exactly as a session would send them.
    Query { verb: String, args: String },
    /// **A named Rust check** — the hatch. Counted and reported.
    Check(String),
}

/// What must be so about the answer. **None of these branches**, which is what
/// keeps the vocabulary from becoming a language somebody has to learn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    Carries(String),
    Lacks(String),
    AtLeast(usize, String),
}

/// **When a lock's query runs.** `Live` is every lock this format has ever
/// had: the query goes to the room as it stands when the run finishes.
/// `OwnPhase` is new and opt-in per lock (`window own-phase`, right beside
/// `say`): the query is never sent live at all — the needles are matched
/// against the boundary text taken right after the lock's OWN phase, sliced
/// to the query's own subject when it names one. A lock silent about this
/// is `Live`, unconditionally — nothing about an existing lock's behaviour
/// changes by this variant existing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    Live,
    OwnPhase,
}

/// One lock.
pub struct Lock {
    pub(crate) asks: Asks,
    pub(crate) expects: Vec<Expect>,
    /// The authored sentence a reader sees when this does not hold.
    pub(crate) say: String,
    /// **What the run calls this check**, which is the sentence under the key
    /// of the phase it was written beneath — `Phase 2 — the pump says sent`.
    ///
    /// The run names a phase nothing asserts over, and it keys a check to a
    /// phase by the `Phase N` its name opens with. An author who wrote the
    /// lock under a heading has said which phase it belongs to already, so the
    /// reader carries it rather than asking for it a second time in the
    /// sentence. A lock under no heading is its sentence alone.
    pub name: String,
    /// **The Rust behind a named check**, once somebody has resolved it.
    ///
    /// A lock read out of a document carries none: reading a document and
    /// knowing which checks a build ships are different jobs, and the reader
    /// does not do the second.
    hatch: Option<Box<dyn crate::run::Checks>>,
    pub(crate) when: When,
    /// **The bare phase key**, kept apart from `name` because `OwnPhase`
    /// needs to ask [`crate::run::Observed::across`] for it and `name`
    /// already carries the authored sentence fused to it. `None` for a lock
    /// written under no heading — `window own-phase` on one of those is
    /// refused at parse time, since there is no boundary to name.
    pub(crate) phase_key: Option<String>,
}

impl std::fmt::Debug for Lock {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Lock")
            .field("asks", &self.asks)
            .field("expects", &self.expects)
            .field("say", &self.say)
            .field("name", &self.name)
            .field("resolved", &self.hatch.is_some())
            .field("when", &self.when)
            .finish()
    }
}

impl Lock {
    /// **The verb and arguments this lock sends**, when it sends any.
    ///
    /// A lock naming a Rust check asks nothing of the room and answers `None`.
    pub fn asked(&self) -> Option<(&str, &str)> {
        match &self.asks {
            Asks::Query { verb, args } => Some((verb.as_str(), args.as_str())),
            Asks::Check(_) => None,
        }
    }

    /// **Give this lock the Rust it names**, when it names one.
    ///
    /// A lock asking a query is left alone: it needs nothing but the room.
    pub fn resolve(&mut self, hatches: &crate::run::Hatches) {
        if let Asks::Check(named) = &self.asks {
            self.hatch = hatches(named);
        }
    }
}

const OPENS: &str = "```locks";
const FENCE: &str = "```";

/// The heading that starts a phase, and the same one the playbook reads.
const PHASE: &str = "## Phase ";

/// **Read the locks out of a document.** Blank lines separate one from the next.
pub fn read(document: &str) -> Result<Vec<Lock>> {
    let mut locks = Vec::new();
    for (at, lines, phase) in blocks(document) {
        locks.push(
            one(&lines, phase.as_deref())
                .with_context(|| format!("reading the lock at line {at}"))?,
        );
    }
    Ok(locks)
}

/// Every lock in the document: where it starts, its lines, and the key of the
/// phase it was written under.
fn blocks(document: &str) -> Vec<(usize, Vec<String>, Option<String>)> {
    let mut all = Vec::new();
    let mut inside = false;
    let mut lines: Vec<String> = Vec::new();
    let mut phase: Option<String> = None;
    let mut began = 0;
    let close = |lines: &mut Vec<String>, all: &mut Vec<_>, began, phase: &Option<String>| {
        if !lines.is_empty() {
            all.push((began, std::mem::take(lines), phase.clone()));
        }
    };
    for (at, line) in document.lines().enumerate() {
        let trimmed = line.trim();
        // **A phase heading outside a block says which phase the locks under
        // it belong to.** Every other heading ends the phase before it, the
        // same way the playbook reads them, so a lock under a maintainer's
        // section is keyed to nothing rather than to whatever came last.
        if !inside && let Some(heading) = line.strip_prefix("## ") {
            phase = line
                .starts_with(PHASE)
                .then(|| crate::run::phase_key(heading.trim()).to_string());
            continue;
        }
        if !inside && trimmed == OPENS {
            inside = true;
            continue;
        }
        if inside && trimmed.starts_with(FENCE) {
            close(&mut lines, &mut all, began, &phase);
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            close(&mut lines, &mut all, began, &phase);
            continue;
        }
        if lines.is_empty() {
            began = at + 1;
        }
        lines.push(trimmed.to_string());
    }
    all
}

fn one(lines: &[String], phase: Option<&str>) -> Result<Lock> {
    let (verb, rest) = lines[0]
        .split_once(char::is_whitespace)
        .with_context(|| "a lock opens with what it asks: a verb and its arguments, or a check")?;
    let asks = match verb {
        "check" => Asks::Check(rest.trim().to_string()),
        _ => Asks::Query {
            verb: verb.to_string(),
            args: rest.trim().to_string(),
        },
    };
    let mut expects = Vec::new();
    let mut say = None;
    let mut when = When::Live;
    for line in &lines[1..] {
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .with_context(|| format!("{line:?} is not a word and then what it is about"))?;
        let rest = rest.trim().to_string();
        match word {
            "carries" => expects.push(Expect::Carries(rest)),
            "lacks" => expects.push(Expect::Lacks(rest)),
            "at" => {
                let counted = rest
                    .strip_prefix("least ")
                    .with_context(|| "the counting assertion reads `at least 3 of <text>`")?;
                let (how_many, what) = counted
                    .split_once(" of ")
                    .with_context(|| "the counting assertion reads `at least 3 of <text>`")?;
                expects.push(Expect::AtLeast(
                    how_many
                        .trim()
                        .parse()
                        .with_context(|| format!("{how_many:?} is no count"))?,
                    what.trim().to_string(),
                ));
            }
            "say" => say = Some(rest),
            "window" => {
                if rest != "own-phase" {
                    bail!(
                        "{rest:?} is not a window this format knows — the one word it takes is `own-phase`"
                    );
                }
                if phase.is_none() {
                    bail!(
                        "a lock written under no phase heading has no boundary to name — \
                         `window own-phase` only means something under a `## Phase N` heading",
                    );
                }
                if !matches!(asks, Asks::Query { .. }) {
                    bail!(
                        "a lock naming a check has no query to window — `window own-phase` is for a verb-naming lock"
                    );
                }
                when = When::OwnPhase;
            }
            other => bail!(
                "{other:?} asserts nothing — a lock says carries, lacks, at least N of, say, or \
                 window",
            ),
        }
    }
    // **The sentence is what makes a failure readable**, and a lock without one
    // reports a boolean to somebody who then has to open the room to find out
    // what it meant.
    let say = say.context("a lock says what a reader sees when it does not hold: `say …`")?;
    if expects.is_empty() && !matches!(asks, Asks::Check(_)) {
        bail!("a lock that asserts nothing holds always — say what must be so");
    }
    // 🚨 **The rule the format exists to hold.**
    if !expects.is_empty() && expects.iter().all(|e| matches!(e, Expect::Lacks(_))) {
        bail!(
            "this lock only says what is ABSENT, so it passes on an answer that came back empty \
             — a store that lost everything satisfies it perfectly. Say what must be THERE as \
             well, in the same lock",
        );
    }
    let name = match phase {
        Some(phase) => format!("{phase} \u{2014} {say}"),
        None => say.clone(),
    };
    Ok(Lock {
        asks,
        expects,
        say,
        name,
        hatch: None,
        when,
        phase_key: phase.map(str::to_string),
    })
}

/// **A lock, as the thing a run checks.**
///
/// The adapter is the whole of what turns a document into a check: the query
/// goes to the room exactly as written, the assertions read the answer, and the
/// authored sentence is what a reader sees.
#[async_trait::async_trait]
impl crate::run::Expectation for Lock {
    fn name(&self) -> &str {
        &self.name
    }

    fn hatch(&self) -> Option<&str> {
        match &self.asks {
            Asks::Check(named) => Some(named.as_str()),
            Asks::Query { .. } => None,
        }
    }

    async fn check(&self, seen: &crate::run::Observed<'_>) -> crate::run::Outcome {
        let Asks::Query { verb, args } = &self.asks else {
            let Asks::Check(named) = &self.asks else {
                unreachable!("a lock asks a query or names a check")
            };
            // **The hatch, run.** The check answers the verdict and this lock
            // answers with its own sentence, so a reader gets the prose
            // somebody wrote about the room with what the check found after it.
            let Some(hatch) = &self.hatch else {
                return crate::run::Outcome {
                    name: self.name.clone(),
                    held: false,
                    applies: true,
                    refused: false,
                    saying: format!("this lock names a check nobody wrote: {named}"),
                };
            };
            return match hatch.run(seen).await {
                Ok(()) => crate::run::Outcome {
                    name: self.name.clone(),
                    held: true,
                    applies: true,
                    refused: false,
                    saying: self.say.clone(),
                },
                Err(found) => crate::run::Outcome {
                    name: self.name.clone(),
                    held: false,
                    applies: true,
                    refused: false,
                    saying: format!("{}: {found}", self.say),
                },
            };
        };
        let args: serde_json::Value = match serde_json::from_str(args) {
            Ok(args) => args,
            Err(e) => {
                return crate::run::Outcome {
                    name: self.name.clone(),
                    held: false,
                    applies: true,
                    refused: false,
                    saying: format!("this lock's query is not json: {e}"),
                };
            }
        };
        let answer = match self.when {
            When::Live => seen.room.call(verb, args).await,
            // **Never sent live at all.** The needles are matched against the
            // boundary taken right after this lock's own phase, sliced to
            // the query's own subject when it names one — see
            // `crate::isolate::boundary_text` for why the slicing matters.
            When::OwnPhase => {
                let phase_key = self
                    .phase_key
                    .as_deref()
                    .expect("window own-phase is refused at parse time under no phase heading");
                let Some((_before, after)) = seen.across(phase_key) else {
                    return crate::run::Outcome {
                        name: self.name.clone(),
                        held: false,
                        applies: true,
                        refused: true,
                        saying: format!(
                            "{}: this lock asks for its own phase's boundary, and none was \
                             recorded for {phase_key:?}",
                            self.say,
                        ),
                    };
                };
                crate::isolate::boundary_text(&after.world, args["subject"].as_str())
            }
        };

        // 🚨 **A REFUSED QUERY IS NOT AN ANSWER, AND COUNTING IN IT IS A
        // VERDICT ABOUT NOTHING.**
        //
        // A blocked read carries no objects, so every `carries` fails and every
        // `lacks` holds — and both read exactly like a room that was asked a
        // fair question. ⛔️ **A run scored a lock zero when `recall` had come
        // back *not an entity jojobot knows*: the needle was never looked for,
        // and the failure named the needle.**
        //
        // **The question was malformed and the answer is no send a reader to
        // different places**, so this says which. It fails rather than
        // abstaining: a lock that cannot ask its question is not a lock that
        // has nothing to say, and a room where one cannot be asked is a room
        // somebody has to fix.
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&answer)
            && parsed["status"] == "blocked"
        {
            return crate::run::Outcome {
                name: self.name.clone(),
                held: false,
                applies: true,
                refused: true,
                saying: format!(
                    "{}: this lock's own query was refused against the finished room, after \
                     every phase, so nothing was measured — {}",
                    self.say,
                    parsed["how_to_proceed"]
                        .as_str()
                        .unwrap_or("the answer carried no way forward"),
                ),
            };
        }

        let short = |what: &str| -> String {
            // **The answer, cut but never summarised.** A reader needs enough
            // to see what came back instead; the whole of a two-hundred-hit
            // read buries the sentence that matters.
            let head: String = what.chars().take(600).collect();
            match what.chars().count() > 600 {
                true => format!("{head}… [{} characters in all]", what.chars().count()),
                false => head,
            }
        };
        for expect in &self.expects {
            let missed = match expect {
                Expect::Carries(text) if !answer.contains(text.as_str()) => {
                    Some(format!("{text:?} is not in what came back"))
                }
                Expect::Lacks(text) if answer.contains(text.as_str()) => {
                    Some(format!("{text:?} is in what came back and should not be"))
                }
                Expect::AtLeast(how_many, text) => {
                    let found = answer.matches(text.as_str()).count();
                    match found < *how_many {
                        true => Some(format!("{text:?} came back {found} times, not {how_many}")),
                        false => None,
                    }
                }
                Expect::Carries(_) | Expect::Lacks(_) => None,
            };
            if let Some(missed) = missed {
                return crate::run::Outcome {
                    name: self.name.clone(),
                    held: false,
                    applies: true,
                    refused: false,
                    saying: format!(
                        "{}: {missed}. What the finished room shows, after every phase: {}",
                        self.say,
                        short(&answer)
                    ),
                };
            }
        }
        crate::run::Outcome {
            name: self.name.clone(),
            held: true,
            applies: true,
            refused: false,
            saying: self.say.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::Expectation;

    /// **A lock is a query, an assertion and a sentence**, and the query is
    /// jojobot's own vocabulary with nothing translated.
    #[test]
    fn a_lock_carries_a_query_an_assertion_and_a_sentence() {
        let read = read(
            "```locks\n\
             recall {\"kind\": \"rhythm\", \"overdue\": {\"as_of\": \"2026-06-14\"}}\n\
             carries thing:floor-pump\n\
             say     the pump was not offered as due\n\
             ```\n",
        )
        .expect("the lock reads");
        assert_eq!(read.len(), 1);
        assert_eq!(
            read[0].asks,
            Asks::Query {
                verb: "recall".into(),
                args: "{\"kind\": \"rhythm\", \"overdue\": {\"as_of\": \"2026-06-14\"}}".into(),
            },
            "the query is sent as written, because that is what a session sends",
        );
        assert_eq!(
            read[0].expects,
            vec![Expect::Carries("thing:floor-pump".into())]
        );
        assert_eq!(read[0].say, "the pump was not offered as due");
    }

    /// 🚨 **A lock that only says what is absent is refused, and the refusal
    /// says why.**
    ///
    /// **Both halves in one case**: the bare negative is refused, and the same
    /// negative paired with a positive lands. Without the second, a parser that
    /// refused every lock would pass.
    #[test]
    fn a_lock_that_only_says_what_is_absent_is_refused_and_the_pair_lands() {
        let refused = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             lacks   person:ralph\n\
             say     ralph was invented\n\
             ```\n",
        )
        .expect_err("a negative alone passes on an empty answer");
        let said = format!("{refused:#}");
        assert!(
            said.contains("empty") && said.contains("THERE"),
            "the refusal says why a bare negative is worthless: {said}",
        );

        let paired = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             lacks   person:ralph\n\
             carries person:milhouse\n\
             say     ralph was invented, and milhouse should still be there\n\
             ```\n",
        )
        .expect("a negative with its positive is a real lock");
        assert_eq!(paired[0].expects.len(), 2);
    }

    /// **`window own-phase` parses, under a phase heading, and carries the
    /// bare phase key rather than the fused name.**
    #[test]
    fn a_lock_may_window_its_own_phase_under_a_heading() {
        let read = read(
            "## Phase 1 — the room\n\n\
             ```locks\n\
             recall {\"subject\": \"person:milhouse\", \"facts\": true}\n\
             carries \"one-marker\"\n\
             say     phase 1 should not see a later write\n\
             window  own-phase\n\
             ```\n",
        )
        .expect("a windowed lock reads");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].when, When::OwnPhase);
        assert_eq!(read[0].phase_key.as_deref(), Some("Phase 1"));
    }

    /// **A lock silent about `window` is `Live`, unconditionally** — the
    /// default every existing lock in every shipped room already relies on.
    #[test]
    fn a_lock_that_never_mentions_window_is_live() {
        let read = read(
            "## Phase 1 — the room\n\n\
             ```locks\n\
             recall {\"subject\": \"person:milhouse\"}\n\
             carries \"one-marker\"\n\
             say     an ordinary lock\n\
             ```\n",
        )
        .expect("an ordinary lock reads");
        assert_eq!(read[0].when, When::Live);
    }

    /// 🚨 **`window own-phase` under no phase heading is refused**, because
    /// there is no boundary to name — an unheaded lock has never had a phase.
    #[test]
    fn windowing_a_lock_under_no_phase_heading_is_refused() {
        let refused = read(
            "```locks\n\
             recall {\"subject\": \"person:milhouse\"}\n\
             carries \"one-marker\"\n\
             say     an unheaded lock\n\
             window  own-phase\n\
             ```\n",
        )
        .expect_err("a windowed lock needs a phase to window to");
        assert!(format!("{refused:#}").contains("no boundary"));
    }

    /// 🚨 **A word other than `own-phase` is refused, named.**
    #[test]
    fn windowing_on_an_unknown_word_is_refused() {
        let refused = read(
            "## Phase 1 — the room\n\n\
             ```locks\n\
             recall {\"subject\": \"person:milhouse\"}\n\
             carries \"one-marker\"\n\
             say     an unheaded lock\n\
             window  every-phase\n\
             ```\n",
        )
        .expect_err("only own-phase is a window this format knows");
        assert!(format!("{refused:#}").contains("every-phase"));
    }

    /// 🚨 **A hatch has no query to window**, so `window own-phase` on one is
    /// refused rather than silently ignored.
    #[test]
    fn windowing_a_named_check_is_refused() {
        let refused = read(
            "## Phase 1 — the room\n\n\
             ```locks\n\
             check   the_cost_reads_as_a_number\n\
             say     what the pump cost did not normalise\n\
             window  own-phase\n\
             ```\n",
        )
        .expect_err("a named check has no query to window");
        assert!(format!("{refused:#}").contains("check"));
    }

    /// **A lock with no sentence is refused.** A boolean sends a reader to the
    /// room to find out what it meant.
    #[test]
    fn a_lock_with_no_sentence_is_refused() {
        let refused = read("```locks\nrecall {}\ncarries person:milhouse\n```\n")
            .expect_err("a lock says what a reader sees");
        assert!(format!("{refused:#}").contains("does not hold"));
    }

    /// **The hatch is a lock like any other**, so a room reaching for Rust is
    /// visible in the same list rather than somewhere else.
    #[test]
    fn a_lock_may_name_a_rust_check_instead_of_a_query() {
        let read = read(
            "```locks\n\
             check   the_cost_reads_as_a_number\n\
             say     what the pump cost did not normalise\n\
             ```\n",
        )
        .expect("the hatch reads");
        assert_eq!(
            read[0].asks,
            Asks::Check("the_cost_reads_as_a_number".into())
        );
    }

    /// **The run can say how many locks reached for Rust, and which.**
    ///
    /// Both halves: the hatch is named, and a lock that asked a query is not
    /// counted as one. Without the second, a counter that named everything
    /// would pass and the number would mean nothing.
    #[test]
    fn the_hatches_are_countable_and_a_query_is_not_one() {
        let read = read(
            "```locks\n\
             check   the_cost_reads_as_a_number\n\
             say     what the pump cost did not normalise\n\
             \n\
             recall {\"kind\": \"person\"}\n\
             carries person:milhouse\n\
             say     milhouse is gone\n\
             ```\n",
        )
        .expect("both read");
        let taken: Vec<Option<&str>> = read.iter().map(crate::run::Expectation::hatch).collect();
        assert_eq!(taken, vec![Some("the_cost_reads_as_a_number"), None]);
    }

    /// **A lock is keyed to the phase it is written under.**
    ///
    /// The run names a phase nothing asserts over, and it keys a check to a
    /// phase by the `Phase N` its name opens with. An author writing a lock
    /// under a phase heading has already said which phase it belongs to, so
    /// the reader carries it rather than asking for it twice.
    ///
    /// **Both halves**: the locks under two headings take their own keys, and
    /// a lock under no heading is named by its sentence alone. Without the
    /// second, a reader that prefixed everything would pass.
    #[test]
    fn a_lock_is_keyed_to_the_phase_it_is_written_under() {
        let keyed = read(
            "## Phase 1 — the room\n\
             \n\
             ```locks\n\
             recall {\"kind\": \"thing\"}\n\
             carries thing:jukebox\n\
             say     the jukebox is gone\n\
             ```\n\
             \n\
             ## Phase 2 — the cold question\n\
             \n\
             ```locks\n\
             recall {\"kind\": \"person\"}\n\
             carries person:milhouse\n\
             say     milhouse is gone\n\
             ```\n",
        )
        .expect("both read");
        assert_eq!(
            keyed.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            vec![
                "Phase 1 — the jukebox is gone",
                "Phase 2 — milhouse is gone",
            ],
            "a lock keyed to no phase leaves that phase reading as uncovered",
        );
        assert_eq!(
            keyed[0].say, "the jukebox is gone",
            "the authored sentence is untouched by the key",
        );

        let unkeyed = read(
            "```locks\n\
             recall {\"kind\": \"thing\"}\n\
             carries thing:jukebox\n\
             say     the jukebox is gone\n\
             ```\n",
        )
        .expect("the lock reads");
        assert_eq!(
            unkeyed[0].name, "the jukebox is gone",
            "a lock under no phase heading was given a key nothing in the document says",
        );
    }

    /// **A named check runs the Rust somebody wrote for it, and the lock keeps
    /// its own sentence.**
    ///
    /// The hatch is for the claims no query expresses. What it must NOT do is
    /// take the authored sentence with it: a reader gets the sentence somebody
    /// wrote about this room, and the check contributes only the verdict and
    /// what it found.
    ///
    /// **Both halves.** A check that holds and one that does not, because a
    /// resolver that reported one verdict for everything would pass on either
    /// alone.
    #[tokio::test]
    async fn a_named_check_supplies_the_verdict_and_the_lock_supplies_the_sentence() {
        let (_room, surface) = crate::room::Room::open_with_client(
            &crate::room::server_binary().expect("a jojobot binary"),
        )
        .await
        .expect("a room");
        let boundaries: Vec<crate::run::Boundary> = Vec::new();
        let seen = crate::run::Observed {
            room: &surface,
            boundaries: &boundaries,
        };

        let document = "```locks\n\
             check   a_check_that_holds\n\
             say     the thing the room is about did not happen\n\
             \n\
             check   a_check_that_misses\n\
             say     the other thing the room is about did not happen\n\
             ```\n";
        let mut locks = read(document).expect("both read");
        for lock in &mut locks {
            lock.resolve(&|named| match named {
                "a_check_that_holds" => Some(crate::run::checked(|_| Box::pin(async { Ok(()) }))),
                "a_check_that_misses" => Some(crate::run::checked(|_| {
                    Box::pin(async { Err("the pump says nothing".to_string()) })
                })),
                _ => None,
            });
        }

        let held = locks[0].check(&seen).await;
        assert!(held.held, "a check that holds was reported as missing");
        assert_eq!(
            held.name, "the thing the room is about did not happen",
            "the check took the authored sentence with it",
        );

        let missed = locks[1].check(&seen).await;
        assert!(!missed.held, "a check that misses was reported as holding");
        assert!(
            missed.saying.contains("the other thing the room is about")
                && missed.saying.contains("the pump says nothing"),
            "a reader gets the sentence and what the check found: {}",
            missed.saying,
        );
    }

    /// **A lock naming a check nobody wrote still fails**, and says so.
    #[tokio::test]
    async fn a_lock_naming_a_check_nobody_wrote_fails_rather_than_holding() {
        let (_room, surface) = crate::room::Room::open_with_client(
            &crate::room::server_binary().expect("a jojobot binary"),
        )
        .await
        .expect("a room");
        let boundaries: Vec<crate::run::Boundary> = Vec::new();
        let seen = crate::run::Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let mut locks = read(
            "```locks\n\
             check   nobody_wrote_this\n\
             say     something about the room\n\
             ```\n",
        )
        .expect("the lock reads");
        locks[0].resolve(&|_| None);
        let outcome = locks[0].check(&seen).await;
        assert!(!outcome.held);
        assert!(
            outcome.saying.contains("nobody_wrote_this"),
            "the failure does not name the check that is missing: {}",
            outcome.saying,
        );
    }

    /// **A refused query is a fourth way `held` reads false, and it must be
    /// told apart from a query that ran and found the room wanting.**
    ///
    /// Both halves, because a resolver that answered `refused` for everything
    /// would pass on this alone: a subject that does not exist is refused, and
    /// a query that runs cleanly is not.
    #[tokio::test]
    async fn a_refused_query_is_told_apart_from_one_that_ran_and_held() {
        let (_room, surface) = crate::room::Room::open_with_client(
            &crate::room::server_binary().expect("a jojobot binary"),
        )
        .await
        .expect("a room");
        let boundaries: Vec<crate::run::Boundary> = Vec::new();
        let seen = crate::run::Observed {
            room: &surface,
            boundaries: &boundaries,
        };

        let locks = read(
            "```locks\n\
             recall  {\"subject\": \"person:nobody-such-handle\"}\n\
             carries \"anything\"\n\
             say     this cannot be measured because the subject does not exist\n\
             \n\
             search  {\"query\": \"*\", \"limit\": 1}\n\
             carries \"results\"\n\
             say     this ran cleanly and holds\n\
             ```\n",
        )
        .expect("both read");

        let refused = locks[0].check(&seen).await;
        assert!(
            refused.refused,
            "a subject that does not exist must be reported as refused: {}",
            refused.saying,
        );
        assert!(
            !refused.held,
            "a refused query cannot also be reported as holding",
        );

        let ran = locks[1].check(&seen).await;
        assert!(
            !ran.refused,
            "a query that ran cleanly must not be reported as refused: {}",
            ran.saying,
        );
        assert!(
            ran.held,
            "a query that ran cleanly and met its assertion must hold: {}",
            ran.saying,
        );
    }

    /// **A verb-naming lock's failure sentence names what it actually
    /// checked** — the finished room, once, after every phase — rather than
    /// implying it watched the phase it is written under. Both sites,
    /// because a lock's query can fail two different ways: it can run
    /// cleanly and miss its assertion, or it can be refused outright, and a
    /// rewording that fixed only one would leave the other still speaking as
    /// though it had watched a phase.
    #[tokio::test]
    async fn a_query_locks_failure_names_the_finished_room_as_its_frame() {
        let (_room, surface) = crate::room::Room::open_with_client(
            &crate::room::server_binary().expect("a jojobot binary"),
        )
        .await
        .expect("a room");
        let boundaries: Vec<crate::run::Boundary> = Vec::new();
        let seen = crate::run::Observed {
            room: &surface,
            boundaries: &boundaries,
        };

        let locks = read(
            "```locks\n\
             recall  {\"kind\": \"person\"}\n\
             carries person:nobody-such-handle\n\
             say     nobody such is on the roster\n\
             \n\
             recall  {\"subject\": \"person:nobody-such-handle\"}\n\
             carries \"anything\"\n\
             say     this cannot be measured because the subject does not exist\n\
             ```\n",
        )
        .expect("both read");

        let missed = locks[0].check(&seen).await;
        assert!(
            !missed.held && !missed.refused,
            "the first lock's query must run cleanly and miss its assertion: {}",
            missed.saying,
        );
        assert!(
            missed.saying.contains("finished room"),
            "a missed assertion's failure did not name the finished room as its frame: {}",
            missed.saying,
        );

        let refused = locks[1].check(&seen).await;
        assert!(
            refused.refused,
            "the second lock's query must be refused: {}",
            refused.saying,
        );
        assert!(
            refused.saying.contains("finished room"),
            "a refused query's failure did not name the finished room as its frame: {}",
            refused.saying,
        );
    }

    /// **Blank lines separate locks**, so a phase carrying three is three.
    #[test]
    fn a_block_may_carry_more_than_one_lock() {
        let read = read(
            "```locks\n\
             recall {\"kind\": \"person\"}\n\
             carries person:milhouse\n\
             say     milhouse is gone\n\
             \n\
             recall {\"kind\": \"thing\"}\n\
             carries thing:jukebox\n\
             say     the jukebox is gone\n\
             ```\n",
        )
        .expect("both read");
        assert_eq!(read.len(), 2, "two locks, not one run together: {read:?}");
    }
}

/// **Where one needle matched, and what that says about the lock.**
///
/// 🚨 **A needle is a substring of the answer as text, so it can match
/// somewhere other than the field it names** — and a lock satisfied by the
/// wrong match holds while measuring nothing. **Three of those shipped in one
/// night**, one of them inside the lock written to guard the class, so this is
/// asked of every lock rather than left to a reader.
///
/// ⛔️ **The question is empirical and cannot be answered from the needle's
/// shape.** A needle naming a key can still match the wrong field — `"count":1`
/// matched an answer's own count of the OBJECTS it returned — and a bare handle
/// in a one-object answer is often unambiguous. **Where it actually matches, in
/// a real answer, is what decides it.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matched {
    /// One place. The lock can say which.
    Once(String),
    /// **More than one.** The lock is satisfied by any of them and cannot say
    /// which — so a claim about one sitting can be met by another's record.
    Ambiguous(Vec<String>),
    /// **Only outside the objects.** The needle matched the answer's own
    /// envelope rather than anything the read returned.
    Envelope(String),
    /// **Nowhere.** Either the lock is failing or the walk could not read the
    /// answer. ⚠️ **Kept apart from a clean result**: a room whose locks are
    /// all Rust hatches contributes nothing here, and that must not look like
    /// a walker that could not read.
    Nowhere,
}

/// One needle's verdict, with the lock it belongs to.
#[derive(Debug, Clone)]
pub struct NeedleVerdict {
    pub lock: String,
    pub needle: String,
    pub matched: Matched,
}

impl NeedleVerdict {
    /// **Whether this needle is a finding rather than a note.**
    ///
    /// ⭐ **An ambiguous needle is harmless when its own lock carries another
    /// needle only the sitting it names could satisfy** — the ambiguous half
    /// cannot carry the lock alone. That refinement explained both false
    /// positives in the hand-check that produced this case, so it is applied
    /// here rather than reported as noise.
    pub fn is_finding(&self, partners: &[&Matched]) -> bool {
        match self.matched {
            Matched::Ambiguous(_) => !partners.iter().any(|m| matches!(m, Matched::Once(_))),
            Matched::Envelope(_) => true,
            Matched::Once(_) | Matched::Nowhere => false,
        }
    }
}

/// **Every distinct place a needle matches**, deduplicated.
///
/// ⚠️ **The dedupe is load-bearing and it is what a first version got wrong.**
/// A match is recorded both at the key/value pair and at the string leaf
/// underneath it, so without this every needle reads as two places and every
/// lock reads as ambiguous. **That version printed fourteen findings where
/// there were three.**
fn places(value: &serde_json::Value, at: String, needle: &str, into: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let here = format!("{at}.{key}");
                if let Some(pair) = rendered_pair(key, child)
                    && pair.contains(needle)
                {
                    into.push(here.clone());
                }
                places(child, here, needle, into);
            }
        }
        serde_json::Value::Array(items) => {
            for (nth, child) in items.iter().enumerate() {
                places(child, format!("{at}[{nth}]"), needle, into);
            }
        }
        serde_json::Value::String(text) if text.contains(needle) => into.push(at),
        _ => {}
    }
}

/// A key and its value as the answer renders them, so a needle naming a key can
/// be matched against the pair rather than against the value alone.
fn rendered_pair(key: &str, value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(format!("\"{key}\":\"{text}\"")),
        serde_json::Value::Number(number) => Some(format!("\"{key}\":{number}")),
        serde_json::Value::Bool(flag) => Some(format!("\"{key}\":{flag}")),
        _ => None,
    }
}

/// **Ask every lock's own query and say where each needle matched.**
///
/// A lock that names a Rust check has no needle and contributes nothing.
pub async fn needle_verdicts(
    room: &crate::surface::Surface,
    locks: &[Lock],
) -> Vec<Vec<NeedleVerdict>> {
    let mut all = Vec::new();
    for lock in locks {
        let Asks::Query { verb, args } = &lock.asks else {
            continue;
        };
        let Ok(sent) = serde_json::from_str::<serde_json::Value>(args) else {
            continue;
        };
        let answer = room.call(verb, sent).await;
        let parsed: serde_json::Value =
            serde_json::from_str(&answer).unwrap_or(serde_json::Value::Null);
        let mut here = Vec::new();
        for expect in &lock.expects {
            // **A negative is not asked.** `lacks` is satisfied by absence, so
            // where it would have matched is not a question about it.
            //
            // ⛔️ **Nor is `at least`, and that is a limit rather than an
            // oversight.** A lock asking for N matches WANTS more than one
            // place, so *more than one place* says nothing about it — the
            // question for those is whether the matches are in the right
            // places, which is a different check and is not this one. **A
            // first version folded them in and reported one as a finding for
            // doing exactly what it was written to do.**
            let needle = match expect {
                Expect::Carries(needle) => needle,
                Expect::Lacks(_) | Expect::AtLeast(..) => continue,
            };
            let mut found = Vec::new();
            places(&parsed, String::new(), needle, &mut found);
            found.sort();
            found.dedup();
            // ⚠️ **The envelope is *outside every collection*, not *outside
            // `objects`*.** A first version named the key that `recall` nests
            // under, and every answer with a different shape read as an
            // envelope match — `list_entities` returns `entities`, and three
            // needles matching exactly once inside a real entity were reported
            // as findings. **What tells them apart is an index: anything the
            // answer RETURNED sits inside an array, and the wrapper does not.**
            let inside_a_collection = |path: &String| path.contains('[');
            let matched = match found.as_slice() {
                [] => Matched::Nowhere,
                [one] if !inside_a_collection(one) => Matched::Envelope(one.clone()),
                [one] => Matched::Once(one.clone()),
                many => Matched::Ambiguous(many.to_vec()),
            };
            here.push(NeedleVerdict {
                lock: lock.name.clone(),
                needle: needle.clone(),
                matched,
            });
        }
        all.push(here);
    }
    all
}

/// **What a room's needles came to**, so a room's own case is three lines
/// rather than a copy of this loop.
#[derive(Debug, Default)]
pub struct NeedleSummary {
    /// Needles resting on a match somewhere other than what they name, with
    /// nothing else in their lock to carry them.
    pub findings: Vec<String>,
    /// How many matched in more than one place, findings or not. **The number
    /// a hand-check can be compared against**, which is what says the walk is
    /// reading the answer the way a person did.
    pub ambiguous: usize,
    /// ⚠️ **Needles that matched nowhere**, kept apart from a clean result: a
    /// room whose locks are all Rust checks contributes nothing here, and that
    /// must not read the same as a walk that could not see.
    pub nowhere: Vec<String>,
}

/// Ask every lock and fold the verdicts into what a room's case asserts.
pub async fn needle_summary(room: &crate::surface::Surface, locks: &[Lock]) -> NeedleSummary {
    let mut summary = NeedleSummary::default();
    for one_lock in needle_verdicts(room, locks).await {
        for verdict in &one_lock {
            match verdict.matched {
                Matched::Ambiguous(_) => summary.ambiguous += 1,
                Matched::Nowhere => summary
                    .nowhere
                    .push(format!("{} — {}", verdict.lock, verdict.needle)),
                _ => {}
            }
            let partners: Vec<&Matched> = one_lock
                .iter()
                .filter(|other| other.needle != verdict.needle)
                .map(|other| &other.matched)
                .collect();
            if verdict.is_finding(&partners) {
                summary
                    .findings
                    .push(format!("{} — {}", verdict.lock, verdict.needle));
            }
        }
    }
    summary
}

/// The locks a room document carries, read from the shipped file.
pub fn locks_of(source: &str) -> Vec<Lock> {
    let path = crate::expectations::room_document(source);
    let document = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the shipped room at {} must read: {e}", path.display()));
    read(&document).unwrap_or_else(|e| panic!("the room's locks must parse: {e:#}"))
}

/// **Whether a needle's own written form proves the claim it names still
/// stands.**
///
/// A lock's assertion is a substring test — `Expect::Carries(text) if
/// !answer.contains(text.as_str())` — so the only question that matters for
/// any needle is: can this string appear in the answer while the thing it
/// claims is not true? It splits exactly along how the needle is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// **A quoted key and value** — `"settled":"paid"`. This is a FOLDED
    /// FIELD: the fold drops every non-active write before it serialises, so
    /// if the string is there, the claim stands. Safe by construction.
    FoldedField,
    /// **Anything else** — a bare handle (`person:ralph`) or bare prose
    /// (`meets on Tuesdays`). This is an edge or a fact's own text, served
    /// with its status MARKED rather than filtered, so a retracted record
    /// still carries it: a match proves the TEXT EXISTS, not that the CLAIM
    /// STANDS.
    BareHandle,
}

/// **A needle's shape, read from the needle alone.** No room is asked: the
/// question is how the needle is WRITTEN, not what it currently matches —
/// [`needle_verdicts`] already asks the empirical question, and it is a
/// different one.
pub fn standing_of(needle: &str) -> Standing {
    match needle.starts_with('"') {
        true => Standing::FoldedField,
        false => Standing::BareHandle,
    }
}

/// **Which way a needle's shape lies to whoever reads it.**
///
/// Two different empirical mistakes, caught by the same walk over the same
/// needles rather than two: a needle that proves the wrong THING (a
/// retracted or superseded record can still satisfy it), and a needle that
/// proves too LITTLE (its own text can be satisfied by a bigger value that
/// was never the claim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// **A bare handle with no companion pinning status.** Served with
    /// status marked rather than filtered, so a retracted or superseded
    /// record still carries it — a match proves the text exists, not that
    /// the claim stands.
    Retraction,
    /// **A bare run of digits.** A needle naming no key and no delimiter
    /// matches as a substring of any longer number that contains it —
    /// `carries 35` holds against a cost of 350 exactly as it holds against
    /// 35, and the match cannot tell the two apart.
    Broad,
}

/// **A needle with nothing in its own lock that rules out the empirical
/// mistake it risks.**
pub struct StandingFinding {
    pub lock: String,
    pub needle: String,
    pub risk: Risk,
}

/// **Classify every `carries` needle in every lock, without asking the room
/// anything.**
///
/// A lock naming a Rust check is scoped to a window by construction — it
/// reads a record's own status server-side, which is the whole reason a room
/// reaches for one — so it is skipped here rather than measured.
///
/// A bare-handle needle is flagged unless its own lock also carries a
/// **companion** — another `carries` needle pinning the record's status
/// explicitly, `carries "status":"<value>"` for ANY value — because naming
/// the status at all is the one thing a retracted or superseded record
/// cannot also satisfy by accident. Which value the companion names is
/// irrelevant: a lock proving something was superseded is as pinned as one
/// proving something stands.
///
/// **A query whose own arguments cannot put a retracted record in its
/// answer is skipped too, and this is structural rather than a per-lock
/// exemption.** Retraction is a property of a FACT or an EDGE: neither rides
/// on a query that asks for no `facts` and follows no edge, because there is
/// no such content anywhere in what comes back — a bare entity listing
/// carries only handles, names and kinds, none of which is ever marked
/// retracted. Unparsable arguments are read as exposed, because a needle
/// this classifier cannot read the shape of is the case it exists to find.
pub fn standing_findings(locks: &[Lock]) -> Vec<StandingFinding> {
    let mut findings = Vec::new();
    for lock in locks {
        if matches!(lock.asks, Asks::Check(_)) {
            continue;
        }
        let Some((_, args)) = lock.asked() else {
            continue;
        };
        let exposed = query_can_carry_a_retracted_record(args);
        let carried: Vec<&String> = lock
            .expects
            .iter()
            .filter_map(|expect| match expect {
                Expect::Carries(needle) => Some(needle),
                Expect::Lacks(_) | Expect::AtLeast(..) => None,
            })
            .collect();
        let paired = carried.iter().any(|needle| pins_status(needle));
        for needle in carried {
            if exposed && !paired && standing_of(needle) == Standing::BareHandle {
                findings.push(StandingFinding {
                    lock: lock.name.clone(),
                    needle: needle.clone(),
                    risk: Risk::Retraction,
                });
            }
            if is_broad(needle) {
                findings.push(StandingFinding {
                    lock: lock.name.clone(),
                    needle: needle.clone(),
                    risk: Risk::Broad,
                });
            }
        }
    }
    findings
}

/// **Whether a needle is a bare run of digits with nothing else in it.**
///
/// A folded field closes on its own quote — `"cost":"35"` cannot be a prefix
/// of a longer number, only of a longer STRING starting `35...`, which is a
/// different and much rarer collision. A bare `35` has no closing character
/// at all, so it matches inside `350`, `1350`, or any number that contains
/// it, exactly as readily as it matches a real `35`.
fn is_broad(needle: &str) -> bool {
    !needle.is_empty() && needle.bytes().all(|byte| byte.is_ascii_digit())
}

/// **Whether a needle pins a status explicitly**, at any value —
/// `"status":"active"`, `"status":"superseded"`, and so on. The key must be
/// exactly `status`: a needle naming a different key, such as
/// `"status_note"`, makes no claim about standing and pins nothing.
fn pins_status(needle: &str) -> bool {
    needle
        .strip_prefix("\"status\":\"")
        .is_some_and(|rest| rest.ends_with('"'))
}

/// **Does this query's own shape make a retracted record reachable at
/// all?** Asked with `facts: true`, the FACTS of a record ride the answer,
/// and a retraction is marked rather than filtered off them. Asked with a
/// `follow`, an edge's own object rides the answer the same way. Neither key
/// present means the answer is entity metadata only — a handle, a name, a
/// kind — and none of that is ever marked retracted, so there is nothing
/// here for a bare-handle needle to be fooled by.
///
/// Text that does not parse as the object a `recall`/`search` call takes is
/// read as exposed rather than exempted: a shape this cannot classify is
/// what the classifier exists to catch, not a reason to wave it through.
fn query_can_carry_a_retracted_record(args: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) else {
        return true;
    };
    parsed.get("facts").and_then(serde_json::Value::as_bool) == Some(true)
        || parsed.get("follow").is_some()
}

#[cfg(test)]
mod standing_tests {
    use super::*;

    /// **The known-bad instance, taken from history rather than invented.**
    ///
    /// This is the pump lock exactly as it stood before commit 18a4cfb: two
    /// bare-handle needles, no companion, run against the finished room. A
    /// paid run had Ralph's account retracted and this lock held anyway,
    /// because a retraction is marked rather than filtered. **If the
    /// classifier does not flag this shape, it measures nothing.**
    #[test]
    fn the_historical_pump_lock_is_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:floor-pump\", \"facts\": true}\n\
             carries person:ralph\n\
             carries person:nelson\n\
             say     one of the two accounts of how the pump came back is gone\n\
             ```\n",
        )
        .expect("the historical lock reads");
        let found = standing_findings(&locks);
        let needles: Vec<&str> = found.iter().map(|f| f.needle.as_str()).collect();
        assert!(
            needles.contains(&"person:ralph") && needles.contains(&"person:nelson"),
            "the classifier does not fire on the exact shape that shipped broken: {needles:?}",
        );
    }

    /// **A fold-backed lock must not be flagged.** Otherwise the classifier
    /// flags everything and says nothing.
    #[test]
    fn a_folded_field_needle_is_not_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:gravel-bike\"}\n\
             carries \"settled\":\"paid\"\n\
             say     the bike's tally is not settled\n\
             ```\n",
        )
        .expect("the lock reads");
        assert!(
            standing_findings(&locks).is_empty(),
            "a folded field was flagged: {:?}",
            standing_findings(&locks)
                .iter()
                .map(|f| &f.needle)
                .collect::<Vec<_>>(),
        );
    }

    /// **A bare handle paired with an explicit standing check is not
    /// flagged.** No shipped room writes this pairing yet — this proves the
    /// escape the classifier grants actually works, rather than being a rule
    /// stated in prose and never exercised.
    #[test]
    fn a_bare_handle_paired_with_an_active_status_needle_is_not_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:floor-pump\", \"facts\": true}\n\
             carries person:ralph\n\
             carries \"status\":\"active\"\n\
             say     ralph's account is not standing\n\
             ```\n",
        )
        .expect("the lock reads");
        assert!(
            standing_findings(&locks).is_empty(),
            "a paired bare handle was flagged anyway: {:?}",
            standing_findings(&locks)
                .iter()
                .map(|f| &f.needle)
                .collect::<Vec<_>>(),
        );
    }

    /// **The companion is any explicit status, not only `active`.** A lock
    /// proving something was archived is as pinned as one proving something
    /// stands — what matters is that the lock NAMES the status it expects,
    /// not which value it names. The real instance: April's Springfield lock
    /// in `rooms/year.md` pins `"status":"archived"` — superseded and
    /// retracted collapsed into that one status, so a status pin alone no
    /// longer tells the two apart, which is why that lock also pairs it with
    /// a `lacks "retracts":`.
    #[test]
    fn a_bare_handle_paired_with_an_archived_status_needle_is_not_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"person:milhouse\", \"facts\": true}\n\
             carries place:springfield\n\
             carries \"status\":\"archived\"\n\
             say     the old claim is marked as no longer true\n\
             ```\n",
        )
        .expect("the lock reads");
        assert!(
            standing_findings(&locks).is_empty(),
            "a bare handle pinned to an explicit non-active status was flagged anyway: {:?}",
            standing_findings(&locks)
                .iter()
                .map(|f| &f.needle)
                .collect::<Vec<_>>(),
        );
    }

    /// **A needle that only mentions `status` does not pin anything.** The
    /// companion is the exact shape `"status":"<value>"` — a key that merely
    /// contains the word is not the same claim, and must not clear the flag
    /// it did not actually make.
    #[test]
    fn a_needle_that_only_mentions_status_does_not_pin() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:floor-pump\", \"facts\": true}\n\
             carries person:ralph\n\
             carries \"status_note\":\"checked\"\n\
             say     ralph's account is not standing\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            found.iter().any(|f| f.needle == "person:ralph"),
            "a needle naming a different key was treated as a status companion: {:?}",
            found.iter().map(|f| &f.needle).collect::<Vec<_>>(),
        );
    }

    /// **A bare run of digits is flagged as broad**, whether or not its lock
    /// carries a status companion — a status pin says the RECORD stands, and
    /// says nothing about whether a longer number contains this one.
    #[test]
    fn a_bare_numeric_needle_is_flagged_as_broad() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:gravel-bike\"}\n\
             carries 35\n\
             say     the bike's tally reads 35\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            found
                .iter()
                .any(|f| f.needle == "35" && f.risk == Risk::Broad),
            "a bare run of digits was not flagged as broad: {:?}",
            found
                .iter()
                .map(|f| (&f.needle, f.risk))
                .collect::<Vec<_>>(),
        );
    }

    /// **The same value, key-scoped, is not flagged as broad.** Pairing it
    /// with the digits case above proves the classifier reads the needle's
    /// shape rather than reacting to any occurrence of "35" — a folded field
    /// closes on its own quote and cannot be a prefix of a longer number.
    #[test]
    fn the_same_value_quoted_as_a_folded_field_is_not_flagged_as_broad() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:gravel-bike\"}\n\
             carries \"cost\":\"35\"\n\
             say     the bike's cost is not settled\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            !found.iter().any(|f| f.risk == Risk::Broad),
            "a folded field needle was flagged as broad: {:?}",
            found
                .iter()
                .map(|f| (&f.needle, f.risk))
                .collect::<Vec<_>>(),
        );
    }

    /// **A lock naming a Rust check is scoped to a window and is never
    /// flagged**, whatever it happens to assert alongside the check.
    ///
    /// The parser allows a `carries` line beside a `check` line even though
    /// no shipped room writes one — the format does not forbid it, so this
    /// pins the bare handle IN THAT SHAPE to make sure the exemption is
    /// doing real work rather than agreeing by accident with an empty
    /// `expects` list.
    #[test]
    fn a_check_lock_is_never_flagged() {
        let locks = read(
            "```locks\n\
             check   both_accounts_of_the_pump_stand\n\
             carries person:ralph\n\
             say     one of the two accounts of how the pump came back is gone\n\
             ```\n",
        )
        .expect("the lock reads");
        assert!(standing_findings(&locks).is_empty());
    }

    /// **A bare handle out of a query that asks for no facts and follows no
    /// edge is not flagged.** Retraction lives on a fact's or an edge's own
    /// status, and neither rides on a bare `{"kind": "thing"}` listing — the
    /// real instance is Phase 2's `thing:floor-pump` needle, which no longer
    /// needs its own allowlist entry once the classifier reads this.
    #[test]
    fn a_bare_handle_out_of_a_kind_listing_is_not_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"kind\": \"thing\"}\n\
             carries thing:floor-pump\n\
             say     the pump the operator lent is not a thing jojobot knows\n\
             ```\n",
        )
        .expect("the lock reads");
        assert!(
            standing_findings(&locks).is_empty(),
            "a bare-listing needle was flagged anyway: {:?}",
            standing_findings(&locks)
                .iter()
                .map(|f| &f.needle)
                .collect::<Vec<_>>(),
        );
    }

    /// **The same needle, asked with `facts: true`, IS flagged.** The
    /// listing-only exemption is scoped to what the query can actually
    /// expose — proving the negative above needs this positive beside it, or
    /// a classifier that stopped flagging everything would pass it too.
    #[test]
    fn the_same_needle_asked_with_facts_is_still_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"subject\": \"thing:floor-pump\", \"facts\": true}\n\
             carries thing:floor-pump\n\
             say     the pump is not on the record\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            found
                .iter()
                .any(|f| f.needle == "thing:floor-pump" && f.risk == Risk::Retraction),
            "a needle out of a facts-bearing query was not flagged: {:?}",
            found.iter().map(|f| &f.needle).collect::<Vec<_>>(),
        );
    }

    /// **A bare listing that also follows an edge IS flagged.** `follow`
    /// walks to another object's own record, which can carry a retracted
    /// status exactly as `facts` can — the exemption is about what the query
    /// can reach, not about which top-level key happens to be missing.
    #[test]
    fn a_kind_listing_that_follows_an_edge_is_still_flagged() {
        let locks = read(
            "```locks\n\
             recall {\"kind\": \"person\", \"follow\": {\"shape\": \"membership\"}}\n\
             carries org:north-trail-club\n\
             say     nothing walks to the club\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            found
                .iter()
                .any(|f| f.needle == "org:north-trail-club" && f.risk == Risk::Retraction),
            "a needle out of a query that follows an edge was not flagged: {:?}",
            found.iter().map(|f| &f.needle).collect::<Vec<_>>(),
        );
    }

    /// **Arguments this classifier cannot parse are read as exposed.** A
    /// shape it cannot classify is the case it exists to find, not a reason
    /// to exempt it.
    #[test]
    fn unparsable_arguments_are_not_exempted() {
        let locks = read(
            "```locks\n\
             recall not-json-at-all\n\
             carries thing:floor-pump\n\
             say     the pump is not on the record\n\
             ```\n",
        )
        .expect("the lock reads");
        let found = standing_findings(&locks);
        assert!(
            found
                .iter()
                .any(|f| f.needle == "thing:floor-pump" && f.risk == Risk::Retraction),
            "unparsable arguments were exempted rather than flagged: {:?}",
            found.iter().map(|f| &f.needle).collect::<Vec<_>>(),
        );
    }
}
