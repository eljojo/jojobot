mod writer_log_contract {
    // The complete-batch contract's cases that ask nothing of a fault harness,
    // run on the log owned by the one writer.
    use jojobot_adapters::{
        dolt::Dolt,
        testing::{Scratch, free_port, migrated_database},
        writer::Writer,
        writer_log::WriterLog,
    };
    use jojobot_domain::log::testing::contract;
    macro_rules! case {
    ($($name:ident),+ $(,)?) => {$(
        #[tokio::test]
        async fn $name() {
            let scratch = Scratch::new("writer-log-contract");
            let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
            let pool = migrated_database(&server, "alpha").await.unwrap();
            let writer = Writer::default();
            let log = WriterLog::open(pool, writer.clone()).await.unwrap();
            contract::$name(&log).await;
            drop(log);
            writer.drain().await;
            drop(writer);
            server.stop().await;
        }
    )+};
}
    case!(
        ordered_complete_batches,
        retries_keep_identity_and_bytes,
        stale_heads_never_append,
        a_head_ahead_of_storage_never_appends,
        snapshot_blob_round_trips,
        an_events_actor_reads_back_as_it_was_appended
    );
}
