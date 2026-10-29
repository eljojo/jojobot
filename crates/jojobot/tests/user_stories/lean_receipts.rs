//! "Keep progress confirmations brief; show me the record when I ask."
//!
//! A bot chooses lean receipts, follows one back to its stored claim, and
//! returns to normal receipts when the extra context is useful again.

use super::dsl::Story;
use super::owed_receipt::a_piece_of_work;
use serde_json::json;

#[tokio::test]
async fn brief_progress_receipts_still_lead_to_the_record_and_normal_answers() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let setting = story.session().await;
    a_piece_of_work(&s).await;
    setting
        .call(
            "capture",
            json!({"subject":"bot:otto", "content":"Keep confirmations brief",
                "sets":{"setting_token_saving":"on"}}),
        )
        .await;

    let progress = "The rigging is ready";
    let lean = s
        .call(
            "capture",
            json!({"subject":"work:sigma", "content":progress,
                "sets":{"runs_out":"2026-10-26"}}),
        )
        .await
        .json();
    assert_eq!(lean["id"], "work:sigma", "{lean}");
    assert_eq!(lean["derived"]["due_on"], "2026-10-26", "{lean}");
    assert!(lean.get("address").is_none(), "{lean}");
    assert!(lean.get("teaching").is_none(), "{lean}");

    let read = s
        .call("recall", json!({"subject":lean["id"], "facts":true}))
        .await
        .json();
    let object = &read["objects"][0];
    assert_eq!(object["id"], lean["id"], "{read}");
    assert_eq!(object["fields"]["due_on"], lean["derived"]["due_on"]);
    let claim = object["facts"]
        .as_array()
        .expect("the record's claims")
        .iter()
        .find(|claim| claim["content"] == progress)
        .expect("the progress the bot recorded");
    assert!(claim["address"].is_string(), "{read}");
    assert!(
        read["teaching"]
            .as_array()
            .is_some_and(|lines| !lines.is_empty()),
        "{read}"
    );

    setting
        .call(
            "capture",
            json!({"subject":"bot:otto", "content":"Restore normal confirmations",
                "sets":{"setting_token_saving":"off"}}),
        )
        .await;
    let normal = s
        .call(
            "capture",
            json!({"subject":"work:sigma", "content":"The rigging passed inspection"}),
        )
        .await
        .json();
    assert!(normal["address"].is_string(), "{normal}");
    assert!(normal["how_to_read"].is_string(), "{normal}");
    assert_ne!(normal["address"], claim["address"]);
    story.finish().await;
}
