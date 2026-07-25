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
/// names (`ada` / `otto`) do not.
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

/// Fold a set of labels to their comparison forms, dropping blanks. Both sides
/// of every name comparison go through this, so "the same name" means one thing.
fn folded(labels: &[&str]) -> Vec<String> {
    labels
        .iter()
        .map(|l| normalize_name(l))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Every existing entity the incoming (`handle`, `labels`) might already be,
/// with the strongest reason each matched. Deterministic: same inputs, same
/// order (reason first, then handle), so two sessions see the same report.
///
/// `labels` is every name the incoming write claims — its display name and any
/// alias — and it may be empty: a `capture` knows only a subject handle, and the
/// slug comparisons still apply. Each is compared against every label the
/// existing entity wears ([`Entity::labels`]), because a nickname the guard
/// doesn't know is a second entity waiting to be created under the name the user
/// actually says.
pub fn screen(handle: &EntityId, labels: &[&str], index: &[Entity]) -> Vec<EntityMatch> {
    let incoming_slug = handle.slug();
    let incoming_names = folded(labels);
    let incoming_name_slugs: Vec<String> =
        incoming_names.iter().map(|n| slugify(n)).collect();

    let mut matches: Vec<EntityMatch> = index
        .iter()
        .filter_map(|e| {
            reason_for(
                handle,
                incoming_slug,
                &incoming_names,
                &incoming_name_slugs,
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

/// Every existing entity an incoming set of **labels** might already be — the
/// relabel channel. `kind` is the kind of the entity being relabelled; there is
/// no slug to compare, because relabelling never touches the handle.
///
/// Same reasons and same order as [`screen`], minus the two handle channels.
/// That subtraction is the whole point: see [`decide_relabel`].
///
/// A set rather than one name, because a display name and an alias are the same
/// kind of claim — "this is what it is called" — and screening only the
/// preferred one leaves the alias channel as an open door onto every collision
/// the other channel refuses.
pub fn screen_labels(kind: Option<EntityKind>, labels: &[&str], index: &[Entity]) -> Vec<EntityMatch> {
    let incoming = folded(labels);
    let incoming_slugs: Vec<String> = incoming.iter().map(|n| slugify(n)).collect();

    let mut matches: Vec<EntityMatch> = index
        .iter()
        .filter_map(|e| {
            name_reason(kind, &incoming, &incoming_slugs, e).map(|reason| {
                EntityMatch {
                    handle: e.id.clone(),
                    kind: e.kind,
                    name: e.name.clone(),
                    source: e.source.clone(),
                    reason,
                }
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
    incoming_names: &[String],
    incoming_name_slugs: &[String],
    existing: &Entity,
) -> Option<MatchReason> {
    if &existing.id == handle {
        return Some(MatchReason::ExactHandle);
    }
    let same_kind = handle.kind() == Some(existing.kind);

    // The handle channel of a name collision: one of the existing entity's
    // labels spells out the incoming handle ("Alpha One" already there,
    // `person:alpha-one` arriving; or an alias "Cosme Fulanito" and `person:cosme-fulanito`) — the
    // same collision wearing a different hat.
    if folded(&existing.labels()).iter().any(|n| slugify(n) == incoming_slug) {
        return Some(if same_kind {
            MatchReason::SameName
        } else {
            MatchReason::SameNameOtherKind
        });
    }

    // The name channels, shared with `screen_labels`.
    let by_name = name_reason(handle.kind(), incoming_names, incoming_name_slugs, existing);
    if matches!(
        by_name,
        Some(MatchReason::SameName | MatchReason::SameNameOtherKind)
    ) {
        return by_name;
    }

    // Typo range is only meaningful within a kind: `place:x` and `person:y`
    // being one letter apart says nothing.
    if !same_kind {
        return None;
    }
    if edit_distance(incoming_slug, existing.id.slug()) <= NEAR {
        return Some(MatchReason::NearSlug);
    }
    by_name
}

/// The strongest reason an incoming set of **names** means an existing entity:
/// some incoming label and some existing label agree once folded, an incoming
/// label spells out the existing handle, or two labels are within a typo.
/// `incoming` is already normalized, `incoming_slugs` are their slugified forms.
///
/// **Every label on both sides.** An entity's aliases are names it answers to,
/// not decoration: matching only the preferred one lets the name the user
/// actually says walk straight past the guard.
fn name_reason(
    kind: Option<EntityKind>,
    incoming: &[String],
    incoming_slugs: &[String],
    existing: &Entity,
) -> Option<MatchReason> {
    let same_kind = kind == Some(existing.kind);
    let existing_names = folded(&existing.labels());

    let agree = incoming.iter().any(|a| existing_names.iter().any(|b| a == b))
        || incoming_slugs.iter().any(|s| s == existing.id.slug());
    if agree {
        return Some(if same_kind {
            MatchReason::SameName
        } else {
            MatchReason::SameNameOtherKind
        });
    }
    if !same_kind {
        return None;
    }
    let near = incoming
        .iter()
        .any(|a| existing_names.iter().any(|b| edit_distance(a, b) <= NEAR));
    near.then_some(MatchReason::NearName)
}

/// The guard's decision on a write that names an entity.
///
/// `create_new` is the caller's explicit "I know, they're different" signal — it
/// clears fuzzy suspicion but **never an exact handle collision**: two entities
/// cannot share a handle, so that case is re-slug-or-confirm, always.
pub fn decide(
    handle: &EntityId,
    labels: &[&str],
    index: &[Entity],
    create_new: bool,
) -> Decision {
    let matches = screen(handle, labels, index);
    let exact = matches.iter().any(|m| m.reason == MatchReason::ExactHandle);
    if matches.is_empty() || (create_new && !exact) {
        Decision::Proceed
    } else {
        Decision::Block(matches)
    }
}

/// The guard's decision on a handle a write **names but must not create** — a
/// capture's subject, an edge's object. This is an **existence gate**, not only
/// a similarity one.
///
/// A handle that resolves exactly is already known and is waved through: it is
/// the entity, not a candidate for it. Everything else blocks — a near miss with
/// the candidates that explain it, an unrecognized handle with an empty list.
///
/// There is deliberately **no create-new escape**. A write that names an entity
/// is not a write that may invent one: auto-provisioning on a novel handle
/// turned every typo, and every plausible-looking id an AI produced, into a
/// nameless entity nobody chose, sitting in the store forever. A genuinely new
/// entity is two deliberate steps — `add_entity`, then the write — and the
/// second one is what proves the first was meant.
pub fn decide_existing(handle: &EntityId, index: &[Entity]) -> Decision {
    if index.iter().any(|e| &e.id == handle) {
        return Decision::Proceed;
    }
    Decision::Block(screen(handle, &[], index))
}

/// The guard's decision on a **relabel** — a change to any of the names an
/// entity answers to, its display name or its aliases alike.
///
/// Relabelling is an entity-touching write, so it faces a gate: without one the
/// guard is trivially side-steppable — create under a throwaway name, then move
/// the contested name on afterwards. **The alias channel is the one that was
/// open**: a patch carrying only aliases named no new display name, so nothing
/// screened it, and search then indexed two entities answering to one word.
///
/// Screened on the **label channel only** ([`screen_labels`]), with three
/// properties of relabelling rather than new policy:
///
/// * **the handle is not changing, so it is not screened.** Screening it
///   re-litigated a near-slug that was adjudicated when the entity was created
///   — turning that one settled decision into a permanent block on the name
///   field: every later name edit came back blocked, on a channel nothing had
///   touched.
/// * the entity being relabelled is excluded, or it would match itself.
/// * **a label it already wears is not a new claim.** Only what the patch
///   actually adds is screened, so a no-op cannot introduce a collision that is
///   not already there — and a patch touching no label at all (source, crm,
///   boot) is screened against nothing, with no special case needed for it.
///
/// `create_new` clears any suspicion here: only a handle can collide
/// unforgivably, and no handle is moving.
pub fn decide_relabel(
    handle: &EntityId,
    incoming: &[&str],
    current: &[&str],
    index: &[Entity],
    create_new: bool,
) -> Decision {
    let worn = folded(current);
    let added: Vec<&str> = incoming
        .iter()
        .filter(|l| !worn.contains(&normalize_name(l)))
        .copied()
        .collect();
    if added.is_empty() {
        return Decision::Proceed;
    }
    let matches: Vec<EntityMatch> = screen_labels(handle.kind(), &added, index)
        .into_iter()
        .filter(|m| &m.handle != handle)
        .collect();
    if matches.is_empty() || create_new {
        Decision::Proceed
    } else {
        Decision::Block(matches)
    }
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
            aliases: Vec::new(),
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
        screen(&EntityId(handle.into()), &name.into_iter().collect::<Vec<_>>(), &index())
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
                decide(&taken, &["Alpha Two"], &index(), create_new)
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
        let Decision::Block(candidates) = decide(&qualified, &["Alpha"], &index(), false) else {
            panic!("a same-name person must block");
        };
        assert_eq!(candidates[0].reason, MatchReason::SameName);
        assert_eq!(
            decide(&qualified, &["Alpha"], &index(), true),
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
        assert_eq!(reasons("person:otto", Some("Bet")), vec![MatchReason::NearName]);
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
            decide(&EntityId("person:zenith".into()), &["Zenith"], &index(), false),
            Decision::Proceed
        );
        assert_eq!(decide(&EntityId("topic:widgets".into()), &[], &[], false), Decision::Proceed);
    }

    /// Several candidates come back strongest-first, then by handle — a stable
    /// report, so two sessions screening the same write read the same list.
    #[test]
    fn candidates_are_ordered_strongest_first_and_deterministically() {
        let mut idx = index();
        // `alpha` → `alpha-b` is two edits, so this one lands on the slug channel.
        idx.push(entity("person:alpha-b", "Unrelated", "user-named"));
        idx.push(entity("person:alpha-a", "Alpha", "user-named"));
        let got: Vec<_> = screen(&EntityId("person:alpha".into()), &["Alpha"], &idx)
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

    /// An entity with no name yet — a doc from before the existence gate, or one
    /// a human started by hand — is still screened on its slug, and never
    /// matches on an empty name.
    #[test]
    fn an_unnamed_entity_still_screens_by_slug() {
        let idx = vec![entity("person:alpha", "", "capture")];
        assert_eq!(
            screen(&EntityId("person:alphaa".into()), &[], &idx)[0].reason,
            MatchReason::NearSlug,
            "with no name on either side, the slugs are all there is to go on"
        );
        assert_eq!(
            screen(&EntityId("person:alphaa".into()), &["Alpha"], &idx)[0].reason,
            MatchReason::SameName,
            "an incoming name that lands exactly on an existing handle is the stronger signal"
        );
        assert!(
            screen(&EntityId("person:zenith".into()), &[""], &idx).is_empty(),
            "an empty name must not match an empty name"
        );
    }

    // --- every label, not just the preferred one -----------------------------

    fn also_known_as(id: &str, name: &str, aliases: &[&str]) -> Entity {
        Entity {
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            ..entity(id, name, "user-named")
        }
    }

    /// **The acceptance case.** Someone known as Homer Simpson and called Cosme Fulanito is one
    /// person. A write arriving as "Cosme Fulanito" has to hit the guard, or the second
    /// entity gets created under the name the user actually says — and from then
    /// on half the facts live on each.
    #[test]
    fn a_write_under_an_alias_is_recognized_as_the_entity_that_wears_it() {
        let idx = vec![also_known_as("person:homer-simpson", "Homer Simpson", &["Cosme Fulanito"])];

        let Decision::Block(candidates) =
            decide(&EntityId("person:cosme-fulanito".into()), &["Cosme Fulanito"], &idx, false)
        else {
            panic!("a name the entity already answers to must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:homer-simpson");
        assert_eq!(candidates[0].reason, MatchReason::SameName);

        // The handle channel of the same collision: a slug that spells the alias.
        assert_eq!(
            screen(&EntityId("person:cosme-fulanito".into()), &[], &idx)[0].reason,
            MatchReason::SameName,
            "a handle spelling out an alias is the same collision in another hat"
        );

        // And a typo of an alias is a near miss, exactly as a typo of a name is.
        assert_eq!(
            screen(&EntityId("person:zzz".into()), &["Cosme Fulanit"], &idx)[0].reason,
            MatchReason::NearName
        );
    }

    /// The incoming side counts too: an entity arriving with an alias that
    /// belongs to someone already here is the same collision read backwards.
    #[test]
    fn an_incoming_alias_collides_with_an_existing_name() {
        let idx = vec![entity("person:homer-simpson", "Homer Simpson", "user-named")];
        let Decision::Block(candidates) =
            decide(&EntityId("person:barney-gumble".into()), &["Barney Gumble", "Homer Simpson"], &idx, false)
        else {
            panic!("an incoming alias that names someone here must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:homer-simpson");
        assert_eq!(candidates[0].reason, MatchReason::SameName);
    }

    /// A rename onto a name an entity already answers to is blocked — the alias
    /// channel of the collision the rename gate exists for.
    #[test]
    fn a_rename_onto_an_existing_alias_is_blocked() {
        let idx = vec![
            also_known_as("person:homer-simpson", "Homer Simpson", &["Cosme Fulanito"]),
            entity("person:zenith", "Zenith", "user-named"),
        ];
        let Decision::Block(candidates) =
            rename(&EntityId("person:zenith".into()), "Cosme Fulanito", "Zenith", &idx, false)
        else {
            panic!("renaming onto an alias must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:homer-simpson");
        assert_eq!(candidates[0].reason, MatchReason::SameName);
    }

    /// A handle a write only names is screened against every label too, so the
    /// candidate list that comes back with a blocked capture can say "you may
    /// mean Homer Simpson" when what was typed was Cosme Fulanito.
    #[test]
    fn a_must_exist_miss_suggests_by_alias() {
        let idx = vec![also_known_as("person:homer-simpson", "Homer Simpson", &["Cosme Fulanito"])];
        let Decision::Block(candidates) = decide_existing(&EntityId("person:cosme-fulanito".into()), &idx)
        else {
            panic!("an unknown handle blocks");
        };
        assert_eq!(
            candidates[0].handle.as_str(),
            "person:homer-simpson",
            "the suggestion is the entity that wears that name: {candidates:?}"
        );
    }

    // --- a handle a write only names -----------------------------------------

    /// A handle a write only NAMES must already exist. An exact handle IS the
    /// entity, so naming it is never suspicious — otherwise every second fact
    /// about someone, and every edge pointing at a known place, would need
    /// confirming. Everything else blocks, near miss or not.
    #[test]
    fn a_named_handle_must_already_exist_and_a_near_miss_still_names_candidates() {
        let idx = index();
        assert_eq!(
            decide_existing(&EntityId("person:alpha".into()), &idx),
            Decision::Proceed,
            "an exact handle is the entity, not a candidate for it"
        );

        let Decision::Block(candidates) = decide_existing(&EntityId("person:alphaa".into()), &idx)
        else {
            panic!("a near-miss handle must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:alpha");

        // **A handle nothing resembles blocks too**, with an empty list. There is
        // no create-new escape here: this is a write that NAMES an entity, and
        // an entity it cannot find is not one it may invent. "I don't know this
        // one" is the answer; there is simply nothing to suggest alongside it.
        let Decision::Block(none) = decide_existing(&EntityId("person:zenith".into()), &idx) else {
            panic!("a handle that resolves to nothing must block, not proceed");
        };
        assert!(none.is_empty(), "nothing to suggest, and nothing invented: {none:?}");
    }

    // --- relabelling goes through the same gate ------------------------------

    /// A rename in the relabel vocabulary: one incoming label replacing the one
    /// currently worn. Renaming is the special case; relabelling is the rule.
    fn rename(
        handle: &EntityId,
        new_name: &str,
        current_name: &str,
        index: &[Entity],
        create_new: bool,
    ) -> Decision {
        decide_relabel(handle, &[new_name], &[current_name], index, create_new)
    }

    /// A rename onto a name the index already holds is blocked, and the same
    /// explicit signal clears it — the creation gate, reused.
    #[test]
    fn a_rename_onto_an_existing_name_is_blocked_and_create_new_clears_it() {
        let renamer = EntityId("person:zenith".into());
        let Decision::Block(candidates) =
            rename(&renamer, "Alpha", "Zenith", &index(), false)
        else {
            panic!("a rename onto an existing name must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:alpha");
        assert_eq!(candidates[0].reason, MatchReason::SameName);
        assert_eq!(
            rename(&renamer, "Alpha", "Zenith", &index(), true),
            Decision::Proceed
        );
    }

    /// A rename screens the NAME, never the handle — the handle isn't changing.
    /// Screening it re-litigated a near-slug that was already adjudicated when
    /// the entity was created, so that one decision froze the name field forever:
    /// every later name edit came back blocked, on a channel nothing had touched.
    #[test]
    fn a_rename_does_not_re_screen_the_immutable_handle() {
        let mut idx = index();
        // `person:alphaa` is one edit from `person:alpha` — settled at creation.
        idx.push(entity("person:alphaa", "Second Alpha", "user-named"));
        let settled = EntityId("person:alphaa".into());
        assert_eq!(
            rename(&settled, "Something Unrelated", "Second Alpha", &idx, false),
            Decision::Proceed,
            "the handle is not changing, so a settled near-slug must not block a name edit"
        );
    }

    /// The name channels still fire on a rename: an incoming name that lands on
    /// an existing handle is the collision the guard exists for.
    #[test]
    fn a_rename_onto_an_existing_handles_spelling_is_blocked() {
        let renamer = EntityId("place:trail-spot".into());
        let Decision::Block(candidates) =
            rename(&renamer, "North Trail", "Trail Spot", &index(), false)
        else {
            panic!("a name that spells out an existing handle must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "place:north-trail");
        assert_eq!(candidates[0].reason, MatchReason::SameName);
    }

    /// A near-*name* is still caught on a rename — only the handle channels are
    /// out of scope.
    #[test]
    fn a_rename_onto_a_near_name_is_blocked() {
        let renamer = EntityId("person:zenith".into());
        let Decision::Block(candidates) =
            rename(&renamer, "Bet", "Zenith", &index(), false)
        else {
            panic!("a name within a typo of an existing one must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:beta");
        assert_eq!(candidates[0].reason, MatchReason::NearName);
    }

    /// An entity must not match itself: it is in the index, so screening it
    /// against the whole index would block every rename on ExactHandle.
    #[test]
    fn a_rename_does_not_screen_the_entity_against_itself() {
        let existing = EntityId("person:alpha".into());
        assert_eq!(
            rename(&existing, "Something Unrelated", "Alpha", &index(), false),
            Decision::Proceed,
            "an entity is not a candidate for its own rename"
        );
    }

    /// **The door the rename gate left open.** An alias is a name, so claiming
    /// one another entity already answers to is the same collision — and it
    /// arrives on a patch that renames nothing, which is exactly why nothing
    /// used to screen it.
    #[test]
    fn an_added_alias_is_screened_like_a_rename() {
        let idx = vec![
            also_known_as("person:homer-simpson", "Homer Simpson", &["Cosme Fulanito"]),
            entity("person:zenith", "Zenith", "user-named"),
        ];
        let borrower = EntityId("person:zenith".into());
        let Decision::Block(candidates) =
            decide_relabel(&borrower, &["Zenith", "Cosme Fulanito"], &["Zenith"], &idx, false)
        else {
            panic!("an alias onto a name another entity wears must block");
        };
        assert_eq!(candidates[0].handle.as_str(), "person:homer-simpson");
        assert_eq!(candidates[0].reason, MatchReason::SameName);

        // Names are not unique; handles are. The same signal clears it.
        assert_eq!(
            decide_relabel(&borrower, &["Zenith", "Cosme Fulanito"], &["Zenith"], &idx, true),
            Decision::Proceed
        );
    }

    /// Only what a patch **adds** is screened. A label the entity already wears
    /// is not a new claim, and a patch that moves no label at all — a source or
    /// crm edit — is therefore screened against nothing, with no special case.
    #[test]
    fn a_label_already_worn_is_not_a_new_claim() {
        let mut idx = index();
        idx.push(also_known_as("person:alpha-two", "Second Alpha", &["Alpha"]));
        let settled = EntityId("person:alpha-two".into());

        assert_eq!(
            decide_relabel(
                &settled,
                &["Second Alpha", "Alpha"],
                &["Second Alpha", "Alpha"],
                &idx,
                false
            ),
            Decision::Proceed,
            "re-sending the labels it already wears is not a collision with anyone"
        );
        assert_eq!(
            decide_relabel(&settled, &["  SECOND   alpha ", "alpha"], &["Second Alpha", "Alpha"], &idx, false),
            Decision::Proceed,
            "case and spacing folded on both sides"
        );
        assert_eq!(
            decide_relabel(&settled, &[], &["Second Alpha"], &idx, false),
            Decision::Proceed,
            "a patch carrying no label at all screens against nothing"
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
            rename(&existing, "  ALPHA  ", "Alpha", &idx, false),
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
