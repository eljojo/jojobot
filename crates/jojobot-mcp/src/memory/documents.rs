//! **What the document verbs share**: which records of a document are its live
//! sections, in order, and the section each one is.
//!
//! A section is a claim on the document thing that carries an `order` field, so
//! the records that make up a document are found by the key they carry, the way
//! a loop is found by the keys that make it one. A record without it is an
//! ordinary claim about the document and is not part of its text.

use super::*;
use jiff::civil::Date;
use jojobot_domain::memory::FactId;
use jojobot_domain::memory::document::{self, Section};
use jojobot_domain::memory::order;

/// One live section of a document: the record, its order key and the section it
/// holds.
pub(crate) struct Held<'a> {
    pub(crate) order: String,
    pub(crate) section: Section,
    pub(crate) fact: &'a Fact,
    /// For a chunk that carries a section on: the local id of the record that
    /// holds that section's heading.
    pub(crate) head: Option<&'a str>,
}

/// **The live sections among a document's records, in reading order.** A section
/// that was archived or taken back is not live: it stays readable by its address
/// and is left out of the page. **A chunk whose heading is not live is not live
/// either**: archiving a section takes the whole of it out. Ties in the order
/// key, which only a hand-written key can make, fall back to the record's id so
/// the page is still the same page every time it is read.
pub(crate) fn live_sections(facts: &[Fact]) -> Vec<Held<'_>> {
    let mut held: Vec<Held<'_>> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .filter_map(|fact| {
            let (order, section) =
                document::from_record(&fact.content, fact.details.as_deref(), &fact.fields)?;
            Some(Held {
                order,
                section,
                fact,
                head: fact
                    .fields
                    .get(document::CONTINUES)
                    .map(String::as_str)
                    .filter(|_| facts_continue(fact)),
            })
        })
        .collect();
    held.sort_by(|a, b| (&a.order, &a.fact.id.0).cmp(&(&b.order, &b.fact.id.0)));
    let heads: std::collections::BTreeSet<&str> = held
        .iter()
        .filter(|h| h.head.is_none())
        .map(|h| h.fact.id.0.as_str())
        .collect();
    held.retain(|h| h.head.is_none_or(|head| heads.contains(head)));
    held
}

/// **The markdown of one live section**, joined from its parts. This is the one
/// join of a section's text: `read_document` pages over it and the web page
/// concatenates it, so the two cannot disagree about what a document says.
pub(crate) fn section_text(held: &Held<'_>) -> String {
    document::join(std::slice::from_ref(&held.section))
}

/// **A document's whole live text**, as `read_document` returns it when it
/// fits in one answer: the live sections in reading order, each joined by
/// [`section_text`]. For a reader that is not a verb, such as the web page.
pub fn live_text(facts: &[Fact]) -> String {
    live_sections(facts).iter().map(section_text).collect()
}

fn facts_continue(fact: &Fact) -> bool {
    fact.fields
        .get(document::CONTINUES)
        .is_some_and(|value| !value.is_empty() && value != "false")
}

