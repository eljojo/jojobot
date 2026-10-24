//! The two bags a story's keys are sorted into.

use serde_json::Value;

/// **A story's keys, told into the two bags a write has.** A key that only
/// describes its record (`read_from`, `starred`, ...) is one of the record's own
/// fields; every other key is the state a story sets on the thing, so it goes in
/// the setting bag, which reaches the thing whether or not own fields do. The
/// describing keys are read from the build, not listed here.
pub(super) fn bags(keys: Value) -> (Value, Value) {
    let describing = jojobot_mcp::seed::describing_keys();
    let (mut own, mut set) = (serde_json::Map::new(), serde_json::Map::new());
    if let Value::Object(map) = keys {
        for (key, value) in map {
            if describing.contains(&key) {
                own.insert(key, value);
            } else {
                set.insert(key, value);
            }
        }
    }
    (Value::Object(own), Value::Object(set))
}
