//! **The clean room** — one jojobot instance, its own store, its own
//! directory.
//!
//! **A run that ends in Rust takes all three with it; a run that is killed
//! takes none of them, and its directory is never taken at all.** Removing the
//! directory is [`Room::drop`]'s doing, and nothing runs `Drop` for a kill. The
//! two cleanups that survive that ending are the kernel's death signal and the
//! sweep the next run makes, and both of them signal processes: an abandoned
//! room's directory stays on the machine until somebody deletes it.
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

/// **How many times a room is spawned before its failure is reported.**
///
/// A port this process tested and let go can be taken before the child binds
/// it, so the first attempt failing says nothing about the room. A real failure
/// — no binary, a server that cannot start — fails every attempt and is
/// reported with the count in it.
const ATTEMPTS: usize = 3;

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
        // Before the first room and only then: what an older run abandoned is
        // there when this one starts, and nothing this run does adds to it.
        static SWEPT: std::sync::Once = std::sync::Once::new();
        SWEPT.call_once(sweep_abandoned_stores);
        Room::spawn(binary, DeathSignal::Ask).await
    }

    /// **A room whose server is not asked to die with this run** — what a
    /// build without [`die_with_this_run`] leaves behind, and the one shape
    /// [`sweep_abandoned_stores`] cannot be shown collecting any other way.
    ///
    /// A run that opens one and then goes without dropping it leaves a server
    /// nobody collects, with its store still running underneath it. That is
    /// the ending the sweep is for, so the sweep's own case needs a way to
    /// arrange it. Every other caller wants [`Room::open`].
    pub async fn open_without_the_death_signal(binary: &Path) -> Result<Room> {
        Room::spawn(binary, DeathSignal::Skip).await
    }

    /// **Bring a server up, and try again when the port was taken from under
    /// it.**
    ///
    /// A port cannot be reserved for a child process that binds it itself: this
    /// process can test a port and hold the listener until the moment it spawns,
    /// and between that moment and the child's own bind the port belongs to
    /// whoever asks. **The window is real and cannot be closed from here** —
    /// closing it would mean handing the child a listener it did not open,
    /// which is not how the server takes its port.
    ///
    /// So a collision is made recoverable instead of rare. A server that exits
    /// before it serves is spawned again on fresh ports, and only a room that
    /// fails every attempt is an error — which is what a real failure, a binary
    /// that cannot start, still is. **The flake this replaces failed two cases
    /// every few full runs, and a suite that fails intermittently gets read as
    /// a regression by whoever meets it next.**
    async fn spawn(binary: &Path, signal: DeathSignal) -> Result<Room> {
        anyhow::ensure!(
            binary.is_file(),
            "no jojobot binary at {} — build the workspace first",
            binary.display(),
        );
        let mut last = None;
        for _ in 0..ATTEMPTS {
            match Room::spawn_once(binary, &signal).await {
                Ok(room) => return Ok(room),
                Err(e) => last = Some(e),
            }
        }
        Err(last
            .unwrap_or_else(|| anyhow::anyhow!("no attempt was made"))
            .context(format!(
                "the room did not come up in {ATTEMPTS} attempts, each on ports of its own"
            )))
    }

    /// One attempt: fresh ports, a fresh directory, one spawn.
    async fn spawn_once(binary: &Path, signal: &DeathSignal) -> Result<Room> {
        let dir = scratch()?;
        // **Held until the spawn, then let go.** Nothing else in this process
        // can take these numbers while the command is being built, which is
        // the half of the window that is ours to close.
        let (served, served_held) = free_port()?;
        let (store, store_held) = free_port()?;
        let endpoint = format!("http://127.0.0.1:{served}/mcp");

        // **Two conditions, not one.** The server refuses to start with no
        // issuer unless it is told to run open, and only then checks that the
        // bind is loopback. A room sets both and binds loopback, so a room can
        // never be the thing that serves somebody's memory unauthenticated.
        let mut spawning = std::process::Command::new(binary);
        spawning
            .env("STATE_DIRECTORY", &dir)
            .env("JOJOBOT_BIND", format!("127.0.0.1:{served}"))
            .env("JOJOBOT_STORE_PORT", store.to_string())
            .env("JOJOBOT_ALLOW_NO_AUTH", "1")
            // The server's own log is the first place to look when a room does
            // not come up, so it goes to this process's stderr rather than into
            // a pipe nobody drains.
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if let DeathSignal::Ask = signal {
            die_with_this_run(&mut spawning);
        }
        drop(served_held);
        drop(store_held);
        let server = spawning
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

    /// **Give up the hold on this room.** The server goes on running, its
    /// store with it, and the directory stays where it is: what comes back is
    /// the server's process and that directory, which are the whole of what is
    /// left to find the room by.
    ///
    /// The counterpart of [`Room::open_without_the_death_signal`], and for the
    /// same one caller. Every other run drops its rooms, which kills the
    /// server and takes the directory.
    pub fn abandon(self) -> (u32, PathBuf) {
        let mut room = std::mem::ManuallyDrop::new(self);
        (room.server.id(), std::mem::take(&mut room.dir))
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
    /// **The room goes when the run returns or unwinds.** A panicking assertion
    /// must not leave a server holding a port and a directory behind, so this
    /// is a `Drop` rather than a step at the end of a happy path.
    ///
    /// **It is half the cleanup, and the other half is not here.** `Drop` runs
    /// only when something in this process runs: a run that is killed, run out
    /// of memory, or stopped with the machine ends without reaching this, and
    /// the directory it leaves is a directory nobody deletes. What covers those
    /// endings is [`die_with_this_run`], which is the kernel's job rather than
    /// this one's — and it reaches the processes, not the directory.
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Whether a room asks the kernel to take its server down with this run.
enum DeathSignal {
    /// What a room is opened with.
    Ask,
    /// What the sweep's own case needs, and nothing else does.
    Skip,
}

/// **Ask the kernel to kill a room's server the moment this run dies, however
/// this run dies.**
///
/// [`Room`]'s `Drop` covers the endings that get to run Rust. The endings this
/// harness actually has do not: a killed run, a run the out-of-memory killer
/// takes, a machine that goes down. Nothing in the process runs then, so the
/// server it started is handed to init and goes on holding its port, its store
/// and its memory — and the next run starts on a machine with less of all
/// three. The only cleanup that survives that ending belongs to the kernel.
///
/// **The chain is two links and both need this.** The server keeps a store of
/// its own underneath it; the server dying is what collects the store, so a
/// server that outlives this run keeps its store alive with it.
///
/// ⚠️ **The flag follows the thread that forks, not the process.** The signal
/// is sent when the spawning thread exits, whenever that is — so a room must be
/// opened by a thread that lives as long as the room is used, or the server is
/// killed underneath a run still talking to it. A room is opened and used by
/// one thread: the thread a case runs on, or the thread the driver runs on.
#[cfg(target_os = "linux")]
fn die_with_this_run(spawning: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure runs in the forked child between fork and exec, where
    // only async-signal-safe calls are allowed. It is two system calls and an
    // error value built from an integer — no allocation, no lock, no logging.
    unsafe {
        spawning.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // **The run can already be gone**, having died between the fork and
            // the line above — and then the signal just asked for has already
            // been sent and missed, leaving a server nobody will ever collect.
            // Reading the parent back closes that window: refusing to exec
            // fails the spawn, which the caller reports, rather than starting a
            // server nobody owns.
            if libc::getppid() != parent {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}

/// Where no such flag exists, `Drop` is the whole of the cleanup.
#[cfg(not(target_os = "linux"))]
fn die_with_this_run(_spawning: &mut std::process::Command) {}

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
    format!("{ROOM_PREFIX}{}-{stamp}-{nth}", std::process::id())
}

/// **What every room's directory begins with.** The name a room is given and
/// the name a room is recognised by come from this one place, so a room this
/// build makes cannot become a room this build no longer knows.
const ROOM_PREFIX: &str = "jojobot-room-";

/// The store binary the server runs underneath a room, as it is invoked.
const STORE_COMMAND: &str = "dolt";

/// **A store process serving a room, and the server it answers to.**
///
/// **The chain is three processes, not two**: the run, the jojobot server the
/// run started, and the store that server started. A store on its own says
/// nothing about whether anybody is using it — what says so is where the chain
/// above it ends, which is why the server is carried here beside the store.
pub struct RoomServer {
    /// The store's process.
    pub pid: u32,
    /// The jojobot server that started it. Whoever started THAT is the run
    /// that owns the room.
    pub parent: u32,
}

/// **Every store on this machine that is serving a room's directory**, live
/// ones and abandoned ones alike.
///
/// It reads the process table and nothing else — no bookkeeping file, no list
/// this process kept. A run that was killed wrote nothing down on its way out,
/// so anything it left behind is only findable by looking at what is running.
pub fn room_servers() -> Vec<RoomServer> {
    let Ok(table) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in table.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        // A process can exit between the listing and either read; then it is
        // not something to report and not something to signal.
        if !serves_a_room(&arguments_of(pid)) {
            continue;
        }
        if let Some(parent) = parent_of(pid) {
            found.push(RoomServer { pid, parent });
        }
    }
    found
}

/// **The process that started `pid`**, or `None` when there is no such process
/// any more.
pub fn parent_of(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))?
        .trim()
        .parse()
        .ok()
}

