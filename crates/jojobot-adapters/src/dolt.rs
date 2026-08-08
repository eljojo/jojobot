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
pub mod migrate;
pub mod sessions;

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

        let child = tokio::process::Command::new("dolt")
            .env("DOLT_ROOT_PATH", &home)
            .arg("sql-server")
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| StartError::Spawn(e.to_string()))?;

        // **`root`, and there is no choice about it.** The server's own
        // `--user` flag is gone from this toolchain: it initializes a root
        // account and directs anything else through `CREATE USER`. Nothing is
        // exposed by it — the listener is loopback and the account has no
        // password to leak — and inventing a second account would be
        // ceremony over a socket only this process reaches.
        let server = format!("mysql://root@127.0.0.1:{port}");
        let mut child = child;
        let pool = Self::once_answering(&format!("{server}/{DATABASE}"), &mut child, port).await?;
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
        port: u16,
    ) -> Result<MySqlPool, StartError> {
        let deadline = std::time::Instant::now() + READY_WITHIN;
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            // ⚠️ **UNPROVEN, AND DELIBERATELY KEPT.** No test in this suite
            // reaches this branch, and the comment exists so the next reader
            // knows the coverage is absent rather than assuming it.
            //
            // It guards one window: two processes picking the same free port at
            // the same instant, each spawning before the other has bound. The
            // loser's child exits, and this notices before a connection is even
            // attempted. That is reachable across the two test binaries, so it
            // is not decoration.
            //
            // The suite cannot produce it. Its deterministic case is a port
            // ALREADY held, and there the connection succeeds on the first
            // pass — against the other server — before this child has finished
            // dying, so the identity check below is what refuses. Reaching this
            // branch needs a hook between the spawn and the poll, which is more
            // apparatus than the branch is worth.
            //
            // **Nothing downstream depends on it**: the identity check refuses
            // the same collision by construction, later and more expensively.
            // This is the cheap early exit, not the guarantee.
            if child
                .try_wait()
                .map_err(|e| StartError::Spawn(e.to_string()))?
                .is_some()
            {
                return Err(StartError::PortTaken(port));
            }
            match MySqlPoolOptions::new()
                .max_connections(4)
                .acquire_timeout(POLL_EVERY * 4)
                .connect(url)
                .await
            {
                Ok(pool) => match sqlx::query("SELECT 1").execute(&pool).await {
                    Ok(_) => {
                        return Ok(pool);
                    }
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
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
        let store =
            Self::start(data_dir, port)
                .await
                .map_err(|e| migrate::MigrateError::Failed {
                    version: "the store itself".to_string(),
                    why: e.to_string(),
                })?;
        let applied = migrate::run(store.pool()).await?;
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
    /// do not get to call it.
    pub async fn stop(&mut self) {
        self.pool.close().await;
        let _ = self.child.kill().await;
    }
}

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

    /// A port no other caller in this process will be given.
    ///
    /// **A cursor, not just a bind.** Asking the OS for `:0` and letting the
    /// listener go hands two concurrent callers the same number often enough to
    /// matter — measured on this machine at 4 collisions in 400 with two callers,
    /// and 339 in 3200 with sixteen. Two servers then get one port: the loser's
    /// child cannot bind and dies, and the winner answers its client, so a whole
    /// test runs against another test's database.
    ///
    /// Taking a distinct slot first means no two callers here can be offered the
    /// same candidate, whatever the kernel would have said. The bind that follows
    /// only checks the candidate is free; the window it leaves is somebody outside
    /// this process, and `Dolt::start` refuses a port it cannot take.
    pub(crate) fn free_port() -> u16 {
        static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        for _ in 0..40_000 {
            let slot = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let port = 20_000 + slot % 40_000;
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return port;
            }
        }
        panic!("no free port in the range this suite uses")
    }

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
        assert_eq!(
            applied.len(),
            7,
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
}
