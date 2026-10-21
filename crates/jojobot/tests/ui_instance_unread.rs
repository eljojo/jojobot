//! **The operator's mail page tells an instance with no operator from an
//! instance whose record cannot be read** (rule 130).
//!
//! One server, three states of the same action. With no operator recorded the
//! action is a 404 that says there is no operator. With the instance's record
//! unreadable it is a 502, never that 404. Once an operator is recorded and the
//! record reads again, the same action lands.

use std::sync::Arc;

use jojobot_adapters::counted::Counted;

mod support;
use support::ui_board::{
    CLIENT_ID, READER, open_in_the_ui, seeded_board_over, seeded_memory, spawn_jojobot_over,
    with_an_operators_mail,
};
use support::{browser, log_in, spawn_idp};

#[tokio::test]
async fn an_unreadable_instance_record_is_not_an_instance_with_no_operator() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let counted = Arc::new(Counted::new(seeded_memory().await));
    let board = seeded_board_over(counted.clone()).await;
    let (addr, ct, board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let origin = format!("http://{addr}");

    // No operator recorded: the instance's record does not exist, and the action
    // says so.
    let absent = open_in_the_ui(addr, &cookie, "nobody-needs-this-id", Some(&origin)).await;
    assert_eq!(absent.status(), reqwest::StatusCode::NOT_FOUND);
    let said = absent.text().await.unwrap();
    assert!(said.contains("no operator"), "{said}");

    // The record cannot be read: a failed read, and never the answer above.
    counted.fail_fields_of(Some("topic:instance"));
    let unread = open_in_the_ui(addr, &cookie, "nobody-needs-this-id", Some(&origin)).await;
    assert_eq!(unread.status(), reqwest::StatusCode::BAD_GATEWAY);
    let said = unread.text().await.unwrap();
    assert!(!said.contains("no operator"), "{said}");

    // An operator recorded and the record readable: the same action lands.
    counted.fail_fields_of(None);
    let mail = with_an_operators_mail(&board).await;
    let opened = open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
    assert!(opened.status().is_redirection(), "{}", opened.status());
    ct.cancel();
}
