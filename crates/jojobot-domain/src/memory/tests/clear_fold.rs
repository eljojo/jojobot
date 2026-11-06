//! A clear is an event about one claim's write. Its effect is decided at its own
//! place in the log: when the claim's write is the one the thing holds, the key
//! leaves the thing and nothing older comes back; otherwise only that claim's
//! write is withdrawn and the thing keeps its value.

use super::*;

/// One write of a key by the claim `fact`.
fn by(fact: &str, key: &str, ordinal: u64, value: Option<&str>) -> KeyWrite {
    KeyWrite {
        fact: FactId(fact.into()),
        ..wrote(key, ordinal, value)
    }
}

fn mood(value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("mood".to_string(), value.to_string())])
}

/// **A clear by a claim that is not the one the thing holds withdraws only that
/// claim's write**: the thing keeps the newer value another claim wrote.
#[test]
fn a_clear_by_a_claim_the_thing_does_not_hold_leaves_the_value_alone() {
    let writes = [
        by("f1", "mood", 1, Some("content")),
        by("f2", "mood", 2, Some("queasy")),
        by("f1", "mood", 3, None),
    ];
    assert_eq!(folded_fields(&writes, &[]), mood("queasy"));
    assert_eq!(
        folded_backing(&writes, &[])["mood"].fact,
        FactId("f2".into())
    );
}

/// **A clear by the claim the thing holds takes the key off the thing, and an
/// older claim's value does not come back.**
#[test]
fn a_clear_by_the_holding_claim_takes_the_key_off_and_nothing_older_returns() {
    let writes = [
        by("f1", "mood", 1, Some("content")),
        by("f2", "mood", 2, Some("queasy")),
        by("f2", "mood", 3, None),
    ];
    assert!(folded_fields(&writes, &[]).is_empty());
    assert!(folded_backing(&writes, &[]).is_empty());
}

/// **A claim withdrawn once and then the holder cleared leaves nothing.**
#[test]
fn clearing_the_older_claim_then_the_holder_takes_the_key_off() {
    let writes = [
        by("f1", "mood", 1, Some("content")),
        by("f2", "mood", 2, Some("queasy")),
        by("f1", "mood", 3, None),
        by("f2", "mood", 4, None),
    ];
    assert!(folded_fields(&writes, &[]).is_empty());
}

/// **A claim that writes the key again after its write was withdrawn holds it
/// again**, as the newest write.
#[test]
fn a_withdrawn_claim_that_writes_the_key_again_holds_it() {
    let writes = [
        by("f1", "mood", 1, Some("content")),
        by("f2", "mood", 2, Some("queasy")),
        by("f1", "mood", 3, None),
        by("f1", "mood", 4, Some("calm")),
    ];
    assert_eq!(folded_fields(&writes, &[]), mood("calm"));
}

/// **The moment of a key is the moment of the write that holds it**, which a
/// withdrawn older write does not change.
#[test]
fn a_withdrawn_write_leaves_the_age_of_the_held_key_alone() {
    let writes = [
        KeyWrite {
            fact: FactId("f1".into()),
            ..wrote_at("mood", 1, Some("content"), "2026-09-01T00:00:00Z")
        },
        KeyWrite {
            fact: FactId("f2".into()),
            ..wrote_at("mood", 2, Some("queasy"), "2026-09-02T00:00:00Z")
        },
        KeyWrite {
            fact: FactId("f1".into()),
            ..wrote_at("mood", 3, None, "2026-09-03T00:00:00Z")
        },
    ];
    assert_eq!(
        folded_ages(&writes, &[])["mood"],
        "2026-09-02T00:00:00Z".parse::<jiff::Timestamp>().unwrap()
    );
}
