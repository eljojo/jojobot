//! The claims family's read loop and its family routing are written once over
//! `LogStore`. These cases run the same bodies as `tests.rs` over the log the
//! writer owns, on a real Dolt, so both stores answer the same routing table and
//! a restart replays from the store.

use std::sync::Arc;

use jojobot_domain::log::LogStore;

use super::tests::*;
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

macro_rules! on_the_writers_log {
    ($($name:ident => |$log:ident| $body:expr),+ $(,)?) => {$(
        #[tokio::test]
        async fn $name() {
            let scratch = Scratch::new("claims-writer-log");
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
    a_restart_replays_to_the_same_state_on_the_writers_log
        => |log| restart_replay_on(log.clone()),
    a_member_of_another_family_leaves_the_replay_equal_on_the_writers_log
        => |log| another_family_leaves_the_replay_equal(log.clone()),
    an_unknown_kind_of_the_claims_own_family_fails_the_replay_on_the_writers_log
        => |log| an_unknown_own_kind_fails_the_replay(log.clone()),
);
