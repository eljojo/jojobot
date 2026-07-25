//! The write guard — the contacts-book check, in pure domain logic.
//!
//! jojobot guards the gate **deterministically**; it never trusts the caller to
//! have checked first. Every entity-touching write fuzzy-matches the incoming
//! handle and name against the entity index — exact, case/whitespace-folded,
//! near-slug, near-name, same-name-other-kind — the way a phone checks the
//! contacts book before adding a second entry under a name it already has.
//!
//! On suspicion the guard neither fails nor guesses: it reports the candidates
//! and the write is refused until the caller confirms the existing entity or
//! re-calls with an explicit create-new signal. **Detection without inference:
//! jojobot notices, the AI decides.** No I/O, no clock, no randomness — every
//! decision here is a pure function of (handle, name, index), which is why it can
//! live on the write path of *both* adapters and cannot be skipped.

use super::{Entity, EntityId, EntityKind};

/// Why an existing entity is a candidate for the incoming write, strongest
/// first. The order is the reporting order — the caller reads the top one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchReason {
    /// The very same handle already exists. Never overridable.
    ExactHandle,
    /// Same kind, and the names (or a name and the other's slug) agree once
    /// case and whitespace are folded.
    SameName,
    /// The names agree, but the kinds differ — `project:atlas` vs
    /// `place:atlas`. Usually two real things; sometimes a mis-kinded write.
    SameNameOtherKind,
    /// Same kind, slugs within a typo of each other (edit distance ≤ 2).
    NearSlug,
    /// Same kind, names within a typo of each other (edit distance ≤ 2).
    NearName,
}

/// An existing entity the guard suspects the incoming write means. Carries what
/// the caller needs to decide — handle, kind, name, **source** (where this one
/// came from is usually what settles same-or-different) — and why it matched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntityMatch {
    /// The existing entity's handle.
    pub handle: EntityId,
    /// Its kind.
    pub kind: EntityKind,
    /// Its display name.
    pub name: String,
    /// Where it came from — never invented.
    pub source: String,
    /// Why the guard flagged it.
    pub reason: MatchReason,
}

/// The guard's verdict on a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No suspicion, or the caller resolved it: the write may proceed.
    Proceed,
    /// Suspicion: **nothing is written**. The caller confirms one of these
    /// candidates or re-calls with an explicit create-new signal.
    Block(Vec<EntityMatch>),
}

/// The edit-distance budget for "this is probably a typo of that". Two is the
/// point where transposition + a dropped letter still match but distinct short
/// names (`ada` / `omar`) do not.
const NEAR: usize = 2;

/// Fold a display name to its comparison form: lowercase, edge-trimmed, inner
/// whitespace collapsed. `"  Alpha   One "` and `"alpha one"` are one name.
pub fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render a display name as the slug it would most likely have been given, so a
/// name can be compared against an existing handle: `"Alpha One"` → `alpha-one`.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Levenshtein edit distance, iterative two-row — no allocation per cell, no
/// recursion. Compares by `char`, so a multi-byte name doesn't score by bytes.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Every existing entity the incoming (`handle`, `name`) might already be, with
/// the strongest reason each matched. Deterministic: same inputs, same order
/// (reason first, then handle), so two sessions see the same report.
///
/// `name` is optional — a `capture` knows only a subject handle, and the slug
/// comparisons still apply.
pub fn screen(handle: &EntityId, name: Option<&str>, index: &[Entity]) -> Vec<EntityMatch> {
    let incoming_slug = handle.slug();
    let incoming_name = name.map(normalize_name).filter(|n| !n.is_empty());
    let incoming_name_slug = incoming_name.as_deref().map(slugify);

    let mut matches: Vec<EntityMatch> = index
        .iter()
        .filter_map(|e| {
            reason_for(
                handle,
                incoming_slug,
                incoming_name.as_deref(),
                incoming_name_slug.as_deref(),
                e,
            )
            .map(|reason| EntityMatch {
                handle: e.id.clone(),
                kind: e.kind,
                name: e.name.clone(),
                source: e.source.clone(),
                reason,
            })
        })
        .collect();
    matches.sort_by(|a, b| a.reason.cmp(&b.reason).then_with(|| a.handle.cmp(&b.handle)));
    matches
}

