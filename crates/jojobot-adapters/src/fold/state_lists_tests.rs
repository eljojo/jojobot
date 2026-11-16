//! **The fold answers the listing, the rename history and the merge events from the
//! claims State, and a rename or a merge reads none of them again.** The store
//! beneath is counted by method name, so a case names the read that reached it.

use super::*;
use crate::counted::Counted;
use jiff::civil::date;
use jojobot_domain::memory::Override;
use jojobot_domain::memory::testing::InMemoryMemory;

fn person(slug: &str) -> EntityId {
    EntityId::person(format!("person:{slug}"))
}

/// A folded store over a counted fake, with the fake lending its claims State.
async fn folded_over_a_counted_fake() -> (Folded, Arc<Counted>) {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("an empty store fills");
    (folded, counted)
}

async fn add(folded: &Folded, id: &EntityId) {
    folded
        .add_entity(NewEntity::new(id.clone(), &id.0, "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
}

async fn rename(folded: &Folded, from: &EntityId, to: &EntityId) {
    folded
        .rename_entity(from, to, None, date(2026, 9, 2), Override::none(), None)
        .await
        .expect("renamed")
        .written()
        .expect("not blocked");
}

async fn merge(folded: &Folded, folded_in: &EntityId, survivor: &EntityId) {
    folded
        .merge(
            folded_in,
            survivor,
            Some("filed twice"),
            date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merged");
}

/// **A rename and a merge reach the next listing with no read of the store's
/// lists.** The claims State holds both writes, so the listing, the rename history
/// and the merge events are read from it: a fold that read the three lists again
/// after each write would count them here.
#[tokio::test]
async fn a_rename_and_a_merge_reach_the_next_listing_with_no_read_of_the_stores_lists() {
    let (folded, counted) = folded_over_a_counted_fake().await;
    let (alpha, homer, marge, lisa) = (
        person("alpha"),
        person("homer"),
        person("marge"),
        person("lisa"),
    );
    for id in [&alpha, &homer, &marge] {
        add(&folded, id).await;
    }
    counted.reset();
    rename(&folded, &alpha, &lisa).await;
    merge(&folded, &homer, &marge).await;

    let listed = folded.list_entities(None).await.expect("lists");
    let ids: Vec<&EntityId> = listed.iter().map(|row| &row.id).collect();
    assert!(ids.contains(&&lisa), "the renamed thing is listed: {ids:?}");
    assert!(!ids.contains(&&alpha), "the old handle is not: {ids:?}");
    let folded_row = listed.iter().find(|row| row.id == homer).expect("kept");
    assert_eq!(folded_row.merged_into.as_ref(), Some(&marge));
    let former = folded.former_handles().await.expect("history");
    assert!(
        former.iter().any(|f| f.former == alpha),
        "the rename history names the old handle: {former:?}"
    );

    let seen = counted.seen();
    for read in ["list_entities", "former_handles", "merge_events"] {
        assert_eq!(seen.get(read), None, "{read}: {seen:?}");
    }
}

/// **An address under a handle a thing wore, and under a thing merged away, is
/// followed to where the claim is now.** The thing is renamed and then merged into
/// another, and the claim is asked for by the first handle it wore: the rename
/// history finds the thing, and the merge events find where its claims went.
///
/// This case guards the State read that replaced the image's. It also passes on the
/// image, so it is not a red-first case.
#[tokio::test]
async fn an_address_under_a_former_handle_follows_the_thing_through_a_merge() {
    let (folded, _) = folded_over_a_counted_fake().await;
    let (alpha, renamed, survivor) = (person("alpha"), person("lisa"), person("marge"));
    add(&folded, &alpha).await;
    add(&folded, &survivor).await;
    let claim = folded
        .capture(NewFact::about(
            alpha.clone(),
            "plays chess",
            date(2026, 9, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    rename(&folded, &alpha, &renamed).await;
    merge(&folded, &renamed, &survivor).await;

    let asked = FactAddress::new(alpha.clone(), claim.id.clone());
    let moved = folded
        .moved_address(&asked)
        .await
        .expect("followed")
        .expect("the claim was moved by the merge");
    assert_eq!(moved.home, survivor, "the claim is on the survivor now");
    let served = folded.recall(&survivor).await.expect("recall ok");
    assert!(
        served.iter().any(|fact| fact.address() == moved),
        "the moved address names a claim the survivor serves: {served:?}"
    );
}

#[tokio::test]
async fn a_numbered_rename_refreshes_the_members_reference_holders() {
    let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
    folded.rebuild().await.expect("the fold fills");
    jojobot_domain::memory::testing::contract::typed_kinds::b5_numbered_rename(&folded).await;
}

/// A word of letters that depends on nothing but `n`, spread far enough
/// apart that the write guard's near-miss screen takes no two for one.
fn word(n: usize) -> String {
    let mut n = n * 7919 + 104_729;
    let mut letters = String::new();
    for _ in 0..8 {
        letters.push((b'a' + (n % 26) as u8) as char);
        n = n / 26 * 31 + 17;
    }
    letters
}

fn thing(n: usize) -> EntityId {
    EntityId::new(EntityKind::THING, word(n))
}

/// A fold over a store of `unrelated` things nothing points at, and the two
/// persons a case merges or renames. Each holds a claim.
async fn fold_of(unrelated: usize) -> (Arc<Counted>, Folded) {
    let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold fills");
    for id in [
        EntityId::person("person:alpha"),
        EntityId::person("person:beta"),
    ] {
        add(&folded, &id).await;
        folded
            .capture(NewFact::about(id.clone(), "a claim", date(2026, 9, 1)))
            .await
            .expect("a claim lands")
            .written()
            .expect("not blocked");
    }
    for n in 0..unrelated {
        folded
            .add_entity(NewEntity::new(thing(n), word(n + 1000), "user-named"))
            .await
            .expect("an entity is added")
            .written()
            .expect("not blocked");
    }
    (counted, folded)
}

/// **The reads a merge and a rename make do not grow with the things they do not
/// touch.** The same write over a store twice as large asks the store for as many
/// reads, and scans no document.
#[tokio::test]
async fn a_merge_and_a_rename_read_the_store_as_much_over_a_large_store_as_over_a_small_one() {
    for write in ["merge", "rename"] {
        let mut reads = Vec::new();
        for unrelated in [20, 40] {
            let (counted, folded) = fold_of(unrelated).await;
            counted.reset();
            if write == "merge" {
                merge(
                    &folded,
                    &EntityId::person("person:beta"),
                    &EntityId::person("person:alpha"),
                )
                .await;
            } else {
                rename(
                    &folded,
                    &EntityId::person("person:beta"),
                    &EntityId::person("person:gamma"),
                )
                .await;
            }
            reads.push((unrelated, counted.calls(), counted.seen()));
        }
        assert_eq!(
            reads[0].1, reads[1].1,
            "the {write}'s reads grew with the store: {reads:?}"
        );
        for (unrelated, _, seen) in &reads {
            assert_eq!(
                seen.get("scan"),
                None,
                "a {write} over {unrelated} things scanned every document: {seen:?}"
            );
        }
    }
}
