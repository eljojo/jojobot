//! "The third paragraph of a long note has a typo. Fix that, and nothing else."
//!
//! **An edit can name the words to change instead of resending the text.**
//! `update_fact` takes `find`, `replace` and `field`. The words have to occur
//! exactly once in the text as it stands, and the answer shows what was taken
//! out and what was put in, so a loss is visible at once.
//!
//! **The negatives are what make the edit safe.** Words that occur twice, and
//! words that occur nowhere, are refused with the count and the text is
//! unchanged. An edit made against a revision the claim has moved past is
//! refused too.

use serde_json::{Value, json};

use super::dsl::Story;

const NOTE: &str = "first paragraph\n\nsecond paragraph has a typo in it\n\nthird paragraph";

/// The claim's `details` as `recall` serves them, whole.
fn details_of(recalled: &Value, address: &str) -> Value {
    recalled["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["address"] == address))
        .map(|fact| fact["details"].clone())
        .unwrap_or_else(|| panic!("the claim {address} is not in {recalled}"))
}

#[tokio::test]
async fn one_span_of_a_long_note_is_changed_and_every_other_byte_stays() {
    let story = Story::begin("bot:gamma").await;
    let run = story.session().await;
    run.add("person:kenny", "Kenny").await;
    let claim = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "a long note",
                "details": NOTE, "provenance": "inference",
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    // **The words occur three times: refused with the count, text unchanged.**
    let refused = run
        .refused(
            "update_fact",
            json!({
                "address": claim, "find": "paragraph", "replace": "part",
                "field": "details", "provenance": "inference",
            }),
        )
        .await;
    refused.says("\"matches\":3").says("\"fix_by\":\"change\"");
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        NOTE
    );

    // **Words that occur nowhere: the count is 0.**
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "a TYPO", "replace": "a fix",
            "field": "details", "provenance": "inference",
        }),
    )
    .await
    .says("\"matches\":0");
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        NOTE,
        "a find that matched nothing left the text as it was"
    );

    // **One occurrence lands, and the answer shows what changed.**
    let receipt = run
        .call(
            "update_fact",
            json!({
                "address": claim, "find": "a typo", "replace": "a fix",
                "field": "details", "provenance": "inference",
            }),
        )
        .await
        .json();
    assert_eq!(receipt["text_edit"]["removed"], "a typo", "{receipt}");
    assert_eq!(receipt["text_edit"]["inserted"], "a fix", "{receipt}");
    assert_eq!(
        receipt["text_edit"]["bytes_after"].as_u64().unwrap() + 1,
        receipt["text_edit"]["bytes_before"].as_u64().unwrap(),
        "{receipt}"
    );
    let recalled = run.recall_with_details("person:kenny").await.json();
    assert_eq!(
        details_of(&recalled, &claim),
        "first paragraph\n\nsecond paragraph has a fix in it\n\nthird paragraph"
    );
    // **The receipt's words are read against the claim, not only against what
    // was sent**: what it says was taken out was in the text before, and what it
    // says was put in is in the text now.
    let after = details_of(&recalled, &claim);
    let after = after.as_str().expect("the details are text");
    let removed = receipt["text_edit"]["removed"].as_str().expect("removed");
    let inserted = receipt["text_edit"]["inserted"].as_str().expect("inserted");
    assert!(
        NOTE.contains(removed) && !after.contains(removed),
        "{receipt}"
    );
    assert!(
        after.contains(inserted) && !NOTE.contains(inserted),
        "{receipt}"
    );

    // **A caller that read an earlier revision is refused, whatever the words.**
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "a fix", "replace": "a better fix",
            "field": "details", "provenance": "inference", "expected_revision": 1,
        }),
    )
    .await
    .says("\"fix_by\":\"retry\"")
    .says("expected_revision: 2");
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        "first paragraph\n\nsecond paragraph has a fix in it\n\nthird paragraph",
        "a stale edit left the text as the last edit made it"
    );
    story.finish().await;
}

