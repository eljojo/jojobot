//! A correction names its source by an edge or by a mention in its words.

use serde_json::{Value, json};

fn facts(correction: Value) -> Vec<Value> {
    vec![
        json!({"address": "thing:skate-tool#f2", "status": "archived",
               "content": "Ralph returned the skate tool"}),
        correction,
    ]
}

/// **A correction that points at Nelson by a mention is still a correction**,
/// as one that draws an edge is. A claim that names the source and says nothing
/// of Nelson, or one that is archived, is not.
#[test]
fn a_correction_names_nelson_by_an_edge_or_by_a_mention() {
    let source = "thing:skate-tool#f2";
    let by_edge = facts(json!({
        "address": "thing:skate-tool#f5", "status": "active", "derived_from": source,
        "content": "it came back", "edge": {"type": "connection", "object": "person:nelson"}}));
    let by_mention = facts(json!({
        "address": "thing:skate-tool#f5", "status": "active", "derived_from": source,
        "content": "@person:nelson brought the skate tool back"}));
    let about_nobody = facts(json!({
        "address": "thing:skate-tool#f5", "status": "active", "derived_from": source,
        "content": "the skate tool came back"}));
    let archived = facts(json!({
        "address": "thing:skate-tool#f5", "status": "archived", "derived_from": source,
        "content": "@person:nelson brought the skate tool back"}));
    assert!(super::a_standing_correction_names(&by_edge, source));
    assert!(
        super::a_standing_correction_names(&by_mention, source),
        "a correction that names Nelson in its words was refused"
    );
    assert!(!super::a_standing_correction_names(&about_nobody, source));
    assert!(!super::a_standing_correction_names(&archived, source));
}
