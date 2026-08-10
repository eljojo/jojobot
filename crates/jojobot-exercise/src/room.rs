//! **The clean room** — one jojobot instance, its own store, its own
//! directory, gone when the run is.
//!
//! A run must start from a known state. The reachable ways to get one are to
//! wipe a long-lived instance or to build a new one, and jojobot has no delete
//! verb by design — so wiping would mean reaching under the surface and writing
//! rows, which is exactly the thing a run of this tier must not do. A room that
//! was never dirty needs no cleaning.
//!
//! The instance is the shipped binary, unmodified: it brings up its own store,
//! applies its own schema and seeds its own default identity before it serves.
//! Nothing here writes to that store. Everything a run puts in a room goes in
//! through the verbs, over MCP, like any other caller.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};

/// How long a room is given to answer before the run gives up. The store is a
/// child process the server brings up and migrates first, so this is minutes of
/// patience rather than seconds — and a run that waits forever on a server that
/// died is a run nobody can read.
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// **A running jojobot nobody else shares.** Its endpoint is what the driver
/// connects to; dropping it takes the process and the directory with it.
pub struct Room {
    server: std::process::Child,
    dir: PathBuf,
    endpoint: String,
}

impl Room {
    /// Bring one up and wait until it answers.
    ///
    /// `binary` is the shipped server. Nothing about the room is compiled in:
    /// the directory, both ports and the open-on-loopback permission all reach
    /// the server the way an operator's service manager passes them.
    pub async fn open(binary: &Path) -> Result<Room> {
        anyhow::ensure!(
            binary.is_file(),
            "no jojobot binary at {} — build the workspace first",
            binary.display(),
        );
        let dir = scratch()?;
        let served = free_port()?;
        let store = free_port()?;
        let endpoint = format!("http://127.0.0.1:{served}/mcp");

        // **Two conditions, not one.** The server refuses to start with no
        // issuer unless it is told to run open, and only then checks that the
        // bind is loopback. A room sets both and binds loopback, so a room can
        // never be the thing that serves somebody's memory unauthenticated.
        let server = std::process::Command::new(binary)
            .env("STATE_DIRECTORY", &dir)
            .env("JOJOBOT_BIND", format!("127.0.0.1:{served}"))
            .env("JOJOBOT_STORE_PORT", store.to_string())
            .env("JOJOBOT_ALLOW_NO_AUTH", "1")
            // The server's own log is the first place to look when a room does
            // not come up, so it goes to this process's stderr rather than into
            // a pipe nobody drains.
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning {}", binary.display()))?;

        let mut room = Room {
            server,
            dir,
            endpoint,
        };
        room.wait_until_answering().await?;
        Ok(room)
    }

