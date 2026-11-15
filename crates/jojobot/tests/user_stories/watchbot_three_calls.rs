//! **An outside controller does its jobs through the app API.** Under the name
//! of an app that holds a token and three grants, it reads the working card by
//! status, records what it saw on a role under a key it was granted, pages the
//! operator, and reads the ready shelf and when a waiting card's status was
//! written. Each step is asserted by what came back, and the writes by what a
//! colleague reads afterwards through the tools a bot uses.

use jojobot_domain::app::operations::OperationSpec;
use serde_json::json;

use super::dsl::{Session, Story};

const APP: &str = "springfield-watch";

#[tokio::test]
async fn a_controller_pages_only_the_boxes_its_grant_names() {
    let post = serde_json::from_value(json!({
        "operation": "post_message", "recipients": ["bot:marge"]
    }))
    .expect("a configured recipient grant");
    let (story, token) = Story::begin_serving_an_app("bot:marge", APP, vec![post]).await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;

    let (status, posted) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "marge", "subject": "gate", "body": "The gate needs a keeper."}),
        )
        .await;
    assert_eq!(
        status, 200,
        "the granted box accepts a bare bot name: {posted}"
    );
    let inbox = s
        .call("read_mailbox", json!({"new_only": false}))
        .await
        .json();
    let message = inbox["messages"]
        .as_array()
        .expect("the bot's inbox")
        .iter()
        .find(|message| message["id"] == posted["id"])
        .unwrap_or_else(|| panic!("the page reached the granted box: {inbox}"));
    assert_eq!(message["body"], "The gate needs a keeper.");
    assert_eq!(message["sender"], format!("app/{APP}"));

    let (status, refused) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "bot:bart", "body": "The gate needs a keeper."}),
        )
        .await;
    assert_eq!(status, 403, "an existing box outside the grant: {refused}");
    let why = refused["error"].as_str().expect("a grant refusal");
    assert!(
        why.contains("bot:marge") && why.contains("bot:bart"),
        "{why}"
    );
}

/// **A controller whose grant lists recipients, on an instance that has an
/// operator and a colleague**: the operator is Homer, the colleague is Marge.
async fn a_controller_granted_pages_to(recipients: &[&str]) -> (Story, String, Session) {
    let post = serde_json::from_value(json!({
        "operation": "post_message", "recipients": recipients
    }))
    .expect("a configured recipient grant");
    let (story, token) = Story::begin_serving_an_app("bot:marge", APP, vec![post]).await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.call(
        "capture",
        json!({"subject": "topic:instance", "content": "who the operator is",
               "provenance": "testimony", "sets": {"operator": "person:homer"}}),
    )
    .await;
    // One boot of the bot holds one run: a second would be offered a resume.
    (story, token, s)
}

/// **A grant that lists the operator pages the operator, and no other box.** The
/// page is in the operator's box; the same call to a colleague's box is refused
/// with the handles of both, and nothing reaches it.
#[tokio::test]
async fn a_grant_that_lists_the_operator_pages_the_operator_and_no_other_box() {
    let (story, token, s) = a_controller_granted_pages_to(&["person:homer"]).await;
    let (status, page) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "person:homer", "body": "The gate is open.", "subject": "gate"}),
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(
        story.operators_mail("person:homer").await,
        [("gate".to_string(), "The gate is open.".to_string())],
        "the page is in the operator's box"
    );

    let (status, refused) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "bot:marge", "body": "The gate is open.", "subject": "gate"}),
        )
        .await;
    assert_eq!(status, 403, "a box the grant does not list: {refused}");
    let why = refused["error"].as_str().expect("a grant refusal");
    assert!(
        why.contains("person:homer") && why.contains("bot:marge"),
        "{why}"
    );
    let inbox = s
        .call("read_mailbox", json!({"new_only": false}))
        .await
        .json();
    assert_eq!(
        inbox["messages"].as_array().map(Vec::len),
        Some(0),
        "nothing reached the refused box: {inbox}"
    );
}