/// **The positions of a section and of every chunk that carries it on**, in
/// reading order. A position that names a chunk stands for the section it
/// carries on.
pub(crate) fn run_of(live: &[Held<'_>], at: usize) -> Vec<usize> {
    let head = match live[at].head {
        Some(head) => live.iter().position(|h| h.fact.id.0 == head).unwrap_or(at),
        None => at,
    };
    let id = live[head].fact.id.0.as_str();
    (0..live.len())
        .filter(|n| *n == head || live[*n].head == Some(id))
        .collect()
}

/// **Where a moved section goes**: the order key for the section itself and for
/// each chunk that carries it on, which move with it.
pub(crate) struct MovePlan {
    pub(crate) head_order: String,
    pub(crate) rest: Vec<(FactAddress, String)>,
}

/// **One write a restore makes.**
enum Step {
    /// A section that stands is edited back to its wording.
    Rewrite {
        address: FactAddress,
        patch: Box<FactPatch>,
    },
    /// A section that was archived since, or whose run is re-made whole, comes
    /// back as a new record that names the one it restores. An archived claim has
    /// no un-archive.
    Recreate {
        old: FactAddress,
        new: Box<NewFact>,
        /// `None` for a section's heading. For a chunk that carries it on: the
        /// heading's record when that stands, or `Some(None)` for the heading
        /// this restore has just written.
        continues: Option<Option<String>>,
    },
    /// A section made since is archived.
    Archive { address: FactAddress },
}

/// **What a restore would write**, worked out before any write, with the
/// sections it would change whose wording is the operator's own.
#[derive(Default)]
struct RestorePlan {
    steps: Vec<Step>,
    /// The sections, by the address they have in the record of that day or now,
    /// that a change would touch and whose current or restored wording is
    /// testimony.
    testimony: Vec<FactAddress>,
    /// The records the plan touches, to name any whose state rests on a write
    /// with no moment.
    touched: Vec<FactId>,
}

/// Whether a record about to be written is a section's heading and not a chunk
/// that carries one on.
fn continues_none(new: &NewFact) -> bool {
    !new.fields.contains_key(document::CONTINUES)
}

/// **A write's outcome as the fact it landed**, or the answer the caller gives
/// for it. A fold that could not refresh after a landed write still landed.
fn landed_fact(
    outcome: Result<Guarded<Fact>, MemoryError>,
    fold_behind: &mut Option<jojobot_domain::memory::search::Behind>,
) -> Result<Fact, Result<CallToolResult, McpError>> {
    match outcome {
        Ok(Guarded::Written(fact)) => Ok(fact),
        Err(MemoryError::FoldBehind {
            landed: Landed::Fact(fact),
            behind,
            ..
        }) => {
            *fold_behind = Some(behind);
            Ok(*fact)
        }
        Ok(Guarded::Blocked {
            attempted,
            candidates,
        }) => Err(Ok(blocked_result(
            &attempted,
            &candidates,
            Blocked::MustExist("put_document"),
        ))),
        Err(e) => Err(memory_declined("put_document", e)),
    }
}

fn is_testimony(held: &Held<'_>) -> bool {
    held.fact.provenance == Provenance::Testimony
}

/// The keys two records of one section are compared by. **A chunk's link to its
/// heading is left out when the records are two: the copy of a chunk points at
/// the copy of its heading, so the link differs and says nothing.**
fn compared_keys(same_record: bool) -> impl Iterator<Item = &'static str> {
    SECTION_KEYS
        .into_iter()
        .filter(move |key| same_record || *key != document::CONTINUES)
}

/// Whether two records of one section read the same: its words and the keys a
/// section is read back from.
fn same_section(then: &Fact, now: &Fact) -> bool {
    let same_record = then.id == now.id;
    then.content == now.content
        && then.details.as_deref().unwrap_or("") == now.details.as_deref().unwrap_or("")
        && compared_keys(same_record).all(|key| then.fields.get(key) == now.fields.get(key))
}

/// The edit that puts a standing section back to its wording of that day.
fn patch_back(then: &Fact, now: &Fact, session: &str) -> FactPatch {
    let mut patch = FactPatch {
        session: Some(session.to_string()),
        ..Default::default()
    };
    if then.content != now.content {
        patch.content = Some(then.content.clone());
    }
    if then.details.as_deref().unwrap_or("") != now.details.as_deref().unwrap_or("") {
        patch.details = Some(then.details.clone().unwrap_or_default());
    }
    for key in compared_keys(then.id == now.id) {
        match (then.fields.get(key), now.fields.get(key)) {
            (Some(was), held) if held != Some(was) => {
                patch.fields.insert(key.to_string(), was.clone());
            }
            (None, Some(_)) => patch.clear_fields.push(key.to_string()),
            _ => {}
        }
    }
    patch
}

/// The new record a section comes back as: the words, provenance and keys it had,
/// and the address of the record it restores.
fn restored_record(subject: &EntityId, then: &Fact, session: &str, day: Date) -> NewFact {
    let mut new = NewFact::about(subject.clone(), then.content.clone(), day);
    new.details = then.details.clone().filter(|details| !details.is_empty());
    new.provenance = then.provenance;
    new.standing = Some(then.standing);
    new.fields = then.fields.clone();
    new.fields.remove(document::CONTINUES);
    // **The copy says which version it reproduces**, and a copy of a copy says
    // the version it was asked to reproduce, so the mark is set after the
    // fields are taken from the section.
    new.fields.insert(
        document::RESTORED_FROM.to_string(),
        then.address().to_string(),
    );
    new.derived_from = Some(then.address());
    new.session = Some(session.to_string());
    new
}

