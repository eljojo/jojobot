//! "Which of my things are still open, and which fall due this week?"
//!
//! **One short line says which things a caller wants**, the way a filter on a
//! task board reads: `status != done && owner = person:milhouse`. This is the
//! recall's `where`, and everything here goes through the served surface.
//!
//! **Every half is paired.** A negation that keeps the thing holding another
//! status is beside the thing holding none, which it does not keep; a
//! comparison a declaration licenses is beside the same comparison on a key
//! nothing declared; a relative date is asked on two different days.
//!
//! The things are on a board: `gamma` is open and due soon, `delta` is open and
//! due later, `epsilon` is done, and `omega` holds no status at all.

use serde_json::json;

use super::dsl::{Session, Story};

/// The board, and the declaration that makes `due` a date, `size` a number and
/// `touches` a list.
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
        ("thing:gamma", "Gamma"),
        ("thing:delta", "Delta"),
        ("thing:epsilon", "Epsilon"),
        ("thing:omega", "Omega"),
    ] {
        s.add(handle, name).await;
    }
    s.event_with(
        "thing:gamma",
        "open, due in three days",
        json!({
            "status": "now", "size": "10", "due": "2026-10-12", "rank": "10",
            "touches": "essay.rs, skills.rs", "owner": "person:milhouse",
        }),
        &[],
    )
    .await;
    s.event_with(
        "thing:delta",
        "open, due next month",
        json!({
            "status": "next", "size": "2", "due": "2026-11-20", "rank": "2",
            "touches": "essay.rs", "owner": "person:milhouse",
        }),
        &[],
    )
    .await;
    s.event_with(
        "thing:epsilon",
        "finished last week",
        json!({ "status": "done", "due": "2026-10-01", "owner": "person:milhouse" }),
        &[],
    )
    .await;
    s.event_with(
        "thing:omega",
        "nobody gave it a status",
        json!({ "note": "holds no status" }),
        &[],
    )
    .await;
}

