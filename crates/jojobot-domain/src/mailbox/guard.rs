//! The mailbox write guard — the same contacts-book check the Memory context
//! runs, in this context's vocabulary.
//!
//! Two gates, and they are not the same gate:
//!
//! * **creating** a box screens the incoming name against the boxes that exist,
//!   so `inbx` beside an existing `inbox` comes back as candidates rather than
//!   as a second box nobody meant. The caller's explicit `create_new` signal
//!   overrides the similarity screen (sibling fleets like `worker-1`,
//!   `worker-2` are legitimate) — but never an exact name, which already
//!   exists.
//! * **naming** a box (posting into one) is an *existence* gate: an exact name
//!   is the box and is waved through; anything else is refused, near miss or
//!   not. There is deliberately no create-new escape — a typo that mints a box
//!   is a message posted where nobody is listening, and it looks like success.
//!
//! Pure: no I/O, no clock, no randomness — every decision here is a function of
//! (name, existing names), which is why it can sit on the adapter's write path
//! and cannot be routed around.

use super::MailboxName;

/// Why an existing mailbox is a candidate for the incoming name, strongest
/// first. The order is the reporting order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum MatchReason {
    /// The very same name already exists.
    Exact,
    /// Within a typo of an existing name (edit distance ≤ 2).
    Near,
    /// One name contains the other — `inbox` beside `work-inbox`. Not a typo,
    /// but the two are routinely confused for each other.
    Contains,
}

/// An existing mailbox the guard suspects the incoming name means.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MailboxMatch {
    /// The existing box's name.
    pub name: MailboxName,
    /// Why the guard flagged it.
    pub reason: MatchReason,
}

/// The guard's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No suspicion: the write may proceed.
    Proceed,
    /// Suspicion: **nothing is written**, and here is what jojobot suspects.
    Block(Vec<MailboxMatch>),
}

/// The edit-distance budget for "this is probably a typo of that" — the same
/// two the Memory guard uses, where transposition plus a dropped letter still
/// matches but two genuinely short distinct names do not.
const NEAR: usize = 2;

/// The shortest name that may match on containment. Below this, containment is
/// noise: every two-letter name is inside half the board.
const CONTAINS_FLOOR: usize = 3;

/// Every existing mailbox the incoming name might already be, strongest reason
/// first, then by name — a stable report, so two sessions screening the same
/// write read the same list.
pub fn screen(name: &MailboxName, existing: &[MailboxName]) -> Vec<MailboxMatch> {
    let incoming = name.as_str();
    let mut matches: Vec<MailboxMatch> = existing
        .iter()
        .filter_map(|other| {
            reason_for(incoming, other.as_str()).map(|reason| MailboxMatch {
                name: other.clone(),
                reason,
            })
        })
        .collect();
    matches.sort_by(|a, b| a.reason.cmp(&b.reason).then_with(|| a.name.cmp(&b.name)));
    matches
}

/// The strongest reason one existing name is a candidate, or `None`.
fn reason_for(incoming: &str, existing: &str) -> Option<MatchReason> {
    if incoming == existing {
        return Some(MatchReason::Exact);
    }
    // Names are already one-spelling by grammar ([a-z0-9-]+), so there is no
    // case or whitespace to fold before comparing — unlike a display name.
    if crate::memory::guard::edit_distance(incoming, existing) <= NEAR {
        return Some(MatchReason::Near);
    }
    let (shorter, longer) = if incoming.len() <= existing.len() {
        (incoming, existing)
    } else {
        (existing, incoming)
    };
    if shorter.len() >= CONTAINS_FLOOR && longer.contains(shorter) {
        return Some(MatchReason::Contains);
    }
    None
}

/// The decision on **creating** a box. Any suspicion blocks; the caller either
/// uses the box that is already there or picks a name that is not a near miss
/// of one.
///
/// `create_new` is the caller's explicit "I know, they're different" signal —
/// the same escape, with the same name, as the Memory guard's. It clears the
/// near/containment screen, because sibling fleets are real (`worker-2` beside
/// `worker-1` must be creatable). It **never clears an exact name**: that box
/// already exists, and there is nothing to create.
pub fn decide_create(name: &MailboxName, existing: &[MailboxName], create_new: bool) -> Decision {
    let candidates = screen(name, existing);
    let exact = candidates.iter().any(|m| m.reason == MatchReason::Exact);
    if candidates.is_empty() || (create_new && !exact) {
        Decision::Proceed
    } else {
        Decision::Block(candidates)
    }
}

