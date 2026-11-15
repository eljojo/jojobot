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

use sqlx::AssertSqlSafe;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};

mod ids;
mod lifetime;
pub mod mailboxes;
pub mod memory;
pub mod migrate;
mod ordinal;
pub mod requests;
pub mod sessions;
mod start_gate;
pub mod storage_check;

/// **Whether the store refused a write on a rule it enforces**, and the kind of
/// rule if it did: a key held twice, a reference to nothing, a required value left
/// out, a failed check. Every rail's error mapper asks this one question, so the
/// four of them cannot disagree about what a refusal is. The words are the
/// kind and what to change, and nothing of the server's own account, which is
/// logged and never crosses.
pub(crate) fn refused_rule(e: &sqlx::Error) -> Option<&'static str> {
    e.as_database_error().and_then(|db| rule_words(db.kind()))
}

/// **The words for one kind of refused rule**: the kind of value that broke, then
/// what the caller changes. Neither names the value, which is the server's own
/// account and stays in the log.
fn rule_words(kind: sqlx::error::ErrorKind) -> Option<&'static str> {
    match kind {
        sqlx::error::ErrorKind::UniqueViolation => Some(
            "a key held twice: another record already holds that key, so send a different key or \
             edit that record",
        ),
        sqlx::error::ErrorKind::ForeignKeyViolation => Some(
            "a reference to nothing: the thing it points at does not exist, so create that thing \
             first or point at one that does",
        ),
        sqlx::error::ErrorKind::NotNullViolation => {
            Some("a required value left out: the write sent none, so send the missing value")
        }
        sqlx::error::ErrorKind::CheckViolation => Some(
            "a check that failed: a value is outside what its field allows, so send a value the \
             field allows",
        ),
        _ => None,
    }
}

/// The database jojobot serves out of its data directory. Named rather than
/// derived from the directory, so a test's temporary path and the deployed
/// `/var/lib/jojobot/db` address the same database by the same name.
pub(crate) const DATABASE: &str = "jojobot";

/// How long a start waits for another start of the same store to finish before it gives up with
/// an error that names what holds the directory.
const START_OWNERSHIP_WAIT: Duration = Duration::from_secs(120);

/// **The address of the server on `port` of the loopback address**, as the process
/// that serves names it, with the `root` account [`Dolt::start`] explains. The one
/// place it is written: the pre-build connects to a serving process's store by it.
pub(crate) fn server_url(port: u16) -> String {
    format!("mysql://root@127.0.0.1:{port}")
}

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

/// How many ports a test's store start tries before it reports the last one taken.
#[cfg(feature = "testing")]
const PORT_ATTEMPTS: usize = 5;

