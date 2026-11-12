//! **A page served by the shared board reads the filled image, not the store**
//! (decision log 292). A board that never fills the fold serves from the
//! store's own reads, so every page case would pass on a path the server never
//! takes once it has started.

use std::sync::Arc;

use jojobot_adapters::counted::Counted;

use crate::support;
use support::ui_board::{
    CLIENT_ID, READER, read, seeded_board_over, seeded_memory, spawn_jojobot_over,
};
use support::{browser, log_in, spawn_idp};

#[tokio::test]
async fn a_page_over_the_shared_board_issues_no_store_read() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let counted = Arc::new(Counted::new(seeded_memory().await));
    let board = seeded_board_over(counted.clone()).await;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    counted.reset();
    let page = read(&client, addr, "/person:alpha/", &cookie).await;
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let page = page.text().await.unwrap();

    assert!(
        page.contains("href=\"/person:alpha/topic:widgets/\""),
        "{page}"
    );
    assert_eq!(counted.calls(), 0, "store reads: {:?}", counted.seen());
    ct.cancel();
}