/// The handles an answer named, in the order it named them.
fn handles_of(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", answer.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

/// The things a line picks out, as the handles the answer names.
async fn which(s: &Session, line: &str) -> Vec<String> {
    handles_of(
        &s.shape(
            &format!("the things where {line}"),
            json!({ "kind": "thing", "where": line }),
        )
        .await,
    )
}

#[tokio::test]
async fn a_caller_says_which_things_in_one_line() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    let ask = |line: &'static str| which(&s, line);

    // ── a negation keeps what holds the key with another value ──────────────
    assert_eq!(
        ask("status != done").await,
        ["thing:delta", "thing:gamma"],
        "epsilon is done, and omega holds no status, so neither has another one"
    );
    // The complement is the other question, and it is why the pair is here.
    assert_eq!(
        ask("!(status = done)").await,
        ["thing:delta", "thing:gamma", "thing:omega"],
        "the complement of done keeps the thing that holds no status"
    );
    assert_eq!(ask("status = done").await, ["thing:epsilon"]);

    // ── any-of, and the booleans ────────────────────────────────────────────
    assert_eq!(
        ask("status in (now, done)").await,
        ["thing:epsilon", "thing:gamma"]
    );
    assert_eq!(
        ask("status = now || size = 2").await,
        ["thing:delta", "thing:gamma"]
    );
    assert_eq!(
        ask("status != done && owner = person:milhouse && !(size = 2)").await,
        ["thing:gamma"]
    );
    assert_eq!(
        ask("due").await,
        ["thing:delta", "thing:epsilon", "thing:gamma"]
    );

    // ── orderings need a declaration, and the number is a number ────────────
    assert_eq!(
        ask("size > 5").await,
        ["thing:gamma"],
        "ten is more than five; as text it would sort below it"
    );
    assert_eq!(ask("size >= 2 && size <= 2").await, ["thing:delta"]);
    assert_eq!(
        ask("due < 2026-10-10").await,
        ["thing:epsilon"],
        "the day before the tenth"
    );

    // ── contains: whole items of a declared list ────────────────────────────
    assert_eq!(ask(r#"touches contains skills.rs"#).await, ["thing:gamma"]);
    assert_eq!(
        ask("touches contains essay.rs").await,
        ["thing:delta", "thing:gamma"]
    );
    assert!(
        ask("touches contains skills").await.is_empty(),
        "a piece of an item is not an item"
    );

    story.finish().await;
}

#[tokio::test]
async fn a_relative_date_means_the_askers_day() {
    let story = Story::begin("bot:otto").await;
    let early = story.session_on("2026-10-09", None).await;
    a_board(&early).await;
    assert_eq!(
        which(&early, "due >= today && due <= today+7d").await,
        ["thing:gamma"],
        "on the ninth, the twelfth is within a week and the first of next month is not"
    );
    assert_eq!(
        which(&early, "due < today").await,
        ["thing:epsilon"],
        "the first has passed"
    );
    let late = story.session_on("2026-11-18", None).await;
    assert_eq!(
        which(&late, "due >= today && due <= today+7d").await,
        ["thing:delta"],
        "the same words on the eighteenth of November reach the twentieth"
    );
    assert_eq!(
        which(&late, "due <= today-2w").await,
        ["thing:epsilon", "thing:gamma"],
        "two weeks before the eighteenth is the fourth of November"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_scope_says_whether_a_term_asks_the_thing_or_a_record() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    // A verdict the claim carries as its own, beside the thing's state: it is
    // on the record and the thing does not hold it.
    s.call(
        "capture",
        json!({
            "subject": "thing:gamma", "content": "reviewed, still open",
            "provenance": "testimony", "fields": { "verdict": "open" },
        }),
    )
    .await;

    // ── asked of a record, the term finds the occasion and brings it ────────
    let by_record = s
        .shape(
            "the things with a record saying the verdict was open",
            json!({
                "kind": "thing", "where": "verdict = open", "scope": "record",
                "facts": true,
            }),
        )
        .await;
    assert_eq!(handles_of(&by_record), ["thing:gamma"]);
    by_record.says("reviewed, still open");
    by_record.never_says("open, due next month");

    // ── asked of the thing, nothing holds the key and the call says so ──────
    let refused = s
        .refused(
            "recall",
            json!({ "kind": "thing", "where": "verdict = open" }),
        )
        .await;
    refused.says("verdict");

    story.finish().await;
}

#[tokio::test]
async fn an_order_is_one_key_and_a_direction_read_the_way_the_key_was_declared() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    let ordered = |key: &'static str, direction: &'static str| {
        let s = &s;
        async move {
            s.shape(
                &format!("the things by {key} {direction}"),
                json!({
                    "kind": "thing",
                    "order": { "key": key, "direction": direction },
                }),
            )
            .await
        }
    };

    // `size` is declared a number: two comes before ten, and the things holding
    // no size come last in either direction, in handle order.
    assert_eq!(
        handles_of(&ordered("size", "ascending").await),
        ["thing:delta", "thing:gamma", "thing:epsilon", "thing:omega"]
    );
    assert_eq!(
        handles_of(&ordered("size", "descending").await),
        ["thing:gamma", "thing:delta", "thing:epsilon", "thing:omega"]
    );

    // `rank` holds the same two values and nobody declared it, so it is text:
    // ten comes before two, and the answer says it read the key that way.
    let as_text = ordered("rank", "ascending").await;
    assert_eq!(
        handles_of(&as_text),
        ["thing:gamma", "thing:delta", "thing:epsilon", "thing:omega"]
    );
    assert!(
        as_text.json()["order_note"]
            .as_str()
            .is_some_and(|note| note.contains("rank")),
        "an order that read a number as text says so: {}",
        as_text.raw()
    );
    // The declared key owes no such line.
    assert!(ordered("size", "ascending").await.json()["order_note"].is_null());

    // The direction is one of two words, and a mistake says which.
    s.refused(
        "recall",
        json!({ "kind": "thing", "order": { "key": "size", "direction": "sideways" } }),
    )
    .await
    .says("sideways");

    story.finish().await;
}

#[tokio::test]
async fn a_comparison_needs_the_declaration_of_the_kind_named_and_every_mistake_says_where() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    let refused = |line: &'static str| {
        let s = &s;
        async move {
            s.refused("recall", json!({ "kind": "thing", "where": line }))
                .await
        }
    };

    // ── a mistake in the line names the token and where it is ───────────────
    let broken = refused("status == done").await;
    broken.says("position 9");
    broken.says("'='");
    refused("status = done &").await.says("position 15");
    refused("(status = done").await.says("')'");
    refused("size > banana").await.says("banana");
    refused("   ").await.says("empty");

    // ── a comparison no declaration licenses is refused by name ─────────────
    //
    // `rank` holds numbers and nobody said so.
    let unlicensed = refused("rank > 1").await;
    unlicensed.says("rank");
    unlicensed.says("number");
    refused("rank contains 10").await.says("rank");
    // A relative date is a date, and only a key declared one is asked as one.
    let undated = refused("rank <= today").await;
    undated.says("rank");
    undated.says("date");

    // …and a declaration named for ANOTHER kind belongs to that kind: a person's
    // `rank` is not a thing's. A call naming the kind is refused, and a call
    // naming none leaves the things out and counts them rather than shrinking in
    // silence.
    s.call(
        "declare_type",
        json!({
            "name": "person",
            "fields": [{ "key": "rank", "holds": "number" }],
        }),
    )
    .await
    .says("\"name\":\"person\"");
    refused("rank > 1").await.says("rank");
    let by_kind = s
        .shape(
            "everything ranked above one, naming no kind",
            json!({ "where": "rank > 1" }),
        )
        .await;
    assert!(handles_of(&by_kind).is_empty(), "{}", by_kind.raw());
    let left_out = &by_kind.json()["left_out_unlicensed"];
    assert_eq!(left_out[0]["kind"], "thing", "{}", by_kind.raw());
    assert_eq!(left_out[0]["count"], 2, "{}", by_kind.raw());

    // A type under a name that is no kind is vocabulary: it licenses any thing
    // carrying its key, with the kind named or not.
    s.call(
        "declare_type",
        json!({
            "name": "ranked",
            "fields": [{ "key": "rank", "holds": "number" }],
        }),
    )
    .await
    .says("\"name\":\"ranked\"");
    assert_eq!(which(&s, "rank > 1").await, ["thing:delta", "thing:gamma"]);
    assert_eq!(
        handles_of(
            &s.shape(
                "everything ranked above one, naming no kind",
                json!({ "where": "rank > 1" }),
            )
            .await
        ),
        ["thing:delta", "thing:gamma"]
    );

    // ── a key nothing holds is not an empty answer ──────────────────────────
    let misspelt = refused("sttaus = done").await;
    misspelt.says("sttaus");
    misspelt.says("status");

    story.finish().await;
}

