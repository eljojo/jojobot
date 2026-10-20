//! "I archived a claim and said why. The reason I gave must not cost me what
//! the claim already said."
//!
//! A claim's details are its own words. An archive's reason is a different
//! thing, so it rides beside the details in a field of its own and neither
//! reads as the other.

use serde_json::json;

use super::dsl::{Session, Story};

async fn the_claim(s: &Session, address: &str) -> serde_json::Value {
    let read = s
        .call(
            "recall",
            json!({"subject": "thread:deploys-alpha", "facts": true, "details": true}),
        )
        .await
        .json();
    read["objects"][0]["facts"]
        .as_array()
        .expect("the facts")
        .iter()
        .find(|fact| fact["address"] == address)
        .unwrap_or_else(|| panic!("{address} in {read}"))
        .clone()
}

async fn said(s: &Session, content: &str) -> String {
    let body = s
        .call(
            "capture",
            json!({
                "subject": "thread:deploys-alpha",
                "content": content,
                "details": "Marge said it at the fair",
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    body["address"].as_str().expect("an address").to_string()
}

#[tokio::test]
async fn an_archive_with_a_reason_keeps_the_details_the_claim_held() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thread:deploys-alpha", "Deploys Alpha").await;

    // The positive: an archive that sends no details changes none.
    let bare = said(&s, "Marge said the picnic is on").await;
    s.call(
        "update_fact",
        json!({"address": bare, "status": "archived"}),
    )
    .await;
    let held = the_claim(&s, &bare).await;
    assert_eq!(held["details"], "Marge said it at the fair", "{held}");
    assert!(held["fields"].get("archive_reason").is_none(), "{held}");

    // The archive that carries a reason keeps the details and adds the reason.
    let reasoned = said(&s, "Marge said the picnic is off").await;
    let archived = s
        .call(
            "update_fact",
            json!({"address": reasoned, "status": "archived", "details": "replaced by #f3"}),
        )
        .await;
    archived.says("archive_reason");
    let held = the_claim(&s, &reasoned).await;
    assert_eq!(held["status"], "archived", "{held}");
    assert_eq!(held["details"], "Marge said it at the fair", "{held}");
    assert_eq!(
        held["fields"]["archive_reason"], "replaced by #f3",
        "{held}"
    );
}

#[tokio::test]
async fn a_read_that_leaves_details_out_names_the_records_it_left_them_out_of() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thread:deploys-alpha", "Deploys Alpha").await;
    let with = said(&s, "Marge said the picnic is on").await;
    s.fact("thread:deploys-alpha", "Homer brought the cooler")
        .await;

    let read = s.recall("thread:deploys-alpha").await.json();
    let note = &read["details_left_out"];
    assert_eq!(note["count"], 1, "{read}");
    assert_eq!(note["addresses"], json!([with]), "{read}");

    // The positive: asking for the details leaves nothing out, so names nothing.
    let whole = s.recall_with_details("thread:deploys-alpha").await.json();
    assert!(whole.get("details_left_out").is_none(), "{whole}");

    let found = s.call("search", json!({"query": "picnic"})).await.json();
    assert_eq!(
        found["details_left_out"]["addresses"],
        json!([with]),
        "{found}"
    );
}
