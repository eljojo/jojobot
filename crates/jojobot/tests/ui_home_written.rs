//! The homepage orders its top level by the newest write of any kind on a thing,
//! as recall's `written_at` says it, and counts what sits inside a thing the way
//! recall counts it.

mod support;

use std::sync::Arc;

use jiff::civil::Date;
use jojobot_domain::memory::mention;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, FactPatch, Memory, NewEntity, NewFact};
use support::ui_board::{CLIENT_ID, READER, read, seeded_board_over, spawn_jojobot_over};
use support::{browser, log_in, spawn_idp};

const OLD: &str = "thing:kappa";
const NEW: &str = "thing:omega";

async fn claim_on(memory: &Arc<dyn Memory>, handle: &str, words: &str) -> NewFact {
    memory
        .add_entity(NewEntity::new(
            EntityId(handle.into()),
            handle,
            "the fixture roster",
        ))
        .await
        .expect("the thing is written");
    NewFact::about(EntityId(handle.into()), words, Date::constant(2026, 4, 1))
}

/// The page at `path` over `memory`.
async fn page(memory: Arc<dyn Memory>, path: &str) -> String {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let body = read(&client, addr, path, &cookie)
        .await
        .text()
        .await
        .unwrap();
    ct.cancel();
    body
}

fn place(page: &str, handle: &str) -> usize {
    page.find(&format!("href=\"/{handle}/\""))
        .unwrap_or_else(|| panic!("{handle} is listed: {page}"))
}

/// **A field written on an old claim is a newer write than a claim that was taken
/// in after it.** The thing whose claim was taken in first, and whose key was
/// written last, comes first when the top level is sorted by last write. Reading
/// only when each claim was taken in puts the other one first.
#[tokio::test]
async fn a_field_only_write_makes_a_thing_the_one_written_last() {
    let raw = InMemoryMemory::booted();
    let clock = raw.clock_handle();
    let memory: Arc<dyn Memory> = Arc::new(mention::Mentioning::new(Arc::new(raw)));
    clock.stating(Date::constant(2026, 4, 2));
    let first = claim_on(&memory, NEW, "taken in first").await;
    let taken = memory
        .capture(first)
        .await
        .expect("the claim is written")
        .written()
        .expect("the guard lets it through");
    clock.stating(Date::constant(2026, 4, 3));
    let second = claim_on(&memory, OLD, "taken in second").await;
    memory.capture(second).await.expect("the claim is written");
    clock.stating(Date::constant(2026, 4, 4));
    memory
        .update_fact(
            &taken.address(),
            FactPatch {
                fields: [("weight".to_string(), "heavy".to_string())].into(),
                ..Default::default()
            },
            &EntityId("bot:otto".into()),
        )
        .await
        .expect("the key is written on the old claim");

    let by_write = page(memory, "/?sort=written").await;
    assert!(
        place(&by_write, NEW) < place(&by_write, OLD),
        "the key written on {NEW}'s claim is the newest write: {by_write}"
    );
}
