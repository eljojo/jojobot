//! **The companion claim of an edit, written alone for a record the build
//! supplies**, proven against any [`Memory`] adapter that knows the record.

use super::*;

/// The settings claim an edit of the shipped view writes: it wears the engine's
/// own label.
fn settings_claim() -> NewFact {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    NewFact {
        sets: ["note_9".to_string()].into(),
        fields: crate::memory::review::settings_fields(
            [("note_9".to_string(), "kept on top".to_string())].into(),
        ),
        ..NewFact::about(
            shipped,
            "Set note_9 on the shipped view.",
            date(2026, 11, 9),
        )
    }
}

/// **The claim is written inside a verb's job and read back, and no row
/// appears.** The store takes the claim an edit would write, chooses no entity,
/// and the record stays supplied: the claim reaches the thing through its keys,
/// and the entity list holds no row for the handle. The job is what makes the
/// case about the route: with none, a public capture takes the same tail and
/// would pass every assertion here.
pub async fn a_companion_claim_for_a_supplied_record_is_written_and_no_row_appears<M: Memory>(
    store: &M,
) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let written = crate::log::claims::job::within(
        crate::log::claims::job::test_job(&crate::actor::Actor::Operator),
        store.capture_companion(settings_claim()),
    )
    .await
    .expect("the companion claim of a supplied record is a write the gate allows")
    .written()
    .expect("nothing blocks it");

    let recalled = store.recall(&shipped).await.expect("recall answers");
    assert!(
        recalled.iter().any(|f| f.id == written.id),
        "the claim that was just written is not in the read: {recalled:?}",
    );
    assert_eq!(
        store
            .fields(&shipped)
            .await
            .expect("the keys read")
            .get("note_9"),
        Some(&"kept on top".to_string()),
        "the key the claim sets is not in what the thing holds",
    );
    let rows = store.list_entities(None).await.expect("the entities list");
    assert!(
        rows.iter().all(|row| row.id != shipped),
        "the companion claim gave the supplied record a row: {rows:?}",
    );
}

/// **A public capture of the engine's label is refused in a verb's job.** The
/// other half of the pair: the companion entry is the one route that writes that
/// label, and the capture a caller reaches is not. It needs a store whose public
/// capture reaches the claims decider.
pub async fn a_public_capture_of_the_engine_label_is_refused_in_a_verbs_job<M: Memory>(store: &M) {
    let refused = crate::log::claims::job::within(
        crate::log::claims::job::test_job(&crate::actor::Actor::Operator),
        store.capture(settings_claim()),
    )
    .await;
    assert!(
        matches!(&refused, Err(MemoryError::InvalidFact(words)) if words.contains("settings_record")),
        "a public capture of the engine's label was not refused: {refused:?}",
    );
}
