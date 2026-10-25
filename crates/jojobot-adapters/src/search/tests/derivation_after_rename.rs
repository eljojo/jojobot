//! The index re-reads the claims that cite a renamed or folded thing's claim.

use jojobot_domain::memory::FactAddress;

use super::*;

/// A thing and a claim on it, and a claim on another thing worked out from it
/// that carries a word only it holds.
async fn a_claim_citing_a_claim_of_a_pet(
    store: &IndexedMemory,
    cite: fn(FactAddress) -> FactPatch,
) -> (EntityId, FactAddress) {
    let pet = EntityId("pet:contract-quokka-kept".into());
    let holder = EntityId::person("person:contract-bellringer");
    for (id, name) in [(&pet, "Kept Pet"), (&holder, "Bell Ringer")] {
        store
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("adds")
            .written()
            .expect("lands");
    }
    let basis = store
        .capture(contract::state(NewFact::about(
            pet.clone(),
            "the quokka eats leaves",
            date(2026, 7, 1),
        )))
        .await
        .expect("captures")
        .written()
        .expect("lands");
    let citing = store
        .capture(contract::state(NewFact::about(
            holder,
            "zeppelin diet noted",
            date(2026, 7, 1),
        )))
        .await
        .expect("captures")
        .written()
        .expect("lands");
    store
        .update_fact(
            &citing.address(),
            cite(basis.address()),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("edits")
        .written()
        .expect("lands");
    (pet, basis.address())
}

/// The source of the citing claim, as the search serves it.
fn the_source_it_cites(hits: &[Hit], read: fn(&Fact) -> Option<FactAddress>) -> FactAddress {
    hits.iter()
        .find_map(|h| match h {
            Hit::Fact { fact, .. } => read(fact),
            _ => None,
        })
        .expect("the citing claim is found")
}

fn the_word() -> SearchQuery {
    SearchQuery {
        text: Some("zeppelin".into()),
        ..SearchQuery::default()
    }
}

/// **A claim citing a claim of a renamed thing reads its source under the new
/// handle.** The citing claim sits on another thing and spells the source's
/// handle only in its pointer, so the re-read has to find it by that pointer.
/// Paired: the same claim read before the rename names the old handle, so the
/// new handle is the rename's doing.
async fn a_rename_re_reads_a_claim_citing_the_renamed_thing(
    cite: fn(FactAddress) -> FactPatch,
    read: fn(&Fact) -> Option<FactAddress>,
) {
    let (_fake, store, retrieval) = an_index_over_the_fake().await;
    let (pet, basis) = a_claim_citing_a_claim_of_a_pet(&store, cite).await;
    let before = the_source_it_cites(&retrieval.search(&the_word()).await.expect("answers"), read);
    assert_eq!(before, basis, "the source reads under the handle it wore");

    let renamed = EntityId("pet:contract-wombat-renamed".into());
    store
        .rename_entity(
            &pet,
            &renamed,
            None,
            date(2026, 7, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename lands");

    let after = the_source_it_cites(&retrieval.search(&the_word()).await.expect("answers"), read);
    assert_eq!(
        after.home, renamed,
        "the citing claim still names the old handle of its source"
    );
}

#[tokio::test]
async fn a_rename_re_reads_the_claim_worked_out_from_a_claim_of_the_renamed_thing() {
    a_rename_re_reads_a_claim_citing_the_renamed_thing(
        |source| FactPatch {
            derived_from: Some(source),
            ..FactPatch::default()
        },
        |fact| fact.derived_from.clone(),
    )
    .await;
}

#[tokio::test]
async fn a_rename_re_reads_the_claim_standing_for_a_claim_of_the_renamed_thing() {
    a_rename_re_reads_a_claim_citing_the_renamed_thing(
        |source| FactPatch {
            stands_for: Some(vec![source]),
            ..FactPatch::default()
        },
        |fact| fact.stands_for.first().cloned(),
    )
    .await;
}
