use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::date;

use super::super::prepare::{CaptureRequest, ClaimSize, Door, Measures, Route, prepare};
use super::super::tests::{archived, body, claim, created, moment, object, replay, write};
use super::super::{ClaimBody, ClaimEvent, ObjectBody};
use crate::actor::Actor;
use crate::log::RecordId;
use crate::memory::{EdgeShape, EntityId, NewFact, capacity, review};

fn bot() -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object("bot:alpha"),
            body: ObjectBody {
                ..body("bot", "Alpha")
            },
        },
        ClaimEvent::AddressBound {
            object: object("bot:alpha"),
            address: "bot:alpha".to_string(),
        },
    ]
}

fn starred(id: &str, label: &str) -> Vec<ClaimEvent> {
    vec![
        created(id, claim(label, "bot:alpha", id, 3)),
        write(id, "bot:alpha", "starred", 1, "true"),
    ]
}

fn labels(events: &[ClaimEvent]) -> Vec<String> {
    replay(events)
        .starred_rules(&object("bot:alpha"))
        .into_iter()
        .map(|rule| rule.label)
        .collect()
}

/// **Which records of a bot hold a seat**, one row for each way a record does
/// not.
#[test]
fn a_starred_rule_holds_a_seat_unless_it_is_something_else() {
    let thought = ClaimBody {
        edge: Some((EdgeShape::Connection.to_string(), object("bot:alpha"))),
        ..claim("f4", "bot:alpha", "thought", 3)
    };
    let events = [
        bot(),
        starred("r1", "f1"),
        starred("r2", "f2"),
        // written without the star
        vec![
            created("r3", claim("f3", "bot:alpha", "r3", 3)),
            write("r3", "bot:alpha", "starred", 1, "false"),
        ],
        // a thought is not a rule
        vec![
            created("r4", thought),
            write("r4", "bot:alpha", "starred", 1, "true"),
        ],
        // bookkeeping is not a rule
        vec![
            created("r5", claim("f5", "bot:alpha", "mark", 3)),
            write("r5", "bot:alpha", "starred", 1, "true"),
            write("r5", "bot:alpha", "review_mark", 1, "x"),
        ],
        // a taken-back rule holds nothing
        starred("r6", "f6"),
        vec![archived("r6", "f6", "bot:alpha")],
        // an unstarred rule
        vec![created("r7", claim("f7", "bot:alpha", "plain", 3))],
    ]
    .concat();
    assert_eq!(labels(&events), vec!["f1", "f2"]);
}

/// **The oldest comes first**: the number of the label breaks a tie, as a
/// number and not as text.
#[test]
fn the_rules_come_oldest_first_and_the_label_number_breaks_a_tie() {
    let events = [
        bot(),
        starred("r10", "f10"),
        starred("r9", "f9"),
        starred("r2", "f2"),
    ]
    .concat();
    assert_eq!(labels(&events), vec!["f2", "f9", "f10"]);
}

/// A starred rule whose record the store stamped at the given moment.
fn stamped(id: &str, label: &str, at: &str) -> Vec<ClaimEvent> {
    vec![
        created(
            id,
            ClaimBody {
                inserted_at: Some(moment(at)),
                ..claim(label, "bot:alpha", id, 3)
            },
        ),
        write(id, "bot:alpha", "starred", 1, "true"),
    ]
}

/// **The events a starred rule is admitted as**, prepared at `at` over the State
/// `before` makes: the rule goes through [`prepare`] as a capture does, not
/// through a helper that builds the claim by hand.
fn prepared(before: &[ClaimEvent], record: &str, at: &str) -> Vec<ClaimEvent> {
    let mut fact = NewFact::about(
        EntityId("bot:alpha".to_string()),
        "a rule just prepared",
        date(2026, 4, 19),
    );
    fact.fields = BTreeMap::from([("starred".to_string(), "true".to_string())]);
    fact.sets = BTreeSet::from(["starred".to_string()]);
    let mut request = CaptureRequest::of_fact(
        &fact,
        RecordId(record.to_string()),
        Route::Verb,
        Door {
            actor: Actor::Operator,
            owned_keys: BTreeSet::new(),
            owner_revoked: false,
            measures: Measures {
                claim: Some(ClaimSize {
                    size: 1,
                    room: 100,
                    row: None,
                }),
                ..Measures::default()
            },
            runs: BTreeMap::new(),
            run: None,
            aged_before: None,
        },
    );
    request.draft.moment = Some(moment(at));
    prepare(&replay(before), &[], &request).expect("the rule is admitted")
}

/// **A rule being prepared takes its moment when it is prepared**, so it is the
/// newest rule there is: it comes after every stamped rule, whatever the number of
/// its label. The stamped rules keep the order of their moments, not of their
/// label numbers.
#[test]
fn a_rule_prepared_at_a_moment_comes_after_every_stamped_rule() {
    let held = [
        bot(),
        stamped("r9", "f9", "2026-04-02T00:00:00Z"),
        stamped("r10", "f10", "2026-04-01T00:00:00Z"),
    ]
    .concat();
    let events = [held.clone(), prepared(&held, "r11", "2026-04-03T00:00:00Z")].concat();
    assert_eq!(labels(&events), vec!["f10", "f9", "f11"]);
}

/// **A record kept from before stamps existed has no moment, and it is the
/// oldest**, as the seats of a bot read it: it comes before every stamped rule.
#[test]
fn a_rule_kept_without_a_moment_comes_before_every_stamped_rule() {
    let events = [
        bot(),
        starred("r1", "f1"),
        stamped("r9", "f9", "2026-04-02T00:00:00Z"),
        stamped("r10", "f10", "2026-04-01T00:00:00Z"),
    ]
    .concat();
    assert_eq!(labels(&events), vec!["f1", "f10", "f9"]);
}

/// **Two rules with no moment**: the label number orders them.
#[test]
fn two_rules_without_a_moment_keep_the_label_order() {
    let events = [bot(), starred("r10", "f10"), starred("r2", "f2")].concat();
    assert_eq!(labels(&events), vec!["f2", "f10"]);
}

/// **Each bookkeeping key alone keeps a record out of the seats.**
#[test]
fn each_bookkeeping_key_alone_keeps_a_starred_record_out() {
    for key in [
        review::REVIEW_MARK,
        review::SETTINGS_RECORD,
        capacity::ENGINE_RECORD,
    ] {
        let events = [
            bot(),
            starred("r1", "f1"),
            vec![
                created("r2", claim("f2", "bot:alpha", "bookkeeping", 3)),
                write("r2", "bot:alpha", "starred", 1, "true"),
                write("r2", "bot:alpha", key, 2, "x"),
            ],
        ]
        .concat();
        assert_eq!(labels(&events), vec!["f1"], "{key}");
    }
}
