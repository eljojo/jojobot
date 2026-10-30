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

/// **A call that sends an override argument to a verb that no longer publishes
/// it is refused as an unknown argument**, naming the argument. Each argument is
/// sent alone, so the refusal is for that one and a verb that still published
/// the other could not carry the case. Paired with the verbs that still publish
/// both, which take them.
#[tokio::test]
async fn leftovers_an_override_argument_is_refused_where_it_is_not_published() {
    let jojobot = handler();
    let sent = |verb: &str, argument: &str| {
        CallToolRequestParams::new(verb.to_string())
            .with_arguments(json!({ argument: "x" }).as_object().unwrap().clone())
    };
    for verb in ["capture", "update_fact"] {
        for argument in ["override_token", "override_reason"] {
            let body = json_of(
                &jojobot
                    .unimplemented_arguments(&sent(verb, argument))
                    .unwrap_or_else(|| panic!("{verb} took {argument}")),
            );
            assert_eq!(body["refusal_code"], "UnknownArgument", "{verb}: {body}");
            assert_eq!(body["wrote"], false, "{verb}: {body}");
            assert!(
                body["how_to_proceed"]
                    .as_str()
                    .is_some_and(|how| how.contains(argument)),
                "{verb} {argument}: {body}"
            );
        }
    }
    for verb in [
        "add_entity",
        "update_entity",
        "rename_entity",
        "declare_type",
    ] {
        for argument in ["override_token", "override_reason"] {
            assert!(
                jojobot
                    .unimplemented_arguments(&sent(verb, argument))
                    .is_none(),
                "{verb} no longer publishes {argument}"
            );
        }
    }
}
