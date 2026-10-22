//! **The testimony gate has no exemption for a section's edges** (rule 606). Any
//! route that reaches a testimony section with `edges` in what it writes, however
//! the verbs above refuse it, is a rewrite like any other key: only the session
//! that wrote the words rewrites them.

use crate::memory::testing::InMemoryMemory;
use crate::memory::{
    EntityId, EntityKind, FactPatch, Memory, MemoryError, NewEntity, NewFact, Provenance,
    settle_rewrite,
};

/// A testimony section of a document, written by session `a`.
async fn a_testimony_section() -> (InMemoryMemory, crate::memory::Fact) {
    let store = InMemoryMemory::booted();
    let document = EntityId("document:homer".into());
    store
        .add_entity(NewEntity::new(
            document.clone(),
            "Fryer manual",
            "user-named",
        ))
        .await
        .expect("the document is made");
    let written = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            session: Some("a".into()),
            fields: [("edges".to_string(), ";;\\n\\n;".to_string())].into(),
            ..NewFact::about(document, "## Opening", jiff::civil::date(2026, 10, 21))
        })
        .await
        .expect("the section is written")
        .written()
        .expect("nothing blocks it");
    assert_eq!(written.home.kind(), Some(EntityKind::DOCUMENT));
    (store, written)
}

/// **Another session's `edges` on a testimony section is a rewrite, and refused;
/// the section's own session may.** The pair is the same patch from session `a`,
/// which wrote the section, and a patch from `b` that changes nothing in it.
#[tokio::test]
async fn another_sessions_edges_on_a_testimony_section_are_a_rewrite() {
    let (_store, section) = a_testimony_section().await;
    let patch_from = |session: &str| FactPatch {
        fields: [("edges".to_string(), ";;\\n\\n\\u{49};".to_string())].into(),
        session: Some(session.into()),
        ..FactPatch::default()
    };
    let refused = settle_rewrite(
        &section.address(),
        &section,
        patch_from("b"),
        Some("a"),
        &[],
    );
    assert!(
        matches!(refused, Err(MemoryError::TestimonyRewritten { .. })),
        "{refused:?}"
    );
    settle_rewrite(
        &section.address(),
        &section,
        patch_from("a"),
        Some("a"),
        &[],
    )
    .expect("the section's own session rewrites it");
}

/// **The seam between two sections is the engine's to set, and nothing else in
/// the edges is.** Another session's patch that changes only a lead of whitespace
/// moves no word and passes the gate; a lead that holds a character, and a change
/// to the tail, are rewrites like any other key. The first is what a put or a move
/// writes to join a heading to the section before it.
#[tokio::test]
async fn another_sessions_blank_lead_on_a_testimony_section_is_a_seam_and_passes() {
    let (_store, section) = a_testimony_section().await;
    let settled = |edges: &str| {
        settle_rewrite(
            &section.address(),
            &section,
            FactPatch {
                fields: [("edges".to_string(), edges.to_string())].into(),
                session: Some("b".into()),
                ..FactPatch::default()
            },
            Some("a"),
            &[],
        )
    };
    settled("\\n\\n;;\\n\\n;").expect("a lead of line feeds is the seam");
    for rewrite in ["\\u{49};;\\n\\n;", ";;\\n\\n;\\n"] {
        assert!(
            matches!(
                settled(rewrite),
                Err(MemoryError::TestimonyRewritten { .. })
            ),
            "{rewrite}: a lead with a character and a changed tail are rewrites"
        );
    }
}
