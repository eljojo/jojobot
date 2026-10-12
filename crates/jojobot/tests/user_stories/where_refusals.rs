//! "I asked the board and it said no. What do I send instead?"
//!
//! **A refusal of a question over keys names the call that fixes it, and the
//! story makes that call.** Each case reads the refusal, makes the call it
//! names, and asserts the answer, so the advice is proven to work and not only
//! printed. The negative stands beside each: a refusal that says one thing does
//! not say it for a caller who made a different mistake.
//!
//! The board is the one `which_things` uses: `gamma` is open and due soon,
//! `delta` is open and due later, `epsilon` is done, `omega` holds no status.

use serde_json::json;

use super::dsl::{Answer, Session, Story};
use super::which_things::a_board;

/// The handles an answer named, in order.
fn handles_of(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", answer.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

/// A board whose keys no type declares, so an ordering on them is not licensed.
async fn an_undeclared_board(s: &Session) {
    for (handle, name, size, touches) in [
        ("thing:gamma", "Gamma", "10", "essay.rs, skills.rs"),
        ("thing:delta", "Delta", "2", "essay.rs"),
    ] {
        s.add(handle, name).await;
        s.event_with(
            handle,
            "a record",
            json!({ "size": size, "touches": touches }),
            &[],
        )
        .await;
    }
}

/// **A doubled equals sign and a quoted date are refused with what to write**, and
/// the call they name answers.
#[tokio::test]
async fn a_parse_refusal_names_the_spelling_that_works() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;

    s.refused(
        "recall",
        json!({ "kind": "thing", "where": "status == done" }),
    )
    .await
    .says("==");
    let fixed = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "status = done" }),
        )
        .await;
    assert_eq!(handles_of(&fixed), ["thing:epsilon"], "{}", fixed.raw());

    s.refused(
        "recall",
        json!({ "kind": "thing", "where": "due < \"2026-10-15\"" }),
    )
    .await
    .says("literal");
    let fixed = s
        .call(
            "recall",
            json!({ "kind": "thing", "where": "due < 2026-10-15" }),
        )
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:epsilon", "thing:gamma"],
        "{}",
        fixed.raw()
    );
    story.finish().await;
}

/// **A key held on records and not on the thing is refused with the scope that
/// asks it**, through a plain call and through a view. A misspelt key is refused
/// without that advice.
#[tokio::test]
async fn a_key_held_on_records_names_the_scope_that_asks_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    for (handle, name, colour) in [
        ("thing:gamma", "Gamma", "red"),
        ("thing:delta", "Delta", "blue"),
    ] {
        s.add(handle, name).await;
        // An own field: it stays on the record and never reaches the thing.
        s.call(
            "capture",
            json!({"subject": handle, "content": "painted", "provenance": "testimony",
                   "fields": {"colour": colour}}),
        )
        .await;
    }
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "marked-things", "name": "Red things",
               "source": "user-named",
               "sets": {"selects": "thing", "where": "colour = red"}}),
    )
    .await;

    // The plain call: the refusal names the scope, and that call answers.
    s.refused("recall", json!({"kind": "thing", "where": "colour = red"}))
        .await
        .says("scope")
        .says("record");
    let fixed = s
        .call(
            "recall",
            json!({"kind": "thing", "where": "colour = red", "scope": "record"}),
        )
        .await;
    assert_eq!(handles_of(&fixed), ["thing:gamma"], "{}", fixed.raw());

    // Through the view: the refusal names the view, and the scope beside it answers.
    s.refused("recall", json!({"view": "marked-things"}))
        .await
        .says("marked-things")
        .says("scope");
    let fixed = s
        .call(
            "recall",
            json!({"view": "marked-things", "scope": "record"}),
        )
        .await;
    assert_eq!(handles_of(&fixed), ["thing:gamma"], "{}", fixed.raw());

    // The negative: a key no scope holds is a spelling mistake, and the refusal
    // does not send the caller to a scope.
    let misspelt = s
        .refused("recall", json!({"kind": "thing", "where": "colur = red"}))
        .await;
    assert!(
        !misspelt.raw().contains("scope"),
        "a misspelt key is not a scope mistake: {}",
        misspelt.raw()
    );
    story.finish().await;
}

/// **A comparison no declaration licenses is refused with the declaration to make**,
/// and making it answers.
#[tokio::test]
async fn a_licence_refusal_names_the_type_to_declare() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    an_undeclared_board(&s).await;

    s.refused("recall", json!({"kind": "thing", "where": "size > 3"}))
        .await
        .says("declare_type")
        .says("size")
        .says("number");
    s.call(
        "declare_type",
        json!({"name": "thing", "fields": [{"key": "size", "holds": "number"}]}),
    )
    .await;
    let fixed = s
        .call("recall", json!({"kind": "thing", "where": "size > 3"}))
        .await;
    assert_eq!(handles_of(&fixed), ["thing:gamma"], "{}", fixed.raw());

    s.refused(
        "recall",
        json!({"kind": "thing", "where": "touches contains essay.rs"}),
    )
    .await
    .says("declare_type")
    .says("touches")
    .says("list");
    s.call(
        "declare_type",
        json!({"name": "thing", "fields": [
            {"key": "size", "holds": "number"},
            {"key": "touches", "holds": "list:text"},
        ]}),
    )
    .await;
    let fixed = s
        .call(
            "recall",
            json!({"kind": "thing", "where": "touches contains essay.rs"}),
        )
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:delta", "thing:gamma"],
        "{}",
        fixed.raw()
    );
    story.finish().await;
}

