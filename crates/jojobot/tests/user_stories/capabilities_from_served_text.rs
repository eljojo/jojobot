//! "I was told nothing. What can I do from what the server says?"
//!
//! Three capabilities a release added, each reached by a session that reads only
//! what the server serves: the description of a verb, the description of an
//! argument, a skill, and the answers it gets. **The story never types a key it
//! has not first read in served text** — the key that numbers a collection, the
//! views and the keys a view takes, the advice on `request_id`.
//!
//! **Each is paired with what it must refuse**: a number a member already holds,
//! a view asked without the input it needs, and a repeated write that must not
//! write twice.

use serde_json::json;

use super::dsl::Story;

const ORDINANCES: &str = "topic:contract-ordinances-story";
const CURFEW: &str = "decision:contract-ordinance-story-curfew";
const PARKING: &str = "decision:contract-ordinance-story-parking";

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

/// The asking skill's text, as the door serves it.
async fn skill_body(story: &Story) -> String {
    story.skill("asking").await["skill"]["body"]
        .as_str()
        .expect("the skill serves a body")
        .to_string()
}

#[tokio::test]
async fn a_cold_session_numbers_ordinances_and_recalls_one_by_its_number() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the key that numbers a collection is in the text a session reads
    // where it sets keys, on both verbs that set them ────────────────────────
    for verb in ["add_entity", "capture"] {
        let sets = s.argument_description(verb, "sets").await;
        assert!(
            sets.contains("numbers_by"),
            "{verb}'s `sets` must name the key that numbers a collection: {sets}"
        );
    }

    s.add(ORDINANCES, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": ORDINANCES, "content": "numbers its ordinances",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    for (handle, name, number) in [(CURFEW, "Curfew", "31"), (PARKING, "Parking", "32")] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named",
                   "parent": ORDINANCES, "sets": {"number": number}}),
        )
        .await;
    }

    // ── ordinance 32 is recalled by its number ──────────────────────────────
    let found = s.recall(&format!("{ORDINANCES}/32")).await.json();
    assert_eq!(found["objects"][0]["id"], PARKING, "{found}");

    // The paired refusal: 32 is held, so a second 32 is not made.
    s.refused(
        "add_entity",
        json!({"kind": "decision", "handle": "contract-ordinance-story-signs",
               "name": "Sign height", "source": "user-named",
               "parent": ORDINANCES, "sets": {"number": "32"}}),
    )
    .await
    .says(PARKING);
    story.finish().await;
}

#[tokio::test]
async fn a_cold_session_declares_a_view_that_takes_a_project_and_runs_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the views that ship, and the keys a view takes, are all in the text
    // that teaches views ─────────────────────────────────────────────────────
    // A served sentence wraps where it likes, so whitespace is compared as one space.
    let flat = |text: String| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let recall_view = flat(s.argument_description("recall", "view").await);
    let asking = flat(skill_body(&story).await);
    for text in [&recall_view, &asking] {
        for name in [
            "colleagues",
            "loops",
            "ready",
            "plan",
            "due",
            "board",
            "of-type",
        ] {
            let asked = format!("view: \"{name}\"");
            assert!(
                text.contains(&asked),
                "the view {name} must be named: {text}"
            );
        }
        for key in ["inputs", "parent", "as_of", "answers_type"] {
            assert!(
                text.contains(key),
                "the view key {key} must be named: {text}"
            );
        }
    }

    s.add("project:atlas", "Atlas").await;
    s.add("project:the-move", "The Move").await;
    let work = |slug: &str, parent: &str| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": {"status": "next"}})
    };
    s.call("add_entity", work("sigma", "project:atlas")).await;
    s.call("add_entity", work("phi", "project:the-move")).await;

    // The shape of the input is read from the answer for a view that ships.
    let board = s.recall("view:board").await.json();
    let taken = board["objects"][0]["fields"]["inputs"]
        .as_str()
        .unwrap_or_else(|| panic!("the board view says what it takes: {board}"))
        .to_string();
    // The input is named for itself, `lane`, so that a word only the missing
    // input can put in an answer is a word the answer can be asked for. The
    // shipped name, `project`, is also the kind it takes and the work's parent.
    let taken = taken.replace("project:", "lane:");
    assert!(
        taken.starts_with("lane:"),
        "the shape of the input: {taken}"
    );

    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "urgent-things", "name": "Urgent things",
               "source": "user-named",
               "sets": {"selects": "work", "inputs": taken, "parent": "$lane"}}),
    )
    .await;
    let under_atlas = s
        .call(
            "recall",
            json!({"view": "urgent-things", "inputs": {"lane": "project:atlas"}}),
        )
        .await;
    assert_eq!(
        listed(&under_atlas),
        ["work:sigma"],
        "{}",
        under_atlas.raw()
    );

    // The paired refusal: the input is required, and the refusal names it. `lane`
    // is in no other answer to this call: the call sent none, and the view is
    // asked by its own name.
    let missing = s
        .refused("recall", json!({"view": "urgent-things", "inputs": {}}))
        .await;
    missing.says("lane");
    assert_eq!(missing.json()["fix_by"], "change", "{}", missing.raw());
    story.finish().await;
}

