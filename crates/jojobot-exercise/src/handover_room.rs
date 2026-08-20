//! **The handover room** — a second identity, and whether the operator's
//! assistant can tell what became of the work.
//!
//! Two phases, the second cold, as every room here.
//!
//! **The constraint shaped this goal rather than limiting it, and the next
//! builder should read that before re-deriving it.** A session is bound to its
//! bot for life and never boots as another to reach its data, so the model
//! cannot pick work up from inside the box it just created: this room CANNOT
//! measure *the helper did the work*. What it measures instead is the better
//! question — ***can the operator's assistant tell whether the work got
//! done*** — and that is a thing a person actually needs to know.
//!
//! **It also passes the range test on the third lever.** Not arithmetic: **state
//! the store owns that no session could have written down.** How much of a pile
//! somebody else has picked up is not in any sentence anywhere, because the
//! session that would have written it was not there when it changed. `list_sent`
//! exists for exactly this — where a message got to is not private to its
//! sender — and a cold session with no memory cannot guess how many were sent.

use serde_json::{Value, json};

use crate::run::{Expectation, Observed, Outcome};
use crate::surface::Seed;

/// The document this room is driven by.
pub const HANDOVER_ROOM: &str = "HANDOVER-ROOM.md";

/// The identity a fresh instance ships with, and the one the occupant wears.
const OCCUPANT: &str = "bot:assistant";

/// How many things the brief hands over. The number is the whole of the
/// terminal question: a cold session cannot guess it, and nothing but the mail
/// rail can tell it.
const PILE: usize = 3;

/// What the brief is called.
const BRIEF: &str = "the reading pile, and somebody to take it";

/// **The whole of what the occupant is told in the first phase.**
///
/// It asks for a colleague and hands over three things. It names no verb, says
/// nothing about how an identity is made, and does not mention that a box comes
/// with one — which is the part the room is watching for.
const BRIEF_BODY: &str = "\
I want a second assistant for the reading I keep not getting to. Give it a \
name that says what it is for, and tell it what it is there to do.

Three things to hand over, one at a time so they can be worked separately:

The long piece on how filesystems handle power loss. The paper about \
consensus that everybody quotes and nobody has read. And the manual for the \
espresso machine, which is forty pages and which I am never going to read.

Leave it so whoever comes next can pick it up.";

/// **What the room holds before the occupant arrives.**
///
/// Almost nothing, and that is the room. **A seed cannot stand an identity
/// up** — creating one is the behaviour under test — so the only furniture is
/// the brief, and everything the checks read is what the occupant made.
pub fn seed() -> anyhow::Result<Seed> {
    Ok(Seed::new().message("assistant", BRIEF, BRIEF_BODY))
}

/// The four locks, in the order the goal reaches them.
pub fn expectations() -> Vec<Box<dyn Expectation>> {
    vec![
        Box::new(TheBriefLeftTheBox),
        Box::new(TheColleagueExistsWithItsBox),
        Box::new(ThePileIsInTheColleaguesBox),
        Box::new(WhatBecameOfItIsOnTheRecord),
    ]
}

fn held(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: true,
        saying: saying.into(),
    }
}

fn missed(name: &str, saying: impl Into<String>) -> Outcome {
    Outcome {
        name: name.to_string(),
        held: false,
        saying: saying.into(),
    }
}

/// **Lock 1 — the brief left the box.**
struct TheBriefLeftTheBox;

#[async_trait::async_trait]
impl Expectation for TheBriefLeftTheBox {
    fn name(&self) -> &str {
        "Phase 1 — the brief left the box"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some(brief) = mail(seen)
            .await
            .into_iter()
            .find(|hit| hit["subject"] == BRIEF)
        else {
            return missed(
                self.name(),
                "the brief is not on the board, so the room was never furnished",
            );
        };
        if brief["state"] == "new" {
            return missed(
                self.name(),
                "the brief is still waiting, so nothing here says the occupant found its box",
            );
        }
        held(self.name(), "the brief left new, so delivery was taken")
    }
}

/// **Lock 2 — the colleague exists, with the box that came with it.**
///
/// A box is not made: it opens with the bot that owns it, in the one act. So
/// what must be true is a second identity carrying a box of its own — and the
/// occupant is never told that a box arrives with a bot, which is the thing
/// this lock is watching.
///
/// **The occupant names its colleague**, so nothing here pins a handle: the
/// colleague is whichever bot is not the one that ships.
struct TheColleagueExistsWithItsBox;

#[async_trait::async_trait]
impl Expectation for TheColleagueExistsWithItsBox {
    fn name(&self) -> &str {
        "Phase 1 — a colleague exists, and its box came with it"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let board = seen.room.call("start_here", json!({"brief": true})).await;
        let bots = bots_on(&board);
        if !bots.iter().any(|bot| bot["handle"] == OCCUPANT) {
            return missed(
                self.name(),
                format!("the shipped identity is not on the board, so nothing was read: {board}"),
            );
        }
        let colleagues: Vec<&Value> = bots
            .iter()
            .filter(|bot| bot["handle"] != OCCUPANT)
            .collect();
        match colleagues.as_slice() {
            [] => missed(
                self.name(),
                "no second identity was made, so there is nobody to hand anything to",
            ),
            [colleague] => {
                if colleague["mail"].is_null() {
                    return missed(
                        self.name(),
                        format!(
                            "{} exists and owns no box, so it cannot be written to",
                            colleague["handle"],
                        ),
                    );
                }
                held(
                    self.name(),
                    format!(
                        "{} stands, with the box that came with it",
                        colleague["handle"]
                    ),
                )
            }
            many => missed(
                self.name(),
                format!(
                    "{} identities were made where the brief asked for one",
                    many.len()
                ),
            ),
        }
    }
}

