//! "Two claims about the diner. One set its price on the diner. One only
//! labelled itself."
//!
//! **A claim carries two bags and a read tells them apart.** What a claim sent
//! in `sets` is a property of the thing and reads under `sets`; what it sent in
//! `fields` is a label on that one claim and reads under `fields`. The receipt
//! of a write tells a caller FIRST when a key it sent as an own field stayed off
//! the thing, and one call moves such a key to the settings.
//!
//! **The negatives are what make the split mean something.** Neither claim is
//! mislabelled, the thing holds the setting and not the label, and the claim
//! that only labelled itself reads with no `sets` at all.

use serde_json::json;

use super::dsl::Story;

fn address_of(captured: &serde_json::Value) -> String {
    captured["address"]
        .as_str()
        .or_else(|| captured["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string()
}

fn claim<'a>(recalled: &'a serde_json::Value, address: &str) -> &'a serde_json::Value {
    recalled["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["address"] == address))
        .unwrap_or_else(|| panic!("{address} is not in {recalled}"))
}

#[tokio::test]
async fn a_setting_and_a_label_read_apart_and_a_label_moves_in_one_call() {
    let story = Story::begin("bot:gamma").await;
    let run = story.session().await;
    run.add("place:leftorium", "The Leftorium").await;

    let priced = address_of(
        &run.call(
            "capture",
            json!({
                "subject": "place:leftorium", "content": "the Thursday special costs 12",
                "provenance": "inference", "sets": {"special_price": "12"},
            }),
        )
        .await
        .json(),
    );
    let labelled = run
        .call(
            "capture",
            json!({
                "subject": "place:leftorium", "content": "a regular called it quiet",
                "provenance": "inference", "fields": {"overheard": "tuesday"},
            }),
        )
        .await
        .json();
    let labelled_address = address_of(&labelled);

    // **The receipt of the label leads with what it costs**, before any other key.
    let said = labelled["a_key_stayed_off_the_thing"]
        .as_str()
        .unwrap_or_else(|| panic!("{labelled}"));
    assert!(
        said.contains("overheard") && said.contains("sets"),
        "{said}"
    );
    assert!(
        labelled
            .to_string()
            .starts_with("{\"a_key_stayed_off_the_thing\""),
        "{labelled}"
    );

    // **A read tells the two apart**, and neither is mislabelled.
    let read = run.recall("place:leftorium").await.json();
    let setting = claim(&read, &priced);
    assert_eq!(setting["sets"]["special_price"], "12", "{read}");
    assert!(setting["fields"]["special_price"].is_null(), "{read}");
    let label = claim(&read, &labelled_address);
    assert_eq!(label["fields"]["overheard"], "tuesday", "{read}");
    assert!(label.get("sets").is_none(), "{read}");
    // **The thing holds the setting and not the label.**
    assert_eq!(
        read["objects"][0]["fields"]["special_price"], "12",
        "{read}"
    );
    assert!(
        read["objects"][0]["fields"]["overheard"].is_null(),
        "{read}"
    );

    // **The same split on a search hit.**
    let found = run
        .call("search", json!({"query": "Thursday special"}))
        .await;
    found.says("\"sets\":{\"special_price\":\"12\"}");

    // **One call moves the label to the settings**, and the receipt says so.
    let moved = run
        .call(
            "update_fact",
            json!({"address": labelled_address, "sets": {"overheard": "tuesday"}}),
        )
        .await
        .json();
    assert_eq!(moved["moved_to_sets"], json!(["overheard"]), "{moved}");
    let after = run.recall("place:leftorium").await.json();
    assert_eq!(
        after["objects"][0]["fields"]["overheard"], "tuesday",
        "{after}"
    );
    let moved_claim = claim(&after, &labelled_address);
    assert_eq!(moved_claim["sets"]["overheard"], "tuesday", "{after}");
    assert!(moved_claim["fields"]["overheard"].is_null(), "{after}");
    story.finish().await;
}