#[tokio::test]
async fn a_walk_keeps_what_an_expression_says() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    let owned = s
        .shape(
            "what Milhouse owns that is not done",
            json!({
                "subject": "person:milhouse",
                "follow": { "direction": "in", "keeping": "status != done" },
            }),
        )
        .await;
    owned.says("thing:gamma");
    owned.says("thing:delta");
    // The done one is reached and not kept.
    owned.never_says("\"id\":\"thing:epsilon\"");
    // A walk's expression is asked of what each reached thing holds now. A
    // record scope sent beside it is refused rather than ignored, and the refusal
    // names the form that does take one.
    s.refused(
        "recall",
        json!({
            "subject": "person:milhouse",
            "scope": "record",
            "follow": { "direction": "in", "keeping": "status != done" },
        }),
    )
    .await
    .says("filters");
    story.finish().await;
}

#[tokio::test]
async fn a_view_holds_the_line_its_order_and_its_scope_and_a_rename_does_not_break_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;

    // ── the view is a record: three keys beside what it selects ─────────────
    s.add("view:open-work", "Open Work").await;
    s.event_with(
        "view:open-work",
        "what Milhouse still has open, biggest first",
        json!({
            "selects": "thing",
            "where": "status != done && owner = person:milhouse",
            "order": "size descending",
        }),
        &[],
    )
    .await;

    // **It answers exactly as the same recall does.**
    let by_view = s
        .shape("open work by name", json!({ "view": "open-work" }))
        .await;
    let direct = s
        .shape(
            "the same question asked directly",
            json!({
                "kind": "thing",
                "where": "status != done && owner = person:milhouse",
                "order": { "key": "size", "direction": "descending" },
            }),
        )
        .await;
    assert_eq!(handles_of(&by_view), ["thing:gamma", "thing:delta"]);
    assert_eq!(handles_of(&by_view), handles_of(&direct));
    // The ascending reading of the same view is the other order, so the pair
    // above is not two empty or unordered answers agreeing.
    assert_eq!(
        handles_of(
            &s.shape(
                "open work, smallest first",
                json!({ "view": "open-work", "order": { "key": "size" } }),
            )
            .await
        ),
        ["thing:delta", "thing:gamma"],
        "a call's own order wins over the view's"
    );
    assert_eq!(
        handles_of(
            &s.shape(
                "what the view's kind holds that is done",
                json!({ "view": "open-work", "where": "status = done" }),
            )
            .await
        ),
        ["thing:epsilon"],
        "a call's own line wins over the view's"
    );

    // ── the handle in the line follows the thing it names ───────────────────
    s.call(
        "rename_entity",
        json!({ "handle": "person:milhouse", "to": "person:ralph" }),
    )
    .await;
    assert_eq!(
        handles_of(
            &s.shape("open work after the rename", json!({ "view": "open-work" }))
                .await
        ),
        ["thing:gamma", "thing:delta"],
        "the saved line still names the same person"
    );
    let saved = s.recall("view:open-work").await;
    saved.says("owner = person:ralph");
    saved.never_says("person:milhouse");

    // ── a line that does not read is not saved ──────────────────────────────
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": "view:open-work", "content": "a broken line",
                "provenance": "testimony", "sets": { "where": "status == done" },
            }),
        )
        .await;
    refused.says("position 9");
    // …nor is a person nobody has heard of.
    s.refused(
        "capture",
        json!({
            "subject": "view:open-work", "content": "a stranger",
            "provenance": "testimony", "sets": { "where": "owner = person:ned-flanders" },
        }),
    )
    .await
    .says("person:ned-flanders");

    story.finish().await;
}

