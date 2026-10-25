use super::*;
use crate::harness::*;
use serde_json::json;

#[tokio::test]
async fn leftovers_subject_lists_are_not_taught_as_a_scalar_subject() {
    let jojobot = handler();
    for subjects in [
        json!(["person:milhouse"]),
        json!(["person:milhouse", "person:bart"]),
    ] {
        let call = CallToolRequestParams::new("recall".to_string())
            .with_arguments(json!({"subjects": subjects}).as_object().unwrap().clone());
        let body = json_of(
            &jojobot
                .unimplemented_arguments(&call)
                .expect("subjects is not taken"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
        let advice = body["how_to_proceed"].as_str().unwrap();
        assert!(advice.contains("subjects"), "{body}");
        assert!(!advice.contains("`subjects` as `subject`"), "{body}");
    }

    let scalar = CallToolRequestParams::new("recall".to_string()).with_arguments(
        json!({"subjects": "person:milhouse"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let scalar = json_of(&jojobot.unimplemented_arguments(&scalar).unwrap());
    assert!(
        scalar["how_to_proceed"]
            .as_str()
            .unwrap()
            .contains("`subjects` as `subject`"),
        "{scalar}"
    );

    let accepted = CallToolRequestParams::new("recall".to_string()).with_arguments(
        json!({"subject": "person:milhouse"})
            .as_object()
            .unwrap()
            .clone(),
    );
    assert!(jojobot.unimplemented_arguments(&accepted).is_none());
}
