//! "Whose mail is it? Can one of my helper assistants quietly point it at
//! somebody else?"
//!
//! The `operator` key on the instance's record decides whose box operator mail
//! lands in: the person the assistants work for. A bot that could point it at
//! somebody else could misdirect that mail, so the key is guarded, but not
//! closed: an instance needs one somehow, and a wrong first write needs a fix.
//!
//! **While nobody is named, any bot may name the operator. Once somebody is,
//! only the bot that heads the chart may change it or take it off.** A bot heads
//! the chart when nothing is above it and something reports to it. With nobody
//! at the head, a held value is changed by nobody, and the refusal says what
//! makes a head.
//!
//! **Every refusal here sits beside the positive it depends on**, in the same
//! story: the same call from the head lands, so a build that refused every write
//! of the key would not pass. The routes that reach the key are each tried: a
//! capture, an edit of the record, the settings of an update, taking the key off
//! and a merge that carries it.

use serde_json::json;

use super::dsl::Story;

fn naming(operator: &str) -> serde_json::Value {
    json!({
        "subject": "topic:instance", "content": "who the operator is",
        "provenance": "testimony", "sets": {"operator": operator},
    })
}

#[tokio::test]
async fn the_operator_is_named_by_any_bot_and_changed_only_by_the_head_of_the_chart() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("topic:widgets", "Widgets").await;
    s.add("person:lisa", "Lisa").await;
    s.add("person:milhouse", "Milhouse").await;
    for bot in ["marge", "patty", "nelson", "ralph"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let marge = story.as_bot("bot:marge").await;
    let nelson = story.as_bot("bot:nelson").await;
    let ralph = story.as_bot("bot:ralph").await;

    // ── nobody is named yet: any bot may name the operator ──────────────────
    let address = ralph
        .call("capture", naming("person:lisa"))
        .await
        .field("address");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── named, and nobody heads the chart: nobody changes it ────────────────
    //
    // The refusal says what makes a head, so the way forward is in it.
    nelson
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false")
        .says("operator")
        .says("chart");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── a head appears: marge, the coordinator, has a report and no manager ─
    marge
        .call(
            "capture",
            json!({
                "subject": "bot:patty", "content": "patty reports to marge",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;

    // ── a bot with reports and a manager above it is not the head ───────────
    //
    // Patty has a report of its own now, and still has marge above it.
    let patty = story.as_bot("bot:patty").await;
    patty
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "ralph reports to patty",
                "provenance": "testimony", "sets": {"reports_to": "bot:patty"},
            }),
        )
        .await;
    patty
        .refused("capture", naming("person:milhouse"))
        .await
        .says("operator")
        .says("bot:marge");

    // ── every route a bot that is not the head might take is refused ────────
    nelson
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false")
        .says("operator")
        .says("bot:marge");
    nelson
        .refused(
            "update_fact",
            json!({"address": address, "sets": {"operator": "person:milhouse"}}),
        )
        .await
        .says("operator")
        .says("bot:marge");
    nelson
        .refused(
            "update_entity",
            json!({"handle": "topic:instance", "sets": {"operator": "person:milhouse"}}),
        )
        .await
        .says("\"wrote\":false")
        .says("operator")
        .says("bot:marge");
    nelson
        .refused(
            "update_fact",
            json!({"address": address, "clear_fields": ["operator"]}),
        )
        .await
        .says("operator")
        .says("bot:marge");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── the head changes it, and still names a person ───────────────────────
    marge
        .refused("capture", naming("bot:nelson"))
        .await
        .says("operator");
    marge.call("capture", naming("person:milhouse")).await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");
    // The settings of an update land for the head too, which the refusal above
    // is paired with. The head then names the operator again by a capture, so
    // the record taken off below is the newest write of the key.
    marge
        .call(
            "update_entity",
            json!({"handle": "topic:instance", "sets": {"operator": "person:lisa"}}),
        )
        .await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");
    let current = marge
        .call("capture", naming("person:milhouse"))
        .await
        .field("address");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");

    // ── a resend of the value it holds is not a change, by either door ──────
    //
    // A bot that heads nothing sends the operator the record already names. The
    // fold does not move, so nothing is asked of the chart, whether the resend
    // arrives as a new claim or as an edit of the claim that holds it. The pair:
    // the same bot naming somebody else is still refused.
    nelson
        .call("capture", naming("person:milhouse"))
        .await
        .field("address");
    nelson
        .call(
            "update_fact",
            json!({"address": current, "sets": {"operator": "person:milhouse"}}),
        )
        .await;
    nelson
        .refused("capture", naming("person:lisa"))
        .await
        .says("operator")
        .says("bot:marge");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");

    // ── the head takes it off, and then any bot may name it again ───────────
    marge
        .call(
            "update_fact",
            json!({"address": current, "clear_fields": ["operator"]}),
        )
        .await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .never_says("\"operator\":");
    nelson.call("capture", naming("person:lisa")).await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── a merge cannot carry the key onto the record either ─────────────────
    nelson
        .call(
            "capture",
            json!({
                "subject": "topic:widgets", "content": "who the operator is",
                "provenance": "testimony", "sets": {"operator": "person:lisa"},
            }),
        )
        .await;
    nelson
        .refused(
            "merge_entities",
            json!({"duplicate": "topic:widgets", "survivor": "topic:instance"}),
        )
        .await
        .says("operator");
    marge
        .call(
            "merge_entities",
            json!({"duplicate": "topic:widgets", "survivor": "topic:instance"}),
        )
        .await;

    // ── on an ordinary record the key is nobody's business ──────────────────
    //
    // The same word may mean something else on another thing, such as who runs a
    // machine. It is named, then changed by a bot that heads nothing, in an
    // instance that has a head. **Written as an inference**, because the edit is
    // another session's and a testimony claim is held to the session that wrote it.
    s.add("thing:handcart", "The Handcart").await;
    let runs = nelson
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "who runs it",
                "provenance": "inference", "sets": {"operator": "person:lisa"},
            }),
        )
        .await
        .field("address");
    ralph
        .call(
            "update_fact",
            json!({"address": runs, "sets": {"operator": "person:milhouse"}}),
        )
        .await;
    s.call("recall", json!({"subject": "thing:handcart"}))
        .await
        .says("\"operator\":\"person:milhouse\"");
    story.finish().await;
}

/// "I retired one of the helpers. Is its manager still the coordinator?"
///
/// **A report held by an archived bot still makes its manager the head.** The
/// guard in the write and the guard the stores run read the chart the same way:
/// archiving a bot takes it out of the lists, not out of the chart. Nelson's only
/// report is archived, and nelson still changes the operator; a bot with no report
/// does not, beside it.
#[tokio::test]
async fn a_report_held_by_an_archived_bot_still_makes_its_manager_the_head() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("person:milhouse", "Milhouse").await;
    for bot in ["nelson", "bart", "ralph"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let nelson = story.as_bot("bot:nelson").await;
    let ralph = story.as_bot("bot:ralph").await;
    s.call("capture", naming("person:lisa")).await;

    nelson
        .call(
            "capture",
            json!({
                "subject": "bot:bart", "content": "bart reports to nelson",
                "provenance": "testimony", "sets": {"reports_to": "bot:nelson"},
            }),
        )
        .await;
    s.call(
        "archive_entity",
        json!({"handle": "bot:bart", "reason": "retired"}),
    )
    .await;

    // The bot without a report is refused, and the one whose only report is
    // archived changes the operator.
    ralph
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false");
    nelson.call("capture", naming("person:milhouse")).await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");
    story.finish().await;
}
