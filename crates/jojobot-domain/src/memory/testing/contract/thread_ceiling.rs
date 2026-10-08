//! **Who may set a thread's ceiling, proven against any [`Memory`] adapter.**
//!
//! A thread is a container, and a ceiling is only a ceiling if raising it costs
//! the bots it binds something other than asking. The bots a thread's ceiling
//! binds are the ones that have written a thought into it: a claim on the bot
//! that draws a `connection` edge at the thread, whatever has become of the
//! claim since. Such a bot never changes the ceiling. A bot above one of them on
//! the chart does, and a bot on another chart does not.
//!
//! A bot's own ceiling stays what it was: any different identity may write it.
//! Every refusal here sits beside the same write made where it is allowed, so a
//! store that refused every write of the key would not pass.

use super::support::{capture, ensure};
use super::*;
use crate::memory::{Edge, EdgeShape, THOUGHT_CAPACITY};

/// **The ceiling of a thread is set above the bots that write into it.**
pub async fn a_threads_ceiling_is_set_only_above_the_bots_that_write_into_it<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-ceiling-line".into());
    let empty = EntityId("thread:contract-ceiling-unwritten".into());
    let writer = EntityId("bot:contract-ceiling-writer".into());
    let boss = EntityId("bot:contract-ceiling-boss".into());
    let head = EntityId("bot:contract-ceiling-head".into());
    let outsider = EntityId("bot:contract-ceiling-outsider".into());
    let rival = EntityId("bot:contract-ceiling-rival".into());
    for id in [&thread, &empty, &writer, &boss, &head, &outsider, &rival] {
        ensure(store, id).await;
    }
    for (who, to) in [(&writer, &boss), (&boss, &head), (&outsider, &rival)] {
        capture(
            store,
            NewFact {
                fields: [(crate::memory::REPORTS_TO.to_string(), to.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(who.clone(), "reports up", date(2026, 10, 1))
            },
        )
        .await;
    }
    let ceiling = |value: &str| FactPatch {
        fields: [(THOUGHT_CAPACITY.to_string(), value.to_string())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let clear = FactPatch {
        clear_fields: vec![THOUGHT_CAPACITY.to_string()],
        ..Default::default()
    };
    // The bots a refusal names as the ones that may make the write.
    let refused = |err: MemoryError, what: &str| -> Vec<String> {
        match err {
            MemoryError::KeyNotYours { key, allowed, .. } if key == THOUGHT_CAPACITY => allowed,
            other => panic!("{what}: expected KeyNotYours for the ceiling, got {other:?}"),
        }
    };
    let held_on = |id: EntityId| async move {
        store
            .fields(&id)
            .await
            .expect("readable")
            .get(THOUGHT_CAPACITY)
            .cloned()
    };
    let claim = capture(
        store,
        NewFact::about(thread.clone(), "the thread's ceiling", date(2026, 10, 2)),
    )
    .await;
    let unwritten = capture(
        store,
        NewFact::about(empty.clone(), "the thread's ceiling", date(2026, 10, 2)),
    )
    .await;

    // ── nobody has written into a thread: nothing is bound, any bot sets it ─
    store
        .update_fact(&unwritten.address(), ceiling("3"), &outsider)
        .await
        .expect("a thread nobody has written into takes a ceiling from any bot")
        .written()
        .expect("nothing blocks it");
    assert_eq!(held_on(empty.clone()).await, Some("3".to_string()));

    // ── the writer's thought makes the thread its own ───────────────────────
    let thought = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, thread.clone())),
            ..NewFact::about(writer.clone(), "still on for the spring", date(2026, 10, 3))
        },
    )
    .await;

    // ── the writer never changes it, and the refusal names who may ──────────
    let err = store
        .update_fact(&claim.address(), ceiling("50"), &writer)
        .await
        .expect_err("a bot bound by the ceiling does not set it");
    let allowed = refused(err, "the writer raising");
    assert!(allowed.contains(&boss.to_string()), "{allowed:?}");
    assert!(allowed.contains(&head.to_string()), "{allowed:?}");
    assert!(!allowed.contains(&writer.to_string()), "{allowed:?}");
    assert_eq!(held_on(thread.clone()).await, None);

    // ── a bot on another chart does not either ──────────────────────────────
    let err = store
        .update_fact(&claim.address(), ceiling("50"), &outsider)
        .await
        .expect_err("a bot above none of the writers does not set it");
    assert!(
        refused(err, "the outsider raising").contains(&boss.to_string()),
        "the refusal names the bots that may"
    );
    assert_eq!(held_on(thread.clone()).await, None);

    // ── the writer's superior raises it, and so does the head above ─────────
    store
        .update_fact(&claim.address(), ceiling("5"), &boss)
        .await
        .expect("the writer's manager sets the ceiling")
        .written()
        .expect("nothing blocks the manager");
    assert_eq!(held_on(thread.clone()).await, Some("5".to_string()));
    store
        .update_fact(&claim.address(), ceiling("8"), &head)
        .await
        .expect("a bot higher on the chain sets it too")
        .written()
        .expect("nothing blocks the head");
    assert_eq!(held_on(thread.clone()).await, Some("8".to_string()));

    // ── taking it off follows the same rule ─────────────────────────────────
    let err = store
        .update_fact(&claim.address(), clear.clone(), &writer)
        .await
        .expect_err("the writer does not take its own ceiling off");
    refused(err, "the writer clearing");
    let err = store
        .update_fact(&claim.address(), clear.clone(), &outsider)
        .await
        .expect_err("a bot on another chart does not take it off");
    refused(err, "the outsider clearing");
    assert_eq!(held_on(thread.clone()).await, Some("8".to_string()));

    // ── nor can the writer retract the claim that holds it ──────────────────
    let err = store
        .retract(
            &claim.address(),
            Some("a mistake"),
            date(2026, 10, 4),
            &writer,
        )
        .await
        .expect_err("the writer does not retract the claim that sets its ceiling");
    refused(err, "the writer retracting");
    assert_eq!(held_on(thread.clone()).await, Some("8".to_string()));

    // ── a thought taken out of the room leaves the bot a writer ─────────────
    //
    // Archiving a thought is how a full room makes way. If it also freed the bot
    // from the ceiling, a bot could archive, raise the ceiling and write again.
    store
        .update_fact(
            &thought.address(),
            FactPatch {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
            &writer,
        )
        .await
        .expect("a thought can be archived")
        .written()
        .expect("nothing blocks it");
    store
        .update_fact(&claim.address(), ceiling("500"), &writer)
        .await
        .expect_err("a bot that wrote into the thread stays bound after archiving");
    assert_eq!(held_on(thread.clone()).await, Some("8".to_string()));

    // ── a bot above a writer is bound too once it writes into the thread ────
    //
    // Being above a writer licenses nothing for a bot the ceiling binds.
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, thread.clone())),
            ..NewFact::about(head.clone(), "watching the spring", date(2026, 10, 4))
        },
    )
    .await;
    let err = store
        .update_fact(&claim.address(), ceiling("500"), &head)
        .await
        .expect_err("a bot above a writer is refused once it writes into the thread itself");
    assert!(
        refused(err, "the head raising").contains(&boss.to_string()),
        "the refusal names the bot that still may"
    );
    assert_eq!(held_on(thread.clone()).await, Some("8".to_string()));
    store
        .update_fact(&claim.address(), ceiling("6"), &boss)
        .await
        .expect("the manager, which writes nothing there, still sets it")
        .written()
        .expect("nothing blocks the manager");
    assert_eq!(held_on(thread.clone()).await, Some("6".to_string()));

    // ── the manager takes it off ────────────────────────────────────────────
    store
        .update_fact(&claim.address(), clear, &boss)
        .await
        .expect("the manager takes the ceiling off")
        .written()
        .expect("nothing blocks the manager");
    assert_eq!(held_on(thread.clone()).await, None);

    // ── a bot's own ceiling is still any different identity's to write ──────
    let own = capture(
        store,
        NewFact::about(writer.clone(), "the writer's room", date(2026, 10, 5)),
    )
    .await;
    store
        .update_fact(&own.address(), ceiling("4"), &writer)
        .await
        .expect_err("a bot does not set its own ceiling");
    store
        .update_fact(&own.address(), ceiling("4"), &outsider)
        .await
        .expect("any different identity sets a bot's ceiling")
        .written()
        .expect("nothing blocks it");
    assert_eq!(held_on(writer.clone()).await, Some("4".to_string()));
}