/// **Lock 3 — the pile is in the colleague's box, one thing at a time.**
///
/// The brief asks for them separately so they can be worked separately, which
/// is the mail rail doing what it is for. A run that handed the whole pile over
/// as one message has left a colleague with one thing to finish rather than
/// three, and the terminal question has nothing to count.
struct ThePileIsInTheColleaguesBox;

#[async_trait::async_trait]
impl Expectation for ThePileIsInTheColleaguesBox {
    fn name(&self) -> &str {
        "Phase 1 — the pile is in the colleague's box, one thing at a time"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some(colleague) = colleague_of(seen).await else {
            return missed(
                self.name(),
                "there is no colleague, so there is no box to have handed anything to",
            );
        };
        let box_name = colleague.trim_start_matches("bot:").to_string();
        let handed: Vec<Value> = mail(seen)
            .await
            .into_iter()
            .filter(|hit| hit["mailbox"] == box_name.as_str())
            .collect();
        if handed.len() != PILE {
            return missed(
                self.name(),
                format!(
                    "{} thing(s) are in the colleague's box where the brief handed over {PILE}",
                    handed.len(),
                ),
            );
        }
        // The positive that makes the count mean anything: they came from the
        // occupant rather than from nowhere.
        if !handed.iter().all(|hit| hit["sender"] == OCCUPANT) {
            return missed(
                self.name(),
                format!("something in the colleague's box was not sent by {OCCUPANT}"),
            );
        }
        held(
            self.name(),
            format!("{PILE} things are waiting in the colleague's box, each its own message"),
        )
    }
}

/// **Lock 4 — what became of the pile is on the record.**
///
/// The terminal lock, and the one the cold phase exists for. A session with no
/// memory of the first cannot know how many things were handed over, and
/// nothing it can read says so: the colleague's box is not its to open, and the
/// state of somebody else's mail is not in any sentence anywhere. **Where mail
/// got to is what the sender's own view answers, and it is the only thing that
/// does.**
///
/// Both halves: the count is written onto the colleague, and the pile really is
/// untouched — so a run that wrote a number without looking is not being
/// credited with an answer that happens to be right.
struct WhatBecameOfItIsOnTheRecord;

#[async_trait::async_trait]
impl Expectation for WhatBecameOfItIsOnTheRecord {
    fn name(&self) -> &str {
        "Phase 2 — how much of the pile is still waiting is on the colleague"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let Some(colleague) = colleague_of(seen).await else {
            return missed(
                self.name(),
                "there is no colleague, so there is nothing to have found out about",
            );
        };
        let box_name = colleague.trim_start_matches("bot:").to_string();
        let waiting = mail(seen)
            .await
            .into_iter()
            .filter(|hit| hit["mailbox"] == box_name.as_str() && hit["state"] == "new")
            .count();
        if waiting != PILE {
            return missed(
                self.name(),
                format!(
                    "{waiting} of the pile is waiting where {PILE} was handed over, so the \
                     answer this lock reads for is not {PILE} at all"
                ),
            );
        }
        let read = seen
            .room
            .call("recall", json!({"subject": colleague, "facts": true}))
            .await;
        if said_the_count(&read, PILE) {
            held(
                self.name(),
                format!("the colleague carries what became of the pile: {PILE} still waiting"),
            )
        } else {
            missed(
                self.name(),
                format!(
                    "nothing on {colleague} says how much of the pile is still waiting, so a \
                     later session has to go and find out again"
                ),
            )
        }
    }
}

/// Whether a read says the number, as a figure or as the word. **Both, because
/// which one somebody writes is not what this room measures** — and a figure
/// alone would fail a session that wrote a sentence.
fn said_the_count(read: &str, count: usize) -> bool {
    const WORDS: [&str; 4] = ["zero", "one", "two", "three"];
    let said = read.to_lowercase();
    said.contains(&count.to_string()) || WORDS.get(count).is_some_and(|word| said.contains(word))
}

/// The colleague's handle — whichever bot is not the one that ships.
async fn colleague_of(seen: &Observed<'_>) -> Option<String> {
    let board = seen.room.call("start_here", json!({"brief": true})).await;
    bots_on(&board)
        .iter()
        .filter(|bot| bot["handle"] != OCCUPANT)
        .filter_map(|bot| bot["handle"].as_str().map(str::to_string))
        .next()
}

/// The bots a boarding snapshot names, each with its mail beside it.
fn bots_on(board: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(board)
        .ok()
        .and_then(|body| body["snapshot"]["entities"]["bots"].as_array().cloned())
        .unwrap_or_default()
}

/// Every message on the board, mail asked for.
async fn mail(seen: &Observed<'_>) -> Vec<Value> {
    let board = seen
        .room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    serde_json::from_str::<Value>(&board)
        .ok()
        .and_then(|body| {
            body["results"].as_array().map(|hits| {
                hits.iter()
                    .filter(|hit| hit["hit"] == "message")
                    .cloned()
                    .collect()
            })
        })
        .unwrap_or_default()
}
