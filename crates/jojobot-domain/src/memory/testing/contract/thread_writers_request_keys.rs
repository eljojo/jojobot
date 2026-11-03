//! **A record that sets a bot request's key is an ordinary fill on a thread**,
//! proven against any [`Memory`] adapter.
//!
//! The keys that guard a bot request mean something on a request alone, so the
//! ceiling keys leave them out ([`crate::memory::ceiling_keys`]). Each store reads
//! the writes of a thread's ceiling keys to tell the setting of a ceiling from a
//! fill; a store that counted a request key there would take a filing bot for a
//! setter, and the thread would have no writer to bind.

use super::support::{capture, ensure, state, state_patch};
use super::*;
use crate::memory::capacity::RECORD_CAPACITY;

/// **The bot that files a record setting a request key is a writer of the
/// thread**, for each of the three kinds of request key: a spec key, a status key
/// and the generation. Only a bot above a writer sets the thread's capacity, so a
/// bot that is not above the filler is refused and the filler's manager is not.
/// The refusal of the outsider is paired with the landing of the manager's write.
pub async fn a_record_setting_a_request_key_makes_its_filer_a_writer_of_the_thread<M: Memory>(
    store: &M,
) {
    let [filler, boss, outsider] = [
        "bot:contract-stocker",
        "bot:contract-overseer",
        "bot:contract-visitor",
    ]
    .map(|handle| EntityId(handle.into()));
    for id in [&filler, &boss, &outsider] {
        ensure(store, id).await;
    }
    capture(
        store,
        state(NewFact {
            fields: [(crate::memory::REPORTS_TO.to_string(), boss.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(filler.clone(), "reports up", date(2026, 10, 1))
        }),
    )
    .await;
    // One thread for each key, no two sharing a word: a near name is refused at
    // creation, and these are three cases of one shape.
    for (key, thread) in [
        ("desired", "thread:contract-lathe-shelf"),
        ("observed_state", "thread:contract-quarry-ledger"),
        ("generation", "thread:contract-harbor-dock"),
    ] {
        let thread = EntityId(thread.into());
        ensure(store, &thread).await;
        let room = capture(
            store,
            NewFact::about(thread.clone(), "the thread's room", date(2026, 10, 2)),
        )
        .await;
        capture(
            store,
            state(NewFact {
                fields: [(key.to_string(), "1".to_string())].into_iter().collect(),
                written_by_bot: Some(filler.clone()),
                session: Some("no-such-run".to_string()),
                ..NewFact::about(thread.clone(), "a record filed", date(2026, 10, 3))
            }),
        )
        .await;

        let capacity = |value: &str| {
            state_patch(FactPatch {
                fields: [(RECORD_CAPACITY.to_string(), value.to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            })
        };
        let refused = store
            .update_fact(&room.address(), capacity("5"), &outsider)
            .await
            .expect_err("an outsider does not set the capacity of a thread a bot filled");
        assert!(
            matches!(refused, MemoryError::KeyNotYours { key: ref held, .. } if held == RECORD_CAPACITY),
            "{key}: {refused:?}"
        );
        store
            .update_fact(&room.address(), capacity("5"), &boss)
            .await
            .unwrap_or_else(|e| panic!("{key}: the manager of the filler sets the capacity: {e:?}"))
            .written()
            .expect("nothing blocks it");
    }
}