/// A claim to edit: `person:kenny` holds one note, and its address comes back.
async fn kenny_with_the_note(story: &Story) -> (super::dsl::Session, String) {
    let run = story.session().await;
    run.add("person:kenny", "Kenny").await;
    let claim = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "a long note",
                "details": NOTE, "provenance": "inference",
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    (run, claim)
}

/// **An address that names no claim is refused as it is for a plain edit, whether
/// or not the caller named a revision.** The refusal lists the addresses that do
/// exist, so the caller does not have to guess. The same words on a real address
/// still land, with and without a revision.
#[tokio::test]
async fn a_find_at_an_address_that_names_no_claim_is_refused_as_a_plain_edit_is() {
    let story = Story::begin("bot:gamma").await;
    let (run, claim) = kenny_with_the_note(&story).await;
    let find = |address: &str, revision: Option<u32>| {
        let mut args = json!({
            "address": address, "find": "a typo", "replace": "a fix",
            "field": "details", "provenance": "inference",
        });
        if let Some(revision) = revision {
            args["expected_revision"] = json!(revision);
        }
        args
    };
    let plain =
        |address: &str| json!({"address": address, "content": "words", "provenance": "inference"});

    for (label, address) in [
        ("a fact id that names no claim", "person:kenny#f99"),
        ("an entity that is not known", "person:milhouse#f1"),
    ] {
        let expected = run.refused("update_fact", plain(address)).await.json();
        for revision in [None, Some(1)] {
            let refused = run
                .refused("update_fact", find(address, revision))
                .await
                .json();
            assert_eq!(
                refused, expected,
                "{label}, revision {revision:?}: the find is refused as a plain edit is"
            );
        }
    }
    // The positive the refusals depend on: the refusal for a missing fact id
    // lists the claim that does exist.
    let missing = run
        .refused("update_fact", find("person:kenny#f99", Some(1)))
        .await;
    missing.says(&claim);

    // …and the real address, with and without a revision, still edits.
    for (words, revision) in [("a typo", None), ("a fix", Some(2))] {
        let mut args = json!({
            "address": claim, "find": words, "replace": "a fix",
            "field": "details", "provenance": "inference",
        });
        if let Some(revision) = revision {
            args["expected_revision"] = json!(revision);
        }
        if revision.is_some() {
            args["find"] = json!("a fix");
            args["replace"] = json!("a better fix");
        }
        let receipt = run.call("update_fact", args).await.json();
        assert!(receipt["text_edit"].is_object(), "{receipt}");
    }
    story.finish().await;
}

/// **The refusal for words that occur nowhere names the read that shows the
/// text**: recall, with the claim's subject and `facts: true`. The read it names
/// returns the text.
#[tokio::test]
async fn a_find_that_matches_nothing_names_the_read_that_shows_the_text() {
    let story = Story::begin("bot:gamma").await;
    let (run, claim) = kenny_with_the_note(&story).await;
    let refused = run
        .refused(
            "update_fact",
            json!({
                "address": claim, "find": "a TYPO", "replace": "a fix",
                "field": "details", "provenance": "inference",
            }),
        )
        .await;
    refused
        .says("\"matches\":0")
        .says("subject: \\\"person:kenny\\\"")
        .says("facts: true");
    // The read it names returns the text the find was meant to match.
    let read = run
        .call(
            "recall",
            json!({"subject": "person:kenny", "facts": true, "details": true}),
        )
        .await
        .json();
    assert_eq!(details_of(&read, &claim), NOTE, "{read}");
    story.finish().await;
}

