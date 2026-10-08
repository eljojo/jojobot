//! "Who may say who the operator is?"
//!
//! The `operator` key on the instance's record decides whose box operator mail
//! lands in. A bot that could point it at somebody else could misdirect that
//! mail, so the key is guarded, but not closed: an instance needs one somehow,
//! and a wrong first write needs a fix.
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
        "provenance": "testimony", "fields": {"operator": operator},
    })
}

#[tokio::test]
async fn the_operator_is_named_by_any_bot_and_changed_only_by_the_head_of_the_chart() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("topic:instance", "This instance").await;
    s.add("topic:widgets", "Widgets").await;
    s.add("person:lisa", "Lisa").await;
    s.add("person:milhouse", "Milhouse").await;
    for bot in ["alpha", "beta", "omega", "sigma"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let alpha = story.as_bot("bot:alpha").await;
    let omega = story.as_bot("bot:omega").await;
    let sigma = story.as_bot("bot:sigma").await;

    // ── nobody is named yet: any bot may name the operator ──────────────────
    let address = sigma
        .call("capture", naming("person:lisa"))
        .await
        .field("address");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── named, and nobody heads the chart: nobody changes it ────────────────
    //
    // The refusal says what makes a head, so the way forward is in it.
    omega
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false")
        .says("operator")
        .says("chart");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── a head appears: alpha has a report and no manager ───────────────────
    alpha
        .call(
            "capture",
            json!({
                "subject": "bot:beta", "content": "beta reports to alpha",
                "provenance": "testimony", "fields": {"reports_to": "bot:alpha"},
            }),
        )
        .await;

    // ── a bot with reports and a manager above it is not the head ───────────
    //
    // Beta has a report of its own now, and still has alpha above it.
    let beta = story.as_bot("bot:beta").await;
    beta.call(
        "capture",
        json!({
            "subject": "bot:sigma", "content": "sigma reports to beta",
            "provenance": "testimony", "fields": {"reports_to": "bot:beta"},
        }),
    )
    .await;
    beta.refused("capture", naming("person:milhouse"))
        .await
        .says("operator")
        .says("bot:alpha");

    // ── every route a bot that is not the head might take is refused ────────
    omega
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false")
        .says("operator")
        .says("bot:alpha");
    omega
        .refused(
            "update_fact",
            json!({"address": address, "fields": {"operator": "person:milhouse"}}),
        )
        .await
        .says("operator")
        .says("bot:alpha");
    omega
        .refused(
            "update_entity",
            json!({"handle": "topic:instance", "sets": {"operator": "person:milhouse"}}),
        )
        .await
        .says("operator");
    omega
        .refused(
            "update_fact",
            json!({"address": address, "clear_fields": ["operator"]}),
        )
        .await
        .says("operator")
        .says("bot:alpha");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── the head changes it, and still names a person ───────────────────────
    alpha
        .refused("capture", naming("bot:omega"))
        .await
        .says("operator");
    let current = alpha
        .call("capture", naming("person:milhouse"))
        .await
        .field("address");
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");

    // ── the head takes it off, and then any bot may name it again ───────────
    alpha
        .call(
            "update_fact",
            json!({"address": current, "clear_fields": ["operator"]}),
        )
        .await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .never_says("\"operator\":");
    omega.call("capture", naming("person:lisa")).await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:lisa\"");

    // ── a merge cannot carry the key onto the record either ─────────────────
    omega
        .call(
            "capture",
            json!({
                "subject": "topic:widgets", "content": "who the operator is",
                "provenance": "testimony", "fields": {"operator": "person:lisa"},
            }),
        )
        .await;
    omega
        .refused(
            "merge_entities",
            json!({"duplicate": "topic:widgets", "survivor": "topic:instance"}),
        )
        .await
        .says("operator");
    alpha
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
    let runs = omega
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "who runs it",
                "provenance": "inference", "fields": {"operator": "person:lisa"},
            }),
        )
        .await
        .field("address");
    sigma
        .call(
            "update_fact",
            json!({"address": runs, "fields": {"operator": "person:milhouse"}}),
        )
        .await;
    s.call("recall", json!({"subject": "thing:handcart"}))
        .await
        .says("\"operator\":\"person:milhouse\"");
    story.finish().await;
}

/// **A report held by an archived bot still makes its manager the head.** The
/// guard in the write and the guard the stores run read the chart the same way:
/// archiving a bot takes it out of the lists, not out of the chart. Omega's only
/// report is archived, and omega still changes the operator; a bot with no report
/// does not, beside it.
#[tokio::test]
async fn a_report_held_by_an_archived_bot_still_makes_its_manager_the_head() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("topic:instance", "This instance").await;
    s.add("person:lisa", "Lisa").await;
    s.add("person:milhouse", "Milhouse").await;
    for bot in ["omega", "psi", "sigma"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let omega = story.as_bot("bot:omega").await;
    let sigma = story.as_bot("bot:sigma").await;
    s.call("capture", naming("person:lisa")).await;

    omega
        .call(
            "capture",
            json!({
                "subject": "bot:psi", "content": "psi reports to omega",
                "provenance": "testimony", "fields": {"reports_to": "bot:omega"},
            }),
        )
        .await;
    s.call(
        "archive_entity",
        json!({"handle": "bot:psi", "reason": "retired"}),
    )
    .await;

    // The bot without a report is refused, and the one whose only report is
    // archived changes the operator.
    sigma
        .refused("capture", naming("person:milhouse"))
        .await
        .says("\"wrote\":false");
    omega.call("capture", naming("person:milhouse")).await;
    s.call("recall", json!({"subject": "topic:instance"}))
        .await
        .says("\"operator\":\"person:milhouse\"");
    story.finish().await;
}
