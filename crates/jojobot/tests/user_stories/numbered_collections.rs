//! "Ordinance 32, not the words I half remember."
//!
//! A container can number what it holds. The assistant finds a member by its
//! number, and jojobot keeps the count: the next number is a read, a number is
//! held by one member for good, and a write that repeats one is refused with
//! the holder and the number that is free.

use serde_json::{Value, json};

use super::dsl::Story;

const COLLECTION: &str = "topic:contract-ordinances-story";
const CURFEW: &str = "decision:contract-ordinance-story-curfew";
const PARKING: &str = "decision:contract-ordinance-story-parking";
const LEAVES: &str = "decision:contract-ordinance-story-leaves";
const FIREWORKS: &str = "decision:contract-ordinance-story-fireworks";
const SIGNS: &str = "decision:contract-ordinance-story-signs";

/// The object of this handle in an answer. A container's read puts its standing
/// decisions first, so the container is not always the first object.
fn the_object<'a>(answer: &'a Value, handle: &str) -> &'a Value {
    answer["objects"]
        .as_array()
        .and_then(|all| all.iter().find(|o| o["id"] == handle))
        .unwrap_or_else(|| panic!("{handle} is in the answer: {answer}"))
}

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
        the_object(&collection, COLLECTION)["next_number"],
        5,
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
        the_object(&collection, COLLECTION)["next_number"].as_u64(),
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
    assert_eq!(the_object(&after, COLLECTION)["next_number"], 6, "{after}");
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
/// the claim should never have been written, and the collection still counts the
/// number as issued, so no member takes it again.
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
    assert_eq!(
        the_object(&collection, COLLECTION)["next_number"],
        3,
        "{collection}"
    );

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
    // mistake can always be taken back; the number was still issued, so the
    // collection never issues it again. A member that is retired is archived and
    // keeps its number, which is also what "never reused" means.
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
    // The taken-back 2 is still issued: the next number is 3, a new member
    // asking for 2 is refused naming the collection, and one asking for 3 lands.
    let collection = s.recall(COLLECTION).await.json();
    assert_eq!(
        the_object(&collection, COLLECTION)["next_number"],
        3,
        "{collection}"
    );
    let asking = |slug: &str, number: &str| {
        json!({"kind": "decision", "handle": slug, "name": format!("Rule {slug}"),
               "source": "user-named", "parent": COLLECTION, "sets": {"number": number}})
    };
    s.refused("add_entity", asking("contract-ordinance-story-fences", "2"))
        .await
        .says(COLLECTION);
    s.call("add_entity", asking("contract-ordinance-story-gates", "3"))
        .await;
    story.finish().await;
}

/// **A merge refused for a number is told the ways a merge can go on, and each
/// way works.** Two numbered members of one collection cannot be folded into
/// one, because the survivor would serve one number and free the other. The
/// refusal an EDIT earns for the same reason sends the caller back to the number
/// it holds, and a merge has no number to send. The merge's refusal names the two
/// routes that exist for a merge: take back the claim that gave one of the
/// numbers and merge again, or archive the duplicate, which keeps its number. The
/// edit's refusal names neither archiving. Both routes are followed here, on two
/// pairs.
#[tokio::test]
async fn a_merge_refused_for_a_number_is_told_the_two_ways_that_work_and_each_works() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    for (handle, name, number) in [
        (CURFEW, "Curfew", "1"),
        (PARKING, "Parking", "2"),
        (SIGNS, "Sign height", "3"),
        (FIREWORKS, "Noise hours", "4"),
    ] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named",
                   "parent": COLLECTION, "sets": {"number": number}}),
        )
        .await;
    }

    // ── the edit that frees a number: sent back to the number it holds ──────
    let edit = s
        .refused(
            "capture",
            json!({"subject": PARKING, "content": "renumbered", "provenance": "testimony",
                   "sets": {"number": "9"}}),
        )
        .await;
    edit.says(&format!("{COLLECTION}/2"));
    edit.never_says("archive_entity");

    // ── the merge that frees a number: told the two ways a merge can go on ──
    let merge = s
        .refused(
            "merge_entities",
            json!({"duplicate": PARKING, "survivor": CURFEW, "reason": "the same ordinance"}),
        )
        .await;
    merge.says("retract").says("archive_entity");
    merge.says(&format!("{COLLECTION}/"));
    // Nothing moved: both numbers still name their members.
    s.recall(&format!("{COLLECTION}/1")).await.says(CURFEW);
    s.recall(&format!("{COLLECTION}/2")).await.says(PARKING);

    // ── way one: take back the claim that gave the duplicate its number ─────
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
    s.merge_entities(PARKING, CURFEW, "the same ordinance", None)
        .await;
    s.recall(&format!("{COLLECTION}/1")).await.says(CURFEW);

    // ── way two: archive the duplicate, which keeps its number ──────────────
    s.refused(
        "merge_entities",
        json!({"duplicate": FIREWORKS, "survivor": SIGNS, "reason": "the same ordinance"}),
    )
    .await
    .says("archive_entity");
    s.call(
        "archive_entity",
        json!({"handle": FIREWORKS, "reason": "the same ordinance as signs"}),
    )
    .await;
    s.recall(&format!("{COLLECTION}/4")).await.says(FIREWORKS);
    story.finish().await;
}