#[tokio::test]
async fn a_view_with_an_old_filter_record_keeps_answering_and_the_line_is_added_to_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    s.add("view:started", "Started").await;
    s.event_with(
        "view:started",
        "what has a status",
        json!({ "selects": "thing" }),
        &[],
    )
    .await;
    // The filter the way a view held it before the line existed: a record whose
    // own fields name the key and the value.
    s.call(
        "capture",
        json!({
            "subject": "view:started", "content": "the status is now or next",
            "provenance": "testimony", "fields": { "key": "owner", "value": "person:milhouse" },
        }),
    )
    .await;
    assert_eq!(
        handles_of(
            &s.shape("the old filter alone", json!({ "view": "started" }))
                .await
        ),
        ["thing:delta", "thing:epsilon", "thing:gamma"],
        "owned by Milhouse, whatever the status"
    );
    // The line is added to the filter record: both must hold.
    s.event_with(
        "view:started",
        "and not done",
        json!({ "where": "status != done" }),
        &[],
    )
    .await;
    assert_eq!(
        handles_of(
            &s.shape("the old filter and the line", json!({ "view": "started" }))
                .await
        ),
        ["thing:delta", "thing:gamma"],
        "the done one is out by the line, the unowned one by the record"
    );
    // ── a view can ask its line of the records instead of the things ────────
    s.add("view:reviewed", "Reviewed").await;
    s.event_with(
        "view:reviewed",
        "what a record says was reviewed open",
        json!({ "selects": "thing", "where": "verdict = open", "scope": "record" }),
        &[],
    )
    .await;
    s.call(
        "capture",
        json!({
            "subject": "thing:gamma", "content": "reviewed, still open",
            "provenance": "testimony", "fields": { "verdict": "open" },
        }),
    )
    .await;
    assert_eq!(
        handles_of(
            &s.shape("the reviewed view", json!({ "view": "reviewed" }))
                .await
        ),
        ["thing:gamma"],
        "the view's scope reaches the records, where no thing holds the verdict"
    );
    // The same line without the scope asks the things, and nothing holds the key.
    s.add("view:unscoped", "Unscoped").await;
    s.event_with(
        "view:unscoped",
        "the same line asked of the things",
        json!({ "selects": "thing", "where": "verdict = open" }),
        &[],
    )
    .await;
    s.refused("recall", json!({ "view": "unscoped" }))
        .await
        .says("verdict");

    story.finish().await;
}

#[tokio::test]
async fn a_kinds_own_declaration_licenses_what_a_call_naming_the_kind_asks() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    s.add("person:bart", "Bart").await;
    s.add_under("person:bart", "promise:gamma", "Call the plumber")
        .await;
    s.add_under("person:bart", "promise:delta", "Return the ladder")
        .await;
    s.event_with(
        "promise:gamma",
        "due in three days",
        json!({ "promised_by": "2026-10-12" }),
        &[],
    )
    .await;
    s.event_with(
        "promise:delta",
        "due next month",
        json!({ "promised_by": "2026-11-30" }),
        &[],
    )
    .await;
    // No type was declared: the promise kind itself says `promised_by` holds a
    // date, and that licenses the ordering.
    let soon = s
        .shape(
            "the promises due within a week",
            json!({ "kind": "promise", "where": "promised_by <= today+7d" }),
        )
        .await;
    assert_eq!(handles_of(&soon), ["promise:gamma"]);
    // A key the kind does not declare is not licensed by being declared elsewhere.
    s.refused(
        "recall",
        json!({ "kind": "promise", "where": "regarding < 2026-01-01" }),
    )
    .await
    .says("regarding");
    story.finish().await;
}

