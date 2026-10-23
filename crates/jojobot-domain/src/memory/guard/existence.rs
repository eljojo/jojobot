//! The existence gates that sit on [`decide_existing`]: a name the store
//! resolved, and the parent a write sits under.

use super::*;

/// [`decide_existing`] for a handle the store's own resolution has already been
/// asked about. **`resolved` is the handle the name answers to today**, found
/// directly, through a rename, or as the member a collection's number names; a
/// write that names a thing by its number is naming that member, so a name
/// resolution finds is waved through and only a name nothing answers to is
/// screened. The caller stores what it resolved, never the spelling it was
/// sent.
pub fn decide_named(handle: &EntityId, resolved: Option<&EntityId>, index: &[Entity]) -> Decision {
    match resolved {
        Some(_) => Decision::Proceed,
        None => decide_existing(handle, index),
    }
}

/// The guard's decision on the entity a write wants to sit **under**.
///
/// Two refusals, in the order they can be answered.
///
/// **Nothing is its own parent.** A block naming the offender itself, with
/// [`MatchReason::SelfParent`] — see that variant for why the candidate is the
/// write rather than something it might have meant. Checked first because the
/// existence gate would otherwise answer it wrongly-but-plausibly: a fresh
/// entity naming itself has indeed not been created yet, so "no such entity"
/// is technically true and completely unhelpful.
///
/// **A parent must already exist.** Beyond that this is [`decide_existing`] and
/// nothing more: naming a parent is naming an entity, so it gets the same gate
/// as a capture's subject or an edge's object, with the same absence of a
/// override. Creation is an intentional act; a tree that grew its own
/// branches on the way past would be exactly the auto-provisioning that rule
/// exists to forbid.
///
/// **A cycle deeper than this cannot be reached**, so nothing here looks for
/// one. Parentage is fixed at creation ([`super::NewEntity::parent`], with no
/// counterpart on [`super::EntityPatch`]), and an entity being created has no
/// children yet — so there is no descendant for it to be filed under. If
/// reparenting ever lands it brings the ancestor walk with it; writing that
/// walk now would be machinery guarding a case no caller can produce, and the
/// test for it could only be written by reaching past the port.
///
/// Kinds are not consulted. The tree sorts by specificity, not taxonomy: a
/// detail under a project is the whole point of it.
pub fn decide_parent(child: &Entity, parent: &EntityId, index: &[Entity]) -> Decision {
    if parent == &child.id {
        return Decision::Block(vec![EntityMatch {
            handle: child.id.clone(),
            kind: child.kind,
            name: child.name.clone(),
            source: child.source.clone(),
            reason: MatchReason::SelfParent,
            told_apart_by: None,
        }]);
    }
    decide_existing(parent, index)
}