/// How long a request on the pool a start returns waits for one of its four
/// connections before it fails with "pool timed out". **Sized for serving, not
/// for the readiness poll**: a boot migration, the boundary snapshot or a slow
/// statement can hold all four for seconds, and a request behind them should
/// wait its turn rather than fail. 30 seconds is sqlx's own default and matches
/// [`READY_WITHIN`]; a server that has stopped answering fails the caller at
/// that point instead of after 200 ms.
const SERVE_ACQUIRE_WITHIN: Duration = Duration::from_secs(30);

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
    /// It ran and never answered. `last` is the store's last answer to the
    /// poll and `stderr` the tail of the server's own log, so the cause is in
    /// the error rather than only in a log somebody has to find.
    #[error(
        "the store's server started and did not answer within {within:?}; last answer: {last}; server log: {stderr}"
    )]
    NeverReady {
        /// How long the poll waited.
        within: Duration,
        /// The last answer the poll got, or nothing when no attempt failed.
        last: String,
        /// The last lines of the server's stderr, bounded — see [`StderrTail`].
        stderr: String,
    },
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
    /// Turns true when the reader has seen the end of the stream.
    ended: tokio::sync::watch::Receiver<bool>,
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
        let (ended_tx, ended) = tokio::sync::watch::channel(false);
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
            let _ = ended_tx.send(true);
        });
        StderrTail { lines, ended }
    }

    /// **Wait for the reader to reach the end of the stream**, so a snapshot
    /// taken as the child exits holds its last words and not the ones read so
    /// far. Bounded, because a descendant that kept the pipe open would
    /// otherwise hold the caller.
    async fn drained(&self) -> &Self {
        let mut ended = self.ended.clone();
        let _ = tokio::time::timeout(Duration::from_secs(2), ended.wait_for(|done| *done)).await;
        self
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
    /// The port the server took, which a test's start may have replaced.
    port: u16,
    /// What the server wrote to its own stderr, the last lines of it, kept for
    /// as long as the store is up. A refused statement is answered with the
    /// store's words kept out (rule 53), and this is where a test reads them.
    stderr: StderrTail,
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
        Self::start_acquiring_within(data_dir, port, POLL_EVERY * 4).await
    }

    /// **The same start, with the window a connection may wait for the pool
    /// named by the caller.** [`start`](Self::start) passes the window it always
    /// had. A case that is about something other than how fast a pool hands out
    /// a connection passes a longer one, so a loaded machine cannot fail it for
    /// a reason it does not assert on.
    pub(crate) async fn start_acquiring_within(
        data_dir: &Path,
        port: u16,
        acquire_within: Duration,
    ) -> Result<Self, StartError> {
        // **A port a test was handed is replaced when another server takes it first.**
        // The port is free when it is claimed and not bound until the store binds
        // it, and on a busy machine another process can take it in between. The
        // start then reads `PortTaken` on a case that asserts nothing about ports.
        // Only a port this process was handed by the shared arbiter is replaced, so
        // a configured port still refuses, and the production build has no arbiter.
        #[cfg(feature = "testing")]
        {
            let mut port = port;
            for _ in 1..PORT_ATTEMPTS {
                match Self::start_once(data_dir, port, acquire_within).await {
                    Err(StartError::PortTaken(taken))
                        if jojobot_ports::is_claimed_for_life(taken) =>
                    {
                        port = jojobot_ports::claim_for_life()
                            .map_err(|refusal| StartError::Spawn(refusal.to_string()))?;
                    }
                    done => return done,
                }
            }
            Self::start_once(data_dir, port, acquire_within).await
        }
        #[cfg(not(feature = "testing"))]
        Self::start_once(data_dir, port, acquire_within).await
    }

    /// **One start on exactly this port**: a port another server holds is the error
    /// [`StartError::PortTaken`], whatever claimed it.
    pub(crate) async fn start_once(
        data_dir: &Path,
        port: u16,
        acquire_within: Duration,
    ) -> Result<Self, StartError> {
        // **A test that starts a store has a log sink installed**, so a refusal
        // the store makes later has somewhere to be written. The error a
        // caller sees keeps the store's words out (rule 53) and the log is the
        // only place they are; with no subscriber the event is dropped.
        #[cfg(test)]
        let _ = crate::log_capture::log_sink();
        let _place = start_gate::process_gate().enter().await;
        let database = data_dir.join(DATABASE);
        std::fs::create_dir_all(&database).map_err(|e| StartError::DataDir {
            path: database.clone(),
            why: e.to_string(),
        })?;
        // **Held until this start returns**, so the check for a server and the collection that
        // follows cannot interleave with another start of the same store.
        let _owning = Self::own_the_start(&database, START_OWNERSHIP_WAIT).await?;
        let home = Self::server_home(data_dir)?;
        let held_a_store = database.join(".dolt").exists();
        Self::init_if_empty(&database, &home)?;
        // **A store that existed is collected before the server is spawned**, while nothing
        // holds it: every lifecycle passes a start, and a crash skips a stop. A store this
        // start just made has nothing to collect.
        if held_a_store {
            Self::collect_garbage(&database, &home).await;
        }

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
        lifetime::lead_a_group(&mut spawning);
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
        let server = server_url(port);
        let pool = Self::once_answering(
            &format!("{server}/{DATABASE}"),
            &mut child,
            &stderr,
            acquire_within,
            READY_WITHIN,
        )
        .await?;
        Self::prove_it_is_ours(&pool, data_dir, port).await?;
        Ok(Dolt {
            child,
            pool,
            server,
            port,
            stderr,
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
        acquire_within: Duration,
        within: Duration,
    ) -> Result<MySqlPool, StartError> {
        let deadline = std::time::Instant::now() + within;
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            // **The child's exit is watched while an attempt is in flight.** An
            // attempt against a listener that accepts and never greets waits out
            // the whole window, and an exit read only before it would be read
            // too late: a server that could not take its port would end as one
            // that never answered, with its reason unread.
            let attempt = Self::attempt(url, acquire_within);
            tokio::pin!(attempt);
            let outcome = tokio::select! {
                biased;
                exit = child.wait() => Err(exit),
                answer = &mut attempt => Ok(answer),
            };
            match outcome {
                Ok(Ok(())) => {
                    // **An answer, even from a child that has exited.**
                    // Two processes can pick the same free port at the
                    // same instant; the loser's child dies, and the
                    // winner is still what answers here. This is not a
                    // failure to report — it is `prove_it_is_ours`'s
                    // question, and reserving `PortTaken` for its
                    // corroborated answer is why this call does not mint
                    // it too.
                    //
                    // **The pool that polled is not the pool that serves.**
                    // Its short window suits a poll that asks again in 50 ms,
                    // and a request queued behind four busy connections
                    // would fail after the same 200 ms. The server is known
                    // to answer, so the serving pool opens lazily.
                    return MySqlPoolOptions::new()
                        .max_connections(4)
                        .acquire_timeout(SERVE_ACQUIRE_WITHIN)
                        .connect_lazy(url)
                        .map_err(|e| StartError::Spawn(e.to_string()));
                }
                Ok(Err(why)) => last = why,
                // **The child is gone.** A lost port race would answer on the
                // port all the same, against the winner, so the port is asked
                // once more before the exit is reported. Reaching the end with
                // nothing answering means the child exited for a reason of its
                // own, and polling a process that is not there any longer would
                // only spend the rest of `READY_WITHIN` to say `NeverReady`,
                // which is honest about nothing having answered and silent
                // about the crash that guaranteed it never would.
                Err(exit) => {
                    let status = exit.map_err(|e| StartError::Spawn(e.to_string()))?;
                    if Self::attempt(url, POLL_EVERY * 4).await.is_ok() {
                        return MySqlPoolOptions::new()
                            .max_connections(4)
                            .acquire_timeout(SERVE_ACQUIRE_WITHIN)
                            .connect_lazy(url)
                            .map_err(|e| StartError::Spawn(e.to_string()));
                    }
                    return Err(StartError::Exited {
                        status,
                        stderr: stderr.drained().await.snapshot(),
                    });
                }
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
        tracing::error!(
            error = %last,
            stderr = %stderr.snapshot(),
            "the store's server never answered"
        );
        Err(StartError::NeverReady {
            within,
            last,
            stderr: stderr.snapshot(),
        })
    }

    /// **One attempt to reach the server**: connect and ask `SELECT 1`, within
    /// `acquire_within`. The error is the store's account of why not.
    async fn attempt(url: &str, acquire_within: Duration) -> Result<(), String> {
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(acquire_within)
            .connect(url)
            .await
            .map_err(|e| e.to_string())?;
        let asked = sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string());
        pool.close().await;
        asked
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
        sqlx::raw_sql(AssertSqlSafe(format!("CREATE DATABASE `{marker}`")))
            .execute(pool)
            .await
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        let ours = data_dir.join(&marker).exists();
        // Cleared whether or not it landed here: on somebody else's server this
        // is tidying up after a database we should never have made.
        let _ = sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE `{marker}`")))
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

    /// **Whether a server serves the store whose data directory this is.** The server writes
    /// `pid:port:secret` to `.dolt/sql-server.info` in its data directory, and the command line
    /// reads that file to find a live server and run its commands ON it. The file outlives a
    /// server that was killed, so it is not the answer alone: a server serves when its process is
    /// there or its port answers. **A file that cannot be read or understood counts as serving**,
    /// because the cost of being wrong the other way is an online collection.
    fn a_server_serves(data_dir: &Path) -> bool {
        let text = match std::fs::read_to_string(data_dir.join(".dolt").join("sql-server.info")) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(_) => return true,
        };
        let mut parts = text.trim().split(':');
        let (Some(pid), Some(port)) = (
            parts.next().and_then(|part| part.parse::<u32>().ok()),
            parts.next().and_then(|part| part.parse::<u16>().ok()),
        ) else {
            return true;
        };
        Path::new(&format!("/proc/{pid}")).exists()
            || std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                Duration::from_millis(500),
            )
            .is_ok()
    }

    /// **Whether a server serves the database directory, as the command line would find it.**
    /// The pinned command walks up from the database directory and reads the first
    /// `.dolt/sql-server.info` it meets, so every directory from the database to the root is
    /// read here, the database's own included.
    ///
    /// **The guard walks the physical path.** Go's Getwd can instead accept a logical absolute
    /// PWD that names the working directory. The GC spawn pins PWD to this same physical path,
    /// so a database reached through a link is guarded where the link leads.
    fn a_server_serves_over(database: &Path) -> bool {
        let physical = database
            .canonicalize()
            .unwrap_or_else(|_| database.to_path_buf());
        physical.ancestors().any(Self::a_server_serves)
    }

    /// **Own the start of the store at this database directory.** An advisory lock on a file
    /// beside the database's physical location, held until the start returns, so two starts of
    /// one store, in this process or another and through any path to it, cannot both find no
    /// server and then collect while the other spawns. The wait for it ends with an error that
    /// names what holds the directory. A server another tool started is outside this lock.
    async fn own_the_start(database: &Path, wait: Duration) -> Result<std::fs::File, StartError> {
        let physical = database
            .canonicalize()
            .unwrap_or_else(|_| database.to_path_buf());
        let beside = physical.parent().unwrap_or(&physical);
        let path = beside.join(".jojobot-start.lock");
        let fail = |why: String| StartError::DataDir {
            path: path.clone(),
            why,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|error| fail(error.to_string()))?;
        let until = std::time::Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(error)) => return Err(fail(error.to_string())),
            }
            if std::time::Instant::now() >= until {
                return Err(fail("another start holds this directory".to_string()));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// **Wait up to `grace` for a server that is going away**, and say whether nothing serves
    /// the store by then. A server that was killed, or is draining its stop, holds its process
    /// for a moment after it stops answering, and a start that follows at once would otherwise
    /// skip its collection and warn about a server that is about to be gone.
    async fn nothing_serves_within(database: &Path, grace: Duration) -> bool {
        let until = std::time::Instant::now() + grace;
        loop {
            if !Self::a_server_serves_over(database) {
                return true;
            }
            if std::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// **Run a command to its end or to a deadline.** The command is killed when the deadline
    /// passes, because its future is dropped and the command was spawned to die with it. None is
    /// the deadline; Some is how the command ended.
    /// This kills only the direct child: it neither kills a process group nor waits for reaping
    /// before returning to the server spawn. Descendants can outlive the deadline.
    /// Documentation only: there is no new behaviour to redden here.
    async fn run_within(
        deadline: Duration,
        command: &mut tokio::process::Command,
    ) -> Option<std::io::Result<std::process::Output>> {
        tokio::time::timeout(deadline, command.output()).await.ok()
    }

    /// **Collect what no commit or table refers to, over the stopped store**, and say what it
    /// cost on a boot line of its own, in the form every boot step uses. Every write leaves
    /// chunks nothing refers to. The server reports a background collection on and does not run
    /// one, so without this a store's tree grows to several times what it holds. The command
    /// line's collection never blocks a server's writes or breaks its connections, because no
    /// server is running. **It runs only when nothing serves the store** ([`Self::a_server_serves`]):
    /// the command finds a live server by itself and would run the collection on it. **It cannot
    /// fail the start**: a store that is not collected still serves, so a skip, a failure and a
    /// pass of its deadline are each a warning that says what happened. The command is killed
    /// with this process and when the deadline passes, as the server's spawn is. Returns whether
    /// the collection ran to its end.
    async fn collect_garbage(database: &Path, home: &Path) -> bool {
        /// A collection that takes longer than this is killed and the start goes on.
        const COLLECTION_DEADLINE: Duration = Duration::from_secs(120);
        Self::collect_garbage_within(database, home, COLLECTION_DEADLINE).await
    }

    async fn collect_garbage_within(database: &Path, home: &Path, deadline: Duration) -> bool {
        /// How long a server that is going away is waited for before the collection is skipped.
        const SERVER_GRACE: Duration = Duration::from_secs(10);
        /// The bytes under `root`, and the entries that could not be read.
        fn tree_bytes(root: &Path) -> (u64, u64) {
            let (mut total, mut unreadable) = (0, 0);
            let mut pending = vec![root.to_path_buf()];
            while let Some(directory) = pending.pop() {
                let Ok(entries) = std::fs::read_dir(&directory) else {
                    unreadable += 1;
                    continue;
                };
                for entry in entries.flatten() {
                    match entry.metadata() {
                        Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                        Ok(meta) => total += meta.len(),
                        Err(_) => unreadable += 1,
                    }
                }
            }
            (total, unreadable)
        }
        let began = std::time::Instant::now();
        if !Self::nothing_serves_within(database, SERVER_GRACE).await {
            let elapsed_ms = began.elapsed().as_millis() as u64;
            tracing::warn!(
                step = "store-collection",
                database = %database.display(),
                elapsed_ms,
                grace_ms = elapsed_ms,
                walk_ms = 0_u64,
                gc_ms = 0_u64,
                grace_s = SERVER_GRACE.as_secs(),
                "store: a server still serves this store; the collection is skipped"
            );
            return false;
        }
        let grace_ms = began.elapsed().as_millis() as u64;
        let walking = std::time::Instant::now();
        let (before, _) = tree_bytes(database);
        let first_walk_ms = walking.elapsed().as_millis() as u64;
        let mut collecting = tokio::process::Command::new("dolt");
        collecting
            .env("DOLT_ROOT_PATH", home)
            .arg("gc")
            .current_dir(database)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Ok(physical) = database.canonicalize() {
            collecting.env("PWD", physical);
        } else {
            collecting.env_remove("PWD");
        }
        die_with_this_process(&mut collecting);
        let collecting_since = std::time::Instant::now();
        let done = Self::run_within(deadline, &mut collecting).await;
        let gc_ms = collecting_since.elapsed().as_millis() as u64;
        let elapsed_ms = began.elapsed().as_millis() as u64;
        match done {
            Some(Ok(done)) if done.status.success() => {
                let walking = std::time::Instant::now();
                let (after, unreadable) = tree_bytes(database);
                let walk_ms = first_walk_ms + walking.elapsed().as_millis() as u64;
                tracing::info!(
                    step = "store-collection",
                    database = %database.display(),
                    elapsed_ms = began.elapsed().as_millis() as u64,
                    grace_ms,
                    walk_ms,
                    gc_ms,
                    bytes_before = before,
                    bytes_after = after,
                    unreadable_entries = unreadable,
                    "boot step finished"
                );
                true
            }
            Some(Ok(done)) => {
                tracing::warn!(
                    step = "store-collection",
                    database = %database.display(),
                    elapsed_ms,
                    grace_ms,
                    walk_ms = first_walk_ms,
                    gc_ms,
                    error = %String::from_utf8_lossy(&done.stderr).trim(),
                    "store: the collection failed; the store starts as it is"
                );
                false
            }
            Some(Err(error)) => {
                tracing::warn!(
                    step = "store-collection",
                    database = %database.display(),
                    elapsed_ms,
                    grace_ms,
                    walk_ms = first_walk_ms,
                    gc_ms,
                    %error,
                    "store: the collection could not run; the store starts as it is"
                );
                false
            }
            None => {
                tracing::warn!(
                    step = "store-collection",
                    database = %database.display(),
                    elapsed_ms,
                    grace_ms,
                    walk_ms = first_walk_ms,
                    gc_ms,
                    deadline_s = deadline.as_secs(),
                    "store: the collection passed its deadline and was stopped; the store starts as it is"
                );
                false
            }
        }
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
        self.database_with(name, 4).await
    }

    /// [`database`](Self::database) with a pool of this many connections, for a
    /// case about what a caller does when it holds all of them.
    pub async fn database_with(
        &self,
        name: &str,
        max_connections: u32,
    ) -> Result<MySqlPool, StartError> {
        sqlx::query(AssertSqlSafe(format!(
            "CREATE DATABASE IF NOT EXISTS `{name}`"
        )))
        .execute(&self.pool)
        .await
        .map_err(|e| StartError::Spawn(e.to_string()))?;
        MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(&format!("{}/{name}", self.server))
            .await
            .map_err(|e| StartError::Spawn(e.to_string()))
    }

    /// The connection pool, for the two adapters that speak to this store.
    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    /// **The port this store serves on.** A test's start replaces a port another
    /// server took, so a caller that hands the port to a second consumer reads it
    /// here and not from the number it asked for.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop the server after the drain: asked, then killed after a grace
    /// ([`lifetime::stop`]). `Drop` and the kernel cover the endings that never get here.
    pub async fn stop(&mut self) {
        lifetime::stop(&self.pool, &mut self.child, lifetime::STOP_GRACE).await;
    }

    /// **What the server itself has said on stderr, the last lines of it.** For a
    /// test or a person reading why a statement was refused; nothing served to
    /// an agent reads it.
    pub fn server_stderr(&self) -> String {
        self.stderr.snapshot()
    }
}

/// **What a failing run should show of the server's own words**, or nothing when
/// the run is not failing or the server said nothing. Separate from the drop so
/// it can be read without a panic.
#[cfg(any(test, feature = "testing"))]
fn the_servers_words_for_a_failure(failing: bool, tail: &str) -> Option<String> {
    (failing && !tail.is_empty()).then(|| format!("the store's own stderr, last lines:\n{tail}"))
}

/// **A test that fails with a store up shows what the store's server said.** The
/// harness prints what a failed test wrote to stderr beside its panic, so a
/// refused statement comes with the server's side of it, which the error a
/// caller gets leaves out. A store dropped on the way to a pass says nothing.
#[cfg(any(test, feature = "testing"))]
impl Drop for Dolt {
    fn drop(&mut self) {
        if let Some(words) =
            the_servers_words_for_a_failure(std::thread::panicking(), &self.stderr.snapshot())
        {
            eprintln!("{words}");
        }
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
    /// It had finished on this store at an earlier boot, so this boot read
    /// none of its table.
    Settled,
}

/// **The one-time passes over stored rows, by the name each logs under and
/// each marker carries.** The order is the order they run in.
///
/// **A pass runs once on a store, and a marker says it has.** Rule 260 resolves
/// a mention on the way in; it never rewrites history. So once a pass has
/// finished, old text that named a handle nothing wore when it was written stays
/// text: its writer could not have meant a thing that did not exist yet, and
/// linking it to whatever takes the name later would be a guess. A pass that
/// read its table on every boot would make exactly that guess, and would also
/// pay for the whole table at every start. Do not return to a read on every
/// boot to catch such a row.
const PASSES: [&str; 4] = [
    "mail mentions",
    "session mentions",
    "session bot column",
    "message sender column",
];

mod store_passes;
pub(crate) use store_passes::{finish_pass, finished_passes, finished_passes_checked};

/// **A pass's outcome, with its marker written when the pass finished.**
/// `Ran` means every row was examined and none failed; anything else leaves the
/// pass owed for the next boot.
async fn settled<E: std::fmt::Display>(
    pool: &MySqlPool,
    name: &str,
    ran: Result<usize, E>,
) -> Migrated {
    match ran {
        Ok(n) => {
            finish_pass(pool, name).await;
            Migrated::Ran(n)
        }
        Err(e) => Migrated::Failed(e.to_string()),
    }
}

/// **The one-time pass that gives a thing whose schedule cannot be read the day it
/// became late**, by the name it logs under and its marker carries.
///
/// The owed read selects on the stored due day alone, and a thing that is late
/// because its schedule cannot be read used to store none, so such a thing never
/// reached the read. A write now stores the day it leaves such a thing late; this
/// reaches the ones that were left before. It is separate from the permanent-id
/// passes because it writes through the memory store and not through a table.
pub const UNREADABLE_DUE_PASS: &str = "unreadable due day";

/// **Stamp `today` as the due day of every thing that is owed and cannot say
/// when**, once, and mark the pass finished.
///
/// A thing is stamped only when the same arithmetic a write runs
/// ([`jojobot_domain::attention::moved_due_moment_as`]) would store a day for
/// it: its carrier answers `Unreadable`, it is not finished, and it holds no
/// stamp for that day already. A thing that is archived or folded into another
/// is out of every browse and is left alone. The stamp is a claim of its own,
/// written as jojobot's inference, so no earlier claim is changed.
///
/// **A pass that finished on this store reads nothing**, like the passes above:
/// a thing that becomes unreadable afterwards is stamped by the write that made
/// it so. A thing that could not be read or stamped leaves the pass owed, and
/// the next boot runs it again; it is idempotent, because a thing stamped for
/// `today` is not stamped twice and a stamp for another day is replaced by the
/// one the next write makes.
// LEGACY_READER(unreadable-loop): work:q-gate-binary-coverage
pub async fn backfill_unreadable_due_days(
    pool: &MySqlPool,
    memory: &dyn jojobot_domain::memory::Memory,
    today: jiff::civil::Date,
) -> Migrated {
    use jojobot_domain::attention::{self, Carrier, Due, DueMove};
    if finished_passes(pool).await.contains(UNREADABLE_DUE_PASS) {
        return Migrated::Settled;
    }
    let entities = match memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(e) => return Migrated::Skipped(e.to_string()),
    };
    let carriers = attention::shipped();
    let asked: Vec<&dyn Carrier> = carriers.iter().map(AsRef::as_ref).collect();
    // What the operator mapped their status words to: a word mapped to `done`
    // finishes a work item, so the pass reads the same record a write reads.
    // A record nobody wrote maps nothing. A record that cannot be read is judged by
    // the function a write over it uses: a thing the unread words cannot change is
    // stamped as the readable pass would stamp it, and a work item or project at a
    // word of the operator's own is left, with the pass owed, because judging it
    // against no mapping would stamp a day on work the operator's words finished
    // and the marker would keep it (rule 130, rule 598).
    let instance = match memory
        .fields(&jojobot_domain::memory::EntityId(
            jojobot_domain::memory::INSTANCE_RECORD.to_string(),
        ))
        .await
    {
        Ok(fields) => Ok(fields),
        Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }) => Ok(Default::default()),
        Err(e) => Err(format!("the instance's record: {e}")),
    };
    let mut waiting = 0usize;
    let mut stamped = 0usize;
    let mut failed = Vec::new();
    for entity in entities
        .iter()
        .filter(|e| e.archived.is_none() && !e.left_out_as_folded())
    {
        // The record itself is the one that could not be read: it carries no schedule,
        // and reading it again here would report the same fault a second time.
        if instance.is_err() && entity.id.as_str() == jojobot_domain::memory::INSTANCE_RECORD {
            continue;
        }
        let fields = match memory.fields(&entity.id).await {
            Ok(fields) => fields,
            Err(e) => {
                failed.push(format!("{}: {e}", entity.id));
                continue;
            }
        };
        let kind = entity.kind.as_token();
        let existing = fields
            .get(attention::DUE_ON)
            .and_then(|held| held.trim().parse().ok());
        let moved = match &instance {
            Ok(instance) => {
                if !matches!(
                    attention::owed_as(kind, &asked, &fields, instance),
                    Due::Unreadable
                ) {
                    continue;
                }
                attention::moved_due_moment_as(
                    kind, &asked, existing, &fields, &fields, instance, today,
                )
            }
            Err(_) => {
                if !matches!(attention::owed(&asked, &fields), Due::Unreadable) {
                    continue;
                }
                match attention::moved_due_moment_under_unread_mapping(
                    kind, &asked, existing, &fields, &fields, today,
                ) {
                    Some(moved) => moved,
                    None => {
                        waiting += 1;
                        continue;
                    }
                }
            }
        };
        let DueMove::Set(day) = moved else {
            continue;
        };
        let stamp = jojobot_domain::memory::NewFact {
            provenance: jojobot_domain::memory::Provenance::Inference,
            fields: std::iter::once((attention::DUE_ON.to_string(), day.to_string())).collect(),
            // The day is jojobot's own arithmetic, so it reaches the thing.
            derived: std::iter::once(attention::DUE_ON.to_string()).collect(),
            ..jojobot_domain::memory::NewFact::about(
                entity.id.clone(),
                "jojobot stamped the day this became late: its schedule cannot be read",
                today,
            )
        };
        match memory.capture(stamp).await {
            Ok(jojobot_domain::memory::Guarded::Written(_)) => stamped += 1,
            Ok(_) => failed.push(format!("{}: the store refused the stamp", entity.id)),
            Err(e) => failed.push(format!("{}: {e}", entity.id)),
        }
    }
    if !failed.is_empty() {
        return Migrated::Failed(failed.join("; "));
    }
    match instance {
        Err(e) if waiting > 0 => Migrated::Skipped(format!(
            "{e}; {waiting} things at a status word of the operator's own wait on it"
        )),
        _ => settled(pool, UNREADABLE_DUE_PASS, Ok::<usize, String>(stamped)).await,
    }
}