#[tokio::test]
async fn a_call_naming_no_kind_asks_each_objects_own_kind_and_counts_what_it_left_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    s.add("person:bart", "Bart").await;
    s.add_under("person:bart", "promise:gamma", "Call the plumber")
        .await;
    s.event_with(
        "promise:gamma",
        "due in three days",
        json!({ "promised_by": "2026-10-12" }),
        &[],
    )
    .await;
    // A thing holding the same key: its kind declares no such key, so the
    // comparison is not its to answer.
    s.add("thing:sigma", "Sigma").await;
    s.event_with(
        "thing:sigma",
        "carries a key another kind declares",
        json!({ "promised_by": "2026-10-13" }),
        &[],
    )
    .await;
    let soon = s
        .shape(
            "whatever is promised within a week",
            json!({ "where": "promised_by <= today+7d" }),
        )
        .await;
    assert_eq!(handles_of(&soon), ["promise:gamma"]);
    let left_out = &soon.json()["left_out_unlicensed"];
    assert_eq!(left_out[0]["kind"], "thing", "{}", soon.raw());
    assert_eq!(left_out[0]["count"], 1, "{}", soon.raw());
    assert!(
        left_out[0]["because"]
            .as_str()
            .is_some_and(|why| why.contains("promised_by")),
        "{}",
        soon.raw()
    );
    // Naming the kind that licenses it has nothing to leave out.
    let named = s
        .shape(
            "the promises due within a week",
            json!({ "kind": "promise", "where": "promised_by <= today+7d" }),
        )
        .await;
    assert_eq!(handles_of(&named), ["promise:gamma"]);
    named.never_says("left_out_unlicensed");
    story.finish().await;
}

#[tokio::test]
async fn a_hostile_where_is_refused_on_every_door_and_the_server_answers_the_next_call() {
    use jojobot_domain::memory::filter::expression::{MAX_DEPTH, MAX_LENGTH};
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    a_board(&s).await;
    s.add("view:open-work", "Open Work").await;
    let deep = format!(
        "{}status = now{}",
        "(".repeat(MAX_DEPTH + 1),
        ")".repeat(MAX_DEPTH + 1)
    );
    let huge = "(".repeat(100_000);
    // A line a view can hold at all (the size guard comes first for a huge one)
    // and that is over the expression limit.
    let long_for_a_view = "(".repeat(MAX_LENGTH + 1000);
    let calendar = "due < today+1043498w";

    // ── a view's where is refused at the write ──────────────────────────────
    for (line, limit) in [(&deep, MAX_DEPTH), (&long_for_a_view, MAX_LENGTH)] {
        s.refused(
            "capture",
            json!({
                "subject": "view:open-work", "content": "a hostile line",
                "provenance": "testimony", "sets": { "where": line },
            }),
        )
        .await
        .says(&limit.to_string());
    }
    s.refused(
        "capture",
        json!({
            "subject": "view:open-work", "content": "a day past the calendar",
            "provenance": "testimony", "sets": { "where": calendar },
        }),
    )
    .await
    .says("relative");

    // ── a recall's where, and a walk's keeping ──────────────────────────────
    for (line, limit) in [(&deep, MAX_DEPTH), (&huge, MAX_LENGTH)] {
        s.refused("recall", json!({ "kind": "thing", "where": line }))
            .await
            .says(&limit.to_string());
        s.refused(
            "recall",
            json!({
                "subject": "person:milhouse",
                "follow": { "direction": "in", "keeping": line },
            }),
        )
        .await
        .says(&limit.to_string());
    }
    s.refused("recall", json!({ "kind": "thing", "where": calendar }))
        .await
        .says("relative");

    // The positive: nothing was taken down, and the same board answers.
    assert_eq!(
        which(&s, "status != done").await,
        ["thing:delta", "thing:gamma"]
    );
    story.finish().await;
}
