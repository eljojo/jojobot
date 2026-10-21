//! **Who writes a thread, decided once** (rule 598).
//!
//! A thread's ceilings bind the bots that write into it, and a key is licensed
//! to a bot above one of them. A bot writes into a thread by keeping a thought
//! that draws a connection at it, or by writing one of its live records. Every
//! gate that asks who writes a thread asks here: the merge guard, the write that
//! widens a ceiling, and the lineage each store reads inside a write. Two answers
//! in two layers would let a refusal name a route its own gate then refuses.

use crate::memory::{EntityId, Fact, FactId, FactStatus, GUARDED_KEYS, capacity};

/// **The records of a thing that ever carried a guarded key**, from the writes a
/// store keeps of each guarded key: each write names the record that carried it,
/// so a record that put a ceiling there and later took it off is still the
/// setting of it.
pub fn settings_among(
    guarded_writes: impl IntoIterator<Item = FactId>,
) -> std::collections::BTreeSet<FactId> {
    guarded_writes.into_iter().collect()
}

/// **Whether a record is a fill of its thing**: a live record that takes a slot
/// and is not the setting of a guarded key. Its first writer is a writer of the
/// thing. A record that sets a ceiling is the setting of it, now or once, and not
/// a fill, so the bot that set a ceiling and took it off again is not made a
/// writer by it; a record that takes no slot is not a fill either. `settings` is
/// [`settings_among`] the thing's writes.
pub fn fills(fact: &Fact, settings: &std::collections::BTreeSet<FactId>) -> bool {
    fact.status == FactStatus::Active
        && !capacity::takes_no_slot(&fact.fields)
        && !GUARDED_KEYS.iter().any(|rule| fact.sets.contains(rule.key))
        && !settings.contains(&fact.id)
}

/// **The bots that write into a thread**: the ones that kept a thought in it,
/// then the ones that wrote one of its fills, each once. A bot that is archived
/// writes nothing, so a caller passes only the bots that stand.
pub fn thread_writers(thoughts: &[EntityId], fills: &[EntityId]) -> Vec<EntityId> {
    let mut writers: Vec<EntityId> = Vec::new();
    for bot in thoughts.iter().chain(fills) {
        if !writers.contains(bot) {
            writers.push(bot.clone());
        }
    }
    writers
}