/// **A grant that lists a colleague refuses the operator's page**: a list names
/// every box the app may reach, so the operator is no default beside it. The
/// operator's box holds nothing afterwards. Paired in the same story with the page
/// the grant does allow: the colleague's box holds that one page and no other, so
/// the refusal was the operator's alone and the colleague's box is a real box that
/// would show a page that went astray.
#[tokio::test]
async fn a_grant_that_lists_only_a_colleague_refuses_the_operator_page() {
    let (story, token, s) = a_controller_granted_pages_to(&["bot:marge"]).await;
    // **The operator's box exists and holds a note of the colleague's own**, so
    // reading it is not reading a box nobody owns, which reads empty whatever
    // happened: the helper returns nothing for a box that is not there.
    s.call(
        "post_message",
        json!({"to": "person:homer", "body": "A note first.", "subject": "note"}),
    )
    .await;
    let note = [("note".to_string(), "A note first.".to_string())];
    assert_eq!(
        story.operators_mail("person:homer").await,
        note,
        "the operator's box is there and holds the colleague's note"
    );
    let (status, refused) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "person:homer", "body": "The gate is open.", "subject": "gate"}),
        )
        .await;
    assert_eq!(status, 403, "the operator is not on the list: {refused}");
    let why = refused["error"].as_str().expect("a grant refusal");
    assert!(
        why.contains("bot:marge") && why.contains("person:homer"),
        "{why}"
    );
    assert_eq!(
        story.operators_mail("person:homer").await,
        note,
        "the refused page did not reach the operator"
    );
    let (status, landed) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "bot:marge", "body": "The gate is open.", "subject": "gate"}),
        )
        .await;
    assert_eq!(status, 200, "the colleague is on the list: {landed}");
    let inbox = s
        .call("read_mailbox", json!({"new_only": false}))
        .await
        .json();
    let pages: Vec<(&str, &str)> = inbox["messages"]
        .as_array()
        .expect("the colleague's messages")
        .iter()
        .map(|message| {
            (
                message["subject"].as_str().unwrap_or_default(),
                message["body"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        pages,
        [("gate", "The gate is open.")],
        "the colleague holds the one page the grant allowed, and not the refused one: {inbox}"
    );
}

fn grant(operation: &str, kinds: &[&str], keys: &[&str]) -> OperationSpec {
    OperationSpec {
        operation: operation.into(),
        kinds: kinds.iter().map(|s| s.to_string()).collect(),
        keys: keys.iter().map(|s| s.to_string()).collect(),
        recipients: vec![],
    }
}

#[tokio::test]
async fn a_controller_reads_a_working_card_records_what_it_saw_pages_the_operator_and_reads_the_shelf()
 {
    let (story, token) = Story::begin_serving_an_app(
        "bot:marge",
        APP,
        vec![
            grant("recall", &[], &[]),
            grant("capture", &["role"], &["watch_state"]),
            grant("post_message", &[], &[]),
        ],
    )
    .await;
    let s = story.session().await.asking_whole();
    s.add("work:omega", "Mow the lawn").await;
    s.add("work:sigma", "Paint the fence").await;
    s.add("role:alpha", "Gatekeeper").await;
    s.add("person:homer", "Homer").await;
    for (card, status) in [("work:omega", "now"), ("work:sigma", "next")] {
        s.call(
            "capture",
            json!({"subject": card, "content": "where it stands",
                   "provenance": "testimony", "sets": {"status": status}}),
        )
        .await;
    }
    s.call(
        "capture",
        json!({"subject": "topic:instance", "content": "who the operator is",
               "provenance": "testimony", "sets": {"operator": "person:homer"}}),
    )
    .await;

    // ── 1. it reads the working card, and only that one ─────────────────────
    let (status, working) = story
        .app_calls(
            &token,
            "recall",
            json!({"kind": "work", "where": "status = now"}),
        )
        .await;
    assert_eq!(status, 200, "{working}");
    let handles: Vec<&str> = working["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|object| object["handle"].as_str())
        .collect();
    assert_eq!(handles, ["work:omega"], "{working}");
    assert_eq!(
        working["objects"][0]["fields"]["status"], "now",
        "{working}"
    );

    // ── 2. it records what it saw, under the one key it was granted ─────────
    let (status, seen) = story
        .app_calls(
            &token,
            "capture",
            json!({"subject": "role:alpha", "content": "Seen at the gate.",
                   "sets": {"watch_state": "ok"}}),
        )
        .await;
    assert_eq!(status, 200, "{seen}");
    assert_eq!(seen["subject"], "role:alpha", "{seen}");
    let role = s
        .call("recall", json!({"subject": "role:alpha"}))
        .await
        .json();
    assert_eq!(
        role["objects"][0]["fields"]["watch_state"], "ok",
        "a colleague reads what the controller recorded: {role}"
    );
    let (status, refused) = story
        .app_calls(
            &token,
            "capture",
            json!({"subject": "role:alpha", "content": "Taken.",
                   "sets": {"holder": "bot:marge"}}),
        )
        .await;
    assert_eq!(
        status, 403,
        "a key it was not granted is refused: {refused}"
    );
    let role = s
        .call("recall", json!({"subject": "role:alpha"}))
        .await
        .json();
    let held = &role["objects"][0]["fields"];
    assert!(
        held.get("holder").is_none(),
        "the refused key was not written: {role}"
    );
    assert_eq!(
        held["watch_state"], "ok",
        "the granted key is still the one that landed: {role}"
    );

    // ── 3. it pages the operator, and the page is in the operator's box ─────
    let (status, page) = story
        .app_calls(
            &token,
            "post_message",
            json!({"to": "person:homer", "body": "The gate is open.", "subject": "gate"}),
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert!(page["id"].is_string(), "the page has an id: {page}");
    assert_eq!(page["mailbox"], "person-homer", "{page}");
    assert_eq!(
        story.operators_mail("person:homer").await,
        [("gate".to_string(), "The gate is open.".to_string())],
        "the page is in the operator's box, read the way the operator's page reads it"
    );

    // ── 4. it reads the shelf, and when a waiting card's status was written ─
    for (card, name) in [
        ("work:lambda", "Rake the leaves"),
        ("work:delta", "Bag the leaves"),
        ("work:theta", "Hang the shutters"),
    ] {
        s.add(card, name).await;
    }
    let before_the_statuses = jiff::Timestamp::now();
    for (card, status, depends_on) in [
        ("work:lambda", "queued", None),
        ("work:delta", "queued", Some("work:lambda")),
        ("work:theta", "waiting", None),
    ] {
        let mut sets = json!({"status": status});
        if let Some(dependency) = depends_on {
            sets["depends_on"] = json!(dependency);
        }
        s.call(
            "capture",
            json!({"subject": card, "content": "where it stands",
                   "provenance": "testimony", "sets": sets}),
        )
        .await;
    }
    let after_the_statuses = jiff::Timestamp::now();
    let (status, shelf) = story
        .app_calls(
            &token,
            "recall",
            json!({"view": "ready", "where": "status = queued"}),
        )
        .await;
    assert_eq!(status, 200, "{shelf}");
    let ready: Vec<&str> = shelf["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|object| object["handle"].as_str())
        .collect();
    assert_eq!(
        ready,
        ["work:lambda"],
        "the card that waits on nothing is on the shelf and the one behind it is not: {shelf}"
    );
    let (status, waiting) = story
        .app_calls(
            &token,
            "recall",
            json!({"subject": "work:theta", "backing": true}),
        )
        .await;
    assert_eq!(status, 200, "{waiting}");
    let written_at = &waiting["objects"][0]["fields_backing"]["status"]["written_at"];
    let written_at: jiff::Timestamp = written_at
        .as_str()
        .and_then(|at| at.parse().ok())
        .unwrap_or_else(|| panic!("the waiting card says when its status was written: {waiting}"));
    assert!(
        before_the_statuses <= written_at && written_at <= after_the_statuses,
        "the instant is the status write's, taken between {before_the_statuses} and \
         {after_the_statuses}, not the card's creation or a constant: {written_at}"
    );
}

/// **A controller reads what a bot is working on only while the session that
/// declared it is live.** The bot declares it in a live session and the app reads
/// it; the same session wraps and the app no longer does, though the bot is still
/// there to read. The paired read in the first half is what makes the second an
/// absence of a value and not of a reader.
#[tokio::test]
async fn a_controller_reads_working_on_only_while_the_declaring_session_is_live() {
    let (story, token) =
        Story::begin_serving_an_app("bot:otto", APP, vec![grant("recall", &[], &[])]).await;
    let otto = story.session().await;
    otto.add("work:omega", "Mow the lawn").await;
    otto.call(
        "journal",
        json!({"entry": "picked up the mowing", "working_on": "work:omega"}),
    )
    .await;

    let (status, live) = story
        .app_calls(&token, "recall", json!({"subject": "bot:otto"}))
        .await;
    assert_eq!(status, 200, "{live}");
    assert_eq!(live["objects"][0]["handle"], "bot:otto", "{live}");
    assert_eq!(
        live["objects"][0]["fields"]["working_on"], "work:omega",
        "the app reads the focus while its session is live: {live}"
    );

    otto.wrap("put the mowing down").await;

    let (status, gone) = story
        .app_calls(&token, "recall", json!({"subject": "bot:otto"}))
        .await;
    assert_eq!(status, 200, "{gone}");
    assert_eq!(gone["objects"][0]["handle"], "bot:otto", "{gone}");
    assert!(
        gone["objects"][0]["fields"].get("working_on").is_none(),
        "the session is gone, so the app does not read its focus: {gone}"
    );
}