/// **An edit's receipt carries the claim's revision, so the next guarded edit
/// needs no read in between.** The number is the claim's write count after the
/// edit, the same number `recall` serves as `revision_count`, and the next edit
/// sent with it lands.
#[tokio::test]
async fn an_edits_receipt_carries_the_revision_the_next_guarded_edit_sends() {
    let story = Story::begin("bot:gamma").await;
    let (run, claim) = kenny_with_the_note(&story).await;

    let first = run
        .call(
            "update_fact",
            json!({
                "address": claim, "find": "a typo", "replace": "a fix",
                "field": "details", "provenance": "inference",
            }),
        )
        .await
        .json();
    assert_eq!(first["revision"], 2, "{first}");
    assert!(
        first["revision_note"]
            .as_str()
            .is_some_and(|note| note.contains("expected_revision")),
        "the receipt says what to do with the number: {first}"
    );
    let read = run.recall_with_details("person:kenny").await.json();
    let served = read["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["address"] == claim))
        .unwrap_or_else(|| panic!("the claim is in {read}"))["revision_count"]
        .clone();
    assert_eq!(
        first["revision"], served,
        "the receipt and recall agree: {read}"
    );

    // The number from the receipt is the one the next guarded edit sends.
    let second = run
        .call(
            "update_fact",
            json!({
                "address": claim, "find": "a fix", "replace": "a better fix",
                "field": "details", "provenance": "inference",
                "expected_revision": first["revision"],
            }),
        )
        .await
        .json();
    assert_eq!(second["revision"], 3, "{second}");

    // A plain edit carries it too, and the first receipt's number is now stale.
    let plain = run
        .call(
            "update_fact",
            json!({"address": claim, "content": "a longer note", "provenance": "inference"}),
        )
        .await
        .json();
    assert_eq!(plain["revision"], 4, "{plain}");
    assert!(plain["revision_note"].is_string(), "{plain}");
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "better", "replace": "best",
            "field": "details", "provenance": "inference",
            "expected_revision": first["revision"],
        }),
    )
    .await
    .says("\"fix_by\":\"retry\"");
    story.finish().await;
}

/// **A find call that is not whole is refused and the text stays.** A find with
/// no `field`, a find with no `replace`, and a find beside a whole `details`
/// for the same field each come back blocked with `change`, and the claim reads
/// back as it was.
#[tokio::test]
async fn a_find_call_that_is_not_whole_changes_nothing() {
    let story = Story::begin("bot:gamma").await;
    let (run, claim) = kenny_with_the_note(&story).await;
    for (label, names, args) in [
        (
            "no field",
            "field",
            json!({"address": claim, "find": "a typo", "replace": "a fix",
                   "provenance": "inference"}),
        ),
        (
            "no replace",
            "replace",
            json!({"address": claim, "find": "a typo", "field": "details",
                   "provenance": "inference"}),
        ),
        (
            "a whole details beside it",
            "details",
            json!({"address": claim, "find": "a typo", "replace": "a fix",
                   "field": "details", "details": "the whole text",
                   "provenance": "inference"}),
        ),
    ] {
        run.refused("update_fact", args)
            .await
            .says("\"fix_by\":\"change\"")
            .says(names);
        assert_eq!(
            details_of(
                &run.recall_with_details("person:kenny").await.json(),
                &claim
            ),
            NOTE,
            "{label}: the text stays"
        );
    }
    story.finish().await;
}

/// **An empty `replace` deletes the words, and a find sent with the revision the
/// claim is at lands.** The text read back is the old text without the words.
#[tokio::test]
async fn an_empty_replace_deletes_and_a_find_at_the_current_revision_lands() {
    let story = Story::begin("bot:gamma").await;
    let (run, claim) = kenny_with_the_note(&story).await;
    let receipt = run
        .call(
            "update_fact",
            json!({
                "address": claim, "find": " has a typo in it", "replace": "",
                "field": "details", "provenance": "inference",
                "expected_revision": 1,
            }),
        )
        .await
        .json();
    assert_eq!(receipt["text_edit"]["inserted"], "", "{receipt}");
    assert_eq!(receipt["revision"], 2, "{receipt}");
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        "first paragraph\n\nsecond paragraph\n\nthird paragraph"
    );
    story.finish().await;
}

