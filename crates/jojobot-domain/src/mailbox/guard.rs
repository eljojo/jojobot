//! The mailbox write guard — the same contacts-book check the Memory context
//! runs, in this context's vocabulary.
//!
//! Two gates, and they are not the same gate:
//!
//! * **creating** a box screens the incoming name against the boxes that exist,
//!   so `inbx` beside an existing `inbox` comes back as candidates rather than
//!   as a second box nobody meant. The token that refusal minted, handed back,
//!   lifts the similarity screen (sibling fleets like `worker-1`, `worker-2`
//!   are legitimate) — but never an exact name, which already exists.
//! * **naming** a box (posting into one) is an *existence* gate: an exact name
//!   is the box and is waved through; anything else is refused, near miss or
//!   not. There is deliberately no override here — a typo that mints a box
//!   is a message posted where nobody is listening, and it looks like success.
//!
//! Pure: no I/O, no clock, no randomness — every decision here is a function of
//! (name, existing names), which is why it can sit on the adapter's write path
//! and cannot be routed around.

use super::MailboxName;
use crate::override_token::Collision;

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
/// **`override_token` is the token THIS refusal minted, handed back** (rule
/// 75) — the same mechanism, and the same type, as the Memory guard's, because
/// one gate's escape and the other's are one thing (rule 51). It clears the
/// near/containment screen, because sibling fleets are real (`worker-2` beside
/// `worker-1` must be creatable). It **never clears an exact name**: that box
/// already exists, and there is nothing to create (rule 61).
pub fn decide_create(
    name: &MailboxName,
    existing: &[MailboxName],
    override_token: Option<&str>,
) -> Decision {
    decide_create_for(name, None, existing, override_token)
}

/// The same decision, for a box being opened **with the bot that owns it**.
///
/// **A box named for its owner's handle is not a second adjudication.** The
/// handle went through the entity screen in the same act, and that screen is
/// where the collision actually happens; asking again here would refuse
/// `bot:worker-2`'s box beside `bot:worker-1`'s for a resemblance somebody has
/// already answered for.
///
/// This is what used to need an override flag on the internal path — jojobot
/// setting the boolean on its own guard, which is the hole the flag was. It is
/// a rule now rather than a permission: the name either IS the owner's handle
/// or it is not, and nothing a caller sends changes which.
pub fn decide_create_for(
    name: &MailboxName,
    owner_slug: Option<&str>,
    existing: &[MailboxName],
    override_token: Option<&str>,
) -> Decision {
    let candidates = screen(name, existing);
    if candidates.is_empty() {
        return Decision::Proceed;
    }
    // **An exact name is never lifted, by anything.** The box is already there,
    // so there is nothing to create and no judgement to make (rule 61).
    let taken = candidates.iter().any(|m| m.reason == MatchReason::Exact);
    if taken {
        return Decision::Block(candidates);
    }
    if owner_slug == Some(name.as_str()) || collision(name, &candidates).honours(override_token) {
        return Decision::Proceed;
    }
    Decision::Block(candidates)
}

/// The refusal this creation would get, as the token mechanism addresses it.
/// `gate` differs from the entity screen's, so a token minted over there does
/// not lift a refusal over here even when the names match.
fn collision(name: &MailboxName, candidates: &[MailboxMatch]) -> Collision {
    Collision {
        gate: "mailbox",
        attempted: name.to_string(),
        candidates: candidates.iter().map(|c| c.name.to_string()).collect(),
    }
}

