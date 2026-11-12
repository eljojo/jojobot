//! **What a read of one piece of a thing's history builds**, counted and not timed.
//! A thing of 2,000 claims answers a read of one claim with one chain, a read of one
//! key with that key's writes, and a read of its ages or versions with no scan of
//! the thing: no read of one piece builds the whole document (the architect's
//! ruling on read cost). Each narrow read also answers what the whole-document
//! read answers for the same piece.

use super::super::tests::{body, claim, created, object, stamped, write};
use super::super::{ClaimEvent, State};
use crate::memory::{FactId, FactStatus};

const CLAIMS: usize = 2000;

/// **A thing of 2,000 claims**, each setting the key `k`, and one more that sets
/// `other` and is kept off the thing.
fn a_thing_of_two_thousand_claims() -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    let mut events = vec![
        ClaimEvent::ObjectCreated {
            object: object("work:alpha"),
            body: body("work", "Alpha"),
        },
        ClaimEvent::AddressBound {
            object: object("work:alpha"),
            address: "work:alpha".to_string(),
        },
    ];
    for n in 1..=CLAIMS {
        let record = format!("r{n}");
        events.push(created(
            &record,
            claim(
                &format!("f{n}"),
                "work:alpha",
                &format!("claim number {n}"),
                3,
            ),
        ));
        events.push(write(&record, "work:alpha", "k", n as u64, &n.to_string()));
    }
    events.push(created(
        "rx",
        claim("fx", "work:alpha", "the odd one out", 3),
    ));
    events.push(write("rx", "work:alpha", "other", 1, "x"));
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    state
}

fn label(text: &str) -> FactId {
    FactId(text.to_string())
}

/// **One claim's chain is one chain, and no scan of the thing.**
#[test]
fn a_claim_read_builds_one_chain_and_no_scan() {
    let state = a_thing_of_two_thousand_claims();
    State::reset_folds_counted();

    let chain = state
        .claim_chain_of(&object("work:alpha"), &label("f1000"))
        .expect("the claim is held");

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].status, FactStatus::Active);
    assert_eq!(State::chains_counted(), 1);
    assert_eq!(State::scans_counted(), 0);
    assert_eq!(chain[0].content, "claim number 1000", "the claim asked for");
}

/// **The writes of one key are that key's writes**: reading `other` builds one
/// write of the 2,001 the thing holds, and the answer is the whole read's piece.
#[test]
fn a_key_read_builds_that_keys_writes_only() {
    let state = a_thing_of_two_thousand_claims();
    State::reset_folds_counted();

    let writes = state.key_history(&object("work:alpha"), "other");

    assert_eq!(writes.len(), 1);
    assert_eq!(State::trail_writes_counted(), 1);
    assert_eq!(State::scans_counted(), 0);
    assert_eq!(
        Some(&writes),
        state.key_trails(&object("work:alpha")).history.get("other"),
        "the same writes"
    );
}

/// **The keys one claim keeps off its thing are read from that claim's writes**,
/// and agree with the trails of the whole thing.
#[test]
fn the_keys_a_claim_keeps_off_its_thing_are_read_from_that_claim() {
    let state = a_thing_of_two_thousand_claims();
    State::reset_folds_counted();

    let kept = state.keys_kept_off(&object("work:alpha"), &label("fx"));

    assert_eq!(State::scans_counted(), 0);
    assert_eq!(State::trail_writes_counted(), 0);
    assert_eq!(
        kept,
        state
            .key_trails(&object("work:alpha"))
            .kept_off
            .get(&label("fx"))
            .cloned()
            .unwrap_or_default(),
        "the same keys"
    );
}

/// A write of `key` by `record`, under `class`.
fn written_under(
    record: &str,
    key: &str,
    ordinal: u64,
    class: crate::memory::WriteClass,
    value: crate::log::claims::FieldValue,
) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: super::super::tests::record(record),
        subject: object("work:alpha"),
        key: super::super::tests::key(key),
        ordinal,
        class,
        write: value,
        targets: Vec::new(),
    }
}