/// **The one-time pass that carries a role claimed in the old shape onto its
/// role object**, by the name it logs under and its marker carries.
///
/// A role used to keep its holder and claim moment on the bot as
/// `role/<role>/holder` and `role/<role>/claimed_at`. Nothing reads those keys
/// as a lease any more, so a store restored from before the change would seat a
/// second session in every role held that way. This puts each such claim on its
/// role object and takes the keys off the bot. It writes through the memory
/// store and not through a table, like the due-day pass above.
pub const OLD_ROLE_CLAIMS_PASS: &str = "old role claims";

/// **Carry every old-shape role claim onto its role object, once, and mark the
/// pass finished.** See [`jojobot_domain::session::role_keys::move_old_role_claims`]
/// for what it moves and what it leaves.
///
/// **A pass that finished on this store reads nothing.** A role that could not
/// be finished leaves the pass owed and its keys in place, so the next boot runs
/// it again; it is idempotent, because a role that moved has no keys left to
/// move.
// LEGACY_READER(half-moved-role-claims): work:q-gate-binary-coverage
pub async fn move_old_role_claims_once(
    pool: &MySqlPool,
    memory: &dyn jojobot_domain::memory::Memory,
    today: jiff::civil::Date,
) -> Migrated {
    if finished_passes(pool).await.contains(OLD_ROLE_CLAIMS_PASS) {
        return Migrated::Settled;
    }
    match jojobot_domain::session::role_keys::move_old_role_claims(memory, today).await {
        Err(e) => Migrated::Skipped(e.to_string()),
        Ok(pass) if pass.failed.is_empty() => {
            settled(pool, OLD_ROLE_CLAIMS_PASS, Ok::<usize, String>(pass.moved)).await
        }
        Ok(pass) => Migrated::Failed(pass.failed.join("; ")),
    }
}

