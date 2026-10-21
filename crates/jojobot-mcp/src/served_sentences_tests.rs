//! **Served sentences that name a route, held to the route that exists.** Each
//! case reads the sentence at the place a caller reads it and holds it to an
//! identifier the engine owns: an argument name, a key an answer carries, a
//! constant.

use crate::harness::*;
use crate::memory::testing::{Down, healthy_and_down};
use crate::orientation::OrientArgs;
use crate::session::WrapSessionArgs;
use rmcp::handler::server::wrapper::Parameters;

/// What a verb says about itself, as the served surface describes it.
fn tool_description(verb: &str) -> String {
    crate::Jojobot::tool_router()
        .list_all()
        .iter()
        .find(|t| t.name.as_ref() == verb)
        .unwrap_or_else(|| panic!("the surface offers {verb}"))
        .description
        .as_deref()
        .unwrap_or_default()
        .to_string()
}

/// The names of the arguments of a verb's published schema.
fn argument_names(schema: schemars::Schema) -> Vec<String> {
    serde_json::to_value(schema).expect("the schema serialises")["properties"]
        .as_object()
        .expect("a schema lists its properties")
        .keys()
        .cloned()
        .collect()
}

/// **The wrap's description names the keys of the `roles` block its answer
/// carries when a release fails.** The keys are read off a real answer, so a
/// key the answer gains or loses moves this case rather than a copy of it.
#[tokio::test]
async fn the_wrap_description_names_the_keys_of_the_roles_block_it_serves() {
    let (healthy, blind) = healthy_and_down(Down::RoleRelease);
    make_bot(&healthy, "gamma").await;
    let booted = json_of(
        &blind
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: Some("dev-dispatch".into()),
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
    );
    let wrapped = json_of(
        &blind
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done with the dispatch for now".into(),
                sid: sid_of(&booted).expect("a handle"),
            }))
            .await
            .expect("wrap ok"),
    );
    let served = wrapped["roles"][0]
        .as_object()
        .unwrap_or_else(|| panic!("a failed release is reported: {wrapped}"));
    let described = tool_description("wrap_session");
    assert!(
        described.contains("`roles`"),
        "the description names the block itself"
    );
    // `reason` is also an argument of other verbs, so a backticked name would
    // read as a parameter this verb does not take: it is named in plain words.
    for key in ["released", "reason", "held_until"] {
        assert!(
            served.contains_key(key),
            "the answer carries `{key}`: {wrapped}"
        );
    }
    assert!(
        described.contains("`held_until`") && described.contains("reason"),
        "the wrap description names what a held role carries"
    );
}

/// **The `kind` list of `add_entity` says which kinds the engine issues.** A
/// voucher and a challenge are issued by the engine, and a call that names one
/// is refused, so the list that names them says so beside them.
#[test]
fn the_add_entity_kind_list_says_which_kinds_the_engine_issues() {
    use jojobot_domain::memory::EntityKind;
    let schema = serde_json::to_value(schemars::schema_for!(crate::memory::AddEntityArgs))
        .expect("the schema serialises");
    let kind = schema["properties"]["kind"]["description"]
        .as_str()
        .expect("kind carries a description");
    let list = kind.split("\n\n").next().expect("a first paragraph");
    let at = |token: &str| {
        list.find(&format!("`{token}`"))
            .unwrap_or_else(|| panic!("the list names `{token}`: {list}"))
    };
    let (voucher, challenge) = (
        at(EntityKind::VOUCHER.as_token()),
        at(EntityKind::CHALLENGE.as_token()),
    );
    let issued = list.find("issued").expect("the list says who issues them");
    assert!(
        voucher < issued && challenge < issued && issued - voucher.min(challenge) < 120,
        "the two are named together, then said to be issued: {list}"
    );
    assert!(
        issued < at(EntityKind::DOCUMENT.as_token()),
        "the kinds a call creates follow the note: {list}"
    );
}

/// **A line that left details or prose out names the narrower call.** The
/// argument names it gives are `recall`'s own, read off the published schema.
#[test]
fn what_a_left_out_line_says_to_ask_names_arguments_recall_has() {
    let recall = argument_names(schemars::schema_for!(crate::memory::RecallArgs));
    let details = crate::memory::wire::details_note(2, &["work:a#f1".into(), "work:b#f2".into()]);
    let how = details["how_to_ask"].as_str().expect("a way to ask");
    for argument in ["records", "details"] {
        assert!(
            recall.iter().any(|a| a == argument),
            "recall has `{argument}`"
        );
        assert!(
            how.contains(argument),
            "the details line names `{argument}`: {how}"
        );
    }
    let prose = crate::memory::wire::prose_note(3);
    let how = prose["how_to_ask"].as_str().expect("a way to ask");
    for argument in ["subject", "prose"] {
        assert!(
            recall.iter().any(|a| a == argument),
            "recall has `{argument}`"
        );
        assert!(
            how.contains(argument),
            "the prose line names `{argument}`: {how}"
        );
    }
}

/// **A refusal that names the assistant does not say whom it acts for.** What
/// holds on every instance is the assistant's place on the chart.
#[test]
fn a_refusal_naming_the_assistant_says_nothing_of_the_operator() {
    use jojobot_domain::memory::{ASSISTANT, MayWrite, MemoryError};
    let merge = crate::memory::declined::memory_declined(
        "merge_entities",
        MemoryError::MergeCarriesGuardedKeys {
            duplicate: "thread:simpson-kitchen".into(),
            survivor: "thread:milhouse-moves".into(),
            keys: "record_capacity".into(),
            may: MayWrite::WritersSuperior,
            allowed: vec![ASSISTANT.into()],
            clear_opens: true,
        },
    )
    .expect("a refusal is an answer");
    let key = crate::memory::declined::memory_declined(
        "capture",
        MemoryError::KeyNotYours {
            subject: "thread:milhouse-moves".into(),
            key: "record_capacity".into(),
            may: MayWrite::WritersSuperior,
            allowed: vec![ASSISTANT.into()],
        },
    )
    .expect("a refusal is an answer");
    for (what, result) in [("merge", merge), ("key write", key)] {
        let said = blocked(&result)["how_to_proceed"]
            .as_str()
            .expect("a way forward")
            .to_string();
        assert!(
            said.contains(ASSISTANT),
            "{what} names the assistant: {said}"
        );
        assert!(!said.contains("operator"), "{what}: {said}");
    }
}
