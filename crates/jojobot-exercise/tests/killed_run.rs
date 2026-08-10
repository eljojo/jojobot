//! **A run that is killed leaves nothing running.**
//!
//! The room's `Drop` covers the endings that unwind — an assertion that failed,
//! a run that returned. It cannot cover the ending this harness actually has:
//! a signal the process does not get to answer, an out-of-memory kill, a
//! machine that went down. Nothing in the process runs then, so the cleanup has
//! to be somebody else's — the kernel's.
//!
//! So the case kills a run outright and asks the process table what survived.
//! A clean exit would prove nothing here: it runs `Drop`, which is the path
//! that already worked.
//!
//! The other half of the same subject is what an older build already left
//! lying about, which a run sweeps when it starts. Both of the shapes it can
//! have are here — a store handed to init, and a whole server nobody collected
//! with its store still under it — and so is the danger, which is the opposite
//! one: sweeping a store a run beside this one is still using.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use jojobot_exercise::room::{
    INIT, Room, SweepTurn, parent_of, room_servers, server_binary, sweep_abandoned_stores,
};

/// What the held run prints once its room is serving.
const UP: &str = "the room is up";

/// What the abandoning run prints once its room is up and no longer its own.
const LEFT: &str = "the room is left behind";

/// The case's other half, by name — a second copy of this binary, running the
/// one test below that opens a room and waits to be killed.
const HELD: &str = "holds_a_room_until_it_is_killed";

/// The same, for the run that leaves a room behind the way a build without the
/// death signal left one.
const LEAVES: &str = "leaves_a_room_the_way_a_build_without_the_death_signal_did";

/// How long the kernel is given to collect what the killed run owned. Generous:
/// the answer is a signal delivered at process death, so a slow machine is the
/// only reason to wait at all.
const COLLECTED_WITHIN: Duration = Duration::from_secs(30);

/// How long a signal is given to land before a process that is still running is
/// taken as one nobody signalled.
const SETTLED: Duration = Duration::from_secs(1);

/// **Kill a run mid-room, and its store is gone too.**
///
/// Both halves in one case. The positive: the run really had a store, running,
/// before anything was killed — without it "the store is gone" passes just as
/// well against a room that never came up, which is the shape of assertion that
/// cannot fail. The negative: once the run is killed, neither the server it
/// spawned nor the store underneath that server is still running.
///
/// The chain is two links and both are checked, because either one holding
/// leaves the store alive: a server that outlives the run keeps its own store,
/// and a store that outlives its server keeps the port and the memory that
/// filled this machine.
#[test]
fn a_killed_run_leaves_neither_its_server_nor_its_store() {
    let mut held = Held::start();
    held.wait_until_the_room_is_up();

    let (servers, stores) = held.what_it_brought_up();
    assert!(
        !stores.is_empty(),
        "the held run has no store to leave behind, so nothing below is being read",
    );
    for pid in servers.iter().chain(&stores) {
        assert!(
            running(*pid),
            "process {pid} is not running before anything was killed",
        );
    }

    held.kill();

    let deadline = Instant::now() + COLLECTED_WITHIN;
    let mut left = Vec::new();
    while Instant::now() < deadline {
        left = servers
            .iter()
            .chain(&stores)
            .copied()
            .filter(|pid| running(*pid))
            .collect();
        if left.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        left.is_empty(),
        "the killed run left {left:?} running, out of servers {servers:?} and stores {stores:?}",
    );
}

