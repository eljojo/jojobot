//! **An empty list of columns is the default columns, and a short list is still
//! refused** (rule 751, and the PM's answer on `q-932-blank-columns-refused`),
//! proven against any [`Memory`] adapter.

use super::support::{add, capture, state};
use super::*;

/// **A project's columns, written blank, are accepted and name no column, so the
/// board shows its default columns; a list that drops a shipped status is refused
/// all the same.** Paired in one case: a guard that refused every unlisted status
/// passes the second half alone, and one that skipped the check for any list
/// passes the first alone. Rule 751 keeps the key, holding an empty list.
pub async fn an_empty_columns_list_is_the_default_and_a_short_one_is_refused<M: Memory>(store: &M) {
    // **The shipped kinds declare their keys first**, as a started server has: the
    // project kind is what makes `columns` a list, and so a blank in it a value.
    crate::memory::kinds::seed(store)
        .await
        .expect("the shipped kinds are declared");
    let board = EntityId("project:contract-columns-empty".into());
    add(
        store,
        NewEntity::new(board.clone(), "Contract Columns Empty", "contract-fixture"),
    )
    .await;
    let columns_on = |columns: &str| {
        state(NewFact {
            fields: [("columns".to_string(), columns.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(board.clone(), "the columns", date(2026, 10, 1))
        })
    };

    let own = "inbox, someday, next, now, waiting, done";
    capture(store, columns_on(own)).await;
    capture(store, columns_on("")).await;
    let held = store.fields(&board).await.expect("the store answers");
    assert_eq!(
        held.get("columns").map(String::as_str),
        Some(""),
        "an emptied list keeps its key: {held:?}"
    );
    assert!(
        crate::memory::kinds::columns_of(&held).is_none(),
        "an empty list names no column, so the board uses its default columns: {held:?}"
    );

    let refused = store
        .capture(columns_on("someday, next, now, done"))
        .await
        .expect_err("a list without 'waiting' is still refused");
    assert!(
        matches!(refused, MemoryError::BreaksType { .. }),
        "expected BreaksType, got {refused:?}"
    );
    let held = store.fields(&board).await.expect("the store answers");
    assert_eq!(
        held.get("columns").map(String::as_str),
        Some(""),
        "the refused list left the empty list as it was: {held:?}"
    );
}
