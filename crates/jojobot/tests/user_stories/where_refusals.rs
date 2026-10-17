//! "I asked the board and it said no. What do I send instead?"
//!
//! **A refusal of a question over keys names the call that fixes it, and the
//! story makes that call.** Each case reads the refusal, makes the call it
//! names, and asserts the answer, so the advice is proven to work and not only
//! printed. The negative stands beside each: a refusal that says one thing does
//! not say it for a caller who made a different mistake.
//!
//! The board is a list of chores: the gate is open and due soon, the fence is
//! open and due later, the porch is done, and the shelf holds no status.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The chores board, and the declaration that makes `due` a date, `size` a
/// number and `touches` a list.
async fn a_board(s: &Session) {
    s.call(
        "declare_type",
        json!({
            "name": "thing",
            "fields": [
                { "key": "due", "holds": "date" },
                { "key": "size", "holds": "number" },
                { "key": "touches", "holds": "list:text" },
            ],
        }),
    )
    .await
    .says("\"name\":\"thing\"");
    s.add("person:milhouse", "Milhouse").await;
    for (handle, name) in [
        ("thing:fix-the-gate", "Fix the gate"),
        ("thing:paint-the-fence", "Paint the fence"),
        ("thing:sweep-the-porch", "Sweep the porch"),
        ("thing:sort-the-shelf", "Sort the shelf"),
    ] {
        s.add(handle, name).await;
    }
    s.event_with(
        "thing:fix-the-gate",
        "open, due in three days",
        json!({
            "status": "now", "size": "10", "due": "2026-10-12", "rank": "10",
            "touches": "hammer, ladder", "owner": "person:milhouse",
        }),
        &[],
    )
    .await;
    s.event_with(
        "thing:paint-the-fence",
        "open, due next month",
        json!({
            "status": "next", "size": "2", "due": "2026-11-20", "rank": "2",
            "touches": "hammer", "owner": "person:milhouse",
        }),
        &[],
    )
    .await;
    s.event_with(
        "thing:sweep-the-porch",
        "finished last week",
        json!({ "status": "done", "due": "2026-10-01", "owner": "person:milhouse" }),
        &[],
    )
    .await;
    s.event_with(
        "thing:sort-the-shelf",
        "nobody gave it a status",
        json!({ "note": "holds no status" }),
        &[],
    )
    .await;
}

/// The handles an answer named, in order.
fn handles_of(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", answer.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

/// The advice a refusal carries, as one string.
fn advice(refusal: &Answer) -> String {
    refusal.json()["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal carries advice: {}", refusal.raw()))
        .to_string()
}

/// The text of `advice` between `start` and the next `end` after it, so a story
/// makes the call the refusal names rather than the one it already knew.
fn between<'a>(advice: &'a str, start: &str, end: &str) -> &'a str {
    let from = advice
        .find(start)
        .unwrap_or_else(|| panic!("the advice has no {start:?}: {advice}"))
        + start.len();
    let rest = &advice[from..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("the advice has no {end:?} after {start:?}: {advice}"));
    &rest[..to]
}

