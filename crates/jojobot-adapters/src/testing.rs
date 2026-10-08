//! **Test support for the suites that spawn a real store.**
//!
//! It is a module of the library rather than a helper beside one suite because
//! the thing it hands out — a port — is shared by every process on the
//! machine, and a helper copied into each suite is a rule each copy can drift
//! from. That drift is what this file exists to end: the same defect was
//! repaired three times in three copies, and the copy nobody repaired is the
//! one two runs meet on.
//!
//! **The ports come from `jojobot-ports`**, the one arbiter the rooms draw from
//! as well. This file once kept a scheme of its own over the same range as the
//! rooms', and a room and a store test were then offered one port at the same
//! moment.

/// **A port no other claimer anywhere will be given**, held for the rest of
/// this process and with nothing bound to it: the caller's server binds it
/// next. The claim lasts as long as the process, so however many tests it runs
/// none is offered the same port twice.
///
/// A range with no port left is a test that cannot start, and this says so in
/// the arbiter's own words. It returns a bare port because every caller wants
/// one and has nowhere to put an error.
///
/// **It also installs the store log sink** ([`install_store_log_sink`]): every
/// test that starts a store claims a port first, so this is the one call they
/// all make, and none has to remember a second.
pub fn free_port() -> u16 {
    install_store_log_sink();
    jojobot_ports::claim_for_life()
        .unwrap_or_else(|refusal| panic!("no port can be claimed for this test: {refusal}"))
}

/// **How long a case that is not about connection speed lets a pool take to
/// hand out a connection while its store starts.** As long as a store is given
/// to answer at all.
const UNHURRIED: std::time::Duration = std::time::Duration::from_secs(30);

/// **Start a store for a case that is about something other than how fast a pool
/// opens a connection.** [`Dolt::start`] gives the pool a probe's window, a fifth
/// of a second, and the proof query that follows runs on that pool. A loaded
/// machine can exceed it without anything being wrong with the case, and the
/// start then fails with a pool timeout. This is the same start with the window
/// a store has to answer in.
pub async fn start_unhurried(
    data_dir: &std::path::Path,
    port: u16,
) -> Result<crate::dolt::Dolt, crate::dolt::StartError> {
    crate::dolt::Dolt::start_acquiring_within(data_dir, port, UNHURRIED).await
}

/// **Install the log sink a test that starts a store reads its refusals from.**
/// Idempotent, and the process's one global subscriber: a second call, from any
/// thread, finds the first one's. `free_port` calls it, because a test that
/// claims a port is about to start a store.
pub fn install_store_log_sink() {
    let _ = crate::log_capture::log_sink();
}

/// **Everything the sink has kept so far**, process-wide, for a test that asserts
/// on what the store's refusal logged.
pub fn store_log() -> String {
    crate::log_capture::log_sink().text()
}

/// **A database of its own on this store, already migrated.**
///
/// The same database `store.database(name)` followed by `migrate::run` leaves,
/// restored from a template instead of migrated from nothing. A fresh
/// migration of the whole chain costs most of a second, and a suite that
/// starts a store per case pays it per case; a restore costs a few hundredths.
/// **It is for a case that wants the finished schema.** A case that is about a
/// migration, or that needs an empty database to start from, keeps
/// `store.database`.
///
/// The kinds are not seeded: a case seeds them or does not, exactly as it did
/// after a fresh migration. A `migrate::run` after this applies nothing, and
/// stays worth keeping as a live check that the template is whole.
pub async fn migrated_database(
    store: &crate::dolt::Dolt,
    name: &str,
) -> Result<sqlx::MySqlPool, crate::dolt::StartError> {
    static TEMPLATE: tokio::sync::OnceCell<std::path::PathBuf> = tokio::sync::OnceCell::const_new();
    let backup = TEMPLATE.get_or_try_init(build_template).await?;
    sqlx::query(&format!(
        "CALL DOLT_BACKUP('restore', 'file://{}', '{name}')",
        backup.display()
    ))
    .execute(store.pool())
    .await
    .map_err(|e| crate::dolt::StartError::Spawn(e.to_string()))?;
    store.database(name).await
}

