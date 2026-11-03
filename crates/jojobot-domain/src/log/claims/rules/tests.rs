use super::super::tests::{archived, body, claim, created, object, replay, write};
use super::super::{ClaimBody, ClaimEvent, ObjectBody};
use crate::memory::EdgeShape;

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
