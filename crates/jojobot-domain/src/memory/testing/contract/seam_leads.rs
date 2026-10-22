//! **The seam between two sections is the one change to a testimony section's
//! edges that another session may make**, proven against any [`Memory`] adapter.
//! A lead of whitespace moves no word, so a put or a move may join a heading to
//! the section before it. A lead that holds a character, or any other run of the
//! edges, is a rewrite of what the section says.

use super::support::{add, other_caller};
use super::*;

/// **Another session's patch to the lead of a testimony section.** The section is
/// written by one session. Whitespace in its lead lands from another session and
/// reads back; a letter in the lead, and a changed tail, are refused with nothing
/// written.
pub async fn another_sessions_blank_lead_on_a_testimony_section_lands_and_nothing_else_does<
    M: Memory,
>(
    store: &M,
) {
    let document = EntityId("document:homer".into());
    add(
        store,
        NewEntity::new(document.clone(), "Fryer manual", "user-named"),
    )
    .await;
    let section = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            session: Some("session-a".into()),
            fields: [("edges".to_string(), ";;\\n\\n;".to_string())].into(),
            ..NewFact::about(document, "## Opening", date(2026, 10, 21))
        })
        .await
        .expect("the section is written")
        .written()
        .expect("nothing blocks it");
    let patch = |edges: &str| FactPatch {
        fields: [("edges".to_string(), edges.to_string())].into(),
        session: Some("session-b".into()),
        ..FactPatch::default()
    };

    for rewrite in [
        "\\u{49};;\\n\\n;",
        ";;\\n\\n;\\n",
        "\\s;;\\n\\n;",
        "\\t;;\\n\\n;",
    ] {
        let refused = store
            .update_fact(&section.address(), patch(rewrite), &other_caller())
            .await;
        assert!(
            matches!(refused, Err(MemoryError::TestimonyRewritten { .. })),
            "{rewrite}: a lead with a character and a changed tail are rewrites: {refused:?}"
        );
    }
    store
        .update_fact(
            &section.address(),
            patch("\\n\\n;;\\n\\n;"),
            &other_caller(),
        )
        .await
        .expect("a lead of line feeds is the seam")
        .written()
        .expect("nothing blocks it");
    let read = store
        .recall(&section.subject)
        .await
        .expect("the section reads");
    assert_eq!(
        read.iter()
            .find(|fact| fact.id == section.id)
            .and_then(|fact| fact.fields.get("edges").cloned())
            .as_deref(),
        Some("\\n\\n;;\\n\\n;"),
        "the seam reads back"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn the_fake_lets_another_session_set_the_seam_and_nothing_else() {
        another_sessions_blank_lead_on_a_testimony_section_lands_and_nothing_else_does(
            &InMemoryMemory::booted(),
        )
        .await;
    }
}
