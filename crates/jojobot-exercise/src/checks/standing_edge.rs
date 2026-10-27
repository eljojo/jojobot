//! The standing-edge helpers read each hit's own status.

use serde_json::json;

/// **An archived edge does not stand**: search answers archived records too.
#[test]
fn an_archived_edge_is_not_a_standing_edge() {
    let council = "org:springfield-town-council";
    let world = format!(
        "{{}}\n{}",
        json!({"results": [{"hit": "fact", "status": "archived",
            "parent": {"id": "person:nelson"},
            "edge": {"type": "membership", "object": council}}]})
    );
    let nelson = "person:nelson";
    assert!(!super::has_standing_edge(
        &world,
        Some(nelson),
        "membership",
        council
    ));
    assert!(!super::has_standing_edge_to_a(
        &world,
        nelson,
        "membership",
        "org:"
    ));
}

/// **An archived claim is not an active claim**, and an active one is.
#[test]
fn an_archived_claim_is_not_an_active_claim() {
    let world = format!(
        "{{}}\n{}",
        json!({"results": [
            {"hit": "fact", "status": "archived", "address": "person:nelson#f1"},
            {"hit": "fact", "status": "active", "address": "person:nelson#f2"},
            {"hit": "entity", "status": "active", "address": "person:nelson"}]})
    );
    let claims = super::active_facts(&world).expect("a results list");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["address"], "person:nelson#f2");
}