/// **The process an orphan is handed to when nothing else claims it.**
///
/// What is guaranteed is one direction only: a process whose parent is `init`
/// has no owner left. The converse does not hold. An ancestor that called
/// `PR_SET_CHILD_SUBREAPER` — a systemd user session, a container running an
/// init of its own — collects the orphans below it instead, and then leavings
/// are handed to that process rather than to 1. A sweep on such a machine
/// recognises nothing and kills nothing, which is the direction this is safe to
/// be wrong in.
pub const INIT: u32 = 1;

/// **Where one sweep at a time is agreed, machine-wide.** A file, because the
/// runs that have to agree are separate processes.
const SWEEP_TURN: &str = "jojobot-rooms.sweep-turn";

/// **The turn to sweep, held for as long as this value lives.**
///
/// Not for the kill: two runs sweeping together would choose the same
/// processes, and the second signal would land on something already dead. It is
/// for what a sweep is allowed to SEE. A run that arranges an abandoned server
/// has to look at it while it is still there, and every other run on this
/// machine is entitled to collect exactly that — so the arranging run takes the
/// turn, and no sweep runs until it gives the turn back.
#[must_use = "the turn lasts only as long as this value is held"]
pub struct SweepTurn {
    /// The locked file, held and never read: closing it is what gives the turn
    /// back, and `None` is a turn nothing was able to lock.
    _locked: Option<std::fs::File>,
}