/// **The directory that holds this process's template, and no other
/// process's.** Named for the process, so two runs on one machine never share
/// one and a migration added between runs cannot meet a stale template.
const TEMPLATE_DIR_PREFIX: &str = "jojobot-store-template-";

/// Build the template once for this process: migrate one database on a store of
/// its own and back it up to a directory a running server can restore from.
///
/// **A backup and not a copy of the data directory.** A running server cannot
/// clone a live database directory, because it holds a chunk journal, and a
/// directory copy is only seen by a server that starts after it. A backup
/// restores into a server that is already up.
async fn build_template() -> Result<std::path::PathBuf, crate::dolt::StartError> {
    use crate::dolt::StartError;
    let root = std::env::temp_dir().join(format!("{TEMPLATE_DIR_PREFIX}{}", std::process::id()));
    sweep_dead_templates();
    let backup = root.join("backup");
    let building = root.join("building");
    std::fs::create_dir_all(&building).map_err(|e| StartError::DataDir {
        path: building.clone(),
        why: e.to_string(),
    })?;
    let mut store = start_unhurried(&building, free_port()).await?;
    let pool = store.database("template").await?;
    crate::dolt::migrate::run(&pool)
        .await
        .map_err(|e| StartError::Spawn(e.to_string()))?;
    sqlx::query(&format!(
        "CALL DOLT_BACKUP('sync-url', 'file://{}')",
        backup.display()
    ))
    .execute(&pool)
    .await
    .map_err(|e| StartError::Spawn(e.to_string()))?;
    pool.close().await;
    store.stop().await;
    let _ = std::fs::remove_dir_all(&building);
    Ok(backup)
}

