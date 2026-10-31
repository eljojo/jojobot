//! The roots narrowing: the things a tree starts from.

use super::*;

/// The query for the roots alone.
fn roots() -> GraphQuery {
    selecting(Selection {
        roots: true,
        ..Selection::default()
    })
}

/// 🚨 **A root is a thing with no parent, or filed under a handle nothing
/// standing answers to.** The positive sits beside the negative: a child of a
/// standing thing is not a root, a child of a handle no entity has is one, and
/// so is a child of a thing that was archived. The page and a bot ask the same
/// question, so the answer is the set the index would start from.
#[test]
fn a_root_has_no_parent_or_a_parent_nothing_standing_answers_to() {
    let under = |handle: &str, parent: &str| Entity {
        parent: Some(EntityId(parent.into())),
        ..entity(handle, handle)
    };
    let archived = Entity {
        archived: Some(crate::memory::Archived {
            reason: "no longer part of the plan".into(),
            at: "2026-10-01T00:00:00Z".parse().expect("a moment"),
        }),
        ..entity("project:omega", "omega")
    };
    let scanned = vec![
        doc(entity("project:atlas", "atlas"), "", Vec::new()),
        doc(under("work:delta", "project:atlas"), "", Vec::new()),
        doc(under("work:sigma", "project:zeta"), "", Vec::new()),
        doc(archived, "", Vec::new()),
        doc(under("work:kappa", "project:omega"), "", Vec::new()),
    ];

    let found = resolve(&scanned, &[], &roots()).expect("roots is a way to select");

    let mut kept = handles(&found.objects);
    kept.sort_unstable();
    assert_eq!(
        kept,
        vec!["project:atlas", "work:kappa", "work:sigma"],
        "the child of a standing thing is not a root, and the dangling ones are"
    );
}
