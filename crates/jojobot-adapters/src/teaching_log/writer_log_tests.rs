//! The teaching ledger's read loop and its family routing are written once over
//! `LogStore`. These cases run the same bodies as `tests.rs` over the log the
//! writer owns, on a real Dolt, so both stores answer the same routing table.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use jojobot_domain::log::{
    Append, LogError, LogStore, Position, SealedBatch, ValidatedSnapshot, Validity,
};
use jojobot_domain::teaching::{TeachingError, Teachings};

use super::LoggedTeachings;
use super::tests::*;
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

macro_rules! on_the_writers_log {
    ($($name:ident => |$log:ident| $body:expr),+ $(,)?) => {$(
        #[tokio::test]
        async fn $name() {
            let scratch = Scratch::new("teaching-writer-log");
            let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
            let pool = migrated_database(&server, "alpha").await.unwrap();
            let writer = Writer::default();
            let $log: Arc<dyn LogStore> =
                Arc::new(WriterLog::open(pool, writer.clone()).await.unwrap());
            $body.await;
            drop($log);
            writer.drain().await;
            drop(writer);
            server.stop().await;
        }
    )+};
}

on_the_writers_log!(
    contacts_and_clears_append_only_what_changes_on_the_writers_log
        => |log| contacts_and_clears_on(log.clone()),
    the_domain_width_is_enforced_on_the_writers_log
        => |log| domain_width_on(log.clone()),
    a_restart_replays_to_the_same_ledger_on_the_writers_log
        => |log| restart_replay_on(log.clone()),
    a_teaching_member_after_a_foreign_one_is_applied_on_the_writers_log
        => |log| a_mixed_batch_is_applied(log.clone(), true),
    a_teaching_member_before_a_foreign_one_is_applied_on_the_writers_log
        => |log| a_mixed_batch_is_applied(log.clone(), false),
    a_member_of_another_family_leaves_the_replay_equal_on_the_writers_log
        => |log| another_family_leaves_the_replay_equal(log.clone()),
    an_unknown_kind_of_teachings_own_family_fails_the_replay_on_the_writers_log
        => |log| an_unknown_own_kind_fails_the_replay(log.clone()),
);

#[derive(Clone, Copy, Debug)]
enum BeforeAppend {
    MoveHead,
    DropTable,
}

/// The tail read succeeds before the fixture changes the real append's store.
struct ChangedAtAppend {
    log: WriterLog,
    pool: sqlx::MySqlPool,
    change: BeforeAppend,
    armed: AtomicBool,
}

#[async_trait::async_trait]
impl LogStore for ChangedAtAppend {
    async fn validity(&self) -> Result<Validity, LogError> {
        self.log.validity().await
    }

    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        if !self.armed.swap(false, Ordering::SeqCst) {
            return self.log.append(head, batch).await;
        }
        match self.change {
            BeforeAppend::MoveHead => {
                let other =
                    jojobot_domain::log::testing::contract::batch("beta", head.0, &[b"foreign"]);
                assert_eq!(
                    self.log.append(head, &other).await.unwrap(),
                    Append::Committed(other.range())
                );
            }
            BeforeAppend::DropTable => {
                sqlx::query("DROP TABLE event_log")
                    .execute(&self.pool)
                    .await
                    .unwrap();
            }
        }
        let answer = self.log.append(head, batch).await;
        match self.change {
            BeforeAppend::MoveHead => assert!(matches!(&answer, Err(LogError::Conflict))),
            BeforeAppend::DropTable => assert!(matches!(&answer, Err(LogError::Store(_)))),
        }
        answer
    }

    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.log.read_from(position, bound).await
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log.load_snapshot().await
    }

    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.log.store_snapshot(snapshot).await
    }
}

/// A moved head is a conflict; a missing table is a store failure. Both happen
/// at the real append after the teaching ledger's tail read, not in a double.
#[tokio::test]
async fn a_teaching_append_conflict_is_distinct_from_a_store_failure_on_real_dolt() {
    for change in [BeforeAppend::MoveHead, BeforeAppend::DropTable] {
        let scratch = Scratch::new("teaching-append-errors");
        let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
        let pool = migrated_database(&server, "alpha").await.unwrap();
        let writer = Writer::default();
        let log = Arc::new(ChangedAtAppend {
            log: WriterLog::open(pool.clone(), writer.clone()).await.unwrap(),
            pool,
            change,
            armed: AtomicBool::new(false),
        });
        let teachings = LoggedTeachings::open(log.clone(), &[])
            .await
            .unwrap()
            .with_writer(writer.clone());
        assert!(
            teachings
                .first_contact(&sid("homer"), "claims", at(0))
                .await
                .unwrap(),
            "the teaching append works before {change:?}"
        );
        log.armed.store(true, Ordering::SeqCst);
        let answer = teachings
            .first_contact(&sid("marge"), "claims", at(1))
            .await;
        match change {
            BeforeAppend::MoveHead => {
                assert!(
                    matches!(&answer, Err(TeachingError::Conflict)),
                    "{answer:?}"
                );
            }
            BeforeAppend::DropTable => {
                assert!(
                    matches!(&answer, Err(TeachingError::Store(_))),
                    "{answer:?}"
                );
            }
        }
        assert!(
            !teachings
                .has_contact(&sid("marge"), "claims")
                .await
                .unwrap()
        );
        drop(teachings);
        drop(log);
        writer.drain().await;
        drop(writer);
        server.stop().await;
    }
}
