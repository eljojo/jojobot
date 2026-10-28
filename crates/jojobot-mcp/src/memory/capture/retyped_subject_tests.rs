use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// A `thing` retyped to a `list`: the stored kind is list, and the handle it
/// wore before is `thing:delta`.
async fn a_list_that_used_to_be_a_thing(jojobot: &Jojobot) {
    ensure(jojobot, "thing:delta").await;
    let renamed = json_of(
        &jojobot
            .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
                request_id: None,
                handle: "thing:delta".into(),
                to: "list:homer-chores".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("rename ok"),
    );
    assert_ne!(renamed["status"], "blocked", "{renamed}");
}

/// **A tick sent to the handle a list used to wear is still a tick on a list.**
/// The kind the tick gate judges is the one the store holds, not the one the
/// caller's spelling carries.
#[tokio::test]
async fn a_tick_through_a_lists_former_handle_ticks_the_entry() {
    let jojobot = handler();
    a_list_that_used_to_be_a_thing(&jojobot).await;
    let entry = capture_ok(&jojobot, capture_args("list:homer-chores", "pay the bill")).await;
    let address = address_of(&entry);

    let ticked = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                tick: Some(TickArg {
                    entry: address,
                    act: "done".into(),
                }),
                ..capture_args("thing:delta", "done")
            }))
            .await
            .expect("capture ok"),
    );
    assert_ne!(ticked["status"], "blocked", "{ticked}");
    assert_eq!(ticked["ticked"]["act"], "done", "{ticked}");
}

/// **A batch of ticks sent to a former handle is judged on the stored kind too.**
#[tokio::test]
async fn a_batch_of_ticks_through_a_lists_former_handle_ticks_the_entries() {
    let jojobot = handler();
    a_list_that_used_to_be_a_thing(&jojobot).await;
    let entry = capture_ok(&jojobot, capture_args("list:homer-chores", "pay the bill")).await;
    let address = address_of(&entry);

    let ticked = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                content: String::new(),
                ticks: Some(vec![TickArg {
                    entry: address,
                    act: "done".into(),
                }]),
                ..capture_args("thing:delta", "")
            }))
            .await
            .expect("capture ok"),
    );
    assert_ne!(ticked["status"], "blocked", "{ticked}");
}

/// **A capture does not list every entity to read the kind of its subject.** The
/// store lists them once for the possible-subject note after the write; reading
/// the kind for the gates must add no listing of its own, because the kind of an
/// ordinary subject is its handle's own and a listing costs the size of the store.
#[tokio::test]
async fn a_capture_reads_no_listing_for_the_kind_of_its_subject() {
    let store = std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::booted());
    let jojobot = handler_over(store.clone());
    for handle in ["person:alpha", "person:beta", "person:delta"] {
        ensure(&jojobot, handle).await;
    }
    let before = store.listings();
    let landed = capture_ok(&jojobot, capture_args("person:alpha", "was at the party")).await;
    assert_ne!(landed["status"], "blocked", "{landed}");
    assert_eq!(
        store.listings() - before,
        1,
        "only the possible-subject note lists after the write"
    );
}

/// **An empty-worded capture through a list's former handle is told the four ways
/// to write to a list**, not the plain refusal for empty words: the gate reads the
/// stored kind.
#[tokio::test]
async fn empty_words_through_a_lists_former_handle_are_told_the_list_routes() {
    let jojobot = handler();
    a_list_that_used_to_be_a_thing(&jojobot).await;
    let refused = json_of(
        &jojobot
            .capture(Parameters(capture_args("thing:delta", "")))
            .await
            .expect("a refusal is an answer"),
    );
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("entries")),
        "{refused}"
    );
}

fn a_tick_batch(subject: &str, address: &str, acts: &[&str]) -> CaptureArgs {
    CaptureArgs {
        content: String::new(),
        ticks: Some(
            acts.iter()
                .map(|act| TickArg {
                    entry: address.into(),
                    act: (*act).into(),
                })
                .collect(),
        ),
        ..capture_args(subject, "")
    }
}

/// **A batch through a former handle is judged in order, as through the current
/// one.** Two `done` ticks on one entry cannot stand, so the batch is refused whole
/// and nothing lands; `done` then `undone` can, and lands.
#[tokio::test]
async fn a_batch_through_a_lists_former_handle_is_judged_in_order() {
    for subject in ["list:homer-chores", "thing:delta"] {
        let jojobot = handler();
        a_list_that_used_to_be_a_thing(&jojobot).await;
        let entry = capture_ok(&jojobot, capture_args("list:homer-chores", "pay the bill")).await;
        let address = address_of(&entry);

        let twice = json_of(
            &jojobot
                .capture(Parameters(a_tick_batch(
                    subject,
                    &address,
                    &["done", "done"],
                )))
                .await
                .expect("a refusal is an answer"),
        );
        assert_eq!(twice["status"], "blocked", "{subject}: {twice}");
        assert_eq!(twice["wrote"], false, "{subject}: {twice}");

        let there_and_back = json_of(
            &jojobot
                .capture(Parameters(a_tick_batch(
                    subject,
                    &address,
                    &["done", "undone"],
                )))
                .await
                .expect("capture ok"),
        );
        assert_ne!(
            there_and_back["status"], "blocked",
            "{subject}: {there_and_back}"
        );
    }
}