/// **Work out what puts the document back as it stood.** `then` and `now` are the
/// live sections of that day and of today, in reading order. A section's run (the
/// heading and the chunks that carry it on) is restored whole or not at all.
fn plan_restore(
    subject: &EntityId,
    then: &[Held<'_>],
    now: &[Held<'_>],
    session: &str,
    day: Date,
) -> RestorePlan {
    let now_by_id: std::collections::HashMap<&str, usize> = now
        .iter()
        .enumerate()
        .map(|(at, held)| (held.fact.id.0.as_str(), at))
        .collect();
    // **A section restored before stands under a new record that says which version
    // it reproduces** (`restored_from`), so a second restore to the same moment finds
    // it there and has nothing to do. An archived claim has no un-archive, which is
    // why the copy is a record of its own.
    //
    // **Only that mark makes a record a restore's copy.** `derived_from` is also how a
    // writer says a record was worked out from another, so a record that names a
    // version there and carries no mark is never taken for its copy.
    let now_by_origin: std::collections::HashMap<String, usize> = now
        .iter()
        .enumerate()
        .filter_map(|(at, held)| {
            held.fact
                .fields
                .get(document::RESTORED_FROM)
                .map(|from| (from.clone(), at))
        })
        .collect();
    let counterpart = |was: &Held<'_>| -> Option<usize> {
        now_by_id
            .get(was.fact.id.0.as_str())
            .copied()
            .or_else(|| now_by_origin.get(&was.fact.address().to_string()).copied())
    };
    let mut plan = RestorePlan::default();
    let mut restored_heads = std::collections::BTreeSet::new();
    for at in (0..then.len()).filter(|n| then[*n].head.is_none()) {
        let run = run_of(then, at);
        let head_id = then[at].fact.id.0.as_str();
        restored_heads.insert(head_id);
        let head_now = counterpart(&then[at]);
        let now_run: Vec<usize> = match head_now {
            Some(head_at) => {
                restored_heads.insert(now[head_at].fact.id.0.as_str());
                run_of(now, head_at)
            }
            None => Vec::new(),
        };
        // A section whose wording is the operator's cannot be edited in place by
        // another session, so a run holding one is re-made whole.
        let remake = now_run.is_empty()
            || run.iter().any(|n| {
                let was = &then[*n];
                match counterpart(was) {
                    Some(held) => {
                        !same_section(was.fact, now[held].fact)
                            && (is_testimony(was) || is_testimony(&now[held]))
                    }
                    None => false,
                }
            });
        if remake {
            for n in &now_run {
                let held = &now[*n];
                plan.steps.push(Step::Archive {
                    address: held.fact.address(),
                });
                plan.touched.push(held.fact.id.clone());
                if is_testimony(held) {
                    plan.testimony.push(held.fact.address());
                }
            }
            for (place, n) in run.iter().enumerate() {
                let was = &then[*n];
                plan.steps.push(Step::Recreate {
                    old: was.fact.address(),
                    new: Box::new(restored_record(subject, was.fact, session, day)),
                    continues: (place > 0).then_some(None),
                });
                plan.touched.push(was.fact.id.clone());
                if is_testimony(was) {
                    plan.testimony.push(was.fact.address());
                }
            }
            continue;
        }
        for n in &run {
            let was = &then[*n];
            match counterpart(was) {
                Some(held) if same_section(was.fact, now[held].fact) => {}
                Some(held) => {
                    plan.steps.push(Step::Rewrite {
                        address: now[held].fact.address(),
                        patch: Box::new(patch_back(was.fact, now[held].fact, session)),
                    });
                    plan.touched.push(was.fact.id.clone());
                }
                None => {
                    // **The heading that STANDS carries the chunk on**, which is the
                    // copy of the heading when an earlier restore made one. The
                    // original is archived by then, and a chunk that continues an
                    // archived heading is never read.
                    let heading = head_now.map_or(head_id, |at| now[at].fact.id.0.as_str());
                    plan.steps.push(Step::Recreate {
                        old: was.fact.address(),
                        new: Box::new(restored_record(subject, was.fact, session, day)),
                        continues: Some(Some(heading.to_string())),
                    });
                    plan.touched.push(was.fact.id.clone());
                    if is_testimony(was) {
                        plan.testimony.push(was.fact.address());
                    }
                }
            }
        }
        let wanted: std::collections::BTreeSet<&str> = run
            .iter()
            .filter_map(|n| counterpart(&then[*n]))
            .map(|held| now[held].fact.id.0.as_str())
            .collect();
        for n in &now_run {
            let held = &now[*n];
            if !wanted.contains(held.fact.id.0.as_str()) {
                plan.steps.push(Step::Archive {
                    address: held.fact.address(),
                });
                plan.touched.push(held.fact.id.clone());
                if is_testimony(held) {
                    plan.testimony.push(held.fact.address());
                }
            }
        }
    }
    // A section that stands now and did not stand then is archived: its heading
    // alone, because the chunks that carry it on go with it.
    for held in now.iter().filter(|held| held.head.is_none()) {
        if !restored_heads.contains(held.fact.id.0.as_str()) {
            plan.steps.push(Step::Archive {
                address: held.fact.address(),
            });
            plan.touched.push(held.fact.id.clone());
            if is_testimony(held) {
                plan.testimony.push(held.fact.address());
            }
        }
    }
    plan
}

/// **A document's records as they stood at one moment**, rebuilt so the same
/// reading code that serves today's page serves that one.
pub(crate) struct AsOf {
    /// The records that existed then, each carrying the wording, the archive
    /// state and the section keys it held then. A record made later is absent.
    pub(crate) facts: Vec<Fact>,
    /// The instant the answer is about.
    pub(crate) moment: jiff::Timestamp,
    /// The records whose state at that moment rests on a write the store kept
    /// with no moment of its own, and that the read therefore placed before
    /// every stamped one. It names them rather than vouching for them.
    pub(crate) unplaced: Vec<FactId>,
}

/// **The keys a section is read back from, besides its words.** What a section
/// held under them then is what places it and shapes its edges.
const SECTION_KEYS: [&str; 4] = [
    document::ORDER,
    document::LEVEL,
    document::EDGES,
    document::CONTINUES,
];

impl Jojobot {
    /// **The instant a caller's `as_of` names.** A day means the end of that
    /// day, in the zone the asking run works in, else the instance's: the
    /// document as it stood when that day was over. An instant is taken as given.
    pub(crate) async fn moment_of(
        &self,
        asked: &str,
        caller: Option<&crate::caller::Caller>,
    ) -> Result<jiff::Timestamp, MemoryError> {
        let asked = asked.trim();
        if let Ok(day) = asked.parse::<jiff::civil::Date>() {
            let zone = match caller.and_then(crate::caller::Caller::zone) {
                Some(zone) => zone,
                None => self.unzoned_frame().await,
            };
            return day
                .at(23, 59, 59, 999_999_999)
                .to_zoned(zone)
                .map(|end| end.timestamp())
                .map_err(|why| {
                    MemoryError::InvalidQuery(format!(
                        "as_of '{asked}' names a day this zone cannot place: {why}"
                    ))
                });
        }
        asked.parse::<jiff::Timestamp>().map_err(|_| {
            MemoryError::InvalidQuery(format!(
                "as_of is a day, YYYY-MM-DD, or an instant such as 2026-08-03T12:00:00Z, and \
                 '{asked}' is neither"
            ))
        })
    }

    /// **Put a document back as it stood at a moment.** Every section that
    /// differs gets a new write carrying its old wording, through the writes the
    /// verbs make; a section archived since comes back as a new record that names
    /// the archived one; a section made since is archived. Nothing is rewritten
    /// in the history.
    ///
    /// **All is checked before any is written.** A restore that would change a
    /// section whose wording is testimony is refused whole unless the call is
    /// confirmed, one to a moment the document held nothing is refused, and one
    /// that rests on a write with no moment is refused naming it. A store that
    /// stops partway leaves what landed, and a second call finishes: it works out
    /// what is left from what is stored.
    pub(crate) async fn restore_document(
        &self,
        subject: &EntityId,
        asked: &str,
        confirmed: bool,
        day: Date,
        caller: &crate::caller::Caller,
    ) -> Result<CallToolResult, McpError> {
        let refuse = |why: String| memory_declined("put_document", MemoryError::InvalidFact(why));
        let moment = match self.moment_of(asked, Some(caller)).await {
            Ok(moment) => moment,
            Err(e) => return memory_declined("put_document", e),
        };
        // Held from the read to the last write, for the reason a put holds it.
        let _placing = match self.hold_placing(subject).await {
            Ok(guard) => guard,
            Err(e) => return memory_declined("put_document", e),
        };
        let then = match self.facts_as_of(subject, moment).await {
            Ok(then) => then,
            Err(e) => return memory_declined("put_document", e),
        };
        let now_facts = match self.memory.recall(subject).await {
            Ok(facts) => facts,
            Err(e) => return memory_declined("put_document", e),
        };
        let then_live = live_sections(&then.facts);
        if then_live.is_empty() {
            return refuse(format!(
                "restore_to '{asked}' names a moment at which {subject} held no sections: it \
                 did not exist yet, or held none. A restore to it would archive every section, \
                 which is not a restoration. Name a later moment, and read it first with \
                 read_document and as_of"
            ));
        }
        let now_live = live_sections(&now_facts);
        let plan = plan_restore(subject, &then_live, &now_live, caller.sid.as_str(), day);

        let unplaced: Vec<String> = plan
            .touched
            .iter()
            .filter(|id| then.unplaced.contains(id))
            .map(|id| format!("{subject}#{}", id.0))
            .collect();
        if !unplaced.is_empty() {
            return refuse(format!(
                "restore_to '{asked}' would place {}, whose state at that moment rests on a \
                 write recorded before writes carried a moment, so no moment places it. \
                 Nothing was written. Restore the others by editing those sections with \
                 update_fact",
                unplaced.join(", ")
            ));
        }
        let mut testimony: Vec<String> = plan.testimony.iter().map(ToString::to_string).collect();
        testimony.sort();
        testimony.dedup();
        if !testimony.is_empty() && !confirmed {
            return refuse(format!(
                "restore_to '{asked}' would change {} whose wording is testimony, the \
                 operator's own words: {}. A restore may not put the operator's old words back \
                 over the current ones on its own say-so, because what replaced them may be \
                 their own correction. Nothing was written. Send confirmed_by_user: true once \
                 the operator has confirmed it",
                crate::answer::counted(testimony.len(), "section", "sections"),
                testimony.join(", ")
            ));
        }
        let known = match self.memory.list_entities(None).await {
            Ok(known) => known,
            Err(e) => return memory_declined("put_document", e),
        };
        for step in &plan.steps {
            if let Step::Recreate { new, .. } = step
                && let Some(refused) = jojobot_domain::memory::mention::refusal_for_words(
                    &new.content,
                    new.details.as_deref(),
                    &known,
                )
            {
                return memory_declined("put_document", refused);
            }
        }

        let before = self.as_it_stands(subject).await;
        let total = plan.steps.len();
        let mut rewritten: Vec<String> = Vec::new();
        let mut recreated: Vec<serde_json::Value> = Vec::new();
        let mut archived: Vec<String> = Vec::new();
        let mut heading_record: Option<String> = None;
        let mut fold_behind = None;
        for step in plan.steps {
            let written = match step {
                Step::Rewrite { address, patch } => {
                    let outcome = self.memory.update_fact(&address, *patch, &caller.bot).await;
                    landed_fact(outcome, &mut fold_behind)
                        .map(|_| rewritten.push(address.to_string()))
                }
                Step::Archive { address } => {
                    let patch = FactPatch {
                        status: Some(FactStatus::Archived),
                        session: Some(caller.sid.as_str().to_string()),
                        ..Default::default()
                    };
                    let outcome = self.memory.update_fact(&address, patch, &caller.bot).await;
                    landed_fact(outcome, &mut fold_behind)
                        .map(|_| archived.push(address.to_string()))
                }
                Step::Recreate {
                    old,
                    mut new,
                    continues,
                } => {
                    if let Some(link) = continues {
                        let heading = link.or_else(|| heading_record.clone()).expect(
                            "a chunk that carries a section on comes after the heading it follows",
                        );
                        new.fields.insert(document::CONTINUES.to_string(), heading);
                    }
                    let is_heading = continues_none(&new);
                    let outcome = self.memory.capture(*new).await;
                    landed_fact(outcome, &mut fold_behind).map(|fact| {
                        if is_heading {
                            heading_record = Some(fact.id.0.clone());
                        }
                        recreated.push(serde_json::json!({
                            "from": old.to_string(),
                            "to": fact.address().to_string(),
                        }));
                    })
                }
            };
            if let Err(refused) = written {
                let done = rewritten.len() + recreated.len() + archived.len();
                let to_finish = format!(
                    "{done} of {total} writes landed and stay. Send the same restore_to again: \
                     it works out what is left from what is stored and writes only that"
                );
                return super::documents::stopped_with(
                    refused,
                    serde_json::json!({
                        "rewritten": rewritten,
                        "recreated": recreated,
                        "archived": archived,
                        "not_written": total - done,
                        "to_finish": to_finish,
                    }),
                    format!("the store stopped partway: {to_finish}"),
                );
            }
        }

        self.beat("put_document", subject.as_str(), Some(caller.sid.as_str()))
            .await;
        let changed_count = rewritten.len() + recreated.len() + archived.len();
        let mut body = serde_json::json!({
            "document": subject.as_str(),
            "restored": {
                "asked": asked,
                "moment": moment.to_string(),
                "changed": changed_count,
                "rewritten": rewritten,
                "recreated": recreated,
                "archived": archived,
            },
        });
        let changed = match fold_behind {
            Some(_) => crate::changed::Changed::not_read(),
            None => self.changed_on(&before, subject).await,
        };
        crate::changed::note_changed(&mut body, changed);
        if let Some(behind) = fold_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        crate::answer::note_postcondition(
            &mut body,
            format!(
                "{subject} was put back as it stood at {moment}: {}. read_document returns it, \
                 and the history of every section still holds every version.",
                crate::answer::counted(changed_count, "write", "writes")
            ),
        );
        json_result(&body)
    }

    /// **Rebuild a document's records as they stood at `moment`.** Each record
    /// takes its newest write at or before the moment, and each section key its
    /// newest write at or before it. A record with no write by then did not exist
    /// and is left out; a record whose write by then was an archive is present
    /// and archived. A write with no moment is older than every write with one.
    pub(crate) async fn facts_as_of(
        &self,
        subject: &EntityId,
        moment: jiff::Timestamp,
    ) -> Result<AsOf, MemoryError> {
        let facts = self.memory.recall(subject).await?;
        let chains = self.memory.claim_histories(subject).await?;
        let trails = self.memory.key_trails(subject).await?;
        let by_then =
            |written_at: Option<jiff::Timestamp>| written_at.is_none_or(|at| at <= moment);
        // **A claim that came in through a merge made after `moment` was not on
        // this document then.** A merge moves claims and keeps their dates, so
        // their writes alone would place them here from the day they were made.
        // The merge event names the ids they wear under this document.
        let arrived_later: std::collections::HashSet<FactId> = self
            .memory
            .merge_events(Some(subject))
            .await?
            .into_iter()
            .filter(|event| &event.survivor == subject && event.merged_at > moment)
            .flat_map(|event| event.claims.into_iter().map(|(_, now)| now))
            .collect();
        let mut kept = Vec::with_capacity(facts.len());
        let mut unplaced = Vec::new();
        for mut fact in facts {
            if arrived_later.contains(&fact.id) {
                continue;
            }
            let Some(write) = chains
                .get(&fact.id)
                .and_then(|chain| chain.iter().rev().find(|write| by_then(write.written_at)))
            else {
                continue;
            };
            if write.written_at.is_none() {
                unplaced.push(fact.id.clone());
            }
            fact.content = write.content.clone();
            fact.details = write.details.clone();
            fact.status = write.status;
            fact.provenance = write.provenance;
            fact.standing = write.standing;
            for key in SECTION_KEYS {
                let held = trails
                    .record_writes
                    .iter()
                    .filter(|w| w.fact == fact.id && w.key == key && by_then(w.written_at))
                    .max_by_key(|w| w.ordinal)
                    .and_then(|w| w.value.clone());
                match held {
                    Some(value) => {
                        fact.fields.insert(key.to_string(), value);
                    }
                    None => {
                        fact.fields.remove(key);
                    }
                }
            }
            kept.push(fact);
        }
        Ok(AsOf {
            facts: kept,
            moment,
            unplaced,
        })
    }

    /// **Where an address a caller holds reads now**: moved by the merge walk when
    /// a merge moved its claim, and homed under the handle its thing answers to.
    async fn served_address(&self, address: &FactAddress) -> Result<FactAddress, MemoryError> {
        let address = self
            .memory
            .moved_address(address)
            .await?
            .unwrap_or_else(|| address.clone());
        Ok(FactAddress::new(
            self.current_handle(&address.home).await?,
            address.local,
        ))
    }

    /// **Plan the move of the section at `address` behind the section `after`
    /// names.** Both have to be live sections of the same document. A section
    /// moves with the chunks that carry it on, and lands behind the target's own
    /// chunks, so a section is never cut in two by a move. A chunk is not moved
    /// on its own: the section it carries on is.
    pub(crate) async fn plan_move(
        &self,
        address: &FactAddress,
        after: &str,
    ) -> Result<MovePlan, MemoryError> {
        let refused = |what: String| {
            MemoryError::InvalidFact(format!(
                "`after` moves a section of a document to the place behind another section of \
                 the same document. {what}"
            ))
        };
        // **Both addresses are read where their claims are now**: under the
        // handle the document answers to today, and past a merge. A caller holds
        // an address from before a rename or a merge, and the sections below
        // are listed under the handle they wear now.
        let address = &self.served_address(address).await?;
        let target = self.served_address(&FactAddress::parse(after)?).await?;
        let facts = self.memory.recall(&address.home).await?;
        let live = live_sections(&facts);
        let is = |held: &Held<'_>, wanted: &FactAddress| {
            held.fact.home == wanted.home && held.fact.id == wanted.local
        };
        let Some(at) = live.iter().position(|held| is(held, address)) else {
            return Err(refused(format!(
                "{address} is not a live section: it is no part of a document, or it was \
                 archived. Edit an ordinary claim without `after`"
            )));
        };
        if live[at].head.is_some() {
            return Err(refused(format!(
                "{address} carries on the section before it. Move that section: this one \
                 moves with it"
            )));
        }
        let run = run_of(&live, at);
        let Some(target_at) = live.iter().position(|held| is(held, &target)) else {
            return Err(refused(format!(
                "{after} is not a live section of {}. Name the address of the section it \
                 should follow",
                address.home
            )));
        };
        if run.contains(&target_at) {
            return Err(refused("A section cannot follow itself.".to_string()));
        }
        let target_end = *run_of(&live, target_at)
            .last()
            .expect("a run holds its section");
        let remaining: Vec<&Held<'_>> = live
            .iter()
            .enumerate()
            .filter(|(n, _)| !run.contains(n))
            .map(|(_, held)| held)
            .collect();
        let behind = remaining
            .iter()
            .position(|held| is(held, &live[target_end].fact.address()))
            .expect("the target's last chunk is among the sections that stay");
        let orders = order::spread(
            Some(&remaining[behind].order),
            remaining.get(behind + 1).map(|held| held.order.as_str()),
            run.len(),
        )
        .map_err(|e| refused(format!("No room is left to place it there: {e}")))?;
        let mut orders = orders.into_iter();
        Ok(MovePlan {
            head_order: orders.next().expect("one order for the section itself"),
            // The chunks in the order they read now, which after a move that
            // stopped partway is not the order the heading sits in.
            rest: run
                .iter()
                .filter(|n| **n != at)
                .map(|n| live[*n].fact.address())
                .zip(orders)
                .collect(),
        })
    }
}

/// **A refusal that came after some writes had landed**, carrying what landed.
/// A refusal that is an answer gains the keys; one that is an error gains them
/// in its data, and its stock words, which say nothing was written, are replaced
/// by `words` because here they would be false.
pub(crate) fn stopped_with(
    refused: Result<CallToolResult, McpError>,
    carried: serde_json::Value,
    words: String,
) -> Result<CallToolResult, McpError> {
    match refused {
        Ok(refused) => {
            let mut body = refused
                .content
                .first()
                .and_then(|block| block.as_text())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text.text).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            body.as_object_mut()
                .expect("a refusal body is an object")
                .extend(carried.as_object().cloned().unwrap_or_default());
            crate::answer::json_result(&body)
        }
        Err(error) => {
            let mut data = error.data.unwrap_or_else(|| serde_json::json!({}));
            if let (Some(data), Some(carried)) = (data.as_object_mut(), carried.as_object()) {
                data.extend(carried.clone());
            }
            Err(McpError::new(error.code, words, Some(data)))
        }
    }
}

#[cfg(test)]
mod tests;