/// **A server an older build abandoned is swept, and so is the store under
/// it.**
///
/// The leak has two shapes and only one of them hands the store to init. A run
/// that dropped its rooms killed each server first, so what it left is a store
/// whose parent is init. A run that was KILLED ran nothing at all: a build
/// without the death signal left the server as well, orphaned, with its store
/// still answering to it — a store whose parent is a live process, which is
/// what a live room's store looks like too. That second shape is the one the
/// sweep was written for, and telling it apart takes reading one step further
/// up the chain.
///
/// So the case arranges it for real, against the process table: a second copy
/// of this binary opens a room whose server is not asked to die with it, gives
/// up its hold and exits, and what is left is an orphaned server holding a
/// running store.
///
/// The positive it rests on: both processes are running, and the server really
/// was handed to init, before the sweep is asked for anything. Without it "they
/// are gone" passes just as well against a fixture that never came up.
///
/// **The turn is what keeps this from racing.** The fixture is precisely what
/// every other run on the machine is entitled to collect — a sibling's own
/// sweep would take it mid-case — so the case holds the sweep's turn from
/// before the fixture is abandoned until it has been looked at.
#[test]
fn a_sweep_takes_an_abandoned_server_and_the_store_under_it() {
    let mut left = Abandoned::start();

    let turn = SweepTurn::take();
    left.give_up_the_hold();

    let owner = parent_of(left.server);
    assert_eq!(
        owner,
        Some(INIT),
        "the abandoned server was handed to {owner:?} rather than to init: this machine hands \
         orphans to a subreaper, and the shape the sweep looks for cannot be arranged on it",
    );
    for pid in [left.server, left.store] {
        assert!(
            running(pid),
            "process {pid} is not running before the sweep was asked for anything",
        );
    }

    drop(turn);
    sweep_abandoned_stores();

    let deadline = Instant::now() + COLLECTED_WITHIN;
    let mut alive = Vec::new();
    while Instant::now() < deadline {
        alive = [left.server, left.store]
            .into_iter()
            .filter(|pid| running(*pid))
            .collect();
        if alive.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        alive.is_empty(),
        "the sweep left {alive:?} running, out of the abandoned server {} and its store {}",
        left.server,
        left.store,
    );
}

/// **The sweep does not touch a store a live room is using.**
///
/// Test binaries run beside each other, each holding rooms of its own, so a
/// sweep is always running with somebody's live store in the process table. It
/// runs here against this case's own room, which stands in for that sibling —
/// the sweep cannot tell one running owner from another.
///
/// The positive it rests on: the scan really does see this room's store, so it
/// was a candidate rather than something the sweep never looked at. Without it,
/// "the store survived" passes identically against a sweep that scanned nothing
/// at all.
#[test]
fn a_sweep_leaves_the_store_of_a_live_room_alone() {
    let room = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(Room::open(
            &server_binary().expect("a jojobot binary to run"),
        ))
        .expect("a room");

    let mine: Vec<u32> = room_servers()
        .into_iter()
        .filter(|server| parent_of(server.parent) == Some(std::process::id()))
        .map(|server| server.pid)
        .collect();
    assert!(
        !mine.is_empty(),
        "the scan does not see this case's own room, so nothing below is being read",
    );

    sweep_abandoned_stores();
    // **A wait, and this is the one claim that needs one.** Everything else
    // here watches for something to happen; this watches for nothing to, and a
    // signal that has been sent takes a moment to land — so a look taken
    // immediately would find a store that was already killed still running.
    std::thread::sleep(SETTLED);

    for pid in &mine {
        assert!(
            running(*pid),
            "the sweep killed store {pid}, which a room this case is holding needs",
        );
    }
    drop(room);
}

/// **A run that opens a room and then does nothing**, so the case above has
/// something to kill while a room is live.
///
/// `#[ignore]`, because on its own this is a process that sits there: it is
/// started by name rather than by an ordinary run of the suite. The wait is
/// bounded so a copy nobody killed goes away by itself.
#[test]
#[ignore]
fn holds_a_room_until_it_is_killed() {
    let room = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(Room::open(
            &server_binary().expect("a jojobot binary to run"),
        ))
        .expect("a room");
    println!("{UP} {}", room.endpoint());
    // The reader on the other end is waiting for that line, and a buffer that
    // has not been flushed is a case that waits for a room that is already up.
    let _ = std::io::stdout().flush();
    std::thread::sleep(Duration::from_secs(120));
}

