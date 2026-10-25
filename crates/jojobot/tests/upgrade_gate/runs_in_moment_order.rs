//! Stored-record readback on the last deployed release's CI-built store.
use super::*;

/// **The runs of the last deployed store read in moment order on the
/// head, and no stored stamp is rewritten** (rule 342, rule 598). The head parses
/// each stamp it reads and orders by the moment, where the build that wrote the
/// rows compared their text. The expected order is worked out here, apart from the
/// store, from the stamps the recording holds; every newest-first read of the head
/// answers it, every summary's last beat is the one its run reads, and the stamps
/// are as the recording left them.
#[tokio::test]
async fn the_runs_of_the_last_deployed_store_read_in_moment_order_on_the_head() {
    use jojobot_adapters::dolt::sessions::DoltSessions;
    use jojobot_domain::session::Sessions;

    let Restored {
        scratch,
        state_dir,
        store_port,
        ..
    } = restore_a_fixture(&fixture_dir(), "runs-in-moment-order").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool().clone();
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let stamps = || async {
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, bot, started_at FROM session ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("the stamps read")
    };
    // **Runs the old stamp would write inside one second**, beside what the
    // recording holds: no digits, half a second, and a tenth and a hundredth
    // that sort as text the other way round. Their text order is not their time
    // order, so a read that orders by text cannot pass.
    let owner: String = sqlx::query_scalar("SELECT bot FROM session ORDER BY id LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("the recording holds a run");
    for (id, at) in [
        ("zwhole", "2031-01-01T00:00:05Z"),
        ("zhalf", "2031-01-01T00:00:05.5Z"),
        ("ztenth", "2031-01-01T00:00:05.1Z"),
        ("zhundredth", "2031-01-01T00:00:05.12Z"),
    ] {
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state)
             VALUES (?, NULL, ?, 'a run the old stamp wrote', ?, 'active')",
        )
        .bind(id)
        .bind(&owner)
        .bind(at)
        .execute(&pool)
        .await
        .expect("the run is added");
    }
    let recorded = stamps().await;
    assert!(
        recorded.len() > 5,
        "the recording holds runs to put in order"
    );
    let newest_first = |bot: Option<&str>| -> Vec<String> {
        let mut held: Vec<(jiff::Timestamp, String)> = recorded
            .iter()
            .filter(|(_, owner, _)| bot.is_none_or(|bot| owner == bot))
            .map(|(id, _, at)| (at.parse().expect("a recorded stamp"), id.clone()))
            .collect();
        held.sort_by(|a, b| b.cmp(a));
        held.into_iter().map(|(_, id)| id).collect()
    };

    let sessions = DoltSessions::open(pool.clone());
    let ids = |runs: Vec<jojobot_domain::session::Session>| -> Vec<String> {
        runs.into_iter().map(|run| run.id.0).collect()
    };
    assert_eq!(
        ids(sessions.all_sessions().await.expect("reads")),
        newest_first(None)
    );
    let every = sessions.all_summaries().await.expect("reads");
    assert_eq!(
        every.iter().map(|run| run.id.0.clone()).collect::<Vec<_>>(),
        newest_first(None)
    );
    let mut bots: Vec<&str> = recorded.iter().map(|(_, bot, _)| bot.as_str()).collect();
    bots.sort();
    bots.dedup();
    for bot in bots {
        let owner = jojobot_domain::memory::EntityId(bot.to_string());
        assert_eq!(
            ids(sessions.sessions_of(&owner).await.expect("reads")),
            newest_first(Some(bot)),
            "{bot}: sessions_of"
        );
        let summarised = sessions.summaries_of(&owner).await.expect("reads");
        assert_eq!(
            summarised
                .iter()
                .map(|run| run.id.0.clone())
                .collect::<Vec<_>>(),
            newest_first(Some(bot)),
            "{bot}: summaries_of"
        );
    }
    for summary in &every {
        let run = sessions.read_session(&summary.id).await.expect("a run");
        assert_eq!(
            summary.last_beat,
            run.last_beat(),
            "{}: the summary's last beat is the run's",
            summary.id.0
        );
        assert_eq!(summary.entry_count, run.entries.len());
    }
    assert_eq!(stamps().await, recorded, "no stored stamp was rewritten");
    store.stop().await;
    drop(scratch);
}
