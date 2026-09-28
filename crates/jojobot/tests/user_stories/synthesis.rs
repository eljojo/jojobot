//! "I have three separate notes about the costume fitting. Give me one
//! summary — and if I ever ask, let me see what it was drawn from."
//!
//! A synthesis is a record like any other, marked as standing for the claims
//! it draws together. The sources stay exactly as written — nobody rewrites
//! three claims into one — and the ordinary read shows the summary alone, so
//! a caller who never asked for the sources is not shown the same ground
//! three times over.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn one_summary_stands_for_three_separate_notes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:gayle", "Gayle").await;

    // ── three separate notes, none of them the summary ───────────────────────
    let too_small = s
        .fact("person:gayle", "the doublet is too small in the shoulders")
        .await;
    let too_long = s
        .fact("person:gayle", "the sleeves run long past the wrist")
        .await;
    let colour = s
        .fact("person:gayle", "the dye came out darker than the swatch")
        .await;

    // ── the summary, written as its own claim and then marked ───────────────
    let summary = s
        .call(
            "capture",
            json!({
                "subject": "person:gayle",
                "content": "the costume needs another fitting before opening night",
            }),
        )
        .await
        .field("address");
    s.call(
        "update_fact",
        json!({
            "address": &summary,
            "stands_for": [&too_small, &too_long, &colour],
        }),
    )
    .await;

    // ── the ordinary read: the summary, and none of what it draws on ────────
    let plain = s.recall("person:gayle").await;
    plain.says("the costume needs another fitting");
    plain
        .never_says("too small in the shoulders")
        .never_says("run long past the wrist")
        .never_says("darker than the swatch");
    // The answer says how many it left out and names the way to read them —
    // never silent about an elision it chose to make.
    assert!(
        plain.json()["objects"][0]["stood_for"]
            .as_str()
            .is_some_and(|note| note.contains('3') && note.contains("stood_for")),
        "the answer must say it left three sources out, and how to ask for them: {}",
        plain.raw(),
    );

    // ── asked for by name: the sources come back beside the summary ─────────
    let whole = s
        .shape(
            "the costume notes, sources included",
            json!({"subject": "person:gayle", "facts": true, "stood_for": true}),
        )
        .await;
    whole
        .says("the costume needs another fitting")
        .says("too small in the shoulders")
        .says("run long past the wrist")
        .says("darker than the swatch");
    // And the elision note is gone: nothing was left out this time.
    assert!(
        whole.json()["objects"][0].get("stood_for").is_none(),
        "nothing was elided, so nothing should say something was: {}",
        whole.raw(),
    );

    s.wrap("filed one summary standing for three separate notes")
        .await;

    // ── a later session asks who is actually vouching for the sizing ────────
    let s2 = story.session().await;

    // Two more claims, holding the same field under different provenance —
    // one the operator said, one the assistant worked out.
    s2.call(
        "capture",
        json!({
            "subject": "person:gayle",
            "content": "worked out from the pattern sheet",
            "provenance": "inference",
            "fields": {"shoulder_width": "narrow"},
        }),
    )
    .await;
    s2.call(
        "capture",
        json!({
            "subject": "person:gayle",
            "content": "the tailor measured it directly",
            "provenance": "testimony",
            "fields": {"shoulder_width": "narrow"},
        }),
    )
    .await;

    let backed = s2
        .shape(
            "what the shoulder width is, and who backs it",
            json!({"subject": "person:gayle", "backing": true}),
        )
        .await;
    backed.says("\"shoulder_width\":\"narrow\"");
    assert_eq!(
        backed.json()["objects"][0]["fields_backing"]["shoulder_width"]["provenance"],
        "testimony",
        "the newest write is the tailor's own testimony, not the earlier guess: {}",
        backed.raw(),
    );

    // The ordinary read carries no pedigree at all, and does not pretend to.
    let unbacked = s2.recall("person:gayle").await;
    assert!(
        unbacked.json()["objects"][0]
            .get("fields_backing")
            .is_none(),
        "backing was not asked for, so none should arrive: {}",
        unbacked.raw(),
    );

    s2.wrap("checked who actually backs the shoulder measurement")
        .await;

    story.finish().await;
}
