//! **The mailbox and session store's process** — jojobot spawns `dolt
//! sql-server` and supervises it.
//!
//! Not a service somebody else administers: the binary is in jojobot's
//! environment and the process is jojobot's to start, wait for, and stop. A
//! store that has to be provisioned separately is one that is missing on the
//! machine where it matters.
//!
//! **Nothing in here reaches a caller.** The server's own vocabulary — SQL,
//! ports, sockets, the product's name — is this file's business and stops
//! here; a failure crosses the boundary as the retryable store class the two
//! rails already have (rule 53).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};

mod ids;
pub mod mailboxes;
pub mod memory;
pub mod migrate;
pub mod sessions;
pub mod teaching;

/// The database jojobot serves out of its data directory. Named rather than
/// derived from the directory, so a test's temporary path and the deployed
/// `/var/lib/jojobot/db` address the same database by the same name.
const DATABASE: &str = "jojobot";

/// Where the server keeps its own configuration, inside the data directory.
///
/// **jojobot names this place, because jojobot runs the process.** The server
/// keeps a global configuration under a home directory and refuses to start
/// when it cannot find one — and a service manager may run jojobot as an
/// account that resolves to no home at all. Pointing the server at a directory
/// jojobot already owns takes the environment out of it.
///
/// It sits beside the database rather than anywhere else on the machine: every
/// piece of the store's state belongs under the one data directory, so a backup
/// of that directory is the whole store and nothing of it is hidden.
const SERVER_HOME: &str = "dolt-home";

/// How long a freshly spawned server has to start answering before the start
/// is called a failure. Generous: a first start initializes storage, and a
/// loaded machine is slow rather than broken.
const READY_WITHIN: Duration = Duration::from_secs(30);

/// How often the readiness poll asks. **A poll, never a sleep** — a fixed wait
/// is a test that passes on a fast machine and a start that fails on a slow
/// one.
const POLL_EVERY: Duration = Duration::from_millis(50);

/// Why the store could not be brought up. Startup only: once running, a
/// failure is the rails' own `Store` class rather than one of these.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The data directory could not be prepared.
    #[error("the store's data directory at {path} is not usable: {why}")]
    DataDir {
        /// Where jojobot was asked to keep the data.
        path: PathBuf,
        /// What went wrong.
        why: String,
    },
    /// The binary did not run.
    #[error("the store's server did not start: {0}")]
    Spawn(String),
    /// It ran and never answered.
    #[error("the store's server started and did not answer within {0:?}")]
    NeverReady(Duration),
    /// **Another server already holds the port.** This one's child could not
    /// bind and has exited, so anything answering there belongs to somebody
    /// else — a different data directory, with somebody else's records in it.
    #[error("the store's server could not take port {0}: another server holds it")]
    PortTaken(u16),
    /// **The child exited before it answered, for a reason of its own** —
    /// checked and ruled out as a port collision: nothing answered in its
    /// place. A corrupt store, a config error or an incompatible binary all
    /// land here, named by their own exit status and what the process itself
    /// said, rather than reported as somebody else holding the port.
    #[error("the store's server exited before it answered (status {status}): {stderr}")]
    Exited {
        /// How the process ended.
        status: std::process::ExitStatus,
        /// The last lines of its stderr, bounded — see [`StderrTail`].
        stderr: String,
    },
}

/// **The last [`STDERR_TAIL_LINES`] lines of a child's stderr**, kept as it
/// runs rather than read after the fact — a process this call is about to
/// kill on drop cannot be asked for its output afterwards.
///
/// Bounded because a long-running server can write far more than a startup
/// failure needs: this exists to say WHY a child exited immediately, not to
/// be its transcript. Lossy on non-UTF-8, because a byte that cannot be
/// shown is not worth failing the capture over.
struct StderrTail {
    lines: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
}

/// How many trailing lines of stderr [`StderrTail`] keeps. Generous enough
/// for a real crash message, bounded so a runaway process cannot grow it
/// without limit.
const STDERR_TAIL_LINES: usize = 40;

