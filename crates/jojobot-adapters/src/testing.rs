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

/// **A scratch path that no other call, in this process or in any other, is
/// given.** `name` says what it is for; the pid and a number drawn from a counter
/// make the path this call's own, so two tests that ask for the same name get two
/// directories. **A path named for the process alone is shared by every test in
/// it**, and two real stores on one data directory fail only when both run, which
/// reads as load. The path is not created: the caller makes it and removes it.
pub fn scratch_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(jojobot_ports::scratch::name(name))
}

/// A directory of this run's own, removed when it is done. Named for the case
/// that asks for it, through [`scratch_path`].
pub struct Scratch(pub std::path::PathBuf);

impl Scratch {
    pub fn new(what: &str) -> Self {
        let path = scratch_path(&format!("jojobot-contract-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CALL DOLT_BACKUP('restore', 'file://{}', '{name}')",
        backup.display()
    )))
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
    let root = std::env::temp_dir().join(format!("{TEMPLATE_DIR_PREFIX}{}", std::process::id()));
    sweep_dead_templates();
    match build_template_in(&root).await {
        Ok(backup) => {
            remove_template_at_exit(root);
            Ok(backup)
        }
        // A build that failed leaves a half-made directory nobody will use.
        Err(e) => {
            let _ = std::fs::remove_dir_all(&root);
            Err(e)
        }
    }
}

async fn build_template_in(
    root: &std::path::Path,
) -> Result<std::path::PathBuf, crate::dolt::StartError> {
    use crate::dolt::StartError;
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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CALL DOLT_BACKUP('sync-url', 'file://{}')",
        backup.display()
    )))
    .execute(&pool)
    .await
    .map_err(|e| StartError::Spawn(e.to_string()))?;
    pool.close().await;
    store.stop().await;
    let _ = std::fs::remove_dir_all(&building);
    Ok(backup)
}

/// **The template lives as long as this process does, and goes when it exits.**
/// Every case restores from it, so no case can own it, and a static is never
/// dropped; the hook below runs on the way out of a normal end and of a failed
/// run alike. A kill does not reach it, which is what the sweep is for.
fn remove_template_at_exit(root: std::path::PathBuf) {
    static ROOT: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
    extern "C" fn at_exit() {
        if let Ok(mut root) = ROOT.lock()
            && let Some(root) = root.take()
        {
            let _ = std::fs::remove_dir_all(root);
        }
    }
    if let Ok(mut held) = ROOT.lock() {
        *held = Some(root);
    }
    // SAFETY: the hook takes no arguments and touches only the path above and
    // the file system.
    unsafe {
        libc::atexit(at_exit);
    }
}

/// Remove the templates of processes that are gone. A process cannot clean up
/// after a kill, so the next one to build a template does it.
///
/// **Only a process that is certainly gone counts.** A process is gone when
/// its entry under `/proc` is, and when `/proc` itself cannot be read nothing is
/// known, so nothing is removed.
fn sweep_dead_templates() {
    sweep_dead_templates_in(&std::env::temp_dir());
}

fn sweep_dead_templates_in(parent: &std::path::Path) {
    if !std::path::Path::new("/proc/self").exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(parent) else {
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
        if !std::path::Path::new("/proc").join(pid.to_string()).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// **How many lines of a server's stderr a failed boot quotes.**
pub const STDERR_TAIL_LINES: usize = 20;

/// **A boot that never printed its serving line**, with what to read it by: how
/// long it was waited for, what it printed on stdout, how its process ended and the
/// last lines of its stderr. A red run with this in it names a broken build or a
/// process killed from outside, which a bare "never served" cannot tell apart.
#[derive(Debug)]
pub struct BootFailure {
    /// How long the boot was waited for.
    pub after: std::time::Duration,
    /// What the process printed on stdout before it stopped.
    pub stdout: Vec<String>,
    /// How the process ended: the signal that killed it, its exit status, or that
    /// it was still running and was killed when the wait ended.
    pub ended: String,
    /// The last [`STDERR_TAIL_LINES`] lines of its stderr, oldest first.
    pub stderr_tail: Vec<String>,
}

impl std::fmt::Display for BootFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "never reached its serving line after {:?}; the process {}; stdout:\n{}\nstderr ended: {}",
            self.after,
            self.ended,
            self.stdout.join("\n"),
            self.stderr_tail.join(" | ")
        )
    }
}

/// **A boot that reached its serving line**, and the pipes to keep reading: a pipe
/// nobody reads fills, after which the process blocks inside a write.
pub struct Serving {
    /// The rest of stdout, to be read until the process stops.
    pub stdout: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    /// What stdout printed up to and including the serving line.
    pub seen: Vec<String>,
}