/// **A move refused for a number is told the member can stay or be archived,
/// and the edit and the merge keep their own advice.** Moving a numbered member
/// out of its collection would free its number, so a move with a new `parent` is
/// refused. An edit is sent back to the number the member holds and a merge is
/// told to retract or archive, and neither is a way a move can go on. The move's
/// advice names the `parent` to leave off and `archive_entity`, and both are
/// followed here.
#[tokio::test]
async fn a_move_refused_for_a_number_is_told_to_leave_it_or_archive_it_and_each_works() {
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
    s.add(OUTSIDER, "Someone elsewhere").await;

    // ── the move: told to leave the member where it is, or to archive it ────
    let moved = s
        .refused(
            "rename_entity",
            json!({"handle": PARKING, "to": PARKING, "parent": OUTSIDER}),
        )
        .await;
    moved
        .says(&format!("{COLLECTION}/2"))
        .says("parent")
        .says("archive_entity")
        .never_says("retract");

    // ── beside it, the edit and the merge keep theirs ───────────────────────
    s.refused(
        "capture",
        json!({"subject": PARKING, "content": "renumbered", "provenance": "testimony",
               "sets": {"number": "9"}}),
    )
    .await
    .says(&format!("{COLLECTION}/2"))
    .never_says("archive_entity")
    .never_says("parent");
    s.refused(
        "merge_entities",
        json!({"duplicate": PARKING, "survivor": CURFEW, "reason": "the same ordinance"}),
    )
    .await
    .says("retract")
    .says("archive_entity")
    .never_says("parent");

    // ── leaving it where it is: nothing moved, and the member takes writes ──
    s.recall(&format!("{COLLECTION}/2")).await.says(PARKING);
    s.call(
        "capture",
        json!({"subject": PARKING, "content": "no parking after ten", "provenance": "testimony"}),
    )
    .await;

    // ── archiving it: the number still names it ─────────────────────────────
    s.call(
        "archive_entity",
        json!({"handle": PARKING, "reason": "no longer in force"}),
    )
    .await;
    s.recall(&format!("{COLLECTION}/2")).await.says(PARKING);
    story.finish().await;
}

/// **A mention written as a member's number links the member.** "See
/// ordinance 2" is written as the collection's handle, a slash and the number,
/// the way `recall` takes it; the claim links the member and reads back as the
/// member's handle, and a number nobody holds is refused naming the collection
/// and the number, so the sentence never links the collection with `/2` left
/// as words.
#[tokio::test]
async fn a_mention_written_as_a_members_number_links_the_member_or_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    let (kind, slug) = PARKING.split_once(':').expect("a handle");
    s.call(
        "add_entity",
        json!({"kind": kind, "handle": slug, "name": "Parking", "source": "user-named",
               "parent": COLLECTION, "sets": {"number": "2"}}),
    )
    .await;
    s.add("thing:kettle", "Kettle").await;

    let landed = s
        .call(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("parked under @{COLLECTION}/2")}),
        )
        .await
        .json();
    let held = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    let served = held["objects"][0]["facts"][0]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        served.contains(PARKING) && !served.contains("/2"),
        "the claim does not link the member: {served} ({landed})"
    );

    let refused = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("parked under @{COLLECTION}/999")}),
        )
        .await;
    refused.says(COLLECTION).says("999");
    story.finish().await;
}

const INNER: &str = "topic:contract-ordinances-guard";
const INNER_MEMBER: &str = "work:zeta";

/// **A number held below a collection that numbers on its own is found.** The
/// town's ordinances are numbered, and one of them is a guard collection that
/// numbers its own members by another key. "See ordinance 2" names a member that
/// sits under the guard collection: the claim links it, and a number nobody holds
/// is refused naming the town's collection and the number, not the numbering
/// marker of the guard collection that stands in the way.
#[tokio::test]
async fn a_mention_of_a_number_held_below_a_numbering_collection_links_the_member() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    let (inner_kind, inner_slug) = INNER.split_once(':').expect("a handle");
    s.call(
        "add_entity",
        json!({"kind": inner_kind, "handle": inner_slug, "name": "Guard ordinances",
               "source": "user-named", "parent": COLLECTION,
               "sets": {"numbers_by": "subnumber"}}),
    )
    .await;
    let (kind, slug) = INNER_MEMBER.split_once(':').expect("a handle");
    s.call(
        "add_entity",
        json!({"kind": kind, "handle": slug, "name": "Zeta", "source": "user-named",
               "parent": INNER, "sets": {"number": "2", "subnumber": "1"}}),
    )
    .await;
    s.add("thing:kettle", "Kettle").await;

    s.call(
        "capture",
        json!({"subject": "thing:kettle", "provenance": "testimony",
               "content": format!("kept under @{COLLECTION}/2")}),
    )
    .await;
    let held = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    let served = held["objects"][0]["facts"][0]["content"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        served.contains(INNER_MEMBER) && !served.contains("/2"),
        "the claim does not link the member held below the guard collection: {served}"
    );

    let refused = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("kept under @{COLLECTION}/9")}),
        )
        .await;
    refused.says(COLLECTION).says("9");
    assert!(
        !refused.json().to_string().contains("/#"),
        "the refusal names a numbering marker, which is no thing: {}",
        refused.json()
    );
    story.finish().await;
}
