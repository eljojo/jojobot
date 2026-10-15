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
        "the largest held is 4, one past it is 5: {collection}"
    );
    // The archived 3 is not the largest, so it does not move this number. That an
    // archived member keeps its number is pinned below: `/3` still names it, and
    // a write repeating 3 is refused.

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
    refusal.says(LEAVES);
    // **The free number is read from the refusal and is the collection's own
    // `next_number`**: a bare "5" is satisfied by any digit in the answer.
    let advice = refusal.json()["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal carries advice: {}", refusal.raw()))
        .to_string();
    let free = advice
        .split("The next free number is ")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .unwrap_or_else(|| panic!("the refusal names the free number: {advice}"))
        .to_string();
    assert_eq!(
        free.parse::<u64>().ok(),
        collection["objects"][0]["next_number"].as_u64(),
        "the refusal hands over the number the collection says is next: {advice}"
    );
    // Nothing was made by the refusal, so the same call with the free number
    // lands: the positive the refusal depends on.
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "contract-ordinance-story-signs",
               "name": "Sign height", "source": "user-named",
               "parent": COLLECTION, "sets": {"number": free}}),
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

/// **A number names one member for good, whatever verb moves the member.**
/// Folding a numbered member into a thing outside and moving it out would each
/// free the number, so each is refused and says which number and whose. Paired:
/// folding an unnumbered member into a numbered one, and taking back a claim
/// that sets no number, change nothing about the numbers and land. **A
/// retraction of the claim that gives the number is served on purpose**: it says
/// the claim should never have been written, so the number is free again.
#[tokio::test]
async fn a_number_stays_with_its_member_through_a_fold_and_a_move_and_a_retraction_is_served() {
    const OUTSIDER: &str = "decision:contract-ordinance-story-outsider";
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    for (handle, name, number) in [(CURFEW, "Curfew", "1"), (PARKING, "Parking", "2")] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named",
                   "parent": COLLECTION, "sets": {"number": number}}),
        )
        .await;
    }
    let (kind, slug) = SIGNS.split_once(':').expect("a handle");
    s.call(
        "add_entity",
        json!({"kind": kind, "handle": slug, "name": "Sign height", "source": "user-named",
               "parent": COLLECTION}),
    )
    .await;
    s.add(OUTSIDER, "Someone elsewhere").await;

    // ── folding a numbered member into a thing outside the collection ───────
    s.refused(
        "merge_entities",
        json!({"duplicate": PARKING, "survivor": OUTSIDER, "reason": "the same ordinance"}),
    )
    .await
    .says(PARKING)
    .says(&format!("{COLLECTION}/2"));
    // ── moving it out ───────────────────────────────────────────────────────
    s.refused(
        "rename_entity",
        json!({"handle": PARKING, "to": PARKING, "parent": OUTSIDER}),
    )
    .await
    .says(PARKING)
    .says(&format!("{COLLECTION}/2"));
    // Nothing moved: 2 still names parking, and the next number is where it was.
    s.recall(&format!("{COLLECTION}/2")).await.says(PARKING);
    let collection = s.recall(COLLECTION).await.json();
    assert_eq!(collection["objects"][0]["next_number"], 3, "{collection}");

    // ── the negatives' positives: the numbers do not change, so these land ──
    s.merge_entities(SIGNS, CURFEW, "the same ordinance", None)
        .await;
    s.recall(&format!("{COLLECTION}/1")).await.says(CURFEW);
    let note = s
        .call(
            "capture",
            json!({"subject": CURFEW, "content": "starts at ten", "provenance": "testimony"}),
        )
        .await;
    s.call(
        "retract",
        json!({"address": note.field("address"), "reason": "written in error"}),
    )
    .await;
    s.recall(&format!("{COLLECTION}/2")).await.says(PARKING);

    // ── a retraction is served on purpose ───────────────────────────────────
    // It says the claim should never have been written, so a number given by
    // mistake can always be taken back; the number is then free, as it was
    // before anybody gave it. A member that is retired is archived and keeps its
    // number, which is what "never reused" means.
    let parking = s.recall(PARKING).await.json();
    let gives_two = parking["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("the member's first claim has an address")
        .to_string();
    s.call(
        "retract",
        json!({"address": gives_two, "reason": "written in error"}),
    )
    .await;
    s.refused("recall", json!({"subject": format!("{COLLECTION}/2")}))
        .await
        .says(&format!("{COLLECTION}/1"));
    story.finish().await;
}
