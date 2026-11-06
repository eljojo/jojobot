//! **A clear changes the thing only when its claim holds the key** (the architect's
//! ruling, 2026-11-05), proven against any [`Memory`] adapter.

use super::support::{capture, edit, ensure};
use super::*;
use std::collections::BTreeMap;

fn mood(value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("mood".to_string(), value.to_string())])
}

/// **A clear sent to a claim whose write of the key is not the one the thing
/// holds withdraws only that claim's write and the thing keeps its value; a clear
/// sent to the claim that backs the value takes the key off the thing.** One
/// case, both halves: a store that refused or took every clear passes one half
/// alone. The older claim still holds the key on its own record, and a newer
/// claim holds the value the thing reads.
pub async fn a_clear_on_an_older_claim_leaves_the_value_and_one_on_the_backing_claim_takes_the_key_off<
    M: Memory,
>(
    store: &M,
) {
    let thing = EntityId("thing:contract-clear-older".into());
    ensure(store, &thing).await;
    let older = capture(
        store,
        NewFact {
            fields: mood("content"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(thing.clone(), "felt content", date(2026, 10, 1))
        },
    )
    .await;
    let newer = capture(
        store,
        NewFact {
            fields: mood("queasy"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(thing.clone(), "felt queasy", date(2026, 10, 2))
        },
    )
    .await;

    let clear = || FactPatch {
        clear_fields: vec!["mood".to_string()],
        ..FactPatch::default()
    };
    let withdrawn = edit(store, &older.address(), clear()).await;
    assert!(
        !withdrawn.fields.contains_key("mood"),
        "the older claim no longer carries the key: {withdrawn:?}"
    );
    let held = store.fields(&thing).await.expect("fields read");
    assert_eq!(
        held.get("mood").map(String::as_str),
        Some("queasy"),
        "the newer value survived the clear on the older claim: {held:?}"
    );

    // This record names no setting: its mood is a Stays write, so its clear
    // cannot touch the newer setting even though both keys have the same name.
    let own = capture(
        store,
        NewFact {
            fields: mood("rested"),
            ..NewFact::about(thing.clone(), "a label on this record", date(2026, 10, 3))
        },
    )
    .await;
    assert!(!own.reaching.contains("mood"), "the label stays: {own:?}");
    let cleared = edit(store, &own.address(), clear()).await;
    assert!(!cleared.fields.contains_key("mood"), "{cleared:?}");
    assert_eq!(store.fields(&thing).await.expect("fields read"), held);

    // **A claim archived while it holds the key stops counting**: the newest
    // write that still counts holds the key again, so an archive that left the
    // claim's write in the fold would leave "rested" here.
    let third = capture(
        store,
        NewFact {
            fields: mood("rested"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(thing.clone(), "felt rested", date(2026, 10, 4))
        },
    )
    .await;
    assert_eq!(
        store.fields(&thing).await.expect("fields read")["mood"],
        "rested",
        "the newest claim holds the key"
    );
    edit(
        store,
        &third.address(),
        FactPatch {
            status: Some(FactStatus::Archived),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(
        store.fields(&thing).await.expect("fields read")["mood"],
        "queasy",
        "the archived claim's write stopped counting"
    );

    edit(store, &newer.address(), clear()).await;
    let held = store.fields(&thing).await.expect("fields read");
    assert!(
        !held.contains_key("mood"),
        "a clear on the backing claim takes the key off: {held:?}"
    );

    // **The holder clears first, and the older claim's value does not return.**
    // The older claim still carries the key on its own record, so a fold that
    // ranked records, or that fell back to the next write, would give "content"
    // here.
    let other = EntityId("thing:contract-clear-holder-first".into());
    ensure(store, &other).await;
    capture(
        store,
        NewFact {
            fields: mood("content"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(other.clone(), "felt content", date(2026, 10, 1))
        },
    )
    .await;
    let holder = capture(
        store,
        NewFact {
            fields: mood("queasy"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(other.clone(), "felt queasy", date(2026, 10, 2))
        },
    )
    .await;
    edit(store, &holder.address(), clear()).await;
    let held = store.fields(&other).await.expect("fields read");
    assert!(
        !held.contains_key("mood"),
        "an older claim's value does not return once the holder clears: {held:?}"
    );
}
