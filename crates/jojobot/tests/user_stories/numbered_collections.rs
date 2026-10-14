//! "Ordinance 32, not the words I half remember."
//!
//! A container can number what it holds. The assistant finds a member by its
//! number, and jojobot keeps the count: the next number is a read, a number is
//! held by one member for good, and a write that repeats one is refused with
//! the holder and the number that is free.

use serde_json::json;

use super::dsl::Story;

const COLLECTION: &str = "topic:contract-ordinances-story";
const CURFEW: &str = "decision:contract-ordinance-story-curfew";
const PARKING: &str = "decision:contract-ordinance-story-parking";
const LEAVES: &str = "decision:contract-ordinance-story-leaves";
const FIREWORKS: &str = "decision:contract-ordinance-story-fireworks";
const SIGNS: &str = "decision:contract-ordinance-story-signs";

#[tokio::test]
async fn a_member_is_found_by_its_number_and_a_number_is_never_held_twice() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    // Members arrive with their number in the call that makes them.
    for (handle, name, number) in [
        (CURFEW, "Curfew", "1"),
        (PARKING, "Parking", "2"),
        (LEAVES, "Leaf burning", "3"),
        (FIREWORKS, "Fireworks", "4"),
    ] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named",
                   "parent": COLLECTION, "sets": {"number": number}}),
        )
        .await;
    }
    s.call(
        "archive_entity",
        json!({"handle": LEAVES, "reason": "repealed"}),
    )
    .await;

    // ── the collection says the number its next member takes ────────────────
    let collection = s.recall(COLLECTION).await.json();
    assert_eq!(
        collection["objects"][0]["next_number"], 5,
        "the largest held is 4 and an archived 3 is held too: {collection}"
    );

    // ── a member is named by its number, in a recall ────────────────────────
    let found = s.recall(&format!("{COLLECTION}/2")).await.json();
    assert_eq!(
        found["objects"][0]["id"], PARKING,
        "/2 names the member holding 2: {found}"
    );
    // The archived member's number still names it.
    let archived = s.recall(&format!("{COLLECTION}/3")).await.json();
    assert_eq!(archived["objects"][0]["id"], LEAVES, "{archived}");

    // ── and a recall by the key finds a member by its number ────────────────
    let by_key = s
        .call(
            "recall",
            json!({"parent": COLLECTION, "fields": [{"key": "number", "value": "4"}]}),
        )
        .await
        .json();
    assert_eq!(by_key["objects"][0]["id"], FIREWORKS, "{by_key}");

    // ── a repeated number is refused, naming the holder and the free number ─
    let refusal = s
        .refused(
            "add_entity",
            json!({"kind": "decision", "handle": "contract-ordinance-story-signs",
                   "name": "Sign height", "source": "user-named",
                   "parent": COLLECTION, "sets": {"number": "3"}}),
        )
        .await;
    refusal.says(LEAVES).says("5");
    // Nothing was made by the refusal, so the same call with the free number
    // lands: the positive the refusal depends on.
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "contract-ordinance-story-signs",
               "name": "Sign height", "source": "user-named",
               "parent": COLLECTION, "sets": {"number": "5"}}),
    )
    .await;
    let after = s.recall(COLLECTION).await.json();
    assert_eq!(after["objects"][0]["next_number"], 6, "{after}");
    s.recall(&format!("{COLLECTION}/5")).await.says(SIGNS);

    // ── a number nobody holds is unknown, with the numbers that exist ───────
    let miss = s
        .refused("recall", json!({"subject": format!("{COLLECTION}/9")}))
        .await;
    miss.says(&format!("{COLLECTION}/2"));
    story.finish().await;
}
