//! "I filed somebody who turns out not to belong in here at all. I don't
//! want them cluttering the list every time I browse, but if I ever come
//! back to their handle I want the record still there."
//!
//! Archiving is not the repair for a duplicate — that is `merge_entities`,
//! folding two handles back into one thing. This is for a handle that was
//! never a duplicate of anything: it should simply stop turning up by
//! default. One way, and it never touches what was actually said — a
//! session naming the exact handle still reads the whole record, reason and
//! all.

use serde_json::json;

use super::dsl::{Session, Story};

/// **The child of an archived-and-restored parent, checked at one moment.** It
/// is listed, it is found under its parent, and its own record still names the
/// parent. Asked before the archive, during it and after the restore, so that
/// "its children exactly as they were" compares three moments and is not one
/// read that would pass whatever the restore did: archiving a parent touches no
/// child, and a build that hid or detached them would fail the middle one.
async fn the_child_is_where_it_was(s: &Session, when: &str) {
    s.list("work").await.says("work:phi");
    s.call("recall", json!({"parent": "project:atlas"}))
        .await
        .says("work:phi");
    let child = s.recall("work:phi").await.json()["objects"][0].clone();
    assert_eq!(child["parent"], "project:atlas", "{when}: {child}");
    assert_eq!(child["name"], "Phi", "{when}: {child}");
}

#[tokio::test]
async fn a_mistaken_entry_drops_out_of_the_default_list_and_stays_reachable_by_name() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // Two people on file — one turns out not to belong here, one is
    // ordinary and stays exactly as it was.
    s.add("person:apu", "Apu").await;
    s.fact("person:apu", "came up once in passing").await;
    s.add("person:skinner", "Skinner").await;
    s.fact("person:skinner", "runs the school").await;

    // ── the positive: browsing lists both, before anything is archived ───────
    s.list("person")
        .await
        .says("person:apu")
        .says("person:skinner");

    // ── archiving names the handle and the reason, and the receipt echoes
    //    both back ───────────────────────────────────────────────────────────
    let archived = s
        .call(
            "archive_entity",
            json!({
                "handle": "person:apu",
                "reason": "came up once, never actually part of anything I'm tracking",
            }),
        )
        .await;
    archived.says("person:apu");
    archived.says("came up once, never actually part of anything I'm tracking");

    // ── the positive `handle` rests on: only the named handle drops out ──────
    //
    // Skinner was never named in the call, so Skinner is exactly where he
    // was — a build that archived the wrong entity, or archived everything
    // in reach, fails this rather than the assertion on Apu alone.
    let after = s.list("person").await;
    after.never_says("person:apu");
    after.says("person:skinner");

    // ── the positive `reason` rests on: the claims stand, only the subject
    //    is out of scope ─────────────────────────────────────────────────────
    //
    // Archiving the entity never touches what was said about it — the
    // fact from before the archive is still there, and the reason the
    // archive itself carried reads back beside it.
    let apu = s.recall("person:apu").await;
    apu.says("came up once in passing");
    apu.says("came up once, never actually part of anything I'm tracking");

    // Skinner's own record is untouched throughout.
    s.recall("person:skinner")
        .await
        .says("runs the school")
        .never_says("came up once");

    s.wrap("archived the one who never belonged, left the other alone")
        .await;

    story.finish().await;
}

/// "I archived it on bad advice. It was fine. Put it back."
///
/// Restoring is the same verb with `restore: true`. The thing returns to the
/// default list with its claims and its children exactly as they were, and a
/// claim on it records both acts, so a later reader can tell it was out of the
/// list for a while and why it came back.
#[tokio::test]
async fn an_archived_thing_is_put_back_with_its_records_and_the_history_of_both_acts() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add_under("project:atlas", "work:phi", "Phi").await;
    s.fact("project:atlas", "the plan for the launch").await;
    s.list("project").await.says("project:atlas");
    the_child_is_where_it_was(&s, "before the archive").await;

    s.call(
        "archive_entity",
        json!({"handle": "project:atlas", "reason": "a mistaken filing"}),
    )
    .await;
    s.list("project").await.never_says("project:atlas");
    the_child_is_where_it_was(&s, "while its parent is archived").await;

    // Asking to restore what was never archived is refused, and says nothing was written.
    s.refused(
        "archive_entity",
        json!({"handle": "work:phi", "reason": "just checking", "restore": true}),
    )
    .await
    .says("work:phi");

    let restored = s
        .call(
            "archive_entity",
            json!({
                "handle": "project:atlas",
                "reason": "the filing was fine after all",
                "restore": true,
            }),
        )
        .await;
    restored.says("project:atlas").says("\"restored\":true");

    // Back in the default list, with its claim and its child as they were.
    s.list("project").await.says("project:atlas");
    let atlas = s.recall("project:atlas").await;
    atlas.says("the plan for the launch");
    the_child_is_where_it_was(&s, "after the restore").await;

    // The history of both acts is on the thing: why it left, when, and why it came back.
    atlas
        .never_says("\"archived\":{")
        .says("a mistaken filing")
        .says("the filing was fine after all");

    story.finish().await;
}

/// "I browsed for people and got nothing back. Is nobody on file, or did the
/// list hide them?"
///
/// An empty answer names what it looked through, and what it left out: the
/// archived entities. The same browse once somebody live is on file returns
/// them and carries no such line.
#[tokio::test]
async fn an_empty_browse_says_what_it_looked_through_and_a_full_one_does_not() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:apu", "Apu").await;
    s.call(
        "archive_entity",
        json!({ "handle": "person:apu", "reason": "never belonged" }),
    )
    .await;

    // ── the empty browse names its population and the archive it left out ────
    let empty = s.list("person").await;
    empty.never_says("person:apu");
    empty.says("searched").says("archived");

    // ── the same browse with one live entity returns it, and no line ─────────
    s.add("person:skinner", "Skinner").await;
    let full = s.list("person").await;
    full.says("person:skinner");
    full.never_says("searched");

    s.wrap("browsed an empty list and read what it had looked through")
        .await;

    story.finish().await;
}
