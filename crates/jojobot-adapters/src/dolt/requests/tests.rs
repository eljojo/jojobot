use super::*;
use crate::dolt::memory::DoltMemory;
use crate::fold::Folded;
use crate::testing::{Scratch, free_port, migrated_database, start_unhurried};
use jojobot_domain::memory::{Memory, NewEntity, NewFact};
use jojobot_domain::request::{Request, Scope, armed, within};

/// A fill belongs to the commit of its call, even after the State has accepted
/// a newer attempt under the same id.
#[tokio::test]
async fn a_calls_fill_does_not_fill_a_newer_attempt() {
    let scratch = Scratch::new("receipt-attempt");
    let mut store = start_unhurried(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "receipt_attempt").await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let bot = EntityId("bot:alpha".into());
    memory
        .add_entity(NewEntity::new(bot.clone(), "Alpha", "fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    let memory = Folded::new(Arc::new(memory));
    let ledger = DoltRequests::new(pool.clone()).with_writer(memory.writer());
    let seam = ledger.seam().await.unwrap();
    let scope = Scope::new(Request {
        bot,
        id: RequestId::parse("alpha").unwrap(),
        verb: "capture".into(),
        digest: "first-digest".into(),
    });
    within(
        scope.clone(),
        armed(memory.capture(NewFact::about(
            scope.request().bot.clone(),
            "the first attempt wrote this",
            jiff::civil::date(2026, 11, 5),
        ))),
    )
    .await
    .unwrap()
    .written()
    .unwrap();
    assert!(scope.was_spent());
    ledger
        .fill(&scope.receipt(), "first-receipt")
        .await
        .unwrap();
    let old = crate::testing::ledger_events(&pool)
        .await
        .into_iter()
        .find_map(|event| match event {
            LedgerEvent::RequestCommitted(commit) => Some(commit),
            _ => None,
        })
        .expect("the call's actual commit key is in the log");
    assert_eq!(
        seam.read(|state| state
            .request(&old.request.key)
            .and_then(|held| held.committed.as_ref())
            .and_then(|commit| commit.receipt.clone())),
        Some("first-receipt".into())
    );

    let later = old.expires_at + jiff::SignedDuration::from_secs(1);
    let mut replacement = old.clone();
    replacement.request.key.generation =
        seam.read(|state| state.generation_for(&old.request.key.actor, &old.request.key.id, later));
    replacement.request.digest = "replacement-digest".into();
    replacement.landed = "replacement-result".into();
    replacement.receipt = None;
    replacement.written_at = later;
    replacement.expires_at = later + KEPT_FOR;
    ledger
        .append(vec![LedgerEvent::RequestCommitted(replacement.clone())])
        .await
        .unwrap();
    assert_eq!(
        seam.read(|state| state
            .request(&replacement.request.key)
            .and_then(|held| held.committed.clone())),
        Some(replacement.clone())
    );

    ledger
        .fill(&scope.receipt(), "late-old-receipt")
        .await
        .unwrap();
    assert_eq!(
        seam.read(|state| state
            .request(&replacement.request.key)
            .and_then(|held| held.committed.clone())),
        Some(replacement.clone()),
        "the old call must leave the newer attempt unchanged"
    );
    ledger
        .finish(&scope.receipt(), "late-old-completion")
        .await
        .unwrap();
    assert_eq!(
        seam.read(|state| state
            .request(&replacement.request.key)
            .and_then(|held| held.committed.clone())),
        Some(replacement),
        "the old call's completion must leave the newer attempt unchanged"
    );
    store.stop().await;
}

/// A resumed call cannot spend an expired generation after its last part has
/// left the keep window.
#[tokio::test]
async fn a_late_finish_does_not_complete_an_expired_part() {
    use jojobot_domain::clock::Clock;
    use jojobot_domain::request::armed_part;

    let scratch = Scratch::new("expired-part-finish");
    let mut store = start_unhurried(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "expired_part_finish")
        .await
        .unwrap();
    let written_on = jiff::civil::date(2026, 11, 5);
    let clock = |days| {
        Clock::stating(
            written_on
                .checked_add(jiff::Span::new().days(days))
                .unwrap(),
        )
    };
    let memory = DoltMemory::open(pool.clone()).on_clock(clock(0));
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let bot = EntityId("bot:alpha".into());
    let subject = EntityId("thing:beta".into());
    for (id, name) in [(&bot, "Alpha"), (&subject, "Beta")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    let memory = Folded::new(Arc::new(memory));
    let ledger = DoltRequests::new(pool.clone())
        .with_writer(memory.writer())
        .on_clock(clock(0));
    let request = Request {
        bot,
        id: RequestId::parse("expired-part").unwrap(),
        verb: "capture".into(),
        digest: "one-part".into(),
    };
    let first = Scope::new(request.clone());
    within(
        first,
        armed_part(
            "one",
            None,
            memory.capture(NewFact::about(subject, "one part", written_on)),
        ),
    )
    .await
    .unwrap()
    .written()
    .unwrap();
    let parts = ledger.parts(&request.bot, &request.id).await.unwrap();
    assert_eq!(parts.len(), 1, "the real write produced one part");
    let resumed = Scope::resuming(
        request.clone(),
        parts[0].key.clone(),
        parts
            .into_iter()
            .map(|part| (part.part, part.recorded))
            .collect(),
    );
    let late = DoltRequests::new(pool.clone())
        .with_writer(memory.writer())
        .on_clock(clock(4));
    assert!(
        late.parts(&request.bot, &request.id)
            .await
            .unwrap()
            .is_empty()
    );
    late.finish(&resumed.receipt(), "late-receipt")
        .await
        .unwrap();
    assert!(
        late.lookup(&request.bot, &request.id)
            .await
            .unwrap()
            .is_none(),
        "an expired part cannot become a fresh whole receipt"
    );
    assert!(
        !crate::testing::ledger_events(&pool)
            .await
            .iter()
            .any(|event| matches!(event, LedgerEvent::RequestCompleted { .. })),
        "finish must not write a completion for an expired part"
    );
    store.stop().await;
}
