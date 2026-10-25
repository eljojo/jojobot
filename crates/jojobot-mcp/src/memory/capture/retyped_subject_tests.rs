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