/// A board whose keys no type declares, so an ordering on them is not licensed.
async fn an_undeclared_board(s: &Session) {
    for (handle, name, size, touches) in [
        ("thing:fix-the-gate", "Fix the gate", "10", "hammer, ladder"),
        ("thing:paint-the-fence", "Paint the fence", "2", "hammer"),
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

    let refusal = s
        .refused(
            "recall",
            json!({ "kind": "thing", "where": "status == done" }),
        )
        .await;
    // The spelling is read from the refusal, not from the sent query: the sent
    // query holds the doubled sign itself, so a needle on it proves nothing.
    let advice = advice(&refusal);
    let fix = between(&advice, "equality is written with one: ", ", not ");
    let fixed = s
        .call("recall", json!({ "kind": "thing", "where": fix }))
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:sweep-the-porch"],
        "{}",
        fixed.raw()
    );

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
        ["thing:fix-the-gate", "thing:sweep-the-porch"],
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
        ("thing:fix-the-gate", "Fix the gate", "red"),
        ("thing:paint-the-fence", "Paint the fence", "blue"),
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
    assert_eq!(
        handles_of(&fixed),
        ["thing:fix-the-gate"],
        "{}",
        fixed.raw()
    );

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
    assert_eq!(
        handles_of(&fixed),
        ["thing:fix-the-gate"],
        "{}",
        fixed.raw()
    );

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

    // The declaration is read from the refusal: its type, its key and what the
    // key holds. The sent query holds `size` itself, so a needle on it proves
    // nothing about what the refusal advises.
    let refusal = s
        .refused("recall", json!({"kind": "thing", "where": "size > 3"}))
        .await;
    refusal.says("declare_type");
    let advice_for_size = advice(&refusal);
    let kind = between(&advice_for_size, "name \"", "\" with a field key");
    let size = json!({
        "key": between(&advice_for_size, "field key \"", "\" holding"),
        "holds": between(&advice_for_size, "holding \"", "\""),
    });
    s.call(
        "declare_type",
        json!({"name": kind, "fields": [size.clone()]}),
    )
    .await;
    let fixed = s
        .call("recall", json!({"kind": "thing", "where": "size > 3"}))
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:fix-the-gate"],
        "{}",
        fixed.raw()
    );

    let refusal = s
        .refused(
            "recall",
            json!({"kind": "thing", "where": "touches contains hammer"}),
        )
        .await;
    refusal.says("declare_type");
    let advice_for_touches = advice(&refusal);
    let touches = json!({
        "key": between(&advice_for_touches, "field key \"", "\" holding"),
        "holds": between(&advice_for_touches, "holding \"", "\""),
    });
    s.call(
        "declare_type",
        json!({"name": kind, "fields": [size, touches]}),
    )
    .await;
    let fixed = s
        .call(
            "recall",
            json!({"kind": "thing", "where": "touches contains hammer"}),
        )
        .await;
    assert_eq!(
        handles_of(&fixed),
        ["thing:fix-the-gate", "thing:paint-the-fence"],
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

    let refusal = s
        .refused(
            "recall",
            json!({"kind": "thing", "order": {"key": "duedate"}}),
        )
        .await;
    // The keys held are read from the refusal as a list. `due` is a prefix of the
    // key sent, so a needle on it is satisfied by the sent query; as an element of
    // the list it is the refusal's own word, and the misspelling is not in it.
    let advice = advice(&refusal);
    let held: Vec<&str> = between(&advice, "the keys held among them are: ", ".")
        .split(", ")
        .collect();
    assert!(held.contains(&"due"), "the held keys name due: {held:?}");
    assert!(
        !held.contains(&"duedate"),
        "duedate is no held key: {held:?}"
    );
    // The positive: the key spelt as held orders them, and the thing holding none
    // comes last.
    let key = held.iter().find(|key| **key == "due").expect("due is held");
    let ordered = s
        .call(
            "recall",
            json!({"kind": "thing", "order": {"key": key, "direction": "ascending"}}),
        )
        .await;
    assert_eq!(
        handles_of(&ordered),
        [
            "thing:sweep-the-porch",
            "thing:fix-the-gate",
            "thing:paint-the-fence",
            "thing:sort-the-shelf"
        ],
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
    fixed.says("thing:fix-the-gate");

    // An object the walk drops for want of a licence is counted, with what to
    // declare; declaring it keeps the object.
    let ordered = s.call("recall", walk("due < 2026-10-15")).await;
    ordered.says("thing:fix-the-gate");
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
    assert_eq!(
        handles_of(&fixed),
        ["thing:sweep-the-porch"],
        "{}",
        fixed.raw()
    );
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
    assert_eq!(handles_of(&own), ["thing:paint-the-fence"], "{}", own.raw());
    let replaced = s
        .call(
            "recall",
            json!({"view": "marked-things", "where": "status = done"}),
        )
        .await;
    assert_eq!(
        handles_of(&replaced),
        ["thing:sweep-the-porch"],
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

/// A handle slug that sorts nowhere near the order the story asks for, so the
/// order under test cannot be the handles' own.
fn scattered_slug(i: usize) -> String {
    let mut state = (i as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..10)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect()
}

/// **An order is held across the cut of an answer.** More things than one window
/// holds are ordered by a declared number, the answer names the offset that reads
/// on, and the pages read one after the other are the whole order: no thing
/// twice, none lost, and none out of place at the seam.
#[tokio::test]
async fn an_order_holds_across_the_pages_of_an_answer() {
    const THINGS: usize = 45;
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    s.call(
        "declare_type",
        json!({"name": "thing", "fields": [{"key": "rank", "holds": "number"}]}),
    )
    .await;
    // Rank runs 1..=THINGS in a scrambled creation order, and every thing carries
    // enough that one answer cannot hold them all.
    let padding = "x".repeat(1_100);
    let mut by_rank: Vec<(usize, String)> = Vec::new();
    for i in 0..THINGS {
        let handle = format!("thing:{}", scattered_slug(i));
        let rank = (i * 17) % THINGS + 1;
        s.add(&handle, &scattered_slug(i + 1_000)).await;
        s.call(
            "capture",
            json!({"subject": handle, "content": "ranked", "provenance": "testimony",
                   "sets": {"rank": rank.to_string(), "padding": padding}}),
        )
        .await;
        by_rank.push((rank, handle));
    }
    by_rank.sort();
    let expected: Vec<String> = by_rank.into_iter().map(|(_, handle)| handle).collect();

    let mut seen: Vec<String> = Vec::new();
    let mut pages = 0;
    let mut offset = 0u64;
    loop {
        let page = s
            .call(
                "recall",
                json!({"kind": "thing", "order": {"key": "rank", "direction": "ascending"},
                       "offset": offset}),
            )
            .await;
        pages += 1;
        seen.extend(handles_of(&page));
        match page.json()["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
        assert!(pages < 20, "the pages never end: {}", page.raw());
    }
    assert!(
        pages >= 2,
        "one window must not hold them all: {pages} page"
    );
    assert_eq!(seen, expected, "the order held across {pages} pages");
    story.finish().await;
}