/// **An order on a key nothing holds is refused as a term on it is**, naming the
/// keys that are held, and an order on a key some hold still puts the rest last.
#[tokio::test]
async fn an_order_on_a_key_nothing_holds_is_refused_not_tied() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;

    s.refused(
        "recall",
        json!({"kind": "thing", "order": {"key": "duedate"}}),
    )
    .await
    .says("duedate")
    .says("due");
    // The positive: the key spelt as held orders them, and the thing holding none
    // comes last.
    let ordered = s
        .call(
            "recall",
            json!({"kind": "thing", "order": {"key": "due", "direction": "ascending"}}),
        )
        .await;
    assert_eq!(
        handles_of(&ordered),
        ["thing:epsilon", "thing:gamma", "thing:delta", "thing:omega"],
        "{}",
        ordered.raw()
    );
    story.finish().await;
}

/// **A walk's `keeping` is checked for its keys as a selection's `where` is**, and
/// what the walk drops for want of a licence is counted, with the declaration
/// that would keep it.
#[tokio::test]
async fn a_walks_keeping_is_checked_and_its_unlicensed_drops_are_counted() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    // A place milhouse owns, whose kind no declaration licenses a date
    // ordering on.
    s.add("place:daredevil-gorge", "Daredevil Gorge").await;
    s.event_with(
        "place:daredevil-gorge",
        "owned, due long ago",
        json!({ "due": "2026-10-01", "owner": "person:milhouse" }),
        &[],
    )
    .await;
    let walk = |keeping: &str| {
        json!({
            "subject": "person:milhouse",
            "follow": { "direction": "in", "keeping": keeping },
        })
    };

    // A key nothing among the things the walk reaches holds is refused, naming the
    // keys held; spelt as held, the same walk answers.
    s.refused("recall", walk("statuz != done"))
        .await
        .says("statuz")
        .says("status");
    let fixed = s.call("recall", walk("status != done")).await;
    fixed.says("thing:gamma");

    // An object the walk drops for want of a licence is counted, with what to
    // declare; declaring it keeps the object.
    let ordered = s.call("recall", walk("due < 2026-10-15")).await;
    ordered.says("thing:gamma");
    ordered.never_says("\"id\":\"place:daredevil-gorge\"");
    let counted = ordered.json()["left_out_unlicensed"].clone();
    assert_eq!(counted[0]["kind"], "place", "{}", ordered.raw());
    assert_eq!(counted[0]["count"], 1, "{}", ordered.raw());
    assert!(
        counted[0]["because"]
            .as_str()
            .is_some_and(|because| because.contains("declare_type")),
        "{}",
        ordered.raw()
    );
    s.call(
        "declare_type",
        json!({"name": "place", "fields": [{"key": "due", "holds": "date"}]}),
    )
    .await;
    let kept = s.call("recall", walk("due < 2026-10-15")).await;
    kept.says("place:daredevil-gorge");
    assert!(
        kept.json().get("left_out_unlicensed").is_none(),
        "nothing is left out once the kind licenses it: {}",
        kept.raw()
    );
    story.finish().await;
}

/// **A recall that narrows nothing is refused, naming `where` among what narrows**,
/// and a `where` alone answers.
#[tokio::test]
async fn the_refusal_of_a_recall_that_narrows_nothing_lists_where() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    s.refused("recall", json!({})).await.says("where");
    let fixed = s.call("recall", json!({"where": "status = done"})).await;
    assert_eq!(handles_of(&fixed), ["thing:epsilon"], "{}", fixed.raw());
    story.finish().await;
}

/// **A view and a caller combine as the view's comment says.** A caller's `where`
/// takes the view's place whole. A `scope` beside a question that has no `where`
/// is refused, not ignored. A view whose own `where` does not parse is refused
/// naming the view, where it is declared.
#[tokio::test]
async fn a_view_and_a_caller_combine_without_silence() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "marked-things", "name": "Marked things",
               "source": "user-named",
               "sets": {"selects": "thing", "where": "status = next"}}),
    )
    .await;
    // The view's own question, and a caller's where in its place, whole.
    let own = s.call("recall", json!({"view": "marked-things"})).await;
    assert_eq!(handles_of(&own), ["thing:delta"], "{}", own.raw());
    let replaced = s
        .call(
            "recall",
            json!({"view": "marked-things", "where": "status = done"}),
        )
        .await;
    assert_eq!(
        handles_of(&replaced),
        ["thing:epsilon"],
        "{}",
        replaced.raw()
    );

    // A scope beside no where is refused; with a where it is the way forward.
    s.refused("recall", json!({"kind": "thing", "scope": "record"}))
        .await
        .says("where");
    let scoped = s
        .call(
            "recall",
            json!({"view": "marked-things", "scope": "record"}),
        )
        .await;
    assert!(scoped.json()["count"].is_number(), "{}", scoped.raw());

    // A view whose where does not parse is refused where it is declared, naming
    // the view and the key; spelt right, the same declaration lands.
    let declare = |line: &str| {
        json!({"kind": "view", "handle": "matches-nothing", "name": "Matches nothing",
               "source": "user-named",
               "sets": {"selects": "thing", "where": line}})
    };
    s.refused("add_entity", declare("status == next"))
        .await
        .says("matches-nothing")
        .says("where")
        .says("==");
    s.call("add_entity", declare("status = next")).await;
    story.finish().await;
}
