//! "I renamed the view the software ships, and after a restart it had its old
//! name again."
//!
//! **A name and aliases the operator puts on a record the build supplies are
//! listed under the operator's name, and still are once the server is served
//! again over the same stores.** The stories run over the in-memory store, so the
//! fault this story guards, a name written to the log and filed where no reader
//! looks, is the real-store case's to see in `dolt_store.rs`
//! (`a_name_and_aliases_set_on_a_supplied_record_are_served_back_over_the_real_store`);
//! this story is the reach: that a session can do it and read it back.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_name_put_on_a_shipped_view_is_listed_and_survives_a_restart() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.call(
        "update_entity",
        json!({"handle": "view:loops", "name": "Weekly rounds", "aliases": ["rounds"]}),
    )
    .await;
    let listed = s.call("list_entities", json!({"kind": "view"})).await;
    listed.says("Weekly rounds").says("rounds");

    // A view of the operator's own lands beside it, and a listing that ships the
    // rows whole again still names the shipped view by the operator's name.
    s.add("view:my-bikes", "My Bikes").await;
    let both = s
        .call("list_entities", json!({"kind": "view", "reship": true}))
        .await;
    both.says("Weekly rounds").says("My Bikes");

    let restarted = story.restarted().await;
    let again = restarted.session_carried_from(&s).await;
    again
        .call("list_entities", json!({"kind": "view"}))
        .await
        .says("Weekly rounds")
        .says("My Bikes");

    story.finish().await;
}
