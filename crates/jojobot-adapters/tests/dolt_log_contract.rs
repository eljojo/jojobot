mod dolt_log_contract {
    // The same complete-batch contract on the pinned native store.
    use jojobot_adapters::{
        dolt::Dolt,
        dolt_log::DoltLog,
        testing::{Scratch, free_port, migrated_database},
    };
    use jojobot_domain::log::testing::contract;
    macro_rules! case {
    ($($name:ident),+ $(,)?) => {$(
        #[tokio::test]
        async fn $name() {
            let scratch = Scratch::new("log-contract");
            let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
            let pool = migrated_database(&server, "alpha").await.unwrap();
            let log = DoltLog::open(pool).await.unwrap();
            contract::$name(&log).await;
            drop(log);
            server.stop().await;
        }
    )+};
}
    case!(
        ordered_complete_batches,
        retries_keep_identity_and_bytes,
        stale_heads_never_append,
        partial_batch_is_invisible,
        lost_acknowledgement_is_recovered_once,
        absence_without_a_terminal_fence_is_uncertain,
        a_fenced_abort_leaves_the_same_head,
        restart_excludes_a_late_predecessor,
        cancelling_an_acknowledgement_does_not_cancel_the_commit,
        snapshot_blob_round_trips
    );
}
