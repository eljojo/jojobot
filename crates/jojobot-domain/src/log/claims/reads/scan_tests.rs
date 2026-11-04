use super::super::tests::{archived, body, claim, clear, created, object, record, stamped, write};
use super::super::{ClaimEvent, FieldValue, ObjectBody, State};
use crate::memory::WriteClass;

fn made(name: &str) -> ClaimEvent {
    ClaimEvent::ObjectCreated {
        object: object(name),
        body: ObjectBody {
            ..body("work", name)
        },
    }
}

fn pointing(id: &str, subject: &str, k: &str, ordinal: u64, to: &[&str]) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object(subject),
        key: super::super::tests::key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set("link".to_string()),
        targets: to.iter().map(|name| object(name)).collect(),
    }
}

fn replayed(events: Vec<ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    state
}

/// **A scan holds the object, its claims in the order of their numbers, what it
/// holds and where the held values point**, and no other thing's claims.
#[test]
fn a_scan_holds_the_thing_its_claims_its_fields_and_its_links() {
    let state = replayed(vec![
        made("work:alpha"),
        made("work:beta"),
        made("work:gamma"),
        created("r10", claim("f10", "work:alpha", "ten", 3)),
        created("r2", claim("f2", "work:alpha", "two", 3)),
        created("r9", claim("f1", "work:beta", "other", 3)),
        write("r2", "work:alpha", "phase", 1, "now"),
        pointing("r2", "work:alpha", "owner", 1, &["work:beta", "work:gamma"]),
        archived("r10", "f10", "work:alpha"),
    ]);
    let scan = state
        .scan(&object("work:alpha"), &[])
        .expect("the thing is held");
    assert_eq!(scan.object, object("work:alpha"));
    assert_eq!(scan.body.name, "work:alpha");
    assert_eq!(
        scan.claims
            .iter()
            .map(|(_, held)| held.claim.label.as_str())
            .collect::<Vec<_>>(),
        vec!["f2", "f10"],
        "by number, taken back included, and none of beta's"
    );
    assert_eq!(scan.fields["phase"], "now");
    assert_eq!(scan.fields["owner"], "link");
    assert_eq!(
        scan.field_links.keys().cloned().collect::<Vec<_>>(),
        vec!["owner".to_string()]
    );
    assert_eq!(
        scan.field_links["owner"],
        vec![object("work:beta"), object("work:gamma")]
    );
}

/// **A cleared key, and a key whose claim was taken back, point at nothing**; a
/// thing the State does not hold has no scan; a folded thing scans as its
/// survivor and its links follow merges.
#[test]
fn a_scan_follows_clears_taken_back_claims_and_merges() {
    let state = replayed(vec![
        made("work:alpha"),
        made("work:beta"),
        made("work:gamma"),
        created("r1", claim("f1", "work:alpha", "a", 3)),
        pointing("r1", "work:alpha", "owner", 1, &["work:beta"]),
        pointing("r1", "work:alpha", "peer", 1, &["work:beta"]),
        clear("r1", "work:alpha", "peer", 2),
        created("r2", claim("f2", "work:alpha", "b", 3)),
        pointing("r2", "work:alpha", "boss", 1, &["work:beta"]),
        archived("r2", "f2", "work:alpha"),
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:gamma"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    ]);
    let scan = state.scan(&object("work:alpha"), &[]).expect("held");
    assert_eq!(
        scan.field_links.keys().cloned().collect::<Vec<_>>(),
        vec!["owner".to_string()],
        "peer was cleared and boss sits in a taken back claim"
    );
    assert_eq!(
        scan.field_links["owner"],
        vec![object("work:gamma")],
        "a link follows the merge of its target"
    );
    assert!(state.scan(&object("work:theta"), &[]).is_none());
    let folded = state.scan(&object("work:beta"), &[]).expect("held");
    assert_eq!(folded.object, object("work:gamma"));
}