/// Remove the templates of processes that are gone. A process cannot clean up
/// after a kill, so the next one to build a template does it.
fn sweep_dead_templates() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_string_lossy()
            .strip_prefix(TEMPLATE_DIR_PREFIX)
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::Scratch;

    /// **A port the suites are handed is claimed in the arbiter every harness
    /// shares**, so a room can never be offered it. The claim is read through
    /// the arbiter's own public door and not through anything this file
    /// holds: a port from a private scheme would come back claimable.
    #[test]
    fn a_free_port_is_held_in_the_arbiter_the_rooms_draw_from() {
        let port = free_port();
        assert!(
            jojobot_ports::try_claim(port)
                .expect("the claim can be tried")
                .is_none(),
            "a port from free_port() was free for another harness to claim",
        );
    }

    /// **Two stores start at the same moment, each on a port of its own, and
    /// both come up.**
    ///
    /// The real thing rather than a model of it: two servers, started
    /// concurrently on two claimed ports, each binding its own for real. This
    /// is the failure as it arrives in a run — `PortTaken`, on a suite that
    /// changed nothing — and it is the one a helper that only narrows the
    /// window still produces.
    ///
    /// **How fast a pool hands out a connection is not what this asserts**, so
    /// the case gives each start a window as long as the one a store has to
    /// answer in. The window `start` itself passes is a probe's, a fifth of a
    /// second, and a machine running several bars at once can take longer than
    /// that to open a connection without anything being wrong with the ports.
    #[tokio::test]
    async fn two_stores_started_together_both_take_a_port() {
        let scratch = Scratch::new("port-block-together");
        let here_port = free_port();
        let there_port = free_port();
        assert_ne!(
            here_port, there_port,
            "the arbiter handed one port to two claimers"
        );

        let here = scratch.0.join("one");
        let there = scratch.0.join("other");
        let (first, second) = tokio::join!(
            start_unhurried(&here, here_port),
            start_unhurried(&there, there_port),
        );

        let mut first = first.expect("the first store comes up");
        let mut second = second.expect("the second store comes up, on a port of its own");
        first.stop().await;
        second.stop().await;
    }

    /// **A process can hold a whole run's cumulative total, not just a handful
    /// of concurrent servers.**
    ///
    /// `free_port` claims a port for the life of the process, and every
    /// `#[tokio::test]` of a suite that spawns its own store draws one. The
    /// busiest lib draws about a hundred in one run, and the busiest room suite
    /// about two hundred over the rooms' own use of the same arbiter. A hundred
    /// distinct ports from one process is the figure this pins: a draw that
    /// repeated a port, or ran out early, would fail a test that changed
    /// nothing.
    #[test]
    fn a_process_is_handed_a_hundred_distinct_ports_in_one_run() {
        let handed: Vec<u16> = (0..100).map(|_| free_port()).collect();

        let mut unique = handed.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            100,
            "a hundred calls must hand out a hundred distinct ports: {handed:?}",
        );
    }

    /// **How many rows each table of the current database holds**, by table.
    async fn row_counts(pool: &sqlx::MySqlPool) -> Vec<(String, i64)> {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = DATABASE() ORDER BY table_name",
        )
        .fetch_all(pool)
        .await
        .expect("tables readable");
        let mut counts = Vec::new();
        for table in tables {
            let (n,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM `{table}`"))
                .fetch_one(pool)
                .await
                .expect("rows countable");
            counts.push((table, n));
        }
        counts
    }

    /// 🚨 **The assertion a templated database rests on.** A database restored
    /// from the template must read the same as one migrated fresh: the same
    /// schema (every table, column, type, nullability and index), the same
    /// migration ledger, nothing left for `migrate::run` to apply, and, once
    /// the kinds are seeded in both, the same rows in every table.
    ///
    /// **Both halves.** The fresh database is the oracle the templated one is
    /// held to, and the ledger is read as a list that must be non-empty, so a
    /// comparison of two empty databases cannot pass. A template that lacks
    /// the last migration fails the schema, the ledger and the no-op run at
    /// once.
    #[tokio::test]
    async fn a_templated_database_reads_the_same_as_a_freshly_migrated_one() {
        use crate::dolt::memory::DoltMemory;
        use crate::dolt::migrate;
        use crate::dolt::migrate::tests::schema_fingerprint;

        let scratch = Scratch::new("templated-canary");
        let mut store = crate::dolt::Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let fresh = store
            .database("fresh")
            .await
            .expect("a database of its own");
        migrate::run(&fresh).await.expect("the schema");
        let templated = migrated_database(&store, "templated")
            .await
            .expect("a templated database");

        assert_eq!(
            schema_fingerprint(&fresh).await,
            schema_fingerprint(&templated).await,
            "a templated database must carry the schema a fresh migration builds"
        );

        let ledger = |pool: sqlx::MySqlPool| async move {
            sqlx::query_scalar::<_, String>("SELECT version FROM schema_migration ORDER BY version")
                .fetch_all(&pool)
                .await
                .expect("the ledger is readable")
        };
        let fresh_ledger = ledger(fresh.clone()).await;
        assert!(
            fresh_ledger.len() > 50,
            "the oracle must have applied the whole chain: {}",
            fresh_ledger.len()
        );
        assert_eq!(
            fresh_ledger,
            ledger(templated.clone()).await,
            "a templated database must carry the ledger a fresh migration writes"
        );
        assert_eq!(
            migrate::run(&templated).await.expect("the schema"),
            Vec::<String>::new(),
            "nothing is left to apply on a templated database"
        );

        for pool in [&fresh, &templated] {
            jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
                .await
                .expect("the kinds are seeded");
        }
        let seeded = row_counts(&fresh).await;
        assert!(
            seeded.iter().any(|(_, n)| *n > 0),
            "seeding the kinds must leave rows, or this compares empty tables: {seeded:?}"
        );
        assert_eq!(
            seeded,
            row_counts(&templated).await,
            "a seeded templated database must hold the rows a seeded fresh one holds"
        );
        store.stop().await;
    }
}