/// How many records of a thing say `content`, read from the thing's own facts.
async fn times_said(s: &super::dsl::Session, subject: &str, content: &str) -> usize {
    let facts = s
        .call("recall", json!({"subject": subject, "facts": true}))
        .await;
    facts.json()["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| panic!("the facts of {subject}: {}", facts.raw()))
        .iter()
        .filter(|fact| fact["content"] == content)
        .count()
}

#[tokio::test]
async fn a_cold_session_sends_a_request_id_on_the_first_call_and_a_repeat_writes_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // The argument is published on the write the story uses, with a description
    // for a reader to find it by.
    let advice = s.argument_description("capture", "request_id").await;
    assert!(!advice.trim().is_empty(), "request_id is described");

    s.add("project:atlas", "Atlas").await;
    let write = json!({"subject": "project:atlas", "content": "kickoff is set",
                       "provenance": "testimony", "request_id": "kickoff-1"});
    let first = s.call("capture", write.clone()).await;
    assert!(first.json().get("repeat").is_none(), "{}", first.raw());
    let again = s.call("capture", write).await;
    again.says("repeat");
    assert_eq!(
        times_said(&s, "project:atlas", "kickoff is set").await,
        1,
        "the repeat wrote nothing"
    );

    // The negative the advice rests on: the same write sent twice with no id
    // is two writes, because nothing says the second is a resend.
    let bare = json!({"subject": "project:atlas", "content": "budget is agreed",
                      "provenance": "testimony"});
    let one = s.call("capture", bare.clone()).await;
    let two = s.call("capture", bare).await;
    assert!(one.json().get("repeat").is_none(), "{}", one.raw());
    assert!(two.json().get("repeat").is_none(), "{}", two.raw());
    assert_eq!(
        times_said(&s, "project:atlas", "budget is agreed").await,
        2,
        "a call sent without an id cannot be told from a new one"
    );
    story.finish().await;
}

/// **A capability arrives as an argument on a verb that exists, and the document
/// verbs are the served exception.** `view` is an argument of `recall` and no
/// verb is named for a view; `read_document` and `put_document` are verbs.
#[tokio::test]
async fn a_capability_arrives_as_an_argument_and_the_document_verbs_are_served_verbs() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // The argument a view arrives as is published by the verb that reads.
    let view = s.argument_description("recall", "view").await;
    assert!(!view.trim().is_empty(), "recall publishes view");
    // The negative: no verb is named for a view, and the verbs beside it are
    // there, so the list that was read is the whole list.
    s.has_no_verb_containing("view", &["recall", "search"])
        .await;
    // The exception: a document is read and written whole, by verbs.
    let served = s.served_verbs().await;
    for verb in ["read_document", "put_document"] {
        assert!(
            served.iter().any(|v| v == verb),
            "{verb} is a served verb: {served:?}"
        );
    }
    story.finish().await;
}