/// **The port a serving line names**, from `serving http://127.0.0.1:PORT/mcp`.
/// A caller that starts the binary on port 0 learns its port here.
pub fn served_port(line: &str) -> Option<u16> {
    let (address, _) = line.split("serving http://").nth(1)?.split_once("/mcp")?;
    address.rsplit(':').next()?.parse().ok()
}

impl Serving {
    /// **The port the binary reports it serves on**, read from the serving line.
    pub fn port(&self) -> u16 {
        self.seen
            .last()
            .and_then(|line| served_port(line))
            .expect("the serving line names a port")
    }
}

/// **Wait for a child to print `serving`, or say how it failed.** The child is
/// spawned with stdout piped, and stderr piped when its words are wanted; stderr
/// is read as it arrives and its last [`STDERR_TAIL_LINES`] lines are kept, and
/// go on being read after a good boot so the pipe never fills. A boot that fails
/// is waited on, and the failure names the signal that killed the process or the
/// status it exited with, not only that its stdout closed.
pub async fn wait_for_serving_line(
    child: &mut tokio::process::Child,
    within: std::time::Duration,
    serving: &str,
) -> Result<Serving, BootFailure> {
    use tokio::io::AsyncBufReadExt;
    let tail = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let reading_stderr = child.stderr.take().map(|stderr| {
        let keeping = tail.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut kept = keeping.lock().expect("the stderr tail");
                if kept.len() == STDERR_TAIL_LINES {
                    kept.pop_front();
                }
                kept.push_back(line);
            }
        })
    });
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut seen = Vec::new();
    let began = std::time::Instant::now();
    let served = tokio::time::timeout(within, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let hit = line.contains(serving);
                    seen.push(line);
                    if hit {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    if served {
        return Ok(Serving {
            stdout: lines,
            seen,
        });
    }
    use std::os::unix::process::ExitStatusExt;
    let ended = match tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await {
        Ok(Ok(status)) => match status.signal() {
            Some(signal) => format!("was killed by signal {signal}"),
            None => format!(
                "exited with status {}",
                status
                    .code()
                    .map_or("unknown".to_string(), |c| c.to_string())
            ),
        },
        Ok(Err(e)) => format!("could not be waited on: {e}"),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            "was still running when the wait ended, and was killed".to_string()
        }
    };
    if let Some(reading) = reading_stderr {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reading).await;
    }
    let stderr_tail = tail
        .lock()
        .expect("the stderr tail")
        .iter()
        .cloned()
        .collect();
    Err(BootFailure {
        after: began.elapsed(),
        stdout: seen,
        ended,
        stderr_tail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Scratch;

    /// **A template of a process that is gone is removed, and one of a live
    /// process is kept.** The dead one is a process this case started and
    /// waited for, so its number is certainly free; the live one is this
    /// process. A directory that is not a template's name is left alone.
    #[test]
    fn the_sweep_removes_the_template_of_a_dead_process_and_keeps_a_live_ones() {
        let parent = Scratch::new("template-sweep");
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("a child to start");
        let dead = child.id();
        child.wait().expect("the child ends");
        let template = |pid: u32| parent.0.join(format!("{TEMPLATE_DIR_PREFIX}{pid}"));
        let gone = template(dead);
        let live = template(std::process::id());
        let other = parent.0.join("jojobot-store-template-not-a-pid");
        for dir in [&gone, &live, &other] {
            std::fs::create_dir_all(dir.join("backup")).expect("a template");
        }
        sweep_dead_templates_in(&parent.0);
        assert!(!gone.exists(), "the template of a dead process was kept");
        assert!(live.is_dir(), "the template of a live process was removed");
        assert!(other.is_dir(), "a name that is no template's was removed");
    }

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
            let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM `{table}`"
            )))
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

    /// **Two calls with one name get two paths, however many run at once.** A
    /// path named for the process alone is the same path for every test in it.
    /// The calls are made from many threads together, as the tests of one binary
    /// are, and the count of distinct paths must be the count of calls.
    #[test]
    fn a_scratch_name_asked_for_by_many_tests_at_once_is_given_a_path_each() {
        let asked = 64;
        let paths: std::collections::BTreeSet<std::path::PathBuf> = (0..asked)
            .map(|_| std::thread::spawn(|| scratch_path("jojobot-scratch-case")))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().expect("a call returns"))
            .collect();
        assert_eq!(paths.len(), asked, "two calls were given one path");
        assert!(
            paths.iter().all(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(jojobot_ports::scratch::owner)
                    == Some(std::process::id())
            }),
            "the shared sweep can identify every scratch owner: {paths:?}"
        );
        assert!(
            paths
                .iter()
                .all(|path| path.starts_with(std::env::temp_dir())),
            "a path is made under the temp directory: {paths:?}"
        );
    }
}