/// **Run every one-time permanent-id migration mail and sessions still
/// need, over one read of the entity list, and say what actually happened
/// for each — never a false `Ran(0)` standing in for a read that never
/// happened.**
///
/// **A pass that has finished on this store reads none of its table.** The
/// markers are read first, in one small query, and a pass with one is
/// reported [`Migrated::Settled`]. When all four are settled the entity list
/// and the rename history are not read either. A pass is marked only after it
/// examined every row and failed on none, so a skipped or failed pass is owed
/// again at the next boot, and a store that still holds a row written the old
/// way is migrated the first time a pass runs over it.
///
/// `memory.list_entities` failing is not an empty store: feeding an empty
/// `Vec` to a migration either way makes it match nothing and report a
/// completed run over rows it never saw. So every migration still owed that
/// needs the read `Skip`s together when it fails, rather than three separate
/// copies of the same mistake — this is the one place that decision is made.
///
/// `former_handles` failing is narrower: only the bot-column and
/// sender-column migrations need it, so only those two `Skip` when it alone
/// fails.
pub async fn migrate_permanent_ids(
    pool: &MySqlPool,
    memory: &dyn jojobot_domain::memory::Memory,
    mail: &mailboxes::DoltMailboxes,
    sessions: &sessions::DoltSessions,
) -> Vec<(&'static str, Migrated)> {
    let done = finished_passes(pool).await;
    let owed = |name: &str| !done.contains(name);
    let settle = |name: &'static str| (name, Migrated::Settled);
    if PASSES.iter().all(|name| !owed(name)) {
        return PASSES.iter().copied().map(settle).collect();
    }
    let known = match memory.list_entities(None).await {
        Ok(known) => known,
        Err(e) => {
            let reason = e.to_string();
            return PASSES
                .iter()
                .copied()
                .map(|name| match owed(name) {
                    true => (name, Migrated::Skipped(reason.clone())),
                    false => settle(name),
                })
                .collect();
        }
    };
    let mail_mentions = match owed("mail mentions") {
        true => settled(pool, "mail mentions", mail.migrate_mentions(&known).await).await,
        false => Migrated::Settled,
    };
    let session_mentions = match owed("session mentions") {
        true => {
            settled(
                pool,
                "session mentions",
                sessions.migrate_mentions(&known).await,
            )
            .await
        }
        false => Migrated::Settled,
    };
    let columns_owed = owed("session bot column") || owed("message sender column");
    let former = match columns_owed {
        true => Some(memory.former_handles().await),
        false => None,
    };
    let bot_column = match (owed("session bot column"), &former) {
        (false, _) => Migrated::Settled,
        (true, Some(Ok(former))) => {
            settled(
                pool,
                "session bot column",
                sessions.migrate_bot_column(&known, former).await,
            )
            .await
        }
        (true, Some(Err(e))) => Migrated::Skipped(e.to_string()),
        (true, None) => Migrated::Skipped("the rename history was not read".to_string()),
    };
    let sender_column = match (owed("message sender column"), &former) {
        (false, _) => Migrated::Settled,
        (true, Some(Ok(former))) => {
            settled(
                pool,
                "message sender column",
                mail.migrate_sender_column(&known, former).await,
            )
            .await
        }
        (true, Some(Err(e))) => Migrated::Skipped(e.to_string()),
        (true, None) => Migrated::Skipped("the rename history was not read".to_string()),
    };
    vec![
        ("mail mentions", mail_mentions),
        ("session mentions", session_mentions),
        ("session bot column", bot_column),
        ("message sender column", sender_column),
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
/// refuses an empty mark, so what staging staged is counted first and the quiet
/// case stays quiet.
///
/// **The mark stages with `DOLT_ADD('-A')` and commits without `-A`.** Staging skips a
/// table that `dolt_ignore` names, which the store's documentation promises and
/// `DOLT_COMMIT('-A')` does not. A table derived from the log, such as the
/// snapshot's, is named there, so no mark ever holds a copy of it in history. A change to
/// such a table alone stages nothing, so it asks for no commit and logs no warning.
pub(crate) async fn snapshot(pool: &MySqlPool, boundary: &str) {
    if let Err(e) = sqlx::query("CALL DOLT_ADD('-A')").execute(pool).await {
        tracing::warn!(boundary, error = %e, "store: could not stage what has changed");
        return;
    }
    let staged: Result<i64, _> =
        sqlx::query_scalar("SELECT COUNT(*) FROM dolt_status WHERE staged")
            .fetch_one(pool)
            .await;
    match staged {
        Ok(0) => {
            tracing::debug!(boundary, "store: nothing staged since the last mark");
            return;
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(boundary, error = %e, "store: could not read what is staged");
            return;
        }
    }
    if let Err(e) = sqlx::query("CALL DOLT_COMMIT('-m', ?)")
        .bind(boundary)
        .execute(pool)
        .await
    {
        tracing::warn!(boundary, error = %e, "store: the boundary was not marked");
    }
}

/// **The newest stamp the store holds on `day` or after it**, or `None` when
/// nothing in it was stamped from that day on.
///
/// **A later day counts.** A store another process filled can hold a stamp
/// after the day a clock is acting out, and a clock that continued only from
/// the day itself would start behind that stamp.
///
/// **Every column that keeps an instant is read**, found in the schema rather
/// than listed here: an instant is text of 40 or 48 characters, and a column
/// added later is read without anybody remembering to name it. The ledger of
/// applied migrations and the record of finished passes are left out, because
/// each records when the work RAN, on the real clock, and not a moment of the run
/// being acted out. **The two tables of the push outbox are left out for the
/// same reason**: a push is chased in real time, so its window and its retries
/// are counted on the wall clock. **So is the table of snapshots**: a snapshot is
/// stamped with the real moment it was taken.
///
/// **The newest is found by the instant, not by the text.** `…:00.5Z` sorts
/// before `…:00Z` as text and is the later of the two. So each column is asked
/// for its newest second first, and only the rows of that second are parsed.
pub async fn newest_stamp_from(
    pool: &MySqlPool,
    day: jiff::civil::Date,
) -> Result<Option<jiff::Timestamp>, sqlx::Error> {
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT CAST(table_name AS CHAR), CAST(column_name AS CHAR) \
         FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND data_type = 'varchar' \
           AND character_maximum_length IN (40, 48) \
           AND table_name NOT IN ('schema_migration', 'store_pass', 'push_outbox', 'push_attempt', \
                                  'log_snapshot')",
    )
    .fetch_all(pool)
    .await?;
    let from_the_day = format!("{day}T");
    let mut newest: Option<jiff::Timestamp> = None;
    for (table, column) in columns {
        let second: Option<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT MAX(LEFT(`{column}`, 19)) FROM `{table}` WHERE `{column}` >= ?"
        )))
        .bind(&from_the_day)
        .fetch_one(pool)
        .await?;
        let Some(second) = second else { continue };
        let cells: Vec<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT `{column}` FROM `{table}` WHERE LEFT(`{column}`, 19) = ?"
        )))
        .bind(second)
        .fetch_all(pool)
        .await?;
        let latest = cells
            .iter()
            .filter_map(|cell| cell.parse::<jiff::Timestamp>().ok())
            .max();
        newest = newest.max(latest);
    }
    Ok(newest)
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

    /// **Each rule word names the kind of value that broke and what to change**:
    /// `<kind>: <what is wrong>, so <what to change>`. The kinds are the words the
    /// boundary has always served, so they are pinned as written.
    #[test]
    fn every_refused_rule_says_what_broke_and_what_to_change() {
        use sqlx::error::ErrorKind as K;
        let kinds = [
            (K::UniqueViolation, "a key held twice"),
            (K::ForeignKeyViolation, "a reference to nothing"),
            (K::NotNullViolation, "a required value left out"),
            (K::CheckViolation, "a check that failed"),
        ];
        for (kind, said_as) in kinds {
            let words = rule_words(kind).expect("a rule the store enforces has words");
            let (broke, rest) = words
                .split_once(": ")
                .unwrap_or_else(|| panic!("{words:?} has no cause after its kind"));
            assert_eq!(broke, said_as);
            let (cause, change) = rest
                .split_once(", so ")
                .unwrap_or_else(|| panic!("{words:?} names no change"));
            assert!(!cause.is_empty() && !change.is_empty(), "{words:?}");
            assert!(!words.contains('\n'), "{words:?}");
        }
        assert_eq!(rule_words(K::Other), None);
    }

    /// **A failing run shows what the server said, and a passing one stays
    /// quiet.** Either half alone passes on a helper that does the other.
    #[test]
    fn the_servers_words_are_shown_for_a_failure_and_for_nothing_else() {
        let shown = the_servers_words_for_a_failure(true, "table already exists")
            .expect("a failing run with something to show says it");
        assert!(shown.contains("table already exists"), "{shown}");
        assert_eq!(the_servers_words_for_a_failure(false, "a line"), None);
        assert_eq!(the_servers_words_for_a_failure(true, ""), None);
    }

    /// **A store that has come up still holds what its server wrote to stderr.**
    /// The tail used to be read only for a start that failed, and was gone by
    /// the time a statement was refused.
    #[tokio::test]
    async fn a_started_store_keeps_what_its_server_said() {
        let scratch = Scratch::new("server-stderr");
        let mut store = Dolt::start(&scratch.0, crate::testing::free_port())
            .await
            .expect("the store comes up");
        let said = store.server_stderr();
        assert!(
            !said.is_empty(),
            "the server writes a line to stderr as it starts, and none was kept",
        );
        store.stop().await;
    }

    use crate::testing::Scratch;

    /// **A test that panics leaves no scratch directory.** The directory is
    /// there, with a file in it, while the test runs, so a guard that removed
    /// nothing would not pass.
    #[test]
    fn a_scratch_directory_is_gone_after_a_test_that_panics() {
        let seen: std::sync::Mutex<Option<(PathBuf, bool)>> = std::sync::Mutex::new(None);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let scratch = Scratch::new("panics");
            std::fs::write(scratch.0.join("file"), b"x").expect("a file");
            *seen.lock().expect("lock") =
                Some((scratch.0.clone(), scratch.0.join("file").exists()));
            panic!("the test fails");
        }));
        assert!(outcome.is_err());
        let (path, existed) = seen.lock().expect("lock").take().expect("the test ran");
        assert!(existed, "the directory was there while the test ran");
        assert!(!path.exists(), "{} was left behind", path.display());
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

        let (mut first, applied) = Dolt::ready(&path, free_port())
            .await
            .expect("the first boot brings the store up");
        // **Counted from the set rather than written down**, so a new migration
        // does not make this case fail for a reason that has nothing to do with
        // what it is about.
        // **Every version but the ones a fresh start records without issuing.**
        // An earlier migration built what 0090 adds, so it is asked first and
        // found standing.
        let not_listed: Vec<&str> = migrate::MIGRATIONS
            .iter()
            .map(|migration| migration.version)
            .filter(|version| !applied.iter().any(|seen| seen == version))
            .collect();
        assert_eq!(
            not_listed,
            ["0090_session_key_hash_index"],
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

        // **One start on exactly this port**: `start` replaces a port the test was
        // handed, so the refusal is asked of the start that does not.
        let second = Dolt::start_once(&intruder.0, port, POLL_EVERY * 4).await;
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

    /// **A start on a port another server took first comes up on a fresh port.** The
    /// port a case is handed is free when it is claimed and not bound until the
    /// store binds it; on a busy machine another process can take it in between,
    /// and the start then reads `PortTaken` and fails a case that asserts nothing
    /// about ports. A port this process was handed by `free_port` is replaced by
    /// another one, a bounded number of times; the refusal itself stays (the case
    /// above).
    #[tokio::test]
    async fn a_start_on_a_port_another_server_took_comes_up_on_a_fresh_port() {
        let held = Scratch::new("port-retry-held");
        let late = Scratch::new("port-retry-late");
        let port = free_port();
        let mut taker = Dolt::start(&held.0, port)
            .await
            .expect("another server takes the port");

        let mut store = Dolt::start(&late.0, port)
            .await
            .expect("the start takes a fresh port when the one it was given is taken");
        assert_ne!(
            store.port(),
            port,
            "the store serves on the port it replaced"
        );
        assert_eq!(
            taker.port(),
            port,
            "the other server keeps the port it took"
        );
        crate::dolt::migrate::run(store.pool())
            .await
            .expect("the store it came up with answers");

        store.stop().await;
        taker.stop().await;
    }

    /// **A port nobody handed this process is not replaced.** The retry is for the
    /// ports `free_port` hands out; a port somebody configured (here one the
    /// operating system gave, which the arbiter never claimed) still reads
    /// `PortTaken`, so a start that was told where to serve does not move.
    #[tokio::test]
    async fn a_start_on_a_configured_port_another_server_holds_still_refuses() {
        let held = Scratch::new("port-configured-held");
        let late = Scratch::new("port-configured-late");
        let given = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("the system gives a port");
        let port = given.local_addr().expect("it has an address").port();
        drop(given);
        let mut taker = Dolt::start_once(&held.0, port, POLL_EVERY * 4)
            .await
            .expect("another server takes the port");

        let second = Dolt::start(&late.0, port).await;

        assert!(
            matches!(second, Err(StartError::PortTaken(taken)) if taken == port),
            "a configured port that is held must refuse and not move"
        );
        taker.stop().await;
    }

    /// 🚨 **A request waits for one of the four connections instead of failing
    /// after the readiness poll's window.**
    ///
    /// The pool a start hands back serves the boot migrations, the boundary
    /// snapshot and every request, and each of those queues behind the four
    /// connections when they are busy. All four are held for longer than the
    /// poll's window here, and a fifth request made meanwhile must still
    /// complete once one is released. **Both halves**: the held connections
    /// really are all four, so the fifth cannot have used a free one.
    #[tokio::test]
    async fn a_request_waits_for_a_connection_longer_than_the_readiness_poll_does() {
        let scratch = Scratch::new("serving-window");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");

        let mut held = Vec::new();
        for _ in 0..4 {
            held.push(store.pool().acquire().await.expect("a connection is free"));
        }
        assert_eq!(
            store.pool().num_idle(),
            0,
            "all four connections are held, so the fifth request has to wait",
        );
        let pool = store.pool().clone();
        let fifth = tokio::spawn(async move { sqlx::query("SELECT 1").execute(&pool).await });
        tokio::time::sleep(POLL_EVERY * 12).await;
        assert!(
            !fifth.is_finished(),
            "the fifth request is still waiting after the poll's window has passed",
        );
        drop(held);
        fifth
            .await
            .expect("the request task ran")
            .expect("the fifth request completes once a connection is released");

        store.stop().await;
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
            POLL_EVERY * 4,
            READY_WITHIN,
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

    /// **A real server over `data_dir` on `port`, as a start spawns it**, with
    /// its stderr read as it runs. The directory holds one initialised
    /// database, `kept`, so the server has something to serve.
    fn serving(data_dir: &Path, kept: &str, port: u16) -> (tokio::process::Child, StderrTail) {
        let home = Dolt::server_home(data_dir).expect("a home for the server");
        let database = data_dir.join(kept);
        std::fs::create_dir_all(&database).expect("a database directory");
        Dolt::init_if_empty(&database, &home).expect("an initialised database");
        let mut child = tokio::process::Command::new("dolt")
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
            .kill_on_drop(true)
            .spawn()
            .expect("dolt itself must be on PATH to run this suite at all");
        let stderr = StderrTail::capture(child.stderr.take().expect("stderr was piped"));
        (child, stderr)
    }

    /// 🚨 **A server that never answers says what it last answered.** The
    /// server runs and holds its database, and the poll asks a port nothing is
    /// listening on, so every attempt fails and none can be blamed on a missing
    /// database.
    #[tokio::test]
    async fn a_server_that_never_answers_carries_its_last_answer_in_the_error() {
        let scratch = Scratch::new("never-answers");
        let (mut child, stderr) = serving(&scratch.0, "jojobot", free_port());
        let elsewhere = free_port();
        // The server has said something before the poll starts, so the tail
        // the error carries is not empty by luck of timing.
        let started = std::time::Instant::now();
        while stderr.snapshot().is_empty() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the server logs"
            );
            tokio::time::sleep(POLL_EVERY).await;
        }

        let err = Dolt::once_answering(
            &format!("mysql://root@127.0.0.1:{elsewhere}/jojobot"),
            &mut child,
            &stderr,
            POLL_EVERY * 4,
            Duration::from_secs(2),
        )
        .await
        .expect_err("nothing listens where the poll asks");
        match &err {
            StartError::NeverReady {
                within,
                last,
                stderr: log,
            } => {
                assert_eq!(*within, Duration::from_secs(2));
                assert!(!last.is_empty(), "the last answer is kept");
                assert!(!log.is_empty(), "the server's log is kept");
                let shown = err.to_string();
                assert!(shown.contains(last.as_str()), "the error names it: {shown}");
                assert!(shown.contains(log.as_str()), "and the log: {shown}");
            }
            other => panic!("expected NeverReady, got {other:?}"),
        }
    }

    /// 🚨 **A server that dies on a port something else holds is reported as
    /// dead, not as a server that never answered, however long a connection
    /// attempt is allowed to wait.**
    ///
    /// A listener that accepts a connection and never greets is the worst
    /// neighbour a start can have: an attempt against it waits for the server's
    /// greeting and gets none until the pool's window ends. With the window a
    /// store is given to answer in (`start_unhurried`'s, thirty seconds) that one
    /// attempt used the whole of `READY_WITHIN`, and the child's exit was read
    /// only before it, so the start ended `NeverReady` with the real reason, that
    /// the child could not take the port, unread.
    ///
    /// The listener is a plain socket of this case's own, and the server is the
    /// real one on its port. No timing is asserted: the start has to return the
    /// child's exit, whatever the window.
    #[tokio::test]
    async fn a_child_that_dies_on_a_held_port_is_reported_whatever_the_attempt_window() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port to hold");
        let port = held.local_addr().expect("its address").port();
        let scratch = Scratch::new("hung-listener");

        let err = Dolt::start_acquiring_within(&scratch.0, port, READY_WITHIN)
            .await
            .map(|_| ())
            .expect_err("a server cannot take a port something else holds");

        match err {
            StartError::Exited { status, .. } => {
                assert!(!status.success(), "the child failed to bind: {status}");
            }
            other => panic!("expected the child's exit, got {other:?}"),
        }
        drop(held);
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

    /// **A mark stages what the commits hold and commits only what it staged**, so a table that
    /// `dolt_ignore` names is in no commit, and a change to it alone makes no mark and no warning.
    ///
    /// The mark stages with `DOLT_ADD('-A')`, which skips an ignored table by its documentation,
    /// counts the staged rows and commits without `-A`. `DOLT_COMMIT('-A')` makes no such promise
    /// about an ignored table. Three boundaries: the setup, a change to the ignored table alone,
    /// and a change to it beside a change to a table the commits hold.
    #[tokio::test]
    async fn a_mark_keeps_an_ignored_table_out_of_every_commit_and_makes_none_for_it_alone() {
        let scratch = Scratch::new("mark-ignored");
        let (mut store, _) = Dolt::ready(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = store.pool().clone();
        for statement in [
            "INSERT INTO dolt_ignore VALUES ('derived_copy', true)",
            "CREATE TABLE derived_copy (slot VARCHAR(16) PRIMARY KEY, body LONGBLOB NOT NULL)",
            "CREATE TABLE kept_beside (id BIGINT PRIMARY KEY, v BIGINT NOT NULL)",
        ] {
            sqlx::query(statement).execute(&pool).await.expect("setup");
        }
        snapshot(&pool, "setup-1f7a").await;
        let after_setup = marks(&pool).await.len();

        sqlx::query("REPLACE INTO derived_copy VALUES ('s', x'01')")
            .execute(&pool)
            .await
            .expect("the ignored table changes");
        snapshot(&pool, "alone-1f7a").await;
        assert_eq!(
            marks(&pool).await.len(),
            after_setup,
            "a change to the ignored table alone made a mark"
        );

        sqlx::query("REPLACE INTO derived_copy VALUES ('s', x'02')")
            .execute(&pool)
            .await
            .expect("the ignored table changes");
        sqlx::query("REPLACE INTO kept_beside VALUES (1, 1)")
            .execute(&pool)
            .await
            .expect("the kept table changes");
        snapshot(&pool, "beside-1f7a").await;
        let after_beside = marks(&pool).await;
        assert_eq!(after_beside.len(), after_setup + 1, "{after_beside:?}");
        assert_eq!(after_beside[0], "beside-1f7a");

        let commits: Vec<String> = sqlx::query_scalar("SELECT commit_hash FROM dolt_log")
            .fetch_all(&pool)
            .await
            .expect("the log reads");
        for commit in &commits {
            let read = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM derived_copy AS OF '{commit}'"
            )))
            .fetch_one(&pool)
            .await;
            assert!(read.is_err(), "the commit {commit} holds the ignored table");
        }
        let said = crate::log_capture::log_sink().text();
        assert!(
            !said
                .lines()
                .any(|line| line.contains("WARN") && line.contains("1f7a")),
            "a boundary of this case was warned about"
        );
        store.stop().await;
    }

    /// **A start collects the garbage a store has piled up, before the server spawns.** Every
    /// write leaves chunks nothing refers to, and the background collection the server reports
    /// on does not run, so the store's tree grows far past what it holds. Four changed copies
    /// of one value are written, the store is stopped and started again: the tree must come out
    /// about one copy large and the value must read back whole. The boot line the collection
    /// logs is read through the binary, in `real_binary_collects_a_stored_stores_garbage`.
    #[tokio::test]
    async fn a_start_collects_the_garbage_of_the_store_before_it_spawns_the_server() {
        fn tree_bytes(root: &Path) -> u64 {
            let mut total = 0;
            let mut pending = vec![root.to_path_buf()];
            while let Some(directory) = pending.pop() {
                for entry in std::fs::read_dir(&directory)
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let meta = entry.metadata().expect("metadata reads");
                    if meta.is_dir() {
                        pending.push(entry.path());
                    } else {
                        total += meta.len();
                    }
                }
            }
            total
        }
        fn unpredictable(seed: u64, len: usize) -> Vec<u8> {
            let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    (x >> 24) as u8
                })
                .collect()
        }
        let scratch = Scratch::new("start-collects");
        let port = free_port();
        let mut store = Dolt::start(&scratch.0, port)
            .await
            .expect("the store comes up");
        let pool = store.pool().clone();
        sqlx::query("CREATE TABLE heap (id BIGINT PRIMARY KEY, body LONGBLOB NOT NULL)")
            .execute(&pool)
            .await
            .expect("the table is created");
        let mut last = Vec::new();
        for round in 0..4u64 {
            last = unpredictable(round + 1, 8 << 20);
            sqlx::query("REPLACE INTO heap VALUES (1, ?)")
                .bind(&last)
                .execute(&pool)
                .await
                .expect("a copy is written");
        }
        drop(pool);
        store.stop().await;
        let piled = tree_bytes(&scratch.0);

        let mut again = Dolt::start(&scratch.0, port)
            .await
            .expect("it starts again");
        let kept: Vec<u8> = sqlx::query_scalar("SELECT body FROM heap WHERE id = 1")
            .fetch_one(again.pool())
            .await
            .expect("the value reads back");
        assert!(kept == last, "the collection changed the value it kept");
        again.stop().await;
        let collected = tree_bytes(&scratch.0);
        assert!(
            collected < piled / 2,
            "the tree was {piled} bytes and is {collected} after the start"
        );
    }

    #[tokio::test]
    async fn the_collection_elapsed_line_includes_the_server_grace() {
        use tracing::instrument::WithSubscriber;

        let scratch = Scratch::new("collection-elapsed");
        let database = scratch.0.join(DATABASE);
        std::fs::create_dir_all(&database).expect("the database directory");
        let home = Dolt::server_home(&scratch.0).expect("the home");
        Dolt::init_if_empty(&database, &home).expect("the database is initialized");
        // The same live-process metadata shape the ancestry guard reads. Removing it
        // models the end of a draining server without starting another server.
        let info = database.join(".dolt/sql-server.info");
        std::fs::write(
            &info,
            format!("{}:{}:secret", std::process::id(), free_port()),
        )
        .expect("the server metadata");
        assert!(Dolt::a_server_serves_over(&database));

        let captured = crate::log_capture::Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();
        let (ran, ()) = async {
            tokio::join!(Dolt::collect_garbage(&database, &home), async {
                tokio::time::sleep(Duration::from_millis(350)).await;
                std::fs::remove_file(&info).expect("the draining server lets go");
            })
        }
        .with_subscriber(subscriber)
        .await;
        assert!(ran, "the offline collection completes");
        let said = captured.text();
        let line = said
            .lines()
            .find(|line| line.contains("step=\"store-collection\""))
            .unwrap_or_else(|| panic!("no collection boot line: {said}"));
        let elapsed: u64 = line
            .split("elapsed_ms=")
            .nth(1)
            .expect("the elapsed field")
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .expect("elapsed milliseconds");
        let gc: u64 = line
            .split("gc_ms=")
            .nth(1)
            .expect("the GC duration field")
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            elapsed >= gc + 300,
            "collection elapsed excludes the 350 ms grace: {line}"
        );
    }

    #[tokio::test]
    async fn the_collection_gc_child_uses_the_guarded_physical_pwd() {
        const CASE: &str = "dolt::tests::the_collection_gc_child_uses_the_guarded_physical_pwd";
        // Reusing the test binary as `dolt` observes the actual GC spawn's environment.
        // Its `gc` argument selects this case; the executable name selects the observer.
        if std::env::args()
            .next()
            .is_some_and(|arg| Path::new(&arg).ends_with("dolt"))
        {
            match std::env::var("COLLECTION_EXIT").as_deref() {
                Ok("failed") => std::process::exit(7),
                Ok("deadline") => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return;
                }
                _ => {}
            }
            std::fs::write(
                std::env::var_os("COLLECTION_PWD_OUTPUT").unwrap(),
                std::env::var("PWD").expect("the child PWD"),
            )
            .expect("record the child environment");
            return;
        }
        if let Some(database) = std::env::var_os("COLLECTION_PWD_DATABASE") {
            let database = PathBuf::from(database);
            let physical = database.canonicalize().unwrap();
            assert_ne!(std::env::var_os("PWD").unwrap(), physical.as_os_str());
            assert_eq!(
                PathBuf::from(std::env::var_os("PWD").unwrap())
                    .canonicalize()
                    .unwrap(),
                physical,
                "the inherited logical PWD really names this working directory"
            );
            assert_eq!(std::env::current_dir().unwrap(), physical);
            let home = database.parent().unwrap().join("home");
            assert!(Dolt::collect_garbage(&database, &home).await);
            let observed =
                std::fs::read_to_string(std::env::var_os("COLLECTION_PWD_OUTPUT").unwrap())
                    .expect("the GC child wrote its environment");
            assert_eq!(
                Path::new(&observed),
                physical,
                "the child must walk the guard's ancestry"
            );
            return;
        }
        let scratch = Scratch::new("collection-pwd");
        let physical = scratch.0.join("beta").join(DATABASE);
        let logical = scratch.0.join("alpha").join(DATABASE);
        std::fs::create_dir_all(&physical).unwrap();
        std::fs::create_dir_all(logical.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&physical, &logical).unwrap();
        let bin = scratch.0.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), bin.join("dolt")).unwrap();
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture"])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("PWD", &logical)
            .env("COLLECTION_PWD_DATABASE", &logical)
            .env("COLLECTION_PWD_OUTPUT", scratch.0.join("observed-pwd"))
            .current_dir(&logical)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    }

    mod the_collection_exits {
        use super::*;
        use tracing::instrument::WithSubscriber;

        async fn check(case: &str) {
            if std::env::var("COLLECTION_EXIT").as_deref() == Ok(case) {
                let database = std::env::current_dir().unwrap();
                let info = database.join(".dolt/sql-server.info");
                std::fs::create_dir_all(info.parent().unwrap()).unwrap();
                std::fs::write(
                    &info,
                    format!("{}:{}:secret", std::process::id(), free_port()),
                )
                .unwrap();
                assert!(Dolt::a_server_serves_over(&database));
                let captured = crate::log_capture::Captured::default();
                let subscriber = tracing_subscriber::fmt()
                    .with_writer(captured.clone())
                    .with_ansi(false)
                    .finish();
                let home = database.join("home");
                let (ran, ()) = async {
                    tokio::join!(
                        Dolt::collect_garbage_within(&database, &home, Duration::from_secs(3)),
                        async {
                            if case != "skipped" {
                                tokio::time::sleep(Duration::from_millis(350)).await;
                                std::fs::remove_file(&info).unwrap();
                            }
                        }
                    )
                }
                .with_subscriber(subscriber)
                .await;
                assert!(
                    !ran,
                    "the selected exit must not report successful collection"
                );
                let said = captured.text();
                let line = said
                    .lines()
                    .find(|line| line.contains("step=\"store-collection\""))
                    .unwrap_or_else(|| panic!("no collection step for {case}: {said}"));
                let field = |key: &str| -> u64 {
                    let prefix = format!("{key}=");
                    line.split_whitespace()
                        .find_map(|part| part.strip_prefix(&prefix))
                        .unwrap_or_else(|| panic!("missing {key} on {case}: {line}"))
                        .parse()
                        .unwrap()
                };
                let elapsed = field("elapsed_ms");
                let grace = field("grace_ms");
                let walk = field("walk_ms");
                let gc = field("gc_ms");
                assert!(
                    grace >= if case == "skipped" { 9_000 } else { 300 },
                    "{line}"
                );
                assert!(
                    elapsed >= grace + walk + gc,
                    "the whole step must cover its phases: {line}"
                );
                match case {
                    "skipped" => {
                        assert!(line.contains("grace_s="));
                        assert_eq!((walk, gc), (0, 0));
                    }
                    "deadline" => {
                        assert!(line.contains("deadline_s="));
                        assert!(gc >= 2_500, "{line}");
                    }
                    _ => assert!(line.contains("error="), "{line}"),
                }
                return;
            }
            let scratch = Scratch::new(&format!("collection-exit-{case}"));
            let database = scratch.0.join(DATABASE);
            let bin = scratch.0.join("bin");
            std::fs::create_dir_all(&database).unwrap();
            std::fs::create_dir(&bin).unwrap();
            if case != "spawn_failed" {
                std::os::unix::fs::symlink(std::env::current_exe().unwrap(), bin.join("dolt"))
                    .unwrap();
            }
            let name = format!("dolt::tests::the_collection_exits::{case}");
            let output = tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .env("COLLECTION_EXIT", case)
                .env("PATH", &bin)
                .current_dir(&database)
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        }

        #[tokio::test]
        async fn skipped() {
            check("skipped").await;
        }
        #[tokio::test]
        async fn spawn_failed() {
            check("spawn_failed").await;
        }
        #[tokio::test]
        async fn failed() {
            check("failed").await;
        }
        #[tokio::test]
        async fn deadline() {
            check("deadline").await;
        }
    }

    /// **The collection never runs against a store a server is serving.** The command line's
    /// collection is offline only when nothing holds the store: the pinned command finds a live
    /// server through the files it leaves beside the database and runs the collection ON it,
    /// which blocks its writes and breaks its connections. A second start, or a start that races
    /// the drain of a stopping server, must skip the collection and leave the server as it is.
    /// The case runs the collection over a served store and reads two things: that it did not
    /// run, and that the server's own connection still answers.
    #[tokio::test]
    async fn the_collection_does_not_run_against_a_store_a_server_is_serving() {
        let scratch = Scratch::new("collect-served");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        sqlx::query("CREATE TABLE served (id BIGINT PRIMARY KEY)")
            .execute(store.pool())
            .await
            .expect("a table is created");
        let home = Dolt::server_home(&scratch.0).expect("the server's home");

        let ran = Dolt::collect_garbage(&scratch.0.join(DATABASE), &home).await;

        assert!(!ran, "the collection ran against a store a server serves");
        sqlx::query("INSERT INTO served VALUES (1)")
            .execute(store.pool())
            .await
            .expect("the server's own connection still answers");
        store.stop().await;
    }

    /// **What the guard reads in the file a server leaves.** No file is no server. A file
    /// whose process is gone and whose port answers nothing is a server that was killed, which
    /// the collection may pass; a file naming a live process is a server; a file that cannot be
    /// understood counts as a server, because the wrong guess the other way is an online
    /// collection.
    #[test]
    fn the_guard_reads_the_server_info_file_by_what_it_names() {
        let scratch = Scratch::new("guard-info");
        let place = scratch.0.join(".dolt");
        std::fs::create_dir_all(&place).expect("the directory");
        let write = |text: &str| std::fs::write(place.join("sql-server.info"), text).unwrap();
        assert!(!Dolt::a_server_serves(&scratch.0), "no file is no server");
        write(&format!("4294967290:{}:secret", free_port()));
        assert!(
            !Dolt::a_server_serves(&scratch.0),
            "a dead process on a closed port is a server that was killed"
        );
        write(&format!("{}:{}:secret", std::process::id(), free_port()));
        assert!(
            Dolt::a_server_serves(&scratch.0),
            "a live process is a server"
        );
        write("not a server file");
        assert!(
            Dolt::a_server_serves(&scratch.0),
            "a file that cannot be understood counts as a server"
        );
    }

    /// **The guard looks where the command line looks.** The pinned command finds a live server by
    /// walking UP from the database directory and reading the first `.dolt/sql-server.info` it
    /// meets, so a server started over the database directory itself, or over any directory above
    /// it, is the one a collection would run on. A guard that read only the data directory would
    /// pass the first of those. The file names a live process at each level in turn.
    #[test]
    fn the_guard_reads_every_directory_the_command_line_reads() {
        let scratch = Scratch::new("guard-ancestors");
        let data_dir = scratch.0.join("data");
        let database = data_dir.join(DATABASE);
        std::fs::create_dir_all(&database).expect("the database directory");
        assert!(!Dolt::a_server_serves_over(&database), "no file anywhere");
        let alive = |dir: &Path| {
            std::fs::create_dir_all(dir.join(".dolt")).expect("the directory");
            std::fs::write(
                dir.join(".dolt").join("sql-server.info"),
                format!("{}:{}:secret", std::process::id(), free_port()),
            )
            .expect("the info file");
        };
        // A server over the database directory itself.
        alive(&database);
        assert!(
            Dolt::a_server_serves_over(&database),
            "a server over the database directory was not seen"
        );
        std::fs::remove_dir_all(database.join(".dolt")).expect("the file goes");
        // A server over the data directory, the layout jojobot's own server uses.
        alive(&data_dir);
        assert!(
            Dolt::a_server_serves_over(&database),
            "a server over the data directory was not seen"
        );
        std::fs::remove_dir_all(data_dir.join(".dolt")).expect("the file goes");
        // A server over a directory higher up, which the command line also finds.
        alive(&scratch.0);
        assert!(
            Dolt::a_server_serves_over(&database),
            "a server over a directory above the data directory was not seen"
        );
    }

    /// **A database reached through a link is guarded where the link leads.** The command walks
    /// up from the physical directory, so a server over the directory the link points into is the
    /// one a collection would run on, and two starts that name the store by different paths hold
    /// one lock.
    #[tokio::test]
    async fn a_database_reached_through_a_link_is_guarded_where_it_leads() {
        let scratch = Scratch::new("guard-link");
        let beta = scratch.0.join("beta");
        let real = beta.join(DATABASE);
        std::fs::create_dir_all(&real).expect("the real database directory");
        let alpha = scratch.0.join("alpha");
        std::fs::create_dir_all(&alpha).expect("the directory holding the link");
        let linked = alpha.join(DATABASE);
        std::os::unix::fs::symlink(&real, &linked).expect("the link");
        std::fs::create_dir_all(beta.join(".dolt")).expect("the server's directory");
        std::fs::write(
            beta.join(".dolt").join("sql-server.info"),
            format!("{}:{}:secret", std::process::id(), free_port()),
        )
        .expect("the info file");
        assert!(
            Dolt::a_server_serves_over(&linked),
            "a server over the directory the link leads into was not seen"
        );

        let first = Dolt::own_the_start(&real, Duration::from_secs(5))
            .await
            .expect("the first start owns the store");
        let through_the_link = Dolt::own_the_start(&linked, Duration::from_millis(300)).await;
        assert!(
            through_the_link.is_err(),
            "a start through the link was let in beside one by the real path"
        );
        drop(first);
    }

    /// **Two starts of one store do not interleave.** The start owns the directory through an
    /// advisory lock until it returns, so the check for a server and the collection after it
    /// cannot run while another start spawns. A second owner waits, and gives up with an error
    /// that names what holds the directory; it is let in when the first lets go.
    #[tokio::test]
    async fn a_second_start_of_a_store_waits_for_the_first_to_let_go() {
        let scratch = Scratch::new("start-ownership");
        let database = scratch.0.join(DATABASE);
        std::fs::create_dir_all(&database).expect("the database directory");
        let first = Dolt::own_the_start(&database, Duration::from_secs(5))
            .await
            .expect("the first start owns the directory");

        let refused = Dolt::own_the_start(&database, Duration::from_millis(300)).await;
        assert!(
            matches!(&refused, Err(StartError::DataDir { why, .. }) if why.contains("another start")),
            "a second start was let in, or refused without saying why: {refused:?}"
        );

        drop(first);
        Dolt::own_the_start(&database, Duration::from_secs(5))
            .await
            .expect("the directory is free once the first start lets go");
    }

    /// **A server that is going away is waited for, a short while, before the guard decides.** A
    /// server that was killed, or is draining its stop, still holds its process for a moment, and
    /// a start that comes straight after would otherwise skip its collection and warn about a
    /// server that is about to be gone. The file names a process that exits after a short time;
    /// the wait must end when it has, and must say a server still serves when one outlasts it.
    #[tokio::test]
    async fn a_server_that_is_going_away_is_waited_for_and_one_that_stays_is_not() {
        let scratch = Scratch::new("guard-grace");
        let place = scratch.0.join(".dolt");
        std::fs::create_dir_all(&place).expect("the directory");
        let mut leaving = tokio::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .expect("a process that exits soon");
        let pid = leaving.id().expect("its process id");
        std::fs::write(
            place.join("sql-server.info"),
            format!("{pid}:{}:secret", free_port()),
        )
        .expect("the info file");
        let reaping = tokio::spawn(async move { leaving.wait().await });

        let began = std::time::Instant::now();
        assert!(
            Dolt::nothing_serves_within(&scratch.0, Duration::from_secs(10)).await,
            "the wait ended with a server still serving that was about to exit"
        );
        assert!(
            began.elapsed() >= Duration::from_millis(500),
            "the process was judged gone before it exited: {:?}",
            began.elapsed()
        );
        reaping
            .await
            .expect("the process is reaped")
            .expect("it exited");

        // A process that stays is not waited for beyond the grace.
        std::fs::write(
            place.join("sql-server.info"),
            format!("{}:{}:secret", std::process::id(), free_port()),
        )
        .expect("the info file");
        let began = std::time::Instant::now();
        assert!(
            !Dolt::nothing_serves_within(&scratch.0, Duration::from_millis(400)).await,
            "a server that stays was judged gone"
        );
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "the wait outlasted its grace: {:?}",
            began.elapsed()
        );
    }

    /// **A command that passes its deadline is stopped, and one that ends in time is not.** A
    /// hung collection would hold the start before its own readiness timeout begins, and a
    /// command left running after the start gave up would keep the store open. The command
    /// writes its process id and then waits; the deadline passes; the process must be gone.
    #[tokio::test]
    async fn a_command_that_passes_its_deadline_is_killed_and_one_that_ends_is_not() {
        let scratch = Scratch::new("deadline");
        let pidfile = scratch.0.join("pid");
        let mut hanging = tokio::process::Command::new("sh");
        hanging
            .arg("-c")
            .arg(format!("echo $$ > {}; sleep 5", pidfile.display()))
            .kill_on_drop(true);
        let ended = Dolt::run_within(Duration::from_millis(500), &mut hanging).await;
        assert!(ended.is_none(), "a command that waits passed its deadline");
        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("the command wrote its process id")
            .trim()
            .parse()
            .expect("a process id");
        let mut gone = false;
        for _ in 0..50 {
            if !Path::new(&format!("/proc/{pid}")).exists() {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the command was left running after its deadline");

        let mut quick = tokio::process::Command::new("true");
        let ended = Dolt::run_within(Duration::from_secs(30), &mut quick).await;
        assert!(
            matches!(&ended, Some(Ok(output)) if output.status.success()),
            "a command that ends in time ends: {ended:?}"
        );
    }

    /// A `Memory` that answers `list_entities` and `former_handles` on
    /// request and refuses to be asked anything else — `migrate_permanent_ids`
    /// makes no other call on the entity world, and a call to one of these
    /// is this test's own bug, not something to paper over with a fake
    /// implementation.
    struct FailingEntityRead {
        /// How many times the entity world was asked for its list.
        reads: std::sync::atomic::AtomicUsize,
    }

    impl FailingEntityRead {
        fn new() -> Self {
            FailingEntityRead {
                reads: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[jojobot_domain::forward(
        jojobot_domain::forward_memory,
        unimplemented = "migrate_permanent_ids only reads the entity world",
        required_only
    )]
    impl jojobot_domain::memory::Memory for FailingEntityRead {
        async fn former_handles(
            &self,
        ) -> Result<Vec<jojobot_domain::memory::FormerHandle>, jojobot_domain::memory::MemoryError>
        {
            Ok(Vec::new())
        }
        async fn list_entities(
            &self,
            _: Option<jojobot_domain::memory::EntityKind>,
        ) -> Result<Vec<jojobot_domain::memory::Entity>, jojobot_domain::memory::MemoryError>
        {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(jojobot_domain::memory::MemoryError::Store(
                "the entity world is down".into(),
            ))
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
                sid: Sid("pm01".to_string()),
                focus: "about @thing:contract-migrate-permanent-ids-skip".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
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

        let results =
            migrate_permanent_ids(store.pool(), &FailingEntityRead::new(), &mail, &sess).await;
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
                sid: Sid("pm02".to_string()),
                focus: "about @thing:contract-migrate-permanent-ids-run".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
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

        let results = migrate_permanent_ids(store.pool(), &memory, &mail, &sess).await;
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

    /// The four passes, by the name each logs under and each marker carries.
    const PASS_NAMES: [&str; 4] = [
        "mail mentions",
        "message sender column",
        "session bot column",
        "session mentions",
    ];

    /// The one thing the legacy rows name. Each case has a store of its own, so
    /// one handle serves them all.
    const LEGACY_HANDLE: &str = "thing:contract-store-pass";

    /// A real store holding one entity, with the three bare adapters over it.
    struct LegacyWorld {
        _scratch: Scratch,
        store: Dolt,
        memory: crate::dolt::memory::DoltMemory,
        mail: mailboxes::DoltMailboxes,
        sess: sessions::DoltSessions,
        handle: String,
    }

    async fn legacy_world(label: &str) -> LegacyWorld {
        use jojobot_domain::memory::{Memory, NewEntity};

        let scratch = Scratch::new(label);
        let mut store = Dolt::start(&scratch.0.clone(), free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let memory = crate::dolt::memory::DoltMemory::open(store.pool().clone());
        let handle = LEGACY_HANDLE.to_string();
        memory
            .add_entity(NewEntity::new(
                jojobot_domain::memory::EntityId(handle.clone()),
                "was",
                "contract-fixture",
            ))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block a fresh handle");
        memory.badge_the_unbadged().await.expect("badges are drawn");

        let owners =
            std::sync::Arc::new(jojobot_domain::memory::testing::InMemoryMemory::default());
        owners
            .add_entity(NewEntity::new(
                jojobot_domain::memory::EntityId("bot:gamma".into()),
                "gamma",
                "contract-fixture",
            ))
            .await
            .expect("an owner for the box")
            .written()
            .expect("not blocked");
        let mail = mailboxes::DoltMailboxes::open(
            store.pool().clone(),
            std::sync::Arc::new(crate::owners::MemoryOwners::new(owners)),
        );
        {
            use jojobot_domain::mailbox::{MailboxName, Mailboxes};
            mail.create_mailbox(
                &MailboxName("inbox".into()),
                &jojobot_domain::memory::EntityId("bot:gamma".into()),
                None,
            )
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        }
        let sess = sessions::DoltSessions::open(store.pool().clone());
        let _ = &mut store;
        LegacyWorld {
            _scratch: scratch,
            store,
            memory,
            mail,
            sess,
            handle,
        }
    }

    /// **A message and a run written the way every row was before mentions
    /// resolved**: through the bare adapters, which keep the handle as typed.
    /// Returns the message id and the run's id.
    async fn plant_legacy_rows(
        world: &LegacyWorld,
        sid: &str,
    ) -> (
        jojobot_domain::mailbox::MessageId,
        jojobot_domain::session::SessionId,
    ) {
        use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
        use jojobot_domain::session::{NewSession, Sessions, Sid};

        let posted = world
            .mail
            .post_message(NewMessage {
                mailbox: MailboxName("inbox".into()),
                body: format!("about @{}", world.handle),
                subject: None,
                sender: "gamma".into(),
                sent_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("not blocked");
        let session = world
            .sess
            .begin(NewSession {
                bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
                sid: Sid(sid.to_string()),
                focus: format!("about @{}", world.handle),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin should succeed");
        (posted.id, session.id)
    }

    async fn markers(pool: &MySqlPool) -> Vec<String> {
        sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
            .fetch_all(pool)
            .await
            .expect("the markers read")
    }

    async fn message_body(world: &LegacyWorld, id: &jojobot_domain::mailbox::MessageId) -> String {
        use jojobot_domain::mailbox::Mailboxes;
        world
            .mail
            .scan_messages()
            .await
            .expect("scan_messages should succeed")
            .into_iter()
            .find(|m| &m.id == id)
            .expect("the message is still there")
            .body
    }

    /// **A pass that finished leaves a marker, and the next boot settles it.**
    /// Rows written the old way are rewritten on the first run, each of the
    /// four passes names itself in the store, and the second run reports every
    /// pass as already done.
    #[tokio::test]
    async fn a_finished_pass_leaves_a_marker_and_the_next_run_settles_it() {
        let mut world = legacy_world("store-pass-finished").await;
        let (message, session) = plant_legacy_rows(&world, "sp01").await;

        let first =
            migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess)
                .await;
        let ran: std::collections::HashMap<_, _> = first
            .into_iter()
            .map(|(name, outcome)| match outcome {
                Migrated::Ran(n) => (name, n),
                other => {
                    panic!("{name}: expected Ran on a store with nothing marked, got {other:?}")
                }
            })
            .collect();
        assert_eq!(ran["mail mentions"], 1, "the legacy message is rewritten");
        assert_eq!(ran["session mentions"], 1, "the legacy focus is rewritten");
        assert!(
            !message_body(&world, &message).await.contains(&world.handle),
            "the first run rewrote the message"
        );
        let _ = session;
        assert_eq!(
            markers(world.store.pool()).await,
            PASS_NAMES,
            "one marker for each pass that finished"
        );

        let second =
            migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess)
                .await;
        assert_eq!(second.len(), PASS_NAMES.len());
        for (name, outcome) in second {
            assert!(
                matches!(outcome, Migrated::Settled),
                "{name} read its table again on a store that had finished it: {outcome:?}"
            );
        }
        world.store.stop().await;
    }

    /// **A settled pass does not read its table.** With every marker in place a
    /// row written the old way is left exactly as written, which is what a read
    /// of the table would have changed. The control is the store before any
    /// marker: the same rows are rewritten (the case above).
    #[tokio::test]
    async fn a_settled_pass_leaves_a_row_written_after_it_alone() {
        let mut world = legacy_world("store-pass-settled").await;
        migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess).await;
        assert_eq!(markers(world.store.pool()).await, PASS_NAMES);

        let (message, session) = plant_legacy_rows(&world, "sp02").await;
        let again =
            migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess)
                .await;
        assert!(
            again
                .iter()
                .all(|(_, outcome)| matches!(outcome, Migrated::Settled)),
            "{again:?}"
        );
        assert!(
            message_body(&world, &message).await.contains(&world.handle),
            "the message table was read and rewritten by a settled pass"
        );
        use jojobot_domain::session::Sessions;
        assert!(
            world
                .sess
                .read_session(&session)
                .await
                .expect("read_session should succeed")
                .focus
                .contains(&world.handle),
            "the session table was read and rewritten by a settled pass"
        );
        world.store.stop().await;
    }

    /// **A pass that failed writes no marker**, and the ones that finished do.
    /// The message table is dropped, so both passes that read it fail; the two
    /// session passes finish and are marked.
    #[tokio::test]
    async fn a_pass_that_failed_leaves_no_marker() {
        let mut world = legacy_world("store-pass-failed").await;
        plant_legacy_rows(&world, "sp03").await;
        sqlx::query("DROP TABLE message")
            .execute(world.store.pool())
            .await
            .expect("the table drops");

        let results =
            migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess)
                .await;
        for (name, outcome) in &results {
            match *name {
                "mail mentions" | "message sender column" => assert!(
                    matches!(outcome, Migrated::Failed(_)),
                    "{name}: {outcome:?}"
                ),
                _ => assert!(matches!(outcome, Migrated::Ran(_)), "{name}: {outcome:?}"),
            }
        }
        assert_eq!(
            markers(world.store.pool()).await,
            ["session bot column", "session mentions"],
            "only the passes that finished are marked"
        );
        world.store.stop().await;
    }

    /// **A pass skipped for a failed read writes no marker**, and a boot with
    /// the read back runs it: the skipped boot is not a finished one.
    #[tokio::test]
    async fn a_skipped_run_leaves_no_marker_and_the_next_run_does_the_work() {
        let mut world = legacy_world("store-pass-skipped").await;
        let (message, _) = plant_legacy_rows(&world, "sp04").await;

        let skipped = migrate_permanent_ids(
            world.store.pool(),
            &FailingEntityRead::new(),
            &world.mail,
            &world.sess,
        )
        .await;
        assert!(
            skipped
                .iter()
                .all(|(_, outcome)| matches!(outcome, Migrated::Skipped(_))),
            "{skipped:?}"
        );
        assert!(markers(world.store.pool()).await.is_empty());

        let healthy =
            migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess)
                .await;
        assert!(
            healthy
                .iter()
                .all(|(_, outcome)| matches!(outcome, Migrated::Ran(_))),
            "{healthy:?}"
        );
        assert!(!message_body(&world, &message).await.contains(&world.handle));
        assert_eq!(markers(world.store.pool()).await, PASS_NAMES);
        world.store.stop().await;
    }

    /// **With every pass settled the entity world is not read at all.** A read
    /// that fails would skip every unfinished pass, and a settled pass has none
    /// to skip.
    #[tokio::test]
    async fn every_pass_settled_reads_no_entities() {
        let mut world = legacy_world("store-pass-no-entities").await;
        migrate_permanent_ids(world.store.pool(), &world.memory, &world.mail, &world.sess).await;

        let entity_world = FailingEntityRead::new();
        let results =
            migrate_permanent_ids(world.store.pool(), &entity_world, &world.mail, &world.sess)
                .await;
        assert!(
            results
                .iter()
                .all(|(_, outcome)| matches!(outcome, Migrated::Settled)),
            "{results:?}"
        );
        assert_eq!(
            entity_world.reads.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "the entity list was read for passes that had nothing to do"
        );
        world.store.stop().await;
    }
}