/// **A note that links to a thing is edited past the link.** The find means
/// what recall shows, so a caller names the handle it sees. Words after the link
/// change and the link stays a link: renaming the thing afterwards still moves
/// what the note reads as. A find that starts or ends inside the handle is
/// refused with nothing written, and one that covers the handle whole replaces
/// it.
#[tokio::test]
async fn a_note_with_a_link_is_edited_past_the_link_and_a_cut_link_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let run = story.session().await;
    run.add("person:kenny", "Kenny").await;
    run.add("person:butters", "Butters").await;
    let claim = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "a note about a visit",
                "details": "met @person:butters at noon. A typo here.",
                "provenance": "inference",
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    // **A find that starts inside the handle names no place in the stored
    // text.** It is refused and the note is as it was.
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "butters at noon", "replace": "x",
            "field": "details", "provenance": "inference",
        }),
    )
    .await
    .says("whole handle")
    .says("\"fix_by\":\"change\"");
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        "met @person:butters at noon. A typo here."
    );

    // **The words after the link change, and the link stays a link.**
    run.call(
        "update_fact",
        json!({
            "address": claim, "find": "A typo", "replace": "A fix",
            "field": "details", "provenance": "inference",
        }),
    )
    .await;
    run.call(
        "rename_entity",
        json!({"handle": "person:butters", "to": "person:marjorine"}),
    )
    .await;
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        "met @person:marjorine at noon. A fix here.",
        "the edit turned the link into the words that were typed"
    );

    // **A find that covers the handle whole replaces the link.**
    run.call(
        "update_fact",
        json!({
            "address": claim, "find": "@person:marjorine", "replace": "a friend",
            "field": "details", "provenance": "inference",
        }),
    )
    .await;
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &claim
        ),
        "met a friend at noon. A fix here."
    );
    story.finish().await;
}

/// **A find and replace inside a fence that opens on the content keeps the code
/// literal.** The content is a fence line and the details hold the code and the
/// closing fence. The words that replace the find carry a handle naming nothing
/// and a handle naming a thing: both are stored as typed, neither is refused, and
/// a rename of the thing leaves the typed handle where it was.
///
/// **Paired**: the same edit outside a fence still screens the handle that names
/// nothing.
#[tokio::test]
async fn an_edit_inside_a_fence_that_opens_on_the_content_keeps_the_code_literal() {
    let story = Story::begin("bot:gamma").await;
    let run = story.session().await;
    run.add("person:kenny", "Kenny").await;
    run.add("person:butters", "Butters").await;
    let fenced = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "```text",
                "details": "met @person:butters at noon\n```", "provenance": "inference",
            }),
        )
        .await
        .json();
    let fenced = fenced["address"]
        .as_str()
        .or_else(|| fenced["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    run.call(
        "update_fact",
        json!({
            "address": fenced, "find": "at noon",
            "replace": "with @person:butters and @person:nelson",
            "field": "details", "provenance": "inference",
        }),
    )
    .await;
    run.call(
        "rename_entity",
        json!({"handle": "person:butters", "to": "person:marjorine"}),
    )
    .await;
    assert_eq!(
        details_of(
            &run.recall_with_details("person:kenny").await.json(),
            &fenced
        ),
        "met @person:butters with @person:butters and @person:nelson\n```",
        "the code stayed as it was typed"
    );

    // The same holds for an edit to the content line, read beside the details
    // that close the fence.
    run.call(
        "update_fact",
        json!({
            "address": fenced, "find": "text", "replace": "@person:nelson",
            "field": "content", "provenance": "inference",
        }),
    )
    .await;
    run.recall_with_details("person:kenny")
        .await
        .says("```@person:nelson");

    let outside = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "a note about a visit",
                "details": "met a friend at noon.", "provenance": "inference",
            }),
        )
        .await
        .json();
    let outside = outside["address"]
        .as_str()
        .or_else(|| outside["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    run.refused(
        "update_fact",
        json!({
            "address": outside, "find": "at noon", "replace": "with @person:nelson",
            "field": "details", "provenance": "inference",
        }),
    )
    .await
    .says("person:nelson");
    story.finish().await;
}