/// **Each way a key can be kept off the thing, and each way it is not**, read from
/// the one claim and compared with the trails of the whole thing: a key a claim
/// only sets on its own record is kept off; one it then cleared holds nothing and
/// is not; one a later write reaches the thing with is not; one a later write
/// takes off the thing is.
#[test]
fn the_kept_off_keys_of_a_claim_follow_its_newest_write_of_each_key() {
    use crate::log::claims::FieldValue::{Clear, Set};
    use crate::memory::WriteClass::{Sets, Stays};
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    let mut events = vec![
        ClaimEvent::ObjectCreated {
            object: object("work:alpha"),
            body: body("work", "Alpha"),
        },
        ClaimEvent::AddressBound {
            object: object("work:alpha"),
            address: "work:alpha".to_string(),
        },
    ];
    for (n, record) in ["ra", "rb", "rc", "rd"].into_iter().enumerate() {
        events.push(created(
            record,
            claim(&format!("f{}", n + 1), "work:alpha", "a claim", 3),
        ));
    }
    events.extend([
        written_under("ra", "a", 1, Stays, Set("1".into())),
        written_under("rb", "b", 2, Stays, Set("1".into())),
        written_under("rb", "b", 3, Stays, Clear),
        written_under("rc", "c", 4, Stays, Set("1".into())),
        written_under("rc", "c", 5, Sets, Set("2".into())),
        written_under("rd", "d", 6, Sets, Set("1".into())),
        written_under("rd", "d", 7, Stays, Set("2".into())),
    ]);
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    let whole = state.key_trails(&object("work:alpha")).kept_off;

    let label_of = label;
    let narrow = |name: &str| state.keys_kept_off(&object("work:alpha"), &label_of(name));

    let kept_set = |keys: &[&str]| keys.iter().map(|key| key.to_string()).collect();
    assert_eq!(narrow("f1"), kept_set(&["a"]), "only set on the record");
    assert_eq!(narrow("f2"), kept_set(&[]), "set and then cleared");
    assert_eq!(
        narrow("f3"),
        kept_set(&[]),
        "a later write reaches the thing"
    );
    assert_eq!(narrow("f4"), kept_set(&["d"]), "a later write takes it off");
    for label in ["f1", "f2", "f3", "f4"] {
        assert_eq!(
            narrow(label),
            whole.get(&label_of(label)).cloned().unwrap_or_default(),
            "{label}: the same keys as the whole thing's trails"
        );
    }
}

/// **A thing's ages and versions are read with no scan of the thing.**
#[test]
fn the_ages_and_versions_of_a_thing_build_no_scan() {
    let state = a_thing_of_two_thousand_claims();
    State::reset_folds_counted();

    let declared = state.declared_types_shared();
    super::STREAMS_EXAMINED.with(|n| n.set(0));
    let ages = state.field_ages_of(&object("work:alpha"), &declared);
    let versions = state.entity_versions_of(&object("work:alpha"));

    assert_eq!(State::scans_counted(), 0);
    assert_eq!(State::chains_counted(), 0);
    assert_eq!(
        super::STREAMS_EXAMINED.with(|n| n.get()),
        0,
        "the ages are looked up, and no write is visited"
    );
    assert_eq!(ages.newest, state.newest_change(&object("work:alpha")));
    assert!(versions.iter().all(|version| version.ordinal > 0));
    assert_eq!(state.held_fields_are_what_a_fold_makes(), Ok(()));
}

/// **Every claim's chain costs in proportion to the thing's claims**, one chain
/// each and no scan, and is the whole read's piece.
#[test]
fn every_claims_chains_build_one_chain_each_and_no_scan() {
    let state = a_thing_of_two_thousand_claims();
    State::reset_folds_counted();

    let histories = state.claim_chains_of(&object("work:alpha"));

    assert_eq!(histories.len(), CLAIMS + 1);
    assert_eq!(State::chains_counted(), CLAIMS + 1);
    assert_eq!(State::scans_counted(), 0);
    assert_eq!(histories[&label("f7")][0].content, "claim number 7");
}