impl SweepTurn {
    /// Wait for the turn.
    ///
    /// **A machine that cannot lock gets a turn that holds nothing.** The file
    /// is shared and in the temp directory, so another user's run may own it
    /// and this one may not be able to open it at all. Sweeping without the
    /// turn is worth more than not sweeping, and the run that needed the turn
    /// is the one that finds out it did not get it.
    pub fn take() -> SweepTurn {
        use std::os::unix::io::AsRawFd;

        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(std::env::temp_dir().join(SWEEP_TURN))
        else {
            return SweepTurn { _locked: None };
        };
        match unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } {
            0 => SweepTurn {
                _locked: Some(file),
            },
            _ => SweepTurn { _locked: None },
        }
    }
}

/// **Kill the rooms an older run left behind.**
///
/// The kernel collects what a run that dies today started. It cannot collect
/// what a run that died before this build started: what that run left is
/// holding its ports and its memory with nobody to notice. A run sweeps on the
/// way in, which is the moment the machine's state is somebody else's leavings
/// rather than this run's own doing.
pub fn sweep_abandoned_stores() {
    let _turn = SweepTurn::take();
    for pid in abandoned(room_servers(), parent_of) {
        // **The scan and the signal are two moments.** A process that has gone
        // between them is the outcome this wanted. A process that has gone AND
        // had its number handed to something else is not, and this cannot tell
        // the two apart: the signal lands on whatever holds the number now.
        // The window is the few microseconds between the read and the kill, and
        // shutting it needs a handle on the process — a `pidfd` — rather than
        // its number.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
}

/// **Which of these processes nothing owns any more**, stores and the servers
/// above them.
///
/// **The question is asked of the SERVER, because that is what a run owns.**
/// Two endings leave a room behind, and only the first hands the store itself
/// to init: a run that dropped its rooms killed the server, so the store below
/// it was orphaned; a run that was killed without dropping anything left a
/// server that nobody collected, with its store still answering to that live
/// server. The second is the ending this sweep exists for, and a store's own
/// parent says nothing about it. So a room is garbage when the chain above the
/// store ends at init — either at the store, whose server is already gone, or
/// one step further up at a server that was handed to init itself. Both the
/// server and its store are killed, because either one left running keeps a
/// port and a directory.
///
/// ⚠️ **The owner is the whole test, and it is deliberately a conservative
/// one.** Test binaries run beside each other, each holding rooms of its own,
/// so a sweep always runs with somebody's live room in the process table — and
/// a sweep that went by the room's directory would take a store a sibling is in
/// the middle of using, which is a far worse defect than the leak it was
/// written for. A live room's server answers to the run that opened it, and
/// that run is a process that is still there. Anything this cannot tell about —
/// a server whose own parent cannot be read, an orphan on a machine that hands
/// orphans to a subreaper rather than to [`INIT`] — is left where it is.
///
/// Pure in the owner it is given, so every shape is checkable without arranging
/// the runs that make them.
fn abandoned(servers: Vec<RoomServer>, owner_of: impl Fn(u32) -> Option<u32>) -> Vec<u32> {
    let mut garbage = Vec::new();
    for server in servers {
        if server.parent == INIT {
            garbage.push(server.pid);
            continue;
        }
        if owner_of(server.parent) == Some(INIT) {
            // The store first: it is the process holding the port and the
            // memory, and killing the server first would leave it to be
            // collected by a signal this call does not wait for.
            garbage.push(server.pid);
            garbage.push(server.parent);
        }
    }
    garbage
}

/// How a process was invoked, one argument per element.
fn arguments_of(pid: u32) -> Vec<String> {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return Vec::new();
    };
    raw.split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

/// **Whether these arguments are a store serving a room.**
///
/// Two conditions, and the second alone would not do: a directory named like a
/// room is only evidence about the directory, and the sweep signals processes.
/// Asking for the store binary as well keeps every other process on the machine
/// out of the answer, whatever it happens to have on its command line.
///
/// Pure, so what it admits and what it refuses is checkable without arranging
/// the process it describes.
fn serves_a_room(arguments: &[String]) -> bool {
    let Some(command) = arguments.first() else {
        return false;
    };
    if Path::new(command).file_name().and_then(|it| it.to_str()) != Some(STORE_COMMAND) {
        return false;
    }
    arguments
        .windows(2)
        .any(|pair| pair[0] == "--data-dir" && names_a_room(&pair[1]))
}

/// **Whether a path is inside a room**, at any depth.
///
/// Any depth rather than the directory itself: the server keeps its store under
/// a name of its own choosing beneath the directory it is given, so what the
/// store process is pointed at is a room's descendant rather than the room. A
/// match on the last component alone would recognise no room at all.
fn names_a_room(dir: &str) -> bool {
    Path::new(dir).components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| name.starts_with(ROOM_PREFIX))
    })
}