/// **A run that leaves a room behind the way a build without the death signal
/// left one**, so the case above has an abandoned server to sweep.
///
/// It opens a room the kernel will not collect, gives up its hold on it and
/// then waits to be told to go. The waiting is what makes the fixture safe to
/// arrange: while this run is alive the room has an owner, so no sweep on the
/// machine may take it, and the case gets to choose the moment it becomes
/// nobody's. A closed pipe means the case is gone, and then leaving is right
/// too.
///
/// `#[ignore]`, because it is started by name rather than by an ordinary run of
/// the suite.
#[test]
#[ignore]
fn leaves_a_room_the_way_a_build_without_the_death_signal_did() {
    let room = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(Room::open_without_the_death_signal(
            &server_binary().expect("a jojobot binary to run"),
        ))
        .expect("a room");
    let (server, dir) = room.abandon();
    println!("{LEFT} {server} {}", dir.display());
    // The reader on the other end is waiting for that line, and a buffer that
    // has not been flushed is a case that waits for a room that is already up.
    let _ = std::io::stdout().flush();
    let mut go = String::new();
    let _ = std::io::stdin().read_line(&mut go);
}

/// A room a second copy of this binary left behind, and the processes it is
/// still made of.
struct Abandoned {
    run: Child,
    /// Writing a line here is what tells the run to go, which is what makes the
    /// room nobody's.
    tell_it_to_go: ChildStdin,
    /// The run's output, held after the one line this reads: a pipe closed
    /// early is a run whose own summary has nowhere to go, and it says so on
    /// the suite's log.
    _said: BufReader<std::process::ChildStdout>,
    /// The orphaned jojobot server.
    server: u32,
    /// The store still answering to that server.
    store: u32,
    /// The room's directory, which nothing else deletes: the death signal and
    /// the sweep both reach processes only.
    dir: PathBuf,
}

impl Abandoned {
    /// Start the run, and read back the room it gave up.
    ///
    /// The store is found through the process table rather than reported,
    /// because it is the run's grandchild: only the kernel knows its number.
    fn start() -> Abandoned {
        let mut run = Command::new(std::env::current_exe().expect("this test binary"))
            .arg("--exact")
            .arg(LEAVES)
            .arg("--ignored")
            .arg("--nocapture")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The server's own log belongs where the rest of the suite's does.
            .stderr(Stdio::inherit())
            .spawn()
            .expect("a second copy of this test binary");
        let tell_it_to_go = run.stdin.take().expect("a pipe that was asked for");
        let mut said = BufReader::new(run.stdout.take().expect("a pipe that was asked for"));

        let mut line = String::new();
        loop {
            line.clear();
            let read = said.read_line(&mut line).expect("the run's output");
            assert!(
                read > 0,
                "the run ended without leaving a room behind — its log is above",
            );
            if line.starts_with(LEFT) {
                break;
            }
        }
        let (server, dir) = line
            .trim()
            .strip_prefix(LEFT)
            .and_then(|rest| rest.trim().split_once(' '))
            .expect("a server pid and a directory on the line the run printed");
        let server: u32 = server.parse().expect("a server pid");
        let dir = PathBuf::from(dir);

        let stores: Vec<u32> = room_servers()
            .into_iter()
            .filter(|store| store.parent == server)
            .map(|store| store.pid)
            .collect();
        assert_eq!(
            stores.len(),
            1,
            "the abandoned server {server} has {} stores, so the fixture is not the shape the \
             case reads",
            stores.len(),
        );
        Abandoned {
            run,
            tell_it_to_go,
            _said: said,
            server,
            store: stores[0],
            dir,
        }
    }

    /// Let the run go, and wait until it is gone: the room is nobody's from the
    /// moment it exits, and not one moment before.
    fn give_up_the_hold(&mut self) {
        let _ = self.tell_it_to_go.write_all(b"go\n");
        let _ = self.tell_it_to_go.flush();
        let _ = self.run.wait();
    }
}