/// The token that lifts the refusal these candidates represent — what a
/// blocked answer hands back so a caller can decide and come again.
pub fn override_token(name: &MailboxName, candidates: &[MailboxMatch]) -> String {
    collision(name, candidates).token()
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
        let Decision::Block(candidates) = decide_create(&name("inbx"), &existing, None) else {
            panic!("a near-miss name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "inbox");
        assert_eq!(candidates[0].reason, MatchReason::Near);
    }

    /// Creating a box that already exists is blocked on the strongest reason
    /// there is — and nothing forces it through, not even the token this very
    /// refusal mints: that box already exists (rule 61).
    #[test]
    fn creating_a_box_that_exists_is_blocked_exactly() {
        let existing = boxes(&["inbox"]);
        let Decision::Block(candidates) = decide_create(&name("inbox"), &existing, None) else {
            panic!("an existing name must block");
        };
        assert_eq!(candidates[0].reason, MatchReason::Exact);

        let minted = override_token(&name("inbox"), &candidates);
        let Decision::Block(again) = decide_create(&name("inbox"), &existing, Some(&minted)) else {
            panic!("an existing name stays blocked, token or not");
        };
        assert_eq!(again[0].reason, MatchReason::Exact);
    }

    /// **The escape hatch: a sibling fleet is deliberate.** `worker-2` beside
    /// `worker-1` blocks as a near miss until the caller hands back the token
    /// that refusal minted — the same mechanism, and the same type, as the
    /// Memory guard's (rule 51). It clears the similarity screen; the exact
    /// name above it stays shut.
    #[test]
    fn a_refusals_own_token_clears_the_similarity_screen() {
        let existing = boxes(&["worker-1"]);

        let Decision::Block(near) = decide_create(&name("worker-2"), &existing, None) else {
            panic!("without a token a near miss blocks");
        };
        assert_eq!(
            decide_create(
                &name("worker-2"),
                &existing,
                Some(&override_token(&name("worker-2"), &near)),
            ),
            Decision::Proceed,
            "the token this refusal minted clears the near-miss screen"
        );

        let Decision::Block(contained) = decide_create(&name("worker-1-audit"), &existing, None)
        else {
            panic!("without a token a containing name blocks");
        };
        assert_eq!(
            decide_create(
                &name("worker-1-audit"),
                &existing,
                Some(&override_token(&name("worker-1-audit"), &contained)),
            ),
            Decision::Proceed,
            "…and the containment screen"
        );
    }

    /// **A token minted elsewhere lifts nothing here.** This is the half that
    /// makes the mechanism one: a guard that accepts any string it is handed is
    /// the boolean again, wearing a longer name.
    #[test]
    fn only_this_refusals_own_token_lifts_it() {
        let existing = boxes(&["worker-1", "errands"]);
        let Decision::Block(elsewhere) = decide_create(&name("errand"), &existing, None) else {
            panic!("a near miss of errands must block");
        };
        let borrowed = override_token(&name("errand"), &elsewhere);

        for offered in [borrowed.as_str(), "0000000000000000", ""] {
            assert!(
                matches!(
                    decide_create(&name("worker-2"), &existing, Some(offered)),
                    Decision::Block(_)
                ),
                "a token this refusal did not mint lifts nothing: {offered:?}"
            );
        }
    }

    /// **A box named for its owner needs no token, and no exception either.**
    /// The handle went through the entity screen in the same act, so screening
    /// the same string again here would refuse `worker-2`'s box beside
    /// `worker-1`'s for a resemblance somebody has already answered for. It is
    /// a rule about the name, not a permission a caller carries: an exact name
    /// still blocks, so the rule cannot open a second box on one bot.
    #[test]
    fn a_box_named_for_its_owner_is_not_screened_twice() {
        let existing = boxes(&["worker-1"]);
        assert_eq!(
            decide_create_for(&name("worker-2"), Some("worker-2"), &existing, None),
            Decision::Proceed,
            "the name IS the owner's handle"
        );
        assert!(
            matches!(
                decide_create_for(&name("worker-2"), Some("shelbyville"), &existing, None),
                Decision::Block(_)
            ),
            "an owner whose handle is some other name buys nothing"
        );
        let Decision::Block(candidates) =
            decide_create_for(&name("worker-1"), Some("worker-1"), &existing, None)
        else {
            panic!("an exact name stays blocked: that box already exists");
        };
        assert_eq!(candidates[0].reason, MatchReason::Exact);
    }

    /// One name inside another is the other confusion: `inbox` and `work-inbox`
    /// are not a typo apart, and a poster who means one routinely types the
    /// other.
    #[test]
    fn a_name_that_contains_an_existing_one_is_flagged() {
        let Decision::Block(candidates) =
            decide_create(&name("work-inbox"), &boxes(&["inbox"]), None)
        else {
            panic!("a containing name must block");
        };
        assert_eq!(candidates[0].name.as_str(), "inbox");
        assert_eq!(candidates[0].reason, MatchReason::Contains);

        // …and read the other way round, which is the same confusion.
        let Decision::Block(candidates) =
            decide_create(&name("inbox"), &boxes(&["work-inbox"]), None)
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
            decide_create(&name("ab"), &boxes(&["ab-reports"]), None),
            Decision::Proceed,
            "a two-letter fragment inside a longer name says nothing"
        );
    }

    #[test]
    fn an_unrelated_name_proceeds() {
        assert_eq!(
            decide_create(&name("shipments"), &boxes(&["inbox", "errands"]), None),
            Decision::Proceed
        );
        assert_eq!(decide_create(&name("inbox"), &[], None), Decision::Proceed);
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