    /// Where a client reaches this room.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// **The listener is the readiness signal, and it is an honest one.** The
    /// server binds it last — after the store is up, the schema has applied,
    /// the index has been built and the default identity is seeded — so a
    /// connection that succeeds is a server that has finished starting, not one
    /// that has begun.
    async fn wait_until_answering(&mut self) -> Result<()> {
        let deadline = std::time::Instant::now() + READY_TIMEOUT;
        let address = self
            .endpoint
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_string();
        loop {
            // Asked before the connection: a server that has exited is never
            // going to answer, and waiting out the timeout to say so buries the
            // reason in two minutes of silence.
            if let Some(status) = self.server.try_wait().context("checking the server")? {
                anyhow::bail!("the server exited before it served ({status}) — its log is above");
            }
            if std::net::TcpStream::connect(&address).is_ok() {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "the server did not answer on {address} within {}s — its log is above",
                    READY_TIMEOUT.as_secs(),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

impl Drop for Room {
    /// **The room goes when the run does, whichever way the run ended.** A
    /// panicking assertion must not leave a server holding a port and a
    /// directory behind, so this is a `Drop` rather than a step at the end of
    /// a happy path.
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A directory of this run's own.
fn scratch() -> Result<PathBuf> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let path = std::env::temp_dir().join(room_name(
        stamp,
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    // **`create_dir`, so a name that is already taken is an error.** A room is
    // a directory with a store inside it, and two rooms sharing one is two runs
    // sharing a store — which is the thing this whole module exists to rule
    // out. Making the parent as needed would let that land silently.
    std::fs::create_dir(&path).with_context(|| format!("making a room at {}", path.display()))?;
    Ok(path)
}

/// **What the `nth` room this process makes at `stamp` is called.**
///
/// The clock alone does not separate two rooms. Concurrent callers read it
/// close enough together to get one answer, and then two servers are pointed at
/// one directory: the second finds a store somebody has already initialised and
/// the run dies on the harness. It is the same lesson the port cursor above
/// carries, and the same answer — a counter no two callers in this process
/// share.
///
/// Pure, so the property is checkable without a clock that stands still.
fn room_name(stamp: u128, nth: u64) -> String {
    format!("jojobot-room-{}-{stamp}-{nth}", std::process::id())
}

/// A port no other caller in this process will be given.
///
/// **A cursor, not just a bind.** Asking the OS for `:0` and letting the
/// listener go hands two concurrent callers the same number often enough to
/// matter, and two rooms on one port means one run reading another run's store.
/// The store suite learned this and wrote it down; this is the same answer,
/// because it is the same problem.
fn free_port() -> Result<u16> {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    for _ in 0..20_000 {
        let slot = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let port = port_at(starting_slot(), slot);
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    anyhow::bail!("no free loopback port in the range a room uses")
}

/// **Where in the range this process starts handing out ports.**
///
/// A cursor makes two callers in one process disagree, and makes every process
/// AGREE: each one begins at the same slot and walks up in the same order, so
/// two test binaries starting together are offered the same numbers. The bind
/// beside the cursor does not close that — the candidate is bound, released,
/// and taken later by whatever the port was for — so the collision was
/// systematic rather than rare.
///
/// Seeding from the process id gives each process a different stretch of the
/// range. **It narrows the window rather than shutting it**, exactly as the
/// cursor did when it replaced asking the OS for `:0`: two processes can still
/// be seeded near each other, and the release-then-take window is unchanged.
/// Whoever meets a collision here next should know it was narrowed and not
/// closed.
fn starting_slot() -> u16 {
    std::process::id() as u16
}

/// The port this process's `slot`-th caller is offered, from where the process
/// starts. Pure, so the property above is checkable without two processes.
fn port_at(seed: u16, slot: u16) -> u16 {
    20_000 + seed.wrapping_add(slot) % 20_000
}

/// **Where the shipped server is**, for a caller that did not say.
///
/// Beside whatever is running: a `cargo run` binary and a test binary both sit
/// under the same target directory, so the server this workspace just built is
/// the one a room gets. `JOJOBOT_BIN` overrides it, which is how a release
/// build or an installed binary is exercised instead.
pub fn server_binary() -> Result<PathBuf> {
    if let Some(named) = std::env::var_os("JOJOBOT_BIN") {
        return Ok(PathBuf::from(named));
    }
    let mut here = std::env::current_exe().context("finding the running binary")?;
    // `target/debug/<exe>` and `target/debug/deps/<test>` are both one or two
    // levels beneath the profile directory the server was built into.
    for _ in 0..3 {
        if !here.pop() {
            break;
        }
        let candidate = here.join("jojobot");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "no jojobot binary found beside this one — build the workspace, or name it in JOJOBOT_BIN"
    )
}

#[cfg(test)]
mod tests {
    use super::{port_at, room_name, starting_slot};

    /// **Two rooms made at one instant are two directories.**
    ///
    /// A room is a directory with a store inside it, so two rooms on one name
    /// means the second server meets a store somebody has already initialised
    /// and the run fails on the harness rather than on the product. The clock
    /// does not separate them: concurrent callers read it close enough together
    /// to be handed one answer, which is why the name carries a counter as
    /// well.
    #[test]
    fn two_rooms_made_at_the_same_instant_are_two_directories() {
        // The positive it rests on: the name is otherwise stable, so the
        // difference below is the counter's doing rather than noise.
        assert_eq!(
            room_name(7, 0),
            room_name(7, 0),
            "the same room asked for twice must have one name",
        );
        assert_ne!(
            room_name(7, 0),
            room_name(7, 1),
            "two rooms made inside one clock tick were given one directory",
        );
    }

    /// **The seed is the process, and nothing else.**
    ///
    /// The whole of what separates two test binaries starting together is this
    /// one value: a seed shared between processes puts every one of them back
    /// on the same stretch of the range, and the port test below cannot see
    /// that — it is handed literal seeds, so it holds just as well when the
    /// cursor no longer varies per process at all.
    #[test]
    fn each_process_seeds_the_cursor_from_itself() {
        assert_eq!(
            starting_slot(),
            std::process::id() as u16,
            "the cursor is seeded from something other than this process",
        );
    }

    /// **Two processes must not be offered the same port at the same moment.**
    ///
    /// The defect was that they were: with no seed, every process began at the
    /// same slot and walked up in the same order, so the collision was
    /// systematic rather than rare. What is checkable here is the mechanism —
    /// the race itself is between processes and no assertion inside one can
    /// watch it.
    #[test]
    fn processes_starting_together_are_offered_different_ports() {
        // The positive it rests on: the mapping is stable, so a difference
        // below is the seed's doing rather than noise.
        assert_eq!(
            port_at(7, 3),
            port_at(7, 3),
            "the same process asking twice for the same slot must get one answer"
        );

        // Two processes, each taking its first three ports.
        let first: Vec<u16> = (0..3).map(|slot| port_at(7, slot)).collect();
        let second: Vec<u16> = (0..3).map(|slot| port_at(4_242, slot)).collect();
        assert!(
            first.iter().all(|port| !second.contains(port)),
            "two processes were offered overlapping ports: {first:?} and {second:?}"
        );

        // And every port stays inside the range a room is allowed to use.
        for port in first.into_iter().chain(second) {
            assert!(
                (20_000..40_000).contains(&port),
                "a port left the range this suite reserves: {port}"
            );
        }
    }
}