impl Drop for Abandoned {
    /// A case about processes nobody collected does not leave processes nobody
    /// collects, and the directory goes too — the sweep under test deletes no
    /// files, so whatever it did there is still a directory here.
    fn drop(&mut self) {
        let _ = self.run.kill();
        let _ = self.run.wait();
        for pid in [self.store, self.server] {
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A second copy of this binary, holding a room open.
struct Held {
    run: Child,
    said: BufReader<std::process::ChildStdout>,
    /// Everything the run brought up, so that a case which fails still takes
    /// its own processes with it.
    brought_up: Vec<u32>,
}

impl Held {
    fn start() -> Held {
        let mut run = Command::new(std::env::current_exe().expect("this test binary"))
            .arg("--exact")
            .arg(HELD)
            .arg("--ignored")
            .arg("--nocapture")
            .stdout(Stdio::piped())
            // The server's own log belongs where the rest of the suite's does.
            .stderr(Stdio::inherit())
            .spawn()
            .expect("a second copy of this test binary");
        let said = BufReader::new(run.stdout.take().expect("a pipe that was asked for"));
        Held {
            run,
            said,
            brought_up: Vec::new(),
        }
    }

    /// Wait for the line the held run prints once its room is serving.
    ///
    /// Reading rather than sleeping: the room's own start is minutes of
    /// patience on a loaded machine, and a fixed wait would either be longer
    /// than the case needs or shorter than the room takes.
    fn wait_until_the_room_is_up(&mut self) {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .said
                .read_line(&mut line)
                .expect("the held run's output");
            assert!(
                read > 0,
                "the held run ended without opening a room — its log is above",
            );
            if line.starts_with(UP) {
                return;
            }
        }
    }

    /// The server the held run spawned, and the store that server spawned.
    ///
    /// Found through the process table rather than reported by the run,
    /// because the store is the run's grandchild: only the kernel knows its
    /// number, and after the kill only the kernel knows whether it is still
    /// there.
    fn what_it_brought_up(&mut self) -> (Vec<u32>, Vec<u32>) {
        let run = self.run.id();
        let mine: Vec<_> = room_servers()
            .into_iter()
            .filter(|server| parent_of(server.parent) == Some(run))
            .collect();
        let servers: Vec<u32> = mine.iter().map(|server| server.parent).collect();
        let stores: Vec<u32> = mine.iter().map(|server| server.pid).collect();
        self.brought_up.extend(servers.iter().chain(&stores));
        (servers, stores)
    }

    /// **Killed, not asked to stop.** A signal it could answer would let the
    /// room's own `Drop` run, which is the path that was never in doubt.
    fn kill(&mut self) {
        let killed = unsafe { libc::kill(self.run.id() as libc::pid_t, libc::SIGKILL) };
        assert_eq!(killed, 0, "the held run could not be killed");
        let _ = self.run.wait();
    }
}

impl Drop for Held {
    /// A case about processes nobody collected does not leave processes nobody
    /// collects: whatever the assertions did, everything this started is
    /// signalled on the way out.
    fn drop(&mut self) {
        let _ = self.run.kill();
        let _ = self.run.wait();
        for pid in &self.brought_up {
            unsafe { libc::kill(*pid as libc::pid_t, libc::SIGKILL) };
        }
    }
}

/// **Whether `pid` is still a running process.**
///
/// A process that has been killed but not yet collected is not running: it
/// holds no port, no directory and no memory, and it is gone the moment
/// somebody reaps it. Counting one as a survivor would fail this case on a
/// machine whose init is slow to collect rather than on the defect.
fn running(pid: u32) -> bool {
    let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
        return false;
    };
    !status
        .lines()
        .any(|line| line.starts_with("State:") && line.contains('Z'))
}
