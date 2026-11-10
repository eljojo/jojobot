//! **A batch's receipt carries every note its items earned** (rules 130, 598).
//!
//! The items of a batch are written by the one path a single entry is written
//! by, so a note that path can put on a receipt is a note the batch carries.
//!
//! **No served-path case sends a note through the fold of unlisted keys today.**
//! The one that did, an unreadable instance record that left `due_on` unworked,
//! cannot happen on a list since the unread words decide only a work item or a
//! project at a word of the instance's own. The case below guards the other side:
//! the keys a plain entry's receipt holds are listed as the item's own, so a note
//! that a later change adds to a receipt reddens it and is either listed or
//! carried. `possible_subject_note` has its own batch cases and does not use this
//! fold.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// **What a batch does not carry is what only the item has.** A plain entry's
/// receipt holds nothing but the item's own keys, so a key a later change adds to
/// the receipt of a record reddens here and is listed as the item's own, or the
/// batch carries it as a note.
#[tokio::test]
async fn a_plain_entrys_receipt_holds_only_the_keys_a_batch_leaves_to_the_item() {
    let jojobot = handler();
    ensure(&jojobot, "list:homer-chores").await;
    let mut checked = 0;
    for _ in 0..2 {
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("list:homer-chores", "mow the lawn")
            },
        )
        .await;
        if receipt["teaching"].is_null() {
            checked += 1;
            let strangers: Vec<&String> = receipt
                .as_object()
                .expect("a receipt is an object")
                .keys()
                .filter(|key| !ITEM_OWN.contains(&key.as_str()))
                .collect();
            assert!(strangers.is_empty(), "{strangers:?} in {receipt}");
        }
    }
    assert!(
        checked > 0,
        "every receipt carried a teaching, so none was read for strangers"
    );
}

/// A ledger in which another call of the session wins one possible-subject row
/// the batch writes, the one at the place given (counted from none): the row is
/// recorded as that call's, and this write answers that it was not the first.
pub(super) struct AnotherCallWinsTheFirstNote {
    inner: jojobot_domain::teaching::testing::InMemoryTeachings,
    wins: usize,
    seen: std::sync::atomic::AtomicUsize,
}

impl AnotherCallWinsTheFirstNote {
    pub(super) fn winning(wins: usize) -> Self {
        AnotherCallWinsTheFirstNote {
            inner: jojobot_domain::teaching::testing::InMemoryTeachings::new(),
            wins,
            seen: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl jojobot_domain::teaching::Teachings for AnotherCallWinsTheFirstNote {
    async fn first_contact(
        &self,
        sid: &jojobot_domain::session::Sid,
        domain: &str,
        at: jiff::Timestamp,
    ) -> Result<bool, jojobot_domain::teaching::TeachingError> {
        let first = self.inner.first_contact(sid, domain, at).await?;
        if first
            && domain.starts_with(super::super::possible_subject::NOTED_DOMAIN)
            && self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == self.wins
        {
            return Ok(false);
        }
        Ok(first)
    }

    async fn forget(
        &self,
        sid: &jojobot_domain::session::Sid,
    ) -> Result<(), jojobot_domain::teaching::TeachingError> {
        self.inner.forget(sid).await
    }

    async fn has_contact(
        &self,
        sid: &jojobot_domain::session::Sid,
        domain: &str,
    ) -> Result<bool, jojobot_domain::teaching::TeachingError> {
        self.inner.has_contact(sid, domain).await
    }

    async fn taught(
        &self,
    ) -> Result<
        Option<Vec<(jojobot_domain::session::Sid, String)>>,
        jojobot_domain::teaching::TeachingError,
    > {
        self.inner.taught().await
    }
}

/// **A batch that loses one row to another call of its session ships the note of
/// the rows no one spent.** Two entries name two people; another call spends the
/// first person's row between the batch's peek and its write. The note names the
/// second person alone and the batch spends nothing it did not ship.
#[tokio::test]
async fn a_batch_that_loses_one_note_row_ships_the_other_note() {
    let jojobot = crate::Jojobot::new(
        std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded()),
        std::sync::Arc::new(crate::memory::testing::SpySearch::default()),
        std::sync::Arc::new(
            jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner(),
        ),
        std::sync::Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        std::sync::Arc::new(AnotherCallWinsTheFirstNote::winning(0)),
        seeded_registry(),
    );
    ensure(&jojobot, "list:homer-chores").await;
    ensure(&jojobot, "person:marge").await;
    ensure(&jojobot, "person:lisa").await;
    // The session's first-contact teaching is outside the budget and crowds a
    // suggestion out of a receipt; an earlier capture says it.
    jojobot
        .capture(rmcp::handler::server::wrapper::Parameters(capture_args(
            "person:marge",
            "plays go",
        )))
        .await
        .expect("the first capture answers");

    let batch = json_of(
        &jojobot
            .capture(rmcp::handler::server::wrapper::Parameters(CaptureArgs {
                provenance: Some("testimony".into()),
                entries: Some(
                    ["call marge", "email lisa"]
                        .iter()
                        .map(|text| crate::memory::capture::EntryItem::Text(text.to_string()))
                        .collect(),
                ),
                ..capture_args("list:homer-chores", "")
            }))
            .await
            .expect("the batch answers"),
    );
    let note = batch["possible_subject_note"]
        .as_str()
        .unwrap_or_else(|| panic!("the free row's note ships: {batch}"));
    assert!(note.contains("person:lisa"), "{note}");
    assert!(!note.contains("person:marge"), "{note}");
}