/// Text of exactly `len` characters, with no blank line in it.
fn filler(len: usize) -> String {
    let mut out: String = "kiln ".repeat(len / 5 + 1).chars().take(len).collect();
    // A trailing space would be trimmed on the way in and move the length.
    if out.ends_with(' ') {
        out.pop();
        out.push('k');
    }
    out
}

/// "I have a long plan to keep. Where does it go?" — every value below is read
/// from an answer the session was given: the route from the refusal, the
/// addresses from the document's reads.
#[tokio::test]
async fn a_cold_session_with_a_plan_too_long_for_a_claim_finds_the_document_verbs_from_the_refusal()
{
    use jojobot_domain::text::ANSWER_CEILING;

    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;

    // ── the text that meets a document's keys, its archived count and the
    // move names them where they are read ────────────────────────────────────
    let kinds = s.argument_description("add_entity", "kind").await;
    assert!(
        kinds.contains("stale_after"),
        "the kind list explains the document: {kinds}"
    );
    let reading = s.description("read_document").await;
    assert!(
        reading.contains("archived_sections_left_out"),
        "the read names the count it leaves out: {reading}"
    );
    let editing = s.description("update_fact").await;
    assert!(
        editing.contains("after"),
        "update_fact names after: {editing}"
    );

    // Six sections that together are longer than one claim can be.
    let each = ANSWER_CEILING / 5;
    let plan: String = (1..=6)
        .map(|n| format!("## Step {n}\n\nmarker-{n} {}\n\n", filler(each)))
        .collect();
    let plan = format!("# The plan\n\n{plan}");

    // ── the capture is refused, and the refusal names the route ─────────────
    let refusal = s
        .refused(
            "capture",
            json!({"subject": "project:atlas", "content": "the plan", "details": plan,
                   "provenance": "testimony"}),
        )
        .await;
    refusal.says("put_document");

    // ── the route: a document, and the plan put onto it ─────────────────────
    s.add("document:homer", "The plan").await;
    let put = s
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": plan}),
        )
        .await
        .json();
    let written: Vec<String> = put["addresses"]
        .as_array()
        .unwrap_or_else(|| panic!("the put answers each section's address: {put}"))
        .iter()
        .map(|a| a.as_str().expect("an address").to_string())
        .collect();
    assert_eq!(written.len(), 7, "the title and six steps: {put}");

    // ── the read answers the addresses of what it shows, in order ───────────
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let shown: Vec<String> = read["addresses"]
        .as_array()
        .unwrap_or_else(|| panic!("the read answers the addresses it shows: {read}"))
        .iter()
        .map(|a| a.as_str().expect("an address").to_string())
        .collect();
    assert!(shown.len() >= 4, "a page shows several sections: {read}");
    assert_eq!(
        shown[..],
        written[..shown.len()],
        "the same addresses, in order"
    );

    // ── the third step is edited through an address an answer gave ──────────
    let third = &shown[3];
    s.call(
        "update_fact",
        json!({"address": third, "field": "details",
               "find": "marker-3", "replace": "marker-3-done"}),
    )
    .await;
    let edited = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let text = edited["markdown"].as_str().expect("the text");
    assert_eq!(text.matches("marker-3-done").count(), 1, "{edited}");
    assert_eq!(
        text.matches("marker-4 ").count(),
        1,
        "the others stand: {edited}"
    );

    // ── a section moves with `after`, and the order the read answers moves ──
    s.call(
        "update_fact",
        json!({"address": shown[3], "after": shown[0]}),
    )
    .await;
    let moved = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let order: Vec<String> = moved["addresses"]
        .as_array()
        .expect("addresses")
        .iter()
        .map(|a| a.as_str().expect("an address").to_string())
        .collect();
    assert_eq!(
        order[1], shown[3],
        "step three now follows the title: {moved}"
    );
    assert_eq!(order[2], shown[1], "step one follows it: {moved}");

    // The pair: putting the plan again with no `after` is refused, and the
    // refusal says what `after` does.
    s.refused(
        "put_document",
        json!({"document": "document:homer", "markdown": "# Again\n\nonce more\n"}),
    )
    .await
    .says("replaces");
    story.finish().await;
}
