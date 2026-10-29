//! The party lock reads the party's own guests and nobody else's.

use serde_json::json;

/// **Somebody at another event is not a guest of the party.** The walk into
/// every event along attendance comes back with each event and its guests; a
/// taken-back attendance carries `retracted` and is left out.
#[test]
fn only_the_partys_own_standing_guests_are_counted() {
    let walked = json!({"objects": [
        {"id": "event:goodbye-party", "connected": [
            {"id": "person:stan", "via": {"type": "attendance"}},
            {"id": "person:kyle", "via": {"type": "attendance", "retracted": "2026-05-02"}},
            {"id": "person:stan", "via": {"type": "attendance"}}]},
        {"id": "event:gorge-safety-inspection", "connected": [
            {"id": "person:milhouse", "via": {"type": "attendance"}}]}]});
    assert_eq!(super::party_guests(&walked), vec!["stan".to_string()]);
}
