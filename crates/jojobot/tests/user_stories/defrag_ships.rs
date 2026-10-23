//! "A fresh jojobot has its organize stage from its first day."
//!
//! `defrag` arrives with the software, as `assistant` does: nobody adds it, and
//! it answers with the charter and the one-liner the build ships and reports to
//! the assistant. These stories never call `add_entity` for it.

use serde_json::json;

use super::dsl::Story;

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_fresh_instance_boots_defrag_and_its_queue_is_the_unreviewed_read() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("person:homer", "Homer").await;
    let defrag = story.as_bot("bot:defrag").await;

    let (booted, _) = story
        .call(
            "start_here",
            json!({"bot": "defrag", "brief": true, "resume": "new"}),
        )
        .await;
    let before = booted.json()["identity"]["review_queue"]["count"]
        .as_u64()
        .expect("the boot names how many wait");
    let waiting = listed(&defrag.call("recall", json!({"unreviewed": true})).await);
    assert_eq!(before, waiting.len() as u64, "{waiting:?}");
    assert!(waiting.contains(&"person:homer".to_string()), "{waiting:?}");

    defrag
        .call(
            "update_entity",
            json!({"handle": "person:homer", "reviewed": true}),
        )
        .await;
    let (booted, _) = story
        .call(
            "start_here",
            json!({"bot": "defrag", "brief": true, "resume": "new"}),
        )
        .await;
    let after = booted.json()["identity"]["review_queue"]["count"]
        .as_u64()
        .expect("the boot names how many wait");
    let waiting_after = listed(&defrag.call("recall", json!({"unreviewed": true})).await);
    assert_eq!(after, before - 1, "the boot's count fell by one");
    assert_eq!(waiting_after.len() as u64, before - 1, "{waiting_after:?}");
}

#[tokio::test]
async fn defrag_arrives_with_the_charter_and_the_one_liner_the_build_ships() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;

    let charter = dev
        .call("recall", json!({"subject": "bot:defrag", "charter": true}))
        .await;
    let object = &charter.json()["objects"][0];
    // The build's charter is served whole to a reader that is not the bot, and
    // the one-liner the build ships rides on the thing's fields.
    assert_eq!(object["charter_elided"], false, "{object}");
    assert!(
        object["charter"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        "{object}"
    );
    assert!(
        object["fields"]["one_liner"]
            .as_str()
            .is_some_and(|line| !line.trim().is_empty()),
        "{object}"
    );
}

#[tokio::test]
async fn defrag_reports_to_the_assistant_on_a_fresh_instance() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;

    let held = dev
        .call("recall", json!({"subject": "bot:defrag"}))
        .await
        .json();
    assert_eq!(
        held["objects"][0]["fields"]["reports_to"], "bot:assistant",
        "{held}"
    );
}

/// **A store that held a thing named like the shipped bot before the software
/// started still boots the shipped bot, and says it was told apart.** The topic
/// `Defrag` resembles `bot:defrag`, which the near-miss screen would refuse for a
/// caller; the software's own handle is created past the screen with the reason
/// kept on the bot as `told_apart_by`. The pair is a store with no neighbour,
/// which creates the bot with no such reason.
#[tokio::test]
async fn a_store_holding_a_topic_named_defrag_still_boots_defrag_and_keeps_why() {
    use jojobot_domain::memory::{EntityId, NewEntity};
    let neighbour = Story::begin_over_a_prefilled_store(
        "bot:otto",
        vec![NewEntity::new(
            EntityId("topic:defrag".into()),
            "Defrag",
            "user-named",
        )],
    )
    .await;
    let told = neighbour
        .session()
        .await
        .call(
            "recall",
            json!({"subject": "bot:defrag", "keys": ["told_apart_by"]}),
        )
        .await
        .json();
    assert_eq!(told["objects"][0]["id"], "bot:defrag", "{told}");
    assert!(
        told["objects"][0]["fields"]["told_apart_by"]
            .as_str()
            .is_some_and(|why| !why.trim().is_empty()),
        "the bot says why it was created beside a resembling thing: {told}"
    );
    neighbour.finish().await;

    let alone = Story::begin("bot:otto").await;
    let plain = alone
        .session()
        .await
        .call(
            "recall",
            json!({"subject": "bot:defrag", "keys": ["told_apart_by"]}),
        )
        .await
        .json();
    assert_eq!(plain["objects"][0]["id"], "bot:defrag", "{plain}");
    assert!(
        plain["objects"][0]["fields"].get("told_apart_by").is_none(),
        "with no neighbour there was nothing to be told apart from: {plain}"
    );
    alone.finish().await;
}

/// **A shipped bot that an operator's store holds without its box gets the box at
/// the start and takes a post.** The paired control is a bot the software does
/// not ship, held the same way: the start opens no box for it.
#[tokio::test]
async fn a_prefilled_defrag_with_no_box_gets_one_at_the_start_and_a_boxless_other_bot_gets_none() {
    use jojobot_domain::memory::{EntityId, NewEntity};
    let story = Story::begin_over_a_prefilled_store(
        "bot:otto",
        vec![
            NewEntity::new(EntityId("bot:defrag".into()), "Defrag", "jojobot"),
            NewEntity::new(EntityId("bot:lisa".into()), "Lisa", "user-named"),
        ],
    )
    .await;
    // Read the way the bots read: defrag polls its own box and finds it there.
    let (before, _, _) = super::box_as::counts_as(&story, "bot:defrag").await;

    let s = story.session().await;
    s.post("defrag", "A first note", "the box takes it").await;
    let (after, _, _) = super::box_as::counts_as(&story, "bot:defrag").await;
    assert_eq!(after, before + 1, "the box took the note");
    // The start opened no box for a bot it does not ship, so a post to it is
    // turned back, naming the bot it was meant for and writing nothing.
    s.refused(
        "post_message",
        json!({"to": "lisa", "subject": "Nobody home", "body": "no box to land in"}),
    )
    .await
    .says("bot:lisa")
    .says("\"wrote\":false")
    // Said of a bot that exists: the refusal sends the caller to the boot that
    // opens its box, and not to a bot of the same name that it is "the name of".
    .says("start_here");
    story.finish().await;
}