/// The strongest reason one existing entity is a candidate, or `None`.
fn reason_for(
    handle: &EntityId,
    incoming_slug: &str,
    incoming_name: Option<&str>,
    incoming_name_slug: Option<&str>,
    existing: &Entity,
) -> Option<MatchReason> {
    if &existing.id == handle {
        return Some(MatchReason::ExactHandle);
    }
    let same_kind = handle.kind() == Some(existing.kind);
    let existing_name = normalize_name(&existing.name);
    let existing_name = (!existing_name.is_empty()).then_some(existing_name);

    // Names agree — or a name agrees with the other side's handle, which is the
    // same collision wearing a different hat ("Alpha One" vs `person:alpha-one`).
    let names_agree = match (incoming_name, existing_name.as_deref()) {
        (Some(a), Some(b)) if a == b => true,
        _ => {
            incoming_name_slug.is_some_and(|s| s == existing.id.slug())
                || existing_name.as_deref().is_some_and(|n| slugify(n) == incoming_slug)
        }
    };
    if names_agree {
        return Some(if same_kind {
            MatchReason::SameName
        } else {
            MatchReason::SameNameOtherKind
        });
    }

    // Typo range is only meaningful within a kind: `place:x` and `person:y`
    // being one letter apart says nothing.
    if !same_kind {
        return None;
    }
    if edit_distance(incoming_slug, existing.id.slug()) <= NEAR {
        return Some(MatchReason::NearSlug);
    }
    match (incoming_name, existing_name.as_deref()) {
        (Some(a), Some(b)) if edit_distance(a, b) <= NEAR => Some(MatchReason::NearName),
        _ => None,
    }
}

/// The guard's decision on a write that names an entity.
///
/// `create_new` is the caller's explicit "I know, they're different" signal — it
/// clears fuzzy suspicion but **never an exact handle collision**: two entities
/// cannot share a handle, so that case is re-slug-or-confirm, always.
pub fn decide(
    handle: &EntityId,
    name: Option<&str>,
    index: &[Entity],
    create_new: bool,
) -> Decision {
    let matches = screen(handle, name, index);
    let exact = matches.iter().any(|m| m.reason == MatchReason::ExactHandle);
    if matches.is_empty() || (create_new && !exact) {
        Decision::Proceed
    } else {
        Decision::Block(matches)
    }
}

