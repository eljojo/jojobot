//! **One arbiter for the loopback ports the test harnesses hand out.**
//!
//! The store suites and the rooms each start servers on ports they choose.
//! Two arbiters with two ranges let a room and a store test be given one port
//! at the same moment, and the store then refuses to start on a port a room
//! took. Both draw from here instead.
//!
//! **The arbiter is an advisory lock on a file named for the port**, taken
//! before the port is handed out and held for as long as the [`Claim`] lives.
//! It reaches every process on the machine and goes when its holder does,
//! however the holder ends, so a run that is killed releases its ports without
//! anybody tidying up. A bind beside the lock asks whether a process that is
//! NOT one of these harnesses holds the port.
//!
//! **The files live in one fixed directory and not in the temp directory**: a
//! shell that sets its own `TMPDIR` per invocation gives two runs two
//! directories, and a claim only one of them can see claims nothing. The
//! directory is the one the rooms already used, so a run on older code and a
//! run on this meet on the same files.
//!
//! **There is no block to exhaust.** A claim is one port, so the number of
//! processes that can run at once is bounded by the range and not by a count of
//! slots.

use std::fs::{File, OpenOptions};
use std::io;
use std::net::TcpListener;
use std::os::fd::AsRawFd as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// The first port a claim may be given.
pub const FIRST: u16 = 20_000;
/// One past the last port a claim may be given.
pub const END: u16 = 32_768;
/// Where the per-port claim files live.
pub const CLAIM_DIR: &str = "/tmp/jojobot-room-ports";

/// A port held for as long as this value lives.
///
/// **Dropping it releases the port, promptly and not instantly.** A lock
/// belongs to an open file, and a process being spawned at the same moment on
/// another thread holds a copy of every open descriptor until it execs, so the
/// release can land a few milliseconds late. The port stays spoken for until
/// then, never the other way round.
pub struct Claim {
    port: u16,
    /// A listener on the port, kept until the caller is ready to start the
    /// server that binds it, so a process outside these harnesses cannot take
    /// the port in between.
    hold: Option<TcpListener>,
    /// The lock itself. Never read: dropping it releases the port.
    _file: File,
}

impl Claim {
    /// The port this claim holds.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Let go of the listener so a child can bind the port, and keep the claim.
    pub fn release_listener(mut self) -> Claim {
        self.hold = None;
        self
    }

    /// Whether the claim still holds a listener on its port.
    pub fn holds_listener(&self) -> bool {
        self.hold.is_some()
    }
}

/// Why a port could not be claimed.
#[derive(Debug)]
pub enum PortsError {
    /// Every port in the range is held.
    Exhausted {
        /// First port of the range.
        first: u16,
        /// One past the last port of the range.
        end: u16,
    },
    /// The claim files could not be used at all.
    Io {
        /// What was being done.
        what: String,
        /// The operating system's answer.
        source: io::Error,
    },
}

impl std::fmt::Display for PortsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortsError::Exhausted { first, end } => {
                write!(f, "every port from {first} up to {end} is held")
            }
            PortsError::Io { what, source } => write!(f, "{what}: {source}"),
        }
    }
}

impl std::error::Error for PortsError {}

/// A source of claims over one range, with its claim files in one directory.
///
/// Two allocators over one directory contend with each other even inside one
/// process, because each claim opens the file afresh and a lock belongs to the
/// open file and not to the process.
pub struct Allocator {
    first: u16,
    end: u16,
    dir: PathBuf,
    /// Where in the range the next search starts. **Spreading the start only
    /// makes a retry rarer; the lock is what makes two claimers disagree.**
    cursor: AtomicU32,
}

impl Allocator {
    /// The allocator every harness on this machine shares.
    pub fn shared() -> Allocator {
        Allocator::within(FIRST, END, PathBuf::from(CLAIM_DIR))
    }

    /// An allocator over a range and a directory of the caller's choosing.
    pub fn within(first: u16, end: u16, dir: PathBuf) -> Allocator {
        let seed = std::process::id().wrapping_mul(2_654_435_761).wrapping_add(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        );
        Allocator {
            first,
            end,
            dir,
            cursor: AtomicU32::new(seed),
        }
    }

    /// Claim one port: the first the search finds that no other claimer holds
    /// and that nothing outside these harnesses is listening on.
    pub fn claim(&self) -> Result<Claim, PortsError> {
        let span = u32::from(self.end.saturating_sub(self.first));
        for _ in 0..span {
            let offset = self.cursor.fetch_add(1, Ordering::Relaxed) % span;
            let port = self.first + offset as u16;
            let Some(mut claim) = self.try_claim(port)? else {
                continue;
            };
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(listener) => {
                    claim.hold = Some(listener);
                    return Ok(claim);
                }
                // Somebody outside these harnesses is on it. Dropping the
                // claim releases the lock, so the port is not lost to the
                // next claimer for good.
                Err(_) => continue,
            }
        }
        Err(PortsError::Exhausted {
            first: self.first,
            end: self.end,
        })
    }

    /// Claim one named port, or `None` when another claimer holds it. This is
    /// the lock alone: it does not ask whether anything is listening.
    pub fn try_claim(&self, port: u16) -> Result<Option<Claim>, PortsError> {
        let io_error = |what: String| move |source| PortsError::Io { what, source };
        std::fs::create_dir_all(&self.dir).map_err(io_error(format!(
            "creating the port claim directory {}",
            self.dir.display()
        )))?;
        let path = self.dir.join(port.to_string());
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io_error(format!(
                "opening the port claim {}",
                path.display()
            )))?;
        // SAFETY: `flock` takes a descriptor this function owns and no pointer.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(Claim {
                port,
                hold: None,
                _file: file,
            }));
        }
        let refused = io::Error::last_os_error();
        if refused.raw_os_error() == Some(libc::EWOULDBLOCK) {
            return Ok(None);
        }
        Err(PortsError::Io {
            what: format!("claiming the port with {}", path.display()),
            source: refused,
        })
    }
}

fn shared() -> &'static Allocator {
    static SHARED: OnceLock<Allocator> = OnceLock::new();
    SHARED.get_or_init(Allocator::shared)
}

/// Claim one port from the allocator every harness on this machine shares.
pub fn claim() -> Result<Claim, PortsError> {
    shared().claim()
}

/// Claim one named port from the shared allocator, or `None` when it is held.
pub fn try_claim(port: u16) -> Result<Option<Claim>, PortsError> {
    shared().try_claim(port)
}

/// **A port claimed for the rest of this process, with nothing bound to it.**
///
/// The claim is kept in a list that is never emptied, so the port stays spoken
/// for until the process ends, however many tests the process runs. The
/// listener is let go at once: the caller's own server binds the port next.
pub fn claim_for_life() -> Result<u16, PortsError> {
    static KEPT: Mutex<Vec<Claim>> = Mutex::new(Vec::new());
    let claim = claim()?.release_listener();
    let port = claim.port();
    KEPT.lock().expect("no claim panics while held").push(claim);
    Ok(port)
}
