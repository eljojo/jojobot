use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_missed_card_number_names_the_work_item_that_holds_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("work:q-reference-link-rule", "Reference link rule")
        .await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("work:q-2743", "Nearby numbered work").await;
    for subject in ["work:q-reference-link-rule", "person:milhouse"] {
        s.call(
            "capture",
            json!({"subject":subject, "content":"numbered card", "sets":{"card":"2742"}}),
        )
        .await;
    }
    let refused = s
        .refused("recall", json!({"subject":"work:q-2742"}))
        .await
        .json();
    assert_eq!(
        refused["candidates"][0]["handle"], "work:q-reference-link-rule",
        "{refused}"
    );
    assert_eq!(refused["candidates"][0]["reason"], "card-number");
    assert!(
        refused["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .any(|candidate| candidate["handle"] == "work:q-2743"),
        "{refused}"
    );
    let advice = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        advice
            .split('.')
            .any(|line| line.contains("work:q-reference-link-rule") && line.contains("2742")),
        "{refused}"
    );
    let found = s
        .call(
            "recall",
            json!({"subject":refused["candidates"][0]["handle"]}),
        )
        .await
        .json();
    assert_eq!(found["objects"][0]["fields"]["card"], "2742", "{found}");

    for subject in ["work:q-1001", "work:q-2742x", "thing:q-2742"] {
        let ordinary = s.refused("recall", json!({"subject":subject})).await.json();
        assert!(
            ordinary["candidates"]
                .as_array()
                .expect("candidates")
                .iter()
                .all(|candidate| candidate["reason"] != "card-number"),
            "{ordinary}"
        );
    }
    story.finish().await;
}

#[tokio::test]
async fn an_existing_card_number_handle_reads_itself() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("work:q-reference-link-rule", "Reference link rule")
        .await;
    s.call("capture", json!({"subject":"work:q-reference-link-rule", "content":"numbered card", "sets":{"card":"2742"}})).await;
    s.add("work:q-2742", "The numbered work item").await;
    let exact = s
        .call("recall", json!({"subject":"work:q-2742"}))
        .await
        .json();
    assert_eq!(exact["objects"][0]["id"], "work:q-2742", "{exact}");
    story.finish().await;
}