/// A port no other caller in this process will be given.
///
/// **A cursor, not just a bind.** Asking the OS for `:0` and letting the
/// listener go hands two concurrent callers the same number often enough to
/// matter, and two rooms on one port means one run reading another run's store.
/// The store suite learned this and wrote it down; this is the same answer,
/// because it is the same problem.
fn free_port() -> Result<(u16, std::net::TcpListener)> {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    for _ in 0..20_000 {
        let slot = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let port = port_at(starting_slot(), slot);
        // **The listener comes back with the number.** Releasing it here left
        // the port free for the whole time the caller spent getting ready to
        // spawn; the caller holds it until the last moment instead.
        if let Ok(held) = std::net::TcpListener::bind(("127.0.0.1", port)) {
            return Ok((port, held));
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
    use super::{INIT, RoomServer, abandoned, port_at, room_name, serves_a_room, starting_slot};

    /// **Every shape a room can be in, put to the sweep at once.**
    ///
    /// The two leaks are not one shape. A run that dropped its rooms killed
    /// each server, so what it left is a store handed to init. A run that was
    /// killed left the server too, and the store under it still answers to that
    /// server — a store whose parent is a live process, which is
    /// indistinguishable from a live room until the question is asked one step
    /// further up.
    ///
    /// So all four in one case: both leaks are chosen, server and store
    /// together; a room a running process still owns is not; and neither is one
    /// this cannot read the owner of. Any of those alone passes against a sweep
    /// that chooses everything, or one that chooses nothing.
    #[test]
    fn a_sweep_takes_both_shapes_of_leak_and_leaves_a_live_room_alone() {
        let run = std::process::id();
        // A run that dropped its rooms: the server is gone, the store is init's.
        let orphaned_store = 4_242;
        // A run that was killed: the server outlived it and holds its store.
        let orphaned_server = 4_243;
        let store_of_the_orphan = 4_244;
        // A room a run is in the middle of using.
        let live_server = 4_245;
        let live_store = 4_246;
        // A server that went away between the two reads.
        let unreadable_server = 4_247;
        let store_of_the_unreadable = 4_248;

        let owner_of = |pid: u32| -> Option<u32> {
            if pid == orphaned_server {
                Some(INIT)
            } else if pid == live_server {
                Some(run)
            } else {
                None
            }
        };
        let chosen = abandoned(
            vec![
                RoomServer {
                    pid: orphaned_store,
                    parent: INIT,
                },
                RoomServer {
                    pid: store_of_the_orphan,
                    parent: orphaned_server,
                },
                RoomServer {
                    pid: live_store,
                    parent: live_server,
                },
                RoomServer {
                    pid: store_of_the_unreadable,
                    parent: unreadable_server,
                },
            ],
            owner_of,
        );

        assert!(
            chosen.contains(&orphaned_store),
            "a store handed to init was left running: {chosen:?}",
        );
        assert!(
            chosen.contains(&store_of_the_orphan) && chosen.contains(&orphaned_server),
            "a server handed to init, or the store under it, was left running: {chosen:?}",
        );
        assert!(
            !chosen.contains(&live_store) && !chosen.contains(&live_server),
            "the sweep chose a room a running process still owns: {chosen:?}",
        );
        assert!(
            !chosen.contains(&store_of_the_unreadable),
            "the sweep chose a store whose owner it could not read: {chosen:?}",
        );
    }

    /// **What the sweep recognises as a store serving a room.**
    ///
    /// The sweep signals what this admits, so what it refuses matters as much:
    /// another suite's store in a directory of its own is not a room's, and
    /// something else that merely names a room's path is not a store.
    #[test]
    fn only_a_store_serving_a_room_is_recognised() {
        let invoked =
            |line: &str| -> Vec<String> { line.split(' ').map(|word| word.to_string()).collect() };
        assert!(
            serves_a_room(&invoked(
                "dolt sql-server --data-dir /tmp/jojobot-room-7-8-0/db --port 20000"
            )),
            "the sweep does not recognise the store a room actually runs",
        );
        assert!(
            !serves_a_room(&invoked(
                "dolt sql-server --data-dir /tmp/jojobot-contract-7-8/db --port 20000"
            )),
            "the sweep reaches for a store that is serving something other than a room",
        );
        assert!(
            !serves_a_room(&invoked("tail -f /tmp/jojobot-room-7-8-0/db/log")),
            "the sweep reaches for a process that is not a store at all",
        );
    }

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