/// The decision on a box a write **names but must not create**. An exact name
/// is the box, not a candidate for it; everything else blocks, with whatever
/// the guard can suggest — which may be nothing at all.
pub fn decide_existing(name: &MailboxName, existing: &[MailboxName]) -> Decision {
    if existing.contains(name) {
        return Decision::Proceed;
    }
    Decision::Block(screen(name, existing))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes(names: &[&str]) -> Vec<MailboxName> {
        names.iter().map(|n| MailboxName(n.to_string())).collect()
    }

    fn name(n: &str) -> MailboxName {
        MailboxName(n.to_string())
    }

    /// **The golden case.** A typo must never mint a second box: `inbx` arriving
    /// beside `inbox` comes back with the box the caller meant, and nothing is
    /// written.
    #[test]
    fn a_typo_of_an_existing_box_is_blocked_with_the_box_it_meant() {
        let existing = boxes(&["inbox", "errands"]);
        let Decision::Block(candidates) = decide_create(&name("inbx"), &existing, false) else {
            panic!("a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "inbox");
        assert_eq!(candidates[0].reason, MatchReason::Near);
    }

    /// Creating a box that already exists is blocked on the strongest reason
    /// there is — and no signal forces it through: that box already exists.
    #[test]
    fn creating_a_box_that_exists_is_blocked_exactly() {
        for create_new in [false, true] {
            let Decision::Block(candidates) =
                decide_create(&name("inbox"), &boxes(&["inbox"]), create_new)
            else {
                panic!("an existing name must block (create_new={create_new})");
            };
            assert_eq!(candidates[0].reason, MatchReason::Exact);
        }
    }

    /// **The escape hatch: a sibling fleet is deliberate.** `worker-2` beside
    /// `worker-1` blocks as a near miss until the caller says so explicitly —
    /// the same `create_new` signal, with the same semantics, as the Memory
    /// guard's: it clears the similarity screen and never an exact name.
    #[test]
    fn create_new_overrides_similarity_but_never_an_exact_name() {
        let existing = boxes(&["worker-1"]);
        assert!(
            matches!(
                decide_create(&name("worker-2"), &existing, false),
                Decision::Block(_)
            ),
            "without the signal a near miss blocks"
        );
        assert_eq!(
            decide_create(&name("worker-2"), &existing, true),
            Decision::Proceed,
            "the signal clears the near-miss screen"
        );
        assert_eq!(
            decide_create(&name("worker-1-audit"), &existing, true),
            Decision::Proceed,
            "…and the containment screen"
        );
    }

    /// One name inside another is the other confusion: `inbox` and `work-inbox`
    /// are not a typo apart, and a poster who means one routinely types the
    /// other.
    #[test]
    fn a_name_that_contains_an_existing_one_is_flagged() {
        let Decision::Block(candidates) =
            decide_create(&name("work-inbox"), &boxes(&["inbox"]), false)
        else {
            panic!("a containing name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "inbox");
        assert_eq!(candidates[0].reason, MatchReason::Contains);

        // …and read the other way round, which is the same confusion.
        let Decision::Block(candidates) =
            decide_create(&name("inbox"), &boxes(&["work-inbox"]), false)
        else {
            panic!("a contained name must block");
        };
        assert_eq!(candidates[0].reason, MatchReason::Contains);
    }

    /// Containment below the floor is noise, not signal — otherwise every short
    /// name collides with half the board.
    #[test]
    fn containment_needs_enough_name_to_be_evidence() {
        assert_eq!(
            decide_create(&name("ab"), &boxes(&["ab-reports"]), false),
            Decision::Proceed,
            "a two-letter fragment inside a longer name says nothing"
        );
    }

    #[test]
    fn an_unrelated_name_proceeds() {
        assert_eq!(
            decide_create(&name("shipments"), &boxes(&["inbox", "errands"]), false),
            Decision::Proceed
        );
        assert_eq!(decide_create(&name("inbox"), &[], false), Decision::Proceed);
    }

    /// A handle a write only NAMES must already exist. The exact box is the box
    /// — otherwise every second message into a known mailbox would need
    /// confirming — and everything else blocks, near miss or not.
    #[test]
    fn posting_needs_a_box_that_exists_and_a_near_miss_still_names_it() {
        let existing = boxes(&["inbox", "errands"]);
        assert_eq!(
            decide_existing(&name("inbox"), &existing),
            Decision::Proceed,
            "an exact name is the box, not a candidate for it"
        );

        let Decision::Block(candidates) = decide_existing(&name("inbx"), &existing) else {
            panic!("a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "inbox");

        // **A name nothing resembles blocks too**, with an empty list: "I don't
        // know this box" is the answer, and there is nothing to suggest beside it.
        let Decision::Block(none) = decide_existing(&name("shipments"), &existing) else {
            panic!("an unknown name must block, not proceed");
        };
        assert!(
            none.is_empty(),
            "nothing to suggest, nothing invented: {none:?}"
        );
    }

    /// Several candidates come back strongest-first, then by name.
    #[test]
    fn candidates_are_ordered_strongest_first_and_deterministically() {
        let existing = boxes(&["reports", "report", "reports-archive"]);
        let got: Vec<(String, MatchReason)> = screen(&name("reports"), &existing)
            .into_iter()
            .map(|m| (m.name.0, m.reason))
            .collect();
        assert_eq!(
            got,
            vec![
                ("reports".to_string(), MatchReason::Exact),
                ("report".to_string(), MatchReason::Near),
                ("reports-archive".to_string(), MatchReason::Contains),
            ]
        );
    }
}