impl StderrTail {
    /// Start reading `stderr` in the background. The returned value stays
    /// live and current for as long as the caller holds it; nothing needs to
    /// be polled or awaited to keep it filling.
    fn capture(stderr: tokio::process::ChildStderr) -> Self {
        let lines = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        let sink = lines.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut read = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = read.next_line().await {
                let mut kept = sink.lock().expect("stderr tail lock");
                if kept.len() == STDERR_TAIL_LINES {
                    kept.pop_front();
                }
                kept.push_back(line);
            }
        });
        StderrTail { lines }
    }

    /// What the tail holds right now, one string, oldest kept line first.
    fn snapshot(&self) -> String {
        self.lines
            .lock()
            .expect("stderr tail lock")
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A running store: the child process, and a pool of connections to it.
///
/// **The child dies with this value.** A jojobot that exits leaving a server
/// holding the database makes the next boot fail in a way nobody can read, so
/// the handle owning the connection also owns the process.
pub struct Dolt {
    child: tokio::process::Child,
    pool: MySqlPool,
    /// The server's address without a database on the end, so another
    /// database on the same server can be opened.
    server: String,
}

impl Dolt {
    /// Bring the store up in `data_dir`, serving on `port` of the loopback
    /// address.
    ///
    /// **Loopback TCP rather than a unix socket, and it is not the first
    /// choice.** A socket under the data directory would put nothing on the
    /// network at all; the server in this toolchain has no socket option, on
    /// the command line or in its config, so there is nothing to ask for.
    /// Loopback with a configured port is the reachable version of the same
    /// intent.
    pub async fn start(data_dir: &Path, port: u16) -> Result<Self, StartError> {
        let database = data_dir.join(DATABASE);
        std::fs::create_dir_all(&database).map_err(|e| StartError::DataDir {
            path: database.clone(),
            why: e.to_string(),
        })?;
        let home = Self::server_home(data_dir)?;
        Self::init_if_empty(&database, &home)?;

        let mut spawning = tokio::process::Command::new("dolt");
        spawning
            .env("DOLT_ROOT_PATH", &home)
            .arg("sql-server")
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        die_with_this_process(&mut spawning);
        let mut child = spawning
            .spawn()
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        // **Read as it runs, not after.** `kill_on_drop` means a failed start
        // is a killed process by the time anything could read its stderr the
        // ordinary way; capturing has to start now, beside the spawn, to have
        // anything left to report.
        let stderr = StderrTail::capture(child.stderr.take().expect("stderr was piped"));

        // **`root`, and there is no choice about it.** The server's own
        // `--user` flag is gone from this toolchain: it initializes a root
        // account and directs anything else through `CREATE USER`. Nothing is
        // exposed by it — the listener is loopback and the account has no
        // password to leak — and inventing a second account would be
        // ceremony over a socket only this process reaches.
        let server = format!("mysql://root@127.0.0.1:{port}");
        let pool =
            Self::once_answering(&format!("{server}/{DATABASE}"), &mut child, &stderr).await?;
        Self::prove_it_is_ours(&pool, data_dir, port).await?;
        Ok(Dolt {
            child,
            pool,
            server,
        })
    }

    /// A pool that has answered at least once, **from the server this call
    /// spawned**.
    ///
    /// The server is up when it answers a query, not when the process exists —
    /// so this asks until it does. Connecting alone is not enough: the listener
    /// accepts before the database is served.
    ///
    /// **And an answer is not enough either.** If another server already holds
    /// the port, this call's child cannot bind and exits, while the port goes
    /// on answering — so a poll that only asked "did something reply" would
    /// hand back a pool aimed at another directory's database and report a
    /// clean start. The child is polled beside the query: if it is gone, the
    /// thing replying is not ours and there is nothing here to return.
    async fn once_answering(
        url: &str,
        child: &mut tokio::process::Child,
        stderr: &StderrTail,
    ) -> Result<MySqlPool, StartError> {
        let deadline = std::time::Instant::now() + READY_WITHIN;
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            let exited = child
                .try_wait()
                .map_err(|e| StartError::Spawn(e.to_string()))?;
            match MySqlPoolOptions::new()
                .max_connections(4)
                .acquire_timeout(POLL_EVERY * 4)
                .connect(url)
                .await
            {
                Ok(pool) => match sqlx::query("SELECT 1").execute(&pool).await {
                    Ok(_) => {
                        // **An answer, even from a child that has exited.**
                        // Two processes can pick the same free port at the
                        // same instant; the loser's child dies, and the
                        // winner is still what answers here. This is not a
                        // failure to report — it is `prove_it_is_ours`'s
                        // question, and reserving `PortTaken` for its
                        // corroborated answer is why this call does not mint
                        // it too.
                        return Ok(pool);
                    }
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
            }
            // **The child is gone, and nothing just answered in its place.**
            // A lost port race would have answered above, against the
            // winner — see the branch that returns `Ok(pool)`. Reaching here
            // with the child exited means it exited for a reason of its
            // own, and polling a process that is not there any longer would
            // only spend the rest of `READY_WITHIN` to say `NeverReady`,
            // which is honest about nothing having answered and silent
            // about the crash that guaranteed it never would.
            if let Some(status) = exited {
                return Err(StartError::Exited {
                    status,
                    stderr: stderr.snapshot(),
                });
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
        tracing::error!(error = %last, "the store's server never answered");
        Err(StartError::NeverReady(READY_WITHIN))
    }

    /// **Prove the server that answered is the one this call spawned.**
    ///
    /// A reply on the port is not evidence. If another server already holds it,
    /// this call's child cannot bind and dies, and the poll above is answered
    /// by the other one — so a start that accepted an answer would hand back a
    /// handle to a different directory's databases and report success. Two
    /// tests would then share one store, which is the isolation their harness
    /// builds a database per case to get.
    ///
    /// **The proof is a marker only this call knows the name of.** Creating a
    /// database makes a directory beside the others, so if it lands in OUR data
    /// directory the server writing it is ours. A server serving somewhere else
    /// puts it somewhere else, and the check fails.
    ///
    /// It is by construction rather than by proxy: no version string, no log
    /// line, nothing about the process table. The one question asked is the one
    /// that matters — is this the data I asked for.
    ///
    /// **It costs a write to find out, and on a collision that write lands on
    /// the other server.** A bind test before spawning would avoid touching it,
    /// and that was here and is gone: two checks answering one question means
    /// whichever runs first hides the other, and a guard nothing can reach is a
    /// guard nothing proves. One check, dropped again immediately, is the
    /// trade — and the name is unique per call, so it collides with nothing.
    async fn prove_it_is_ours(
        pool: &MySqlPool,
        data_dir: &Path,
        port: u16,
    ) -> Result<(), StartError> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let marker = format!(
            "whose_server_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        sqlx::raw_sql(&format!("CREATE DATABASE `{marker}`"))
            .execute(pool)
            .await
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        let ours = data_dir.join(&marker).exists();
        // Cleared whether or not it landed here: on somebody else's server this
        // is tidying up after a database we should never have made.
        let _ = sqlx::raw_sql(&format!("DROP DATABASE `{marker}`"))
            .execute(pool)
            .await;
        if !ours {
            return Err(StartError::PortTaken(port));
        }
        Ok(())
    }

    /// Make the directory the server reads its own configuration from, and
    /// give back an absolute path to it.
    ///
    /// **Absolute, and that is not tidiness.** `dolt init` runs with the
    /// database directory as its working directory, so a relative path would
    /// send the server's configuration somewhere else — a second home, one
    /// directory further down, for the one call that makes the database.
    fn server_home(data_dir: &Path) -> Result<PathBuf, StartError> {
        let home = data_dir.join(SERVER_HOME);
        std::fs::create_dir_all(&home).map_err(|e| StartError::DataDir {
            path: home.clone(),
            why: e.to_string(),
        })?;
        home.canonicalize().map_err(|e| StartError::DataDir {
            path: home,
            why: e.to_string(),
        })
    }

    /// Initialize the database directory if nothing is there yet.
    ///
    /// Idempotent by inspection rather than by ignoring a failure: an `init`
    /// that fails for a reason other than "already initialized" is a real
    /// failure, and swallowing it would hand back a handle to nothing.
    fn init_if_empty(database: &Path, home: &Path) -> Result<(), StartError> {
        if database.join(".dolt").exists() {
            return Ok(());
        }
        let done = std::process::Command::new("dolt")
            .env("DOLT_ROOT_PATH", home)
            .arg("init")
            .arg("--name")
            .arg("jojobot")
            .arg("--email")
            .arg("jojobot@localhost")
            .current_dir(database)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        if !done.status.success() {
            return Err(StartError::DataDir {
                path: database.to_path_buf(),
                why: String::from_utf8_lossy(&done.stderr).trim().to_string(),
            });
        }
        Ok(())
    }

    /// **Bring the store up and its schema to the shape this build expects.**
    ///
    /// The one call a boot makes. It is separate from [`start`](Self::start)
    /// because a server that is running with the wrong schema is not ready, and
    /// a caller that had to remember two steps would eventually do one.
    ///
    /// Returns the migrations it applied, which is empty on every start after
    /// the first. **That is the ordinary case, not a special one** — the ledger
    /// records what has run, so a restart against a migrated store applies
    /// nothing and says so.
    pub async fn ready(
        data_dir: &Path,
        port: u16,
    ) -> Result<(Self, Vec<String>), migrate::MigrateError> {
        Self::brought_up(data_dir, port, None).await
    }

    /// The same boot with its boundary marked somewhere else, **so a boot
    /// whose mark cannot be written can be watched coming up anyway**. Every
    /// boot marks now, so the mark sits on the steady-state path and a mark
    /// that failed the boot would take the server down with it.
    #[cfg(test)]
    pub(crate) async fn ready_marking_over(
        data_dir: &Path,
        port: u16,
        marks: MySqlPool,
    ) -> Result<(Self, Vec<String>), migrate::MigrateError> {
        Self::brought_up(data_dir, port, Some(marks)).await
    }

    async fn brought_up(
        data_dir: &Path,
        port: u16,
        marks: Option<MySqlPool>,
    ) -> Result<(Self, Vec<String>), migrate::MigrateError> {
        let store =
            Self::start(data_dir, port)
                .await
                .map_err(|e| migrate::MigrateError::Failed {
                    version: "the store itself".to_string(),
                    why: e.to_string(),
                })?;
        let applied = migrate::run(store.pool()).await?;
        // **The boundary that bounds the night.** Anything that reached the
        // store outside a session — a migration this call just applied, a
        // repair somebody made by hand, whatever a crash left behind — would
        // otherwise sit inside the next session's span and read as that
        // session's doing.
        snapshot(marks.as_ref().unwrap_or(store.pool()), "server boot").await;
        Ok((store, applied))
    }

    /// A pool onto a database of its own on this server, created if needed.
    ///
    /// **For tests that need isolation from each other**, which is a real need
    /// rather than a convenience: two contract cases sharing one database
    /// would let one case's rows satisfy another's assertions. Production uses
    /// [`pool`](Self::pool) and the one database.
    pub async fn database(&self, name: &str) -> Result<MySqlPool, StartError> {
        sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{name}`"))
            .execute(&self.pool)
            .await
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        MySqlPoolOptions::new()
            .max_connections(4)
            .connect(&format!("{}/{name}", self.server))
            .await
            .map_err(|e| StartError::Spawn(e.to_string()))
    }

    /// The connection pool, for the two adapters that speak to this store.
    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    /// Stop the server. Called on the way down; `Drop` covers the paths that
    /// do not get to call it, and the kernel covers the paths where nothing in
    /// this process runs at all — see [`die_with_this_process`].
    pub async fn stop(&mut self) {
        self.pool.close().await;
        let _ = self.child.kill().await;
    }
}

/// One permanent-id migration's own outcome, for a caller to log — the
/// words a restart's log should use are the composition root's call, never
/// this crate's.
#[derive(Debug)]
pub enum Migrated {
    /// The read this migration needs failed; nothing was attempted, and
    /// nothing already migrated was touched.
    Skipped(String),
    /// It ran, and rewrote this many rows — zero is a real, successful
    /// answer here, never conflated with [`Migrated::Skipped`].
    Ran(usize),
    /// It ran and failed partway.
    Failed(String),
}

/// **Run every one-time permanent-id migration mail and sessions still
/// need, over one read of the entity list, and say what actually happened
/// for each — never a false `Ran(0)` standing in for a read that never
/// happened.**
///
/// `memory.list_entities` failing is not an empty store: feeding an empty
/// `Vec` to a migration either way makes it match nothing and report a
/// completed run over rows it never saw. So every migration needing that
/// read `Skip`s together when it fails, rather than three separate copies
/// of the same mistake — this is the one place that decision is made.
///
/// `former_handles` failing is narrower: only the bot-column migration
/// needs it, so only that one migration `Skip`s when it alone fails.
pub async fn migrate_permanent_ids(
    memory: &dyn jojobot_domain::memory::Memory,
    mail: &mailboxes::DoltMailboxes,
    sessions: &sessions::DoltSessions,
) -> Vec<(&'static str, Migrated)> {
    let known = match memory.list_entities(None).await {
        Ok(known) => known,
        Err(e) => {
            let reason = e.to_string();
            return vec![
                ("mail mentions", Migrated::Skipped(reason.clone())),
                ("session mentions", Migrated::Skipped(reason.clone())),
                ("session bot column", Migrated::Skipped(reason)),
            ];
        }
    };
    let mail_mentions = match mail.migrate_mentions(&known).await {
        Ok(n) => Migrated::Ran(n),
        Err(e) => Migrated::Failed(e.to_string()),
    };
    let session_mentions = match sessions.migrate_mentions(&known).await {
        Ok(n) => Migrated::Ran(n),
        Err(e) => Migrated::Failed(e.to_string()),
    };
    let bot_column = match memory.former_handles().await {
        Ok(former) => match sessions.migrate_bot_column(&known, &former).await {
            Ok(n) => Migrated::Ran(n),
            Err(e) => Migrated::Failed(e.to_string()),
        },
        Err(e) => Migrated::Skipped(e.to_string()),
    };
    vec![
        ("mail mentions", mail_mentions),
        ("session mentions", session_mentions),
        ("session bot column", bot_column),
    ]
}

/// **Mark a boundary in the store's own history, so a person can see what
/// changed between two of them.**
///
/// Three moments take one: the server boots, a session opens, a session ends.
/// It is for human forensics and for the break-glass case where somebody has
/// to undo a sitting by hand, using the store's own tooling. **Nothing jojobot
/// answers is derived from these**, no verb exposes them, and no caller is
/// told they happen (rules 53 and 158).
///
/// **A boundary is not an author.** Runs overlap — a bot may have two at once —
/// and a mark is global to the store, so one session's boundary takes in
/// whatever another has written since the last one. What two marks honestly
/// bound is *what changed between them*, never *what one session did*, which
/// is why nothing here is named for the session's work.
///
/// **It cannot fail its caller.** Booting and opening a session are the acts
/// that matter; this is a convenience beside them, so it returns nothing and
/// every failure is a log line rather than a refusal. Silence would be the
/// other failure — a store quietly not marking anything looks exactly like one
/// that is.
///
/// **Nothing to mark is the ordinary case**, not an error: a boot that changed
/// no schema and a run that only read leave no difference behind. The store
/// refuses an empty mark, so the difference is checked first and the quiet case
/// stays quiet.
pub(crate) async fn snapshot(pool: &MySqlPool, boundary: &str) {
    let changed: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM dolt_status")
        .fetch_one(pool)
        .await;
    match changed {
        Ok(0) => {
            tracing::debug!(boundary, "store: nothing changed since the last mark");
            return;
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(boundary, error = %e, "store: could not read what has changed");
            return;
        }
    }
    if let Err(e) = sqlx::query("CALL DOLT_COMMIT('-A', '-m', ?)")
        .bind(boundary)
        .execute(pool)
        .await
    {
        tracing::warn!(boundary, error = %e, "store: the boundary was not marked");
    }
}

/// **Ask the kernel to kill the store the moment jojobot dies, however jojobot
/// dies.**
///
/// `kill_on_drop` and [`Dolt::stop`] cover every ending that gets to run Rust:
/// a return, an unwind, a value going out of scope. They cover none of the
/// endings where nothing in this process runs — a kill, an out-of-memory kill,
/// a machine going down — and a store that outlives its owner holds a port and
/// gigabytes of memory until somebody goes looking for it. The one cleanup that
/// survives an ending like that belongs to the kernel, so the kernel is asked
/// for it.
///
/// ⚠️ **The flag follows the thread that forks, not the process.** The signal
/// is sent when the spawning thread exits, whenever that is — so a store must
/// be started from a thread that lives as long as the store is wanted, or it is
/// killed underneath a caller still using it. jojobot starts its store on the
/// thread `main` runs on, and a test starts one on the thread that test runs
/// on; both outlive every use of what they started.
#[cfg(target_os = "linux")]
fn die_with_this_process(spawning: &mut tokio::process::Command) {
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure runs in the forked child between fork and exec, where
    // only async-signal-safe calls are allowed. It is two system calls and an
    // error value built from an integer — no allocation, no lock, no logging.
    unsafe {
        spawning.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // **The parent can already be gone**, having died between the fork
            // and the line above — and then the signal just asked for has
            // already been sent and missed, leaving a server nobody will ever
            // collect. Reading the parent back is what closes that window:
            // refusing to exec fails the spawn, which is a caller's error to
            // handle rather than a process nobody owns.
            if libc::getppid() != parent {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}

/// Where no such flag exists, `Drop` is the whole of the cleanup.
#[cfg(not(target_os = "linux"))]
fn die_with_this_process(_spawning: &mut tokio::process::Command) {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A directory of this test's own, removed when it is done.
    pub(crate) struct Scratch(pub(crate) PathBuf);

    impl Scratch {
        pub(crate) fn new(what: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "jojobot-dolt-{}-{what}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("a clock after 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// **What the store's own history says**, newest first — the surface a
    /// person looking for what a sitting did actually reads, and the only
    /// evidence a boundary was marked at all.
    pub(crate) async fn marks(pool: &MySqlPool) -> Vec<String> {
        sqlx::query_scalar("SELECT message FROM dolt_log ORDER BY date DESC")
            .fetch_all(pool)
            .await
            .expect("the store keeps its own history")
    }

    /// **The port helper lives in [`crate::testing`]**, so the suites in this
    /// crate and the ones beside it hand out ports from one block rather than
    /// from a copy each. The copies are what let the same race be repaired
    /// three times and still arrive.
    pub(crate) use crate::testing::free_port;

    /// **The store comes up on an empty directory and answers.**
    ///
    /// The whole of step one: jojobot starts the server itself, waits until it
    /// is really serving rather than merely spawned, and can talk to it. Both
    /// halves, because a handle that exists and cannot answer is the failure
    /// this is guarding against.
    #[tokio::test]
    async fn the_store_comes_up_on_an_empty_directory() {
        let scratch = Scratch::new("empty");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");

        let (answer,): (i64,) = sqlx::query_as("SELECT 1")
            .fetch_one(store.pool())
            .await
            .expect("the store answers");
        assert_eq!(answer, 1, "a store that is up answers a query");

        // …and the database jojobot addresses is the one it is connected to,
        // rather than whatever the directory happened to be called.
        let (name,): (String,) = sqlx::query_as("SELECT DATABASE()")
            .fetch_one(store.pool())
            .await
            .expect("the store names its database");
        assert_eq!(name, DATABASE);

        store.stop().await;
    }

    /// **A second start on the same directory finds the data that is there.**
    ///
    /// Restarting jojobot must not initialize over the top of the store: the
    /// first start creates it, and every one after it opens it. Proven with a
    /// row rather than with a file check — what matters is that the DATA
    /// survives, not that a directory does.
    #[tokio::test]
    async fn a_restart_opens_the_store_rather_than_remaking_it() {
        let scratch = Scratch::new("restart");

        let mut first = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the first start comes up");
        sqlx::query("CREATE TABLE kept (n INT PRIMARY KEY)")
            .execute(first.pool())
            .await
            .expect("a table is created");
        sqlx::query("INSERT INTO kept (n) VALUES (7)")
            .execute(first.pool())
            .await
            .expect("a row is written");
        first.stop().await;

        let mut second = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the second start comes up");
        let (n,): (i32,) = sqlx::query_as("SELECT n FROM kept")
            .fetch_one(second.pool())
            .await
            .expect("the row is still there");
        assert_eq!(n, 7, "a restart opens the store it left behind");
        second.stop().await;
    }

    /// **A restart against a migrated store applies nothing and still comes
    /// up.** The most-run path there is: every boot after the first.
    ///
    /// Both halves. "Applied nothing" alone is satisfied by a store that never
    /// applied anything, so the schema is exercised afterwards to show it is
    /// really there.
    #[tokio::test]
    async fn a_restart_against_a_migrated_store_applies_nothing() {
        let scratch = Scratch::new("ready-twice");
        let path = scratch.0.clone();
        std::mem::forget(scratch);

        let (mut first, applied) = Dolt::ready(&path, free_port())
            .await
            .expect("the first boot brings the store up");
        // **Counted from the set rather than written down**, so a new migration
        // does not make this case fail for a reason that has nothing to do with
        // what it is about.
        assert_eq!(
            applied.len(),
            migrate::MIGRATIONS.len(),
            "the first boot applies the set: {applied:?}"
        );
        // **And it opened the directory it was ASKED for.** Without this the
        // case only shows the two boots agreeing with each other, which they
        // would do just as well against a directory nobody named.
        assert!(
            path.join("jojobot").is_dir(),
            "the store landed under the directory the caller gave: {path:?}"
        );
        first.stop().await;

        let (mut again, applied) = Dolt::ready(&path, free_port())
            .await
            .expect("the second boot brings the store up");
        assert!(
            applied.is_empty(),
            "a restart against a migrated store applies nothing: {applied:?}"
        );

        // …and the schema those migrations were for is really there, which is
        // what stops the assertion above from passing over a store that has
        // nothing in it.
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES ('gamma', 'bot:gamma')")
            .execute(again.pool())
            .await
            .expect("the schema survived the restart");

        again.stop().await;
    }

    /// **A store refuses a server it did not spawn.**
    ///
    /// Two starts can be handed one port: `free_port` binds, reads the number
    /// and releases it, so two concurrent callers can be given the same one.
    /// The loser's child then cannot bind and dies — silently, its output goes
    /// nowhere — and the readiness poll connects to the WINNER and gets an
    /// answer.
    ///
    /// **The handle it returned would then be aimed at another test's
    /// database**, in another directory, with that test's tables and rows in
    /// it. That is not a flake: it is two cases sharing one store, which is
    /// the thing the contract harness builds a database per case to prevent,
    /// defeated one level below where it looks.
    ///
    /// So a start that cannot verify the server is its own must fail rather
    /// than hand back a working-looking handle to somebody else's data.
    #[tokio::test]
    async fn a_start_refuses_a_server_it_did_not_spawn() {
        let held = Scratch::new("port-held");
        let intruder = Scratch::new("port-intruder");
        let port = free_port();

        let mut first = Dolt::start(&held.0, port)
            .await
            .expect("the first server takes the port");
        crate::dolt::migrate::run(first.pool())
            .await
            .expect("the first server's schema");

        let second = Dolt::start(&intruder.0, port).await;
        assert!(
            matches!(second, Err(StartError::PortTaken { .. })),
            "a second start on a held port must refuse, not adopt the server \
             already there"
        );

        // **And it left nothing behind on the server it collided with.** The
        // check has to write a marker to learn whose store answered, so on a
        // collision that write lands over there — and clearing it is part of
        // the check rather than tidiness, because a refused start that leaves
        // a database in somebody else's store has damaged what it was
        // protecting.
        let intruded: Vec<_> = std::fs::read_dir(&held.0)
            .expect("the first server's directory is readable")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("whose_server_"))
            .collect();
        assert!(
            intruded.is_empty(),
            "a refused start must leave no marker in the store it collided with: {intruded:?}"
        );

        // **The positive it rests on.** A start on a port nobody holds still
        // works — otherwise the refusal above passes on a build where no
        // server ever comes up.
        let free = Scratch::new("port-free");
        let mut ours = Dolt::start(&free.0, free_port())
            .await
            .expect("a free port still starts");
        assert!(
            !crate::dolt::migrate::run(ours.pool())
                .await
                .expect("its schema is its own")
                .is_empty(),
            "and the database it reached is empty, so it is not the first one"
        );

        ours.stop().await;
        first.stop().await;
    }

    /// 🚨 **An early exit that is NOT a port collision names its own
    /// reason, rather than being reported as one.**
    ///
    /// Calls `once_answering` directly, on a child that was never going to
    /// come up: a subcommand `dolt` does not have, on a port nothing is
    /// listening on, so there is no answer for the collision branch to
    /// mistake for a lost race. This is the branch the module doc used to
    /// call unproven — the existing `PortTaken` test above reaches its
    /// sibling instead (an answer FROM the winner, before this child has
    /// finished dying), never this one.
    #[tokio::test]
    async fn an_early_exit_that_is_not_a_port_collision_names_its_own_reason() {
        let mut child = tokio::process::Command::new("dolt")
            .arg("this-is-not-a-real-dolt-subcommand")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("dolt itself must be on PATH to run this suite at all");
        let stderr = StderrTail::capture(child.stderr.take().expect("stderr was piped"));
        let port = free_port();

        let err = Dolt::once_answering(
            &format!("mysql://root@127.0.0.1:{port}/jojobot"),
            &mut child,
            &stderr,
        )
        .await
        .expect_err("a nonexistent subcommand cannot come up, and nothing else answers for it");

        match err {
            StartError::Exited { status, stderr } => {
                assert!(
                    !status.success(),
                    "a bad subcommand must exit non-zero: {status}"
                );
                assert!(
                    !stderr.is_empty(),
                    "the real reason must be captured, not silently discarded"
                );
            }
            other => panic!("expected Exited, naming the real reason — got {other:?} instead"),
        }
    }

    /// **A boot whose boundary cannot be marked still serves.**
    ///
    /// Every boot marks now, so the mark is on the path a server takes on an
    /// ordinary restart with nothing to apply. Booting is the act that matters
    /// and the mark is a convenience beside it: a store that refuses the mark
    /// must still come up, apply its schema and answer.
    ///
    /// The marks go to a pool aimed at a port nothing is listening on, so every
    /// mark this boot tries to write fails while the store itself is untouched.
    /// **Both halves**: the boot answers, and the history really is unmarked —
    /// without the second this passes on a build where marking works.
    #[tokio::test]
    async fn a_boot_whose_boundary_cannot_be_marked_still_comes_up() {
        let scratch = Scratch::new("boot-unmarkable");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let nowhere = MySqlPoolOptions::new()
            .acquire_timeout(Duration::from_secs(2))
            .connect_lazy("mysql://root@127.0.0.1:1/nothing-is-there")
            .expect("a pool that will never answer");

        let (mut store, applied) = Dolt::ready_marking_over(&path, free_port(), nowhere)
            .await
            .expect("the boot stands even when its boundary cannot be marked");
        assert!(
            !applied.is_empty(),
            "the schema still applied, so the boot did its work"
        );
        // …and the store it handed back is serving, not merely constructed.
        let served: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(store.pool())
            .await
            .expect("the store answers a read of the schema it just applied");
        assert_eq!(served, 0);

        assert!(
            marks(store.pool()).await.iter().all(|m| m != "server boot"),
            "the mark really did fail, or this proves nothing"
        );

        store.stop().await;
    }

    /// **A boot marks a boundary, and a boot with nothing behind it marks
    /// none.**
    ///
    /// The first start applies the schema, which is a change nobody has
    /// bounded: without a mark here it would land inside the first session's
    /// span and read as that session's doing. The second start applies nothing
    /// and changes nothing, which is the ordinary case rather than a failure —
    /// so it must add no mark and must not refuse the boot, and the store
    /// itself refuses an empty mark.
    #[tokio::test]
    async fn a_boot_marks_a_boundary_and_a_boot_with_nothing_to_mark_does_not() {
        let scratch = Scratch::new("boot-boundary");
        let path = scratch.0.clone();
        // Leaked deliberately: dropping it removes the data under a running
        // server. The stores are stopped below and the directory goes with the
        // process.
        std::mem::forget(scratch);
        let port = free_port();

        let (mut first, applied) = Dolt::ready(&path, port).await.expect("the store comes up");
        assert!(
            !applied.is_empty(),
            "a first boot applies the schema, which is the change this mark bounds"
        );
        let after_boot = marks(first.pool()).await;
        assert!(
            after_boot.iter().any(|m| m == "server boot"),
            "the boot marks its own boundary: {after_boot:?}"
        );
        first.stop().await;

        let (mut again, applied) = Dolt::ready(&path, port).await.expect("it comes up again");
        assert!(
            applied.is_empty(),
            "the schema is current, so this boot changes nothing"
        );
        let after_second = marks(again.pool()).await;
        assert_eq!(
            after_second.len(),
            after_boot.len(),
            "a boundary with nothing behind it leaves no mark: {after_second:?}"
        );
        again.stop().await;
    }

    /// A `Memory` that answers `list_entities` and `former_handles` on
    /// request and refuses to be asked anything else — `migrate_permanent_ids`
    /// makes no other call on the entity world, and a call to one of these
    /// is this test's own bug, not something to paper over with a fake
    /// implementation.
    struct FailingEntityRead;

    #[async_trait::async_trait]
    impl jojobot_domain::memory::Memory for FailingEntityRead {
        async fn list_entities(
            &self,
            _: Option<jojobot_domain::memory::EntityKind>,
        ) -> Result<Vec<jojobot_domain::memory::Entity>, jojobot_domain::memory::MemoryError>
        {
            Err(jojobot_domain::memory::MemoryError::Store(
                "the entity world is down".into(),
            ))
        }
        async fn add_entity(
            &self,
            _: jojobot_domain::memory::NewEntity,
        ) -> Result<
            jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn declare_type(
            &self,
            _: jojobot_domain::memory::types::DeclaredType,
        ) -> Result<jojobot_domain::memory::types::DeclaredType, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn declared_types(
            &self,
        ) -> Result<
            Vec<jojobot_domain::memory::types::DeclaredType>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn declare_kind(
            &self,
            _: &str,
            _: jojobot_domain::memory::types::Origin,
            _: Vec<jojobot_domain::memory::types::Field>,
        ) -> Result<(), jojobot_domain::memory::MemoryError> {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn declared_kinds(
            &self,
        ) -> Result<
            Vec<(String, jojobot_domain::memory::types::Origin)>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn reclaim_kind(&self, _: &str) -> Result<(), jojobot_domain::memory::MemoryError> {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn update_entity(
            &self,
            _: &jojobot_domain::memory::EntityId,
            _: jojobot_domain::memory::EntityPatch,
        ) -> Result<
            jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn rename_entity(
            &self,
            _: &jojobot_domain::memory::EntityId,
            _: &jojobot_domain::memory::EntityId,
            _: Option<jojobot_domain::memory::EntityId>,
            _: jiff::civil::Date,
            _: Option<&str>,
        ) -> Result<
            jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn capture(
            &self,
            _: jojobot_domain::memory::NewFact,
        ) -> Result<
            jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn recall(
            &self,
            _: &jojobot_domain::memory::EntityId,
        ) -> Result<Vec<jojobot_domain::memory::Fact>, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn history(
            &self,
            _: &jojobot_domain::memory::EntityId,
            _: &str,
        ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn claim_history(
            &self,
            _: &jojobot_domain::memory::FactAddress,
        ) -> Result<Vec<jojobot_domain::memory::ClaimWrite>, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn fields(
            &self,
            _: &jojobot_domain::memory::EntityId,
        ) -> Result<std::collections::BTreeMap<String, String>, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn update_fact(
            &self,
            _: &jojobot_domain::memory::FactAddress,
            _: jojobot_domain::memory::FactPatch,
        ) -> Result<
            jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
            jojobot_domain::memory::MemoryError,
        > {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn retract(
            &self,
            _: &jojobot_domain::memory::FactAddress,
            _: Option<&str>,
            _: jiff::civil::Date,
        ) -> Result<jojobot_domain::memory::Retraction, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn merge(
            &self,
            _: &jojobot_domain::memory::EntityId,
            _: &jojobot_domain::memory::EntityId,
            _: Option<&str>,
            _: jiff::civil::Date,
        ) -> Result<jojobot_domain::memory::Merge, jojobot_domain::memory::MemoryError> {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn set_prose(
            &self,
            _: &jojobot_domain::memory::EntityId,
            _: &str,
        ) -> Result<String, jojobot_domain::memory::MemoryError> {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
        async fn scan(
            &self,
        ) -> Result<Vec<jojobot_domain::memory::search::DocScan>, jojobot_domain::memory::MemoryError>
        {
            unimplemented!("migrate_permanent_ids only reads the entity world")
        }
    }

    /// 🚨 **A failed entity read skips every migration that needs it,
    /// against a REAL store carrying REAL legacy text — never a false
    /// `Ran(0)` standing in for a read that never happened.**
    #[tokio::test]
    async fn migrate_permanent_ids_skips_rather_than_guesses_the_store_is_empty() {
        use jojobot_domain::memory::{Memory, NewEntity};
        use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};

        let scratch = Scratch::new("migrate-permanent-ids-skip");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");

        let real_memory = crate::dolt::memory::DoltMemory::open(store.pool().clone());
        real_memory
            .add_entity(NewEntity::new(
                jojobot_domain::memory::EntityId(
                    "thing:contract-migrate-permanent-ids-skip".to_string(),
                ),
                "was",
                "contract-fixture",
            ))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block a fresh handle");
        real_memory
            .badge_the_unbadged()
            .await
            .expect("badges are drawn");

        let mail = mailboxes::DoltMailboxes::open(
            store.pool().clone(),
            std::sync::Arc::new(crate::owners::MemoryOwners::new(std::sync::Arc::new(
                jojobot_domain::memory::testing::InMemoryMemory::default(),
            ))),
        );
        let sess = sessions::DoltSessions::open(store.pool().clone());
        let session = sess
            .begin(NewSession {
                bot: jojobot_domain::memory::EntityId("bot:contract-migrate-permanent-ids".into()),
                sid: Sid("sid-permanent-ids".to_string()),
                focus: "about @thing:contract-migrate-permanent-ids-skip".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        sess.append(
            &session.id,
            NewEntry::manual(
                "found @thing:contract-migrate-permanent-ids-skip",
                "2026-01-01T00:01:00Z".parse().expect("a fixed instant"),
                None,
            ),
        )
        .await
        .expect("append should succeed");

        let results = migrate_permanent_ids(&FailingEntityRead, &mail, &sess).await;
        for (name, outcome) in &results {
            assert!(
                matches!(outcome, Migrated::Skipped(_)),
                "{name} must skip on a failed read rather than run against an empty \
                 stand-in for one"
            );
        }

        // **And nothing moved.** The legacy text is exactly as it was —
        // proof that `Skipped` is not just the right word but the right
        // absence of a write.
        let untouched = sess
            .read_session(&session.id)
            .await
            .expect("read_session should succeed");
        assert!(
            untouched
                .focus
                .contains("thing:contract-migrate-permanent-ids-skip"),
            "a skipped migration must leave the bare handle exactly as it was: {}",
            untouched.focus
        );

        store.stop().await;
    }

    /// **The healthy path still reports what really happened**, each
    /// migration's own real count — the positive `_skips_rather_than_guesses`
    /// rests on, so that test cannot pass on a build where nothing ever runs.
    #[tokio::test]
    async fn migrate_permanent_ids_reports_the_real_count_when_the_read_succeeds() {
        use jojobot_domain::memory::{Memory, NewEntity};
        use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};

        let scratch = Scratch::new("migrate-permanent-ids-run");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");

        let memory = crate::dolt::memory::DoltMemory::open(store.pool().clone());
        memory
            .add_entity(NewEntity::new(
                jojobot_domain::memory::EntityId(
                    "thing:contract-migrate-permanent-ids-run".to_string(),
                ),
                "was",
                "contract-fixture",
            ))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block a fresh handle");
        memory.badge_the_unbadged().await.expect("badges are drawn");

        let mail = mailboxes::DoltMailboxes::open(
            store.pool().clone(),
            std::sync::Arc::new(crate::owners::MemoryOwners::new(std::sync::Arc::new(
                jojobot_domain::memory::testing::InMemoryMemory::default(),
            ))),
        );
        let sess = sessions::DoltSessions::open(store.pool().clone());
        let session = sess
            .begin(NewSession {
                bot: jojobot_domain::memory::EntityId(
                    "bot:contract-migrate-permanent-ids-2".into(),
                ),
                sid: Sid("sid-permanent-ids-2".to_string()),
                focus: "about @thing:contract-migrate-permanent-ids-run".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        sess.append(
            &session.id,
            NewEntry::manual(
                "found @thing:contract-migrate-permanent-ids-run",
                "2026-01-01T00:01:00Z".parse().expect("a fixed instant"),
                None,
            ),
        )
        .await
        .expect("append should succeed");

        let results = migrate_permanent_ids(&memory, &mail, &sess).await;
        let ran: std::collections::HashMap<_, _> = results
            .into_iter()
            .map(|(name, outcome)| match outcome {
                Migrated::Ran(n) => (name, n),
                other => panic!("{name}: expected Ran on a healthy read, got {other:?}"),
            })
            .collect();
        assert_eq!(
            ran["session mentions"], 2,
            "the focus and the entry both had a bare handle to rewrite"
        );

        let after = sess
            .read_session(&session.id)
            .await
            .expect("read_session should succeed");
        assert!(
            !after
                .focus
                .contains("thing:contract-migrate-permanent-ids-run"),
            "the bare handle must not survive a run that reports Ran: {}",
            after.focus
        );

        store.stop().await;
    }
}
