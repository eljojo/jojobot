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

    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "urgent-things", "name": "Urgent things",
               "source": "user-named",
               "sets": {"selects": "work", "inputs": taken, "parent": "$project"}}),
    )
    .await;
    let under_atlas = s
        .call(
            "recall",
            json!({"view": "urgent-things", "inputs": {"project": "project:atlas"}}),
        )
        .await;
    assert_eq!(
        listed(&under_atlas),
        ["work:sigma"],
        "{}",
        under_atlas.raw()
    );

    // The paired refusal: the project is required, and the refusal says so.
    s.refused("recall", json!({"view": "urgent-things", "inputs": {}}))
        .await
        .says("project");
    story.finish().await;
}

#[tokio::test]
async fn a_cold_session_sends_a_request_id_on_the_first_call_and_a_repeat_writes_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    let advice = s.argument_description("capture", "request_id").await;
    let advice = advice.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        advice.contains("on the first call"),
        "the text says when to send it: {advice}"
    );

    s.add("project:atlas", "Atlas").await;
    let write = json!({"subject": "project:atlas", "content": "kickoff is set",
                       "provenance": "testimony", "request_id": "kickoff-1"});
    s.call("capture", write.clone()).await;
    let again = s.call("capture", write).await;
    again.says("repeat");
    let facts = s
        .call("recall", json!({"subject": "project:atlas", "facts": true}))
        .await;
    assert_eq!(
        facts.raw().matches("kickoff is set").count(),
        1,
        "the repeat wrote nothing: {}",
        facts.raw()
    );
    story.finish().await;
}

#[tokio::test]
async fn the_asking_skill_names_the_document_verbs_as_the_verbs_a_capability_does_arrive_as() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // The skill says a capability rarely arrives as a new verb and that the
    // document verbs are the exception; the verbs it points at are served.
    let asking = skill_body(&story).await;
    assert!(
        asking.contains("document verbs"),
        "the skill names the exception: {asking}"
    );
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
    for word in ["after", "request_id"] {
        assert!(
            editing.contains(word),
            "update_fact names {word}: {editing}"
        );
    }

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
