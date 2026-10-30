//! The typed pages read their data through the shared read, not through the
//! store: what a page shows is what the verb's answer carries.

mod support;

use support::ui_board::{CLIENT_ID, READER, read, seeded_board_over, seeded_memory, set_on};
use support::{browser, log_in, spawn_idp};

use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A store that cannot read the history of a key on one piece of work, when told
/// not to.
struct FailingHistory {
    inner: Arc<dyn Memory>,
    failing: AtomicBool,
}

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = inner)]
impl Memory for FailingHistory {
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        if entity.as_str() == "work:omicron" && self.failing.load(Ordering::SeqCst) {
            return Err(MemoryError::Store("history unavailable".into()));
        }
        self.inner.history(entity, key).await
    }
}

/// **A piece of work's page lists the writes of its status, newest first, and
/// takes them from the one answer it asks for.** Paired: the list is on the page,
/// so a page that dropped it cannot pass. An unreadable history refuses the page
/// as any other unreadable part of that answer does, and never leaves a task page
/// that looks as if nobody had written its status.
#[tokio::test]
async fn a_works_status_history_comes_with_the_pages_one_answer() {
    let memory = Arc::new(FailingHistory {
        inner: seeded_memory().await,
        failing: AtomicBool::new(false),
    });
    let work = EntityId("work:omicron".into());
    memory
        .inner
        .add_entity(NewEntity::new(
            work.clone(),
            "Raise the frame",
            "the fixture roster",
        ))
        .await
        .unwrap();
    for (word, day) in [("next", 10), ("now", 12)] {
        memory
            .inner
            .capture(set_on(NewFact {
                fields: [("status".to_string(), word.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    work.clone(),
                    "where the work stands",
                    jiff::civil::Date::constant(2026, 3, day),
                )
            }))
            .await
            .unwrap();
    }
    let board = seeded_board_over(memory.clone()).await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) =
        support::ui_board::spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/work:omicron/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let list = page
        .split("id=\"status-history\"")
        .nth(1)
        .expect("the page lists the writes of its status");
    let (now, next) = (
        list.find(">now<").expect("the write that holds it now"),
        list.find(">next<").expect("the write before it"),
    );
    assert!(now < next, "newest first: {list}");

    memory.failing.store(true, Ordering::SeqCst);
    let refused = read(&client, addr, "/work:omicron/", &cookie).await;
    assert_eq!(refused.status().as_u16(), 502);
    ct.cancel();
}
