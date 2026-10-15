//! **What an answer says about the things it left out**, built in one place.
//!
//! `recall` serves these notes, the page of a view serves the same notes, and
//! the write guard's envelope is as wide as the widest of them. A second site
//! that spelled one of them would let the three disagree about what an answer
//! carries.

use std::collections::BTreeMap;

use jojobot_domain::memory::readiness::Ask;

use crate::memory::view_run::LeftOut;

/// Why the owed pass left things out, by reason and kind, and the call that
/// lists them. `None` when nothing was left out.
pub fn overdue_excluded_because(
    because: &BTreeMap<LeftOut, BTreeMap<String, usize>>,
) -> Option<serde_json::Value> {
    if because.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "reasons": because
            .iter()
            .map(|(why, kinds)| (
                why.token().to_string(),
                serde_json::json!({
                    "count": kinds.values().sum::<usize>(),
                    "kinds": kinds,
                }),
            ))
            .collect::<serde_json::Map<_, _>>(),
        "how_to_read": "recall again with kind set to one of the kinds named and \
                        with no overdue question and no view, to list those things; \
                        the not_due_yet ones also come back from this same call \
                        asked about a later day",
    }))
}

/// The line beside a readiness list: what ready means, and the argument that
/// narrows it.
pub fn ready_note(ask: Ask) -> &'static str {
    match ask {
        Ask::Ready => {
            "ready means unblocked and not finished, so work already under way is in \
             this list; where narrows it to your board's not-started status, for \
             example where: \"status = next\""
        }
        Ask::Plan => {
            "ready says whether a piece of work can be started and, when it cannot, \
             blocked_by says what stops it; the work here is not finished, except a \
             piece you name with subject, which comes back whatever its state"
        }
    }
}

/// How many pieces of work the readiness question left out, and the call that
/// says why. `None` when nothing was left out.
pub fn ready_left_out(left: usize, ask: Option<Ask>) -> Option<serde_json::Value> {
    (left > 0).then(|| {
        serde_json::json!({
            "count": left,
            "how_to_read": match ask {
                Some(Ask::Plan) => {
                    "these pieces of work are finished or archived, so they are not in the \
                     plan; recall one by handle to read it"
                }
                _ => {
                    "recall again with this view and subject set to one piece of work: it \
                     comes back with ready: false and blocked_by naming what stops it"
                }
            },
        })
    })
}

/// **The widest `overdue_excluded_because` an answer of one thing can carry**:
/// the thing is left out for exactly one reason, so the note names the widest
/// reason, with the thing's kind at the largest count.
pub(crate) fn widest_overdue_excluded_because(kind: &str) -> serde_json::Value {
    LeftOut::ALL
        .into_iter()
        .filter_map(|why| {
            let because = BTreeMap::from([(why, BTreeMap::from([(kind.to_string(), usize::MAX)]))]);
            overdue_excluded_because(&because)
        })
        .max_by_key(|note| note.to_string().chars().count())
        .expect("a reason exists")
}

/// **The widest `ready_note` and the widest `ready_left_out` an answer can
/// carry.**
pub(crate) fn widest_ready_note() -> &'static str {
    Ask::ALL
        .into_iter()
        .map(ready_note)
        .max_by_key(|note| note.chars().count())
        .expect("a readiness ask exists")
}

pub(crate) fn widest_ready_left_out() -> serde_json::Value {
    [None, Some(Ask::Ready), Some(Ask::Plan)]
        .into_iter()
        .filter_map(|ask| ready_left_out(usize::MAX, ask))
        .max_by_key(|note| note.to_string().chars().count())
        .expect("a count above zero is a note")
}
