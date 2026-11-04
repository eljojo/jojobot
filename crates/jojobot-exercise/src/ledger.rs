//! **The coverage ledger of the rooms: every shipped lock, as a row.**
//!
//! Rule 767 moves the rooms into fewer, faster lanes and loses no coverage. A
//! lane cannot be moved against a claim nobody wrote down, so this is the claim:
//! for each lock of each shipped room, the input it is asked of, the oracle it
//! holds the answer to, the failure its sentence names, the case that holds it
//! today and the proof that it can fail. The rows are read from the shipped
//! rooms and the failability registry, never typed, so the file the repository
//! keeps cannot drift from the rooms without a test going red.
//!
//! The last column is the case that will hold the assertion after a move. It is
//! empty until a move names one.

use crate::failability::{NEGATIVE_CONTROLS, PENDING};
use crate::lock::{Asks, Expect};
use crate::{expectations, lock};

/// The columns of a row, in order.
pub const HEADER: &str = "room\tlock\tasks\toracle\tfailure\theld_by\tproof\tmoves_to";

/// One text with the characters that would break a row folded to a space.
fn one_line(text: &str) -> String {
    text.split(['\t', '\n', '\r']).collect::<Vec<_>>().join(" ")
}

fn oracle(expects: &[Expect]) -> String {
    expects
        .iter()
        .map(|expect| match expect {
            Expect::Carries(text) => format!("carries {text:?}"),
            Expect::CarriesAnyCase(text) => format!("carries-any-case {text:?}"),
            Expect::CarriesEither(any) => format!("carries-either {any:?}"),
            Expect::Lacks(text) => format!("lacks {text:?}"),
            Expect::AtLeast(count, text) => format!("at-least {count} {text:?}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// **The ledger, whole**: the header and one row per lock of every shipped room,
/// in the order the rooms are shipped.
pub fn render() -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for room in expectations::shipped_rooms() {
        for lock in lock::locks_of(room) {
            let asks = match &lock.asks {
                Asks::Query { verb, args } => format!("{verb} {args}"),
                Asks::Check(name) => format!("check {name}"),
            };
            let proof = match NEGATIVE_CONTROLS
                .iter()
                .find(|c| c.room == room && c.lock == lock.name)
            {
                Some(control) => format!("{}::{}", control.file, control.function),
                None if PENDING
                    .iter()
                    .any(|p| p.room == room && p.lock == lock.name) =>
                {
                    "pending".to_string()
                }
                None => "unnamed".to_string(),
            };
            let row = [
                room.to_string(),
                lock.name.clone(),
                asks,
                oracle(&lock.expects),
                lock.say.clone(),
                format!("lock of {room}"),
                proof,
                String::new(),
            ];
            out.push_str(&row.map(|cell| one_line(&cell)).join("\t"));
            out.push('\n');
        }
    }
    out
}
