//! "Why does my bot's page list a row of note ids I never wrote?"
//!
//! **A thing's page leaves out jojobot's bookkeeping keys exactly as `recall`
//! does**, decided by the one function `recall` asks (rule 598): the release
//! notes a bot was told and the stamp defrag sets on a thing it touched. Each case
//! reads the Fields table of the page, and each is paired with an ordinary key on
//! the same thing, which the table still shows. The keys are written by the
//! code that writes them in production: a bot made through the surface, a bot an
//! older build made and the start sends the notes, and a thing defrag marks.

use serde_json::json;

use super::dsl::Story;

/// A bot an older build made, with its box, written into the stores under the
/// server: the start sends it the shipped notes and records that it was told.
async fn an_old_bot(story: &Story, slug: &str, name: &str) {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes};
    use jojobot_domain::memory::{EntityId, NewEntity};
    let id = EntityId(format!("bot:{slug}"));
    story
        .memory_store()
        .add_entity(NewEntity::new(id.clone(), name, "user-named"))
        .await
        .expect("the bot is written")
        .written()
        .expect("nothing blocks a roster bot");
    story
        .mail_store()
        .create_mailbox(&MailboxName(slug.to_string()), &id, None)
        .await
        .expect("its box opens");
    story.the_store_changed_underneath().await;
}

/// **A bot's page holds no row for the release notes it was told.** One bot is
/// made through the surface, which tells it every shipped note, and one is a bot an
/// older build made, which the start sends the notes and then records. Both hold
/// the key, which a read that names it proves; neither page shows it. The paired
/// positive is an ordinary key written on the same bots.
#[tokio::test]
async fn a_bots_page_shows_no_row_for_the_release_notes_it_was_told() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    an_old_bot(&story, "gamma", "Gamma").await;
    let later = story.restarted().await;
    let reader = later.session_in("UTC", Some("new")).await;
    for handle in ["bot:delta", "bot:gamma"] {
        reader
            .call(
                "capture",
                json!({"subject": handle, "content": "keeps the kiln lit",
                       "provenance": "testimony", "sets": {"favourite_colour": "green"}}),
            )
            .await;
        let held = reader
            .call(
                "recall",
                json!({"subject": handle, "keys": ["release_notes_told"]}),
            )
            .await
            .json();
        assert!(
            held["objects"][0]["fields"]["release_notes_told"].is_string(),
            "{handle} holds the key the page must not show: {held}"
        );
        let fields = later.page(handle).await.section("fields");
        fields.says("favourite_colour");
        fields.never_says("release_notes_told");
    }
    later.finish().await;
}

/// **A thing defrag marked reviewed shows no `defragged_at` row.** The stamp is
/// set by defrag's own write, and a read that names it proves the thing holds it.
/// The paired positive is an ordinary key on the same thing, which the table shows.
#[tokio::test]
async fn a_page_shows_no_row_for_the_stamp_defrag_leaves() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.call(
        "capture",
        json!({"subject": "person:homer", "content": "likes donuts",
               "provenance": "testimony", "sets": {"favourite_colour": "green"}}),
    )
    .await;
    let defrag = story.as_bot("bot:defrag").await;
    defrag
        .call(
            "update_entity",
            json!({"handle": "person:homer", "reviewed": true}),
        )
        .await;
    let held = s
        .call(
            "recall",
            json!({"subject": "person:homer", "keys": ["defragged_at"]}),
        )
        .await
        .json();
    assert!(
        held["objects"][0]["fields"]["defragged_at"].is_string(),
        "homer holds the stamp the page must not show: {held}"
    );

    let fields = story.page("person:homer").await.section("fields");
    fields.says("favourite_colour");
    fields.never_says("defragged_at");
    story.finish().await;
}
