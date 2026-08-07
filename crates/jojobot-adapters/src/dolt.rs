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

pub mod mailboxes;
pub mod migrate;
pub mod sessions;

/// The database jojobot serves out of its data directory. Named rather than
/// derived from the directory, so a test's temporary path and the deployed
/// `/var/lib/jojobot/db` address the same database by the same name.
const DATABASE: &str = "jojobot";

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
        Self::init_if_empty(&database)?;

        let child = tokio::process::Command::new("dolt")
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
        let pool = Self::once_answering(&format!("{server}/{DATABASE}")).await?;
        Ok(Dolt {
            child,
            pool,
            server,
        })
    }

    /// A pool that has answered at least once.
    ///
    /// The server is up when it answers a query, not when the process exists —
    /// so this asks until it does. Connecting alone is not enough: the listener
    /// accepts before the database is served.
    async fn once_answering(url: &str) -> Result<MySqlPool, StartError> {
        let deadline = std::time::Instant::now() + READY_WITHIN;
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            match MySqlPoolOptions::new()
                .max_connections(4)
                .acquire_timeout(POLL_EVERY * 4)
                .connect(url)
                .await
            {
                Ok(pool) => match sqlx::query("SELECT 1").execute(&pool).await {
                    Ok(_) => return Ok(pool),
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
        tracing::error!(error = %last, "the store's server never answered");
        Err(StartError::NeverReady(READY_WITHIN))
    }

    /// Initialize the database directory if nothing is there yet.
    ///
    /// Idempotent by inspection rather than by ignoring a failure: an `init`
    /// that fails for a reason other than "already initialized" is a real
    /// failure, and swallowing it would hand back a handle to nothing.
    fn init_if_empty(database: &Path) -> Result<(), StartError> {
        if database.join(".dolt").exists() {
            return Ok(());
        }
        let done = std::process::Command::new("dolt")
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

    /// A port nothing is listening on, taken by asking the OS for one and
    /// letting it go. Racy in principle and the alternative is a fixed port,
    /// which collides with the developer's own database.
    pub(crate) fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("a free port")
            .local_addr()
            .expect("a bound address")
            .port()
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
}