/// **The bots above any of the writers**, from the chain above each, each once.
pub fn above_the_writers(chains: impl IntoIterator<Item = Vec<EntityId>>) -> Vec<EntityId> {
    let mut above: Vec<EntityId> = Vec::new();
    for bot in chains.into_iter().flatten() {
        if !above.contains(&bot) {
            above.push(bot);
        }
    }
    above
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{FactId, Provenance, Standing};
    use std::collections::BTreeSet;

    fn handle(text: &str) -> EntityId {
        EntityId(text.into())
    }

    fn record(on: &str, content: &str) -> Fact {
        Fact {
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            id: FactId("f1".into()),
            home: handle(on),
            subject: handle(on),
            content: content.into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 10, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
            receipt: None,
            reaching: Default::default(),
        }
    }

    /// **A live record is a fill; the setting of the number, an archived record
    /// and a record that takes no slot are not.** The one predicate every store
    /// and the mcp count writers by.
    #[test]
    fn a_fill_is_a_live_record_that_takes_a_slot_and_does_not_set_the_number() {
        let note = record("thread:milhouse-moves", "a note");
        let none = BTreeSet::new();
        assert!(fills(&note, &none));

        for key in [
            capacity::RECORD_CAPACITY,
            crate::memory::THOUGHT_CAPACITY,
            crate::memory::THOUGHT_AGES_AFTER_RUNS,
        ] {
            let mut setting = note.clone();
            setting.sets = BTreeSet::from([key.to_string()]);
            assert!(
                !fills(&setting, &none),
                "the setting of {key} is not a fill"
            );
        }
        let mut ordinary = note.clone();
        ordinary.sets = BTreeSet::from(["colour".to_string()]);
        assert!(
            fills(&ordinary, &none),
            "a record that sets an ordinary key is a fill"
        );

        let mut archived = note.clone();
        archived.status = FactStatus::Archived;
        assert!(!fills(&archived, &none), "an archived record is not a fill");

        let mut engine = note;
        engine
            .fields
            .insert(capacity::ENGINE_RECORD.to_string(), "1".into());
        assert!(
            !fills(&engine, &none),
            "a record that takes no slot is not a fill"
        );
    }

    /// **Thoughts first, then fills, each bot once**; and the bots above them
    /// likewise.
    #[test]
    fn the_writers_are_the_thought_writers_then_the_fill_writers_each_once() {
        let writers = thread_writers(
            &[handle("bot:marge"), handle("bot:bart")],
            &[handle("bot:bart"), handle("bot:lisa")],
        );
        assert_eq!(
            writers,
            vec![handle("bot:marge"), handle("bot:bart"), handle("bot:lisa")]
        );
        assert_eq!(
            above_the_writers([
                vec![handle("bot:otto")],
                vec![handle("bot:otto"), handle("bot:patty")],
            ]),
            vec![handle("bot:otto"), handle("bot:patty")]
        );
        assert!(thread_writers(&[], &[]).is_empty());
    }

    /// **A bot that wrote a live record of a thread is one of its writers, in the
    /// lineage a merge is judged by**, in the in-memory store as in the real one
    /// (rule 598). The merge refusal says a clear of the duplicate's ceiling opens
    /// only to a caller the clear would let through: lisa wrote a note on one
    /// duplicate and not on the other. The runs come from the store's own
    /// run-to-bot reader, which the shared contract cannot supply.
    #[tokio::test]
    async fn the_in_memory_store_counts_the_bots_that_wrote_a_record_among_the_writers() {
        use crate::memory::testing::InMemoryMemory;
        use crate::memory::{Edge, EdgeShape, Memory, MemoryError, NewEntity, NewFact};
        use crate::session::testing::InMemorySessions;
        use crate::session::{NewSession, Sessions, Sid};
        use std::sync::Arc;

        let runs = Arc::new(InMemorySessions::new());
        let memory = InMemoryMemory::booted().with_run_bots(runs.clone());
        for (id, name) in [
            ("bot:milhouse", "Milhouse"),
            ("bot:lisa", "Lisa"),
            ("thread:milhouse-moves", "Milhouse moves"),
            ("thread:simpson-kitchen", "Simpson kitchen"),
            ("thread:krusty-telethon", "Krusty telethon"),
            ("thread:alpha", "Alpha"),
            ("thread:beta", "Beta"),
        ] {
            memory
                .add_entity(NewEntity::new(handle(id), name, "case"))
                .await
                .expect("added")
                .written()
                .expect("not blocked");
        }
        for (bot, run) in [("bot:milhouse", "sa01"), ("bot:lisa", "sb01")] {
            runs.begin(NewSession {
                bot: handle(bot),
                sid: Sid(run.into()),
                focus: "writing".into(),
                started_at: jiff::Timestamp::UNIX_EPOCH,
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        }
        let day = jiff::civil::date(2026, 10, 1);
        memory
            .capture(NewFact {
                edge: Some(Edge::new(
                    EdgeShape::Connection,
                    handle("thread:milhouse-moves"),
                )),
                session: Some("sa01".into()),
                ..NewFact::about(handle("bot:milhouse"), "milhouse follows it", day)
            })
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
        for duplicate in ["thread:simpson-kitchen", "thread:krusty-telethon"] {
            memory
                .capture(NewFact {
                    sets: ["record_capacity".to_string()].into_iter().collect(),
                    fields: [("record_capacity".to_string(), "5".to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(handle(duplicate), "how many notes it holds", day)
                })
                .await
                .expect("captured")
                .written()
                .expect("not blocked");
        }
        memory
            .capture(NewFact {
                session: Some("sb01".into()),
                ..NewFact::about(handle("thread:simpson-kitchen"), "a note from lisa", day)
            })
            .await
            .expect("captured")
            .written()
            .expect("not blocked");

        let clear_opens = |duplicate: &str| {
            let memory = &memory;
            let duplicate = handle(duplicate);
            async move {
                match memory
                    .merge(
                        &duplicate,
                        &handle("thread:milhouse-moves"),
                        None,
                        jiff::civil::date(2026, 10, 2),
                        &handle("bot:lisa"),
                        None,
                    )
                    .await
                {
                    Err(MemoryError::MergeCarriesGuardedKeys { clear_opens, .. }) => clear_opens,
                    other => panic!("the merge was expected to be refused: {:?}", other.is_ok()),
                }
            }
        };
        assert!(
            !clear_opens("thread:simpson-kitchen").await,
            "lisa wrote a note there, so the clear is refused to lisa and is not named"
        );
        assert!(
            clear_opens("thread:krusty-telethon").await,
            "lisa wrote nothing there, so the clear opens"
        );

        // ── lisa set a ceiling on another thread through a record and took it off ──
        // That record is the setting of the ceiling, once or now, and not a fill, so
        // lisa is no writer there and may carry a ceiling onto it.
        let alpha = handle("thread:alpha");
        let setting = memory
            .capture(NewFact {
                sets: ["thought_capacity".to_string()].into_iter().collect(),
                fields: [("thought_capacity".to_string(), "3".to_string())]
                    .into_iter()
                    .collect(),
                session: Some("sb01".into()),
                ..NewFact::about(
                    alpha.clone(),
                    "how many thoughts it holds",
                    jiff::civil::date(2026, 10, 3),
                )
            })
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
        memory
            .update_fact(
                &setting.address(),
                crate::memory::FactPatch {
                    clear_fields: vec!["thought_capacity".to_string()],
                    ..Default::default()
                },
                &handle("bot:lisa"),
            )
            .await
            .expect("the ceiling is taken off")
            .written()
            .expect("not blocked");
        let beta = handle("thread:beta");
        memory
            .capture(NewFact {
                sets: ["record_capacity".to_string()].into_iter().collect(),
                fields: [("record_capacity".to_string(), "5".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    beta.clone(),
                    "how many notes it holds",
                    jiff::civil::date(2026, 10, 3),
                )
            })
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
        memory
            .merge(
                &beta,
                &alpha,
                None,
                jiff::civil::date(2026, 10, 4),
                &handle("bot:lisa"),
                None,
            )
            .await
            .expect("lisa wrote no record of her own on alpha, so she may carry a ceiling onto it");
    }
}