/// The guard's decision on a **rename** — the same screen a creation gets, with
/// two adjustments that are properties of renaming rather than new policy:
///
/// * the entity being renamed is excluded from the index, or it would always
///   match itself on [`MatchReason::ExactHandle`] and no rename could proceed;
/// * a name that isn't actually changing is not screened, because a no-op
///   cannot introduce a collision that isn't already there.
///
/// Without this, the guard is trivially side-steppable: create under a
/// throwaway name, then rename onto the collision.
pub fn decide_rename(
    handle: &EntityId,
    new_name: &str,
    current_name: &str,
    index: &[Entity],
    create_new: bool,
) -> Decision {
    if normalize_name(new_name) == normalize_name(current_name) {
        return Decision::Proceed;
    }
    let others: Vec<Entity> = index.iter().filter(|e| &e.id != handle).cloned().collect();
    decide(handle, Some(new_name), &others, create_new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: &str, name: &str, source: &str) -> Entity {
        let id = EntityId(id.into());
        Entity {
            kind: id.kind().expect("test ids are well-formed"),
            id,
            name: name.into(),
            source: source.into(),
            crm: None,
            boot: Default::default(),
        }
    }

    /// A synthetic cast — deliberately not anyone's real people or places: this
    /// is user-agnostic software and carries no user PII, fixtures included.
    fn index() -> Vec<Entity> {
        vec![
            entity("person:alpha", "Alpha", "crm-card"),
            entity("person:beta", "Beta", "user-named"),
            entity("project:atlas", "Atlas", "user-named"),
            entity("place:north-trail", "North Trail", "user-named"),
        ]
    }

    fn reasons(handle: &str, name: Option<&str>) -> Vec<MatchReason> {
        screen(&EntityId(handle.into()), name, &index())
            .into_iter()
            .map(|m| m.reason)
            .collect()
    }

    // --- the golden case: two same-named people cannot merge silently --------

    /// A second person arriving at an existing handle is blocked, and
    /// `create_new` does NOT override it — a handle has exactly one owner, so
    /// the caller must confirm the existing one or qualify the slug.
    #[test]
    fn a_second_person_at_the_same_handle_can_never_be_forced_through() {
        let taken = EntityId("person:alpha".into());
        for create_new in [false, true] {
            let Decision::Block(candidates) =
                decide(&taken, Some("Alpha Two"), &index(), create_new)
            else {
                panic!("a colliding handle must block (create_new={create_new})");
            };
            assert_eq!(candidates[0].reason, MatchReason::ExactHandle);
            assert_eq!(candidates[0].handle.as_str(), "person:alpha");
            assert_eq!(candidates[0].source, "crm-card", "the caller decides on the source");
        }
    }

    /// The same name under a qualified slug is still flagged — but here
    /// `create_new` is the escape hatch, because the handles differ.
    #[test]
    fn a_qualified_slug_is_flagged_by_name_and_create_new_clears_it() {
        let qualified = EntityId("person:alpha-two".into());
        let Decision::Block(candidates) = decide(&qualified, Some("Alpha"), &index(), false) else {
            panic!("a same-name person must block");
        };
        assert_eq!(candidates[0].reason, MatchReason::SameName);
        assert_eq!(
            decide(&qualified, Some("Alpha"), &index(), true),
            Decision::Proceed,
            "an explicit create-new signal resolves a fuzzy match"
        );
    }

    // --- each detection channel ---------------------------------------------

    #[test]
    fn case_and_whitespace_are_folded_before_comparing() {
        assert_eq!(reasons("person:alpha-2", Some("  ALPHA  ")), vec![MatchReason::SameName]);
        assert_eq!(normalize_name("  Alpha   One "), "alpha one");
    }

    #[test]
    fn a_name_matches_an_existing_handle_and_vice_versa() {
        // "North Trail" slugifies onto the existing `place:north-trail` handle.
        assert_eq!(
            reasons("place:trail-spot", Some("north trail")),
            vec![MatchReason::SameName]
        );
        // …and an incoming handle that spells out an existing entity's name.
        assert_eq!(reasons("place:north-trail-2", None), vec![MatchReason::NearSlug]);
    }

    #[test]
    fn a_typo_in_the_slug_is_caught_within_two_edits() {
        assert_eq!(reasons("person:alphaa", None), vec![MatchReason::NearSlug]);
        assert_eq!(reasons("person:bet", None), vec![MatchReason::NearSlug]);
        // Three edits is a different person, not a typo.
        assert!(reasons("person:alphonse", None).is_empty());
    }

    #[test]
    fn a_typo_in_the_name_is_caught_within_two_edits() {
        assert_eq!(reasons("person:omar-r", Some("Bet")), vec![MatchReason::NearName]);
    }

    #[test]
    fn the_same_name_under_another_kind_is_reported_not_hidden() {
        assert_eq!(
            reasons("place:atlas", Some("Atlas")),
            vec![MatchReason::SameNameOtherKind]
        );
    }

    #[test]
    fn near_matching_does_not_cross_kinds() {
        // `place:bet` is one edit from `person:beta`, but a place is not a
        // person — only an outright name collision crosses kinds.
        assert!(reasons("place:bet", None).is_empty());
    }

    // --- no false positives, and a stable report ------------------------------

    #[test]
    fn an_unrelated_entity_proceeds() {
        assert_eq!(
            decide(&EntityId("person:zenith".into()), Some("Zenith"), &index(), false),
            Decision::Proceed
        );
        assert_eq!(decide(&EntityId("topic:widgets".into()), None, &[], false), Decision::Proceed);
    }

    /// Several candidates come back strongest-first, then by handle — a stable
    /// report, so two sessions screening the same write read the same list.
    #[test]
    fn candidates_are_ordered_strongest_first_and_deterministically() {
        let mut idx = index();
        // `alpha` → `alpha-b` is two edits, so this one lands on the slug channel.
        idx.push(entity("person:alpha-b", "Unrelated", "user-named"));
        idx.push(entity("person:alpha-a", "Alpha", "user-named"));
        let got: Vec<_> = screen(&EntityId("person:alpha".into()), Some("Alpha"), &idx)
            .into_iter()
            .map(|m| (m.handle.0, m.reason))
            .collect();
        assert_eq!(
            got,
            vec![
                ("person:alpha".to_string(), MatchReason::ExactHandle),
                ("person:alpha-a".to_string(), MatchReason::SameName),
                ("person:alpha-b".to_string(), MatchReason::NearSlug),
            ]
        );
    }

    /// An entity with no name yet (a doc self-provisioned by `capture`) is still
    /// screened on its slug, and never matches on an empty name.
    #[test]
    fn an_unnamed_entity_still_screens_by_slug() {
        let idx = vec![entity("person:alpha", "", "capture")];
        assert_eq!(
            screen(&EntityId("person:alphaa".into()), None, &idx)[0].reason,
            MatchReason::NearSlug,
            "with no name on either side, the slugs are all there is to go on"
        );
        assert_eq!(
            screen(&EntityId("person:alphaa".into()), Some("Alpha"), &idx)[0].reason,
            MatchReason::SameName,
            "an incoming name that lands exactly on an existing handle is the stronger signal"
        );
        assert!(
            screen(&EntityId("person:zenith".into()), Some(""), &idx).is_empty(),
            "an empty name must not match an empty name"
        );
    }

    // --- renames go through the same gate ------------------------------------

    /// A rename onto a name the index already holds is blocked, and the same
    /// explicit signal clears it — the creation gate, reused.
    #[test]
    fn a_rename_onto_an_existing_name_is_blocked_and_create_new_clears_it() {
        let renamer = EntityId("person:zenith".into());
        let Decision::Block(candidates) =
            decide_rename(&renamer, "Alpha", "Zenith", &index(), false)
        else {
            panic!("a rename onto an existing name must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:alpha");
        assert_eq!(candidates[0].reason, MatchReason::SameName);
        assert_eq!(
            decide_rename(&renamer, "Alpha", "Zenith", &index(), true),
            Decision::Proceed
        );
    }

    /// An entity must not match itself: it is in the index, so screening it
    /// against the whole index would block every rename on ExactHandle.
    #[test]
    fn a_rename_does_not_screen_the_entity_against_itself() {
        let existing = EntityId("person:alpha".into());
        assert_eq!(
            decide_rename(&existing, "Something Unrelated", "Alpha", &index(), false),
            Decision::Proceed,
            "an entity is not a candidate for its own rename"
        );
    }

    /// A name that isn't changing isn't screened — otherwise editing an
    /// entity's source would trip over a collision that already exists and was
    /// already confirmed.
    #[test]
    fn an_unchanged_name_is_not_screened() {
        let mut idx = index();
        idx.push(entity("person:alpha-two", "Alpha", "user-named"));
        let existing = EntityId("person:alpha-two".into());
        assert_eq!(
            decide_rename(&existing, "  ALPHA  ", "Alpha", &idx, false),
            Decision::Proceed,
            "case and spacing folded: this is the same name, not a new collision"
        );
    }

    #[test]
    fn edit_distance_is_the_textbook_levenshtein() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("café", "cafe"), 1, "compares chars, not bytes");
    }

    #[test]
    fn slugify_renders_a_name_as_the_handle_it_would_have_got() {
        assert_eq!(slugify("Alpha One"), "alpha-one");
        assert_eq!(slugify("  The North Trail! "), "the-north-trail");
        assert_eq!(slugify("North Trail Club"), "north-trail-club");
        assert_eq!(slugify("!!!"), "");
    }
}
