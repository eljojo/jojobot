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

/// **Targets that meet at one survivor are served once**, in the order first
/// met, beside a control whose targets stay distinct.
#[test]
fn targets_that_converge_at_a_merge_are_served_once() {
    let state = replayed(vec![
        made("work:alpha"),
        made("work:beta"),
        made("work:gamma"),
        made("work:delta"),
        created("r1", claim("f1", "work:alpha", "a", 3)),
        pointing("r1", "work:alpha", "owner", 1, &["work:beta", "work:gamma"]),
        pointing(
            "r1",
            "work:alpha",
            "peers",
            1,
            &["work:delta", "work:beta", "work:gamma"],
        ),
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:gamma"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    ]);
    let scan = state.scan(&object("work:alpha"), &[]).expect("held");
    assert_eq!(scan.field_links["owner"], vec![object("work:gamma")]);
    assert_eq!(
        scan.field_links["peers"],
        vec![object("work:delta"), object("work:gamma")],
        "the first meeting of each survivor fixes its place"
    );
}

/// **A claim a merge left unmapped is named apart and never replaces one of the
/// survivor's own**, though they share a label; a mapped claim of a fresh label
/// is the survivor's own.
#[test]
fn an_unmapped_claim_is_named_apart_from_the_survivors_own() {
    for mapped in [false, true] {
        let moved = if mapped {
            vec![super::super::MovedRecord {
                record: record("r2"),
                label: "f9".to_string(),
            }]
        } else {
            Vec::new()
        };
        let state = replayed(vec![
            made("work:alpha"),
            made("work:beta"),
            created("r1", claim("f1", "work:alpha", "own", 3)),
            created("r2", claim("f1", "work:beta", "theirs", 3)),
            ClaimEvent::ObjectsMerged {
                folded: object("work:beta"),
                survivor: object("work:alpha"),
                records: moved,
                writes: Vec::new(),
            },
        ]);
        let scan = state.scan(&object("work:alpha"), &[]).expect("held");
        let ids = |list: &[(&super::super::RecordId, &super::super::Record)]| {
            list.iter().map(|(id, _)| id.0.clone()).collect::<Vec<_>>()
        };
        if mapped {
            assert_eq!(ids(&scan.claims), vec!["r1", "r2"]);
            assert!(scan.unmapped.is_empty());
        } else {
            assert_eq!(ids(&scan.claims), vec!["r1"], "the survivor keeps its own");
            assert_eq!(ids(&scan.unmapped), vec!["r2"], "and the other is named");
        }
    }
}

fn bound(name: &str, address: &str) -> ClaimEvent {
    ClaimEvent::AddressBound {
        object: object(name),
        address: address.to_string(),
    }
}

fn marked(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| crate::memory::marked_stored_handle(name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// **A scan serves a marked item as the handle its thing answers to now**: a
/// renamed thing under its new handle, two marks that meet at one survivor as
/// one handle, and text that wears no mark, or a mark naming no thing the State
/// holds, as written.
#[test]
fn a_scan_serves_marked_items_as_the_handles_things_answer_to_now() {
    let state = replayed(vec![
        made("work:alpha"),
        bound("work:alpha", "work:alpha"),
        made("work:beta"),
        bound("work:beta", "work:beta"),
        made("work:gamma"),
        bound("work:gamma", "work:gamma"),
        made("work:delta"),
        bound("work:delta", "work:delta"),
        created("r1", claim("f1", "work:alpha", "a", 3)),
        write(
            "r1",
            "work:alpha",
            "owner",
            1,
            &marked(&["work:beta", "work:gamma"]),
        ),
        write("r1", "work:alpha", "peer", 1, &marked(&["work:delta"])),
        write("r1", "work:alpha", "note", 1, "work:beta, plain words"),
        write("r1", "work:alpha", "ghost", 1, &marked(&["work:theta"])),
        ClaimEvent::AddressRetired {
            object: object("work:delta"),
            address: "work:delta".to_string(),
        },
        bound("work:delta", "work:zeta"),
        ClaimEvent::AddressRetired {
            object: object("work:beta"),
            address: "work:beta".to_string(),
        },
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:gamma"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    ]);
    let fields = state.scan(&object("work:alpha"), &[]).expect("held").fields;
    assert_eq!(
        fields["owner"], "work:gamma",
        "two marks, one survivor, once"
    );
    assert_eq!(fields["peer"], "work:zeta", "the handle it answers to now");
    assert_eq!(
        fields["note"], "work:beta, plain words",
        "text that wears no mark is served as written"
    );
    assert_eq!(
        fields["ghost"],
        marked(&["work:theta"]),
        "a mark naming no held thing is served as written"
    );
}
