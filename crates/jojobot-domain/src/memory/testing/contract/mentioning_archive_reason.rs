use super::support::{add, capture, edit, ensure};
use super::*;
use crate::memory::ARCHIVE_REASON;

/// 🚨 **The reason an archive records is served with its mentions as handles.**
///
/// An archive sends its reason as the claim's details. The mention layer stores
/// those words with each mention as the badge it wears, and the store then keeps
/// them in the record's own `archive_reason`. That field is text a reader is
/// served, so it is rendered like the claim's words: the receipt of the archive
/// and a later read both show the handle. A badge in either is a link served
/// bare, which a reader cannot follow.
///
/// **Both halves**: the store keeps the badge and not the handle, and both
/// reads serve the handle and not the badge.
pub async fn the_reason_an_archive_records_is_served_with_its_mentions_as_handles<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let author = EntityId::person("person:contract-archive-reasoner");
    ensure(mentioning, &author).await;
    let thread = EntityId("thread:contract-archive-reason".into());
    add(
        mentioning,
        NewEntity::new(thread.clone(), "Archive reason thread", "contract-fixture"),
    )
    .await;
    let claim = capture(
        mentioning,
        NewFact::about(author.clone(), "a claim to archive", date(2026, 4, 18)),
    )
    .await;

    let archived = edit(
        mentioning,
        &claim.address(),
        FactPatch {
            status: Some(FactStatus::Archived),
            details: Some(format!("moved to @{thread}")),
            ..Default::default()
        },
    )
    .await;
    let served = archived
        .fields
        .get(ARCHIVE_REASON)
        .cloned()
        .unwrap_or_default();
    assert!(
        served.contains(thread.as_str()) && !served.contains(mention::MARK),
        "the receipt of the archive serves {served:?}, not the handle"
    );

    let later = mentioning
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the claim is there");
    let later = later
        .fields
        .get(ARCHIVE_REASON)
        .cloned()
        .unwrap_or_default();
    assert!(
        later.contains(thread.as_str()) && !later.contains(mention::MARK),
        "a later read serves {later:?}, not the handle"
    );

    let kept = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the claim is there")
        .fields
        .get(ARCHIVE_REASON)
        .cloned()
        .unwrap_or_default();
    assert!(
        kept.contains(mention::MARK) && !kept.contains(thread.as_str()),
        "the store keeps {kept:?}: a link is a badge there, never the handle"
    );
}
