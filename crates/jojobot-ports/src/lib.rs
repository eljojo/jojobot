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
use std::path::{Path, PathBuf};
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
/// **Dropping it releases the port at once.** A lock belongs to an open file,
/// and a process being spawned at the same moment on another thread holds a
/// copy of every open descriptor until it execs. Dropping unlocks the file
/// first, so that copy does not keep the port claimed.
pub struct Claim {
    port: u16,
    /// A listener on the port, kept until the caller is ready to start the
    /// server that binds it, so a process outside these harnesses cannot take
    /// the port in between.
    hold: Option<TcpListener>,
    /// The lock itself. Dropping the claim unlocks it and closes it.
    file: File,
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

impl Drop for Claim {
    /// **Unlock, then let the file close.** Closing the file alone releases the
    /// lock only when every copy of its descriptor is gone, and a child forked
    /// by another thread in the same moment holds one until it execs. The unlock
    /// ends the claim on the open file for every copy, so the port is free the
    /// moment the claim is dropped.
    fn drop(&mut self) {
        // SAFETY: `flock` takes a descriptor this value owns and no pointer.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
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

/// **What a refused open of a claim file means**: `Ok` when the port is held by
/// somebody else, an error when nothing can be claimed from where the file is.
///
/// A claim file another user made and this one may not write is a port somebody
/// else holds, which is the answer a held lock gives. That is a refusal on a file
/// that is THERE. The same kind of refusal on a file that is not there is the
/// directory refusing the file's creation, and every port in the range would
/// read as held while none is, so it is an error that names the path.
///
/// `file_exists` is passed in, so the rule holds without a file system that
/// denies the process anything.
fn open_refusal(path: &Path, refused: io::Error, file_exists: bool) -> Result<(), PortsError> {
    if refused.kind() == io::ErrorKind::PermissionDenied && file_exists {
        return Ok(());
    }
    Err(PortsError::Io {
        what: format!("opening the port claim {}", path.display()),
        source: refused,
    })
}

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

    /// Claim one named port, or `None` when another claimer holds it or its
    /// claim file cannot be opened. This is the lock alone: it does not ask
    /// whether anything is listening.
    pub fn try_claim(&self, port: u16) -> Result<Option<Claim>, PortsError> {
        let io_error = |what: String| move |source| PortsError::Io { what, source };
        std::fs::create_dir_all(&self.dir).map_err(io_error(format!(
            "creating the port claim directory {}",
            self.dir.display()
        )))?;
        let path = self.dir.join(port.to_string());
        let file = match OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(refused) => {
                return match open_refusal(&path, refused, path.exists()) {
                    Ok(()) => Ok(None),
                    Err(error) => Err(error),
                };
            }
        };
        // SAFETY: `flock` takes a descriptor this function owns and no pointer.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(Claim {
                port,
                hold: None,
                file,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn denied() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    /// **A refused claim file that exists is a held port; one that does not
    /// exist is an error naming the path.** Fed an error of the kind, so it needs
    /// no file system that denies the process a write, and holds under root.
    #[test]
    fn a_refusal_on_a_file_that_is_there_is_held_and_one_that_is_not_is_an_error() {
        let path = Path::new("/claims/31900");
        assert!(
            open_refusal(path, denied(), true).is_ok(),
            "a file another user made is a port somebody holds",
        );
        match open_refusal(path, denied(), false) {
            Err(PortsError::Io { what, source }) => {
                assert!(what.contains("/claims/31900"), "the path is named: {what}");
                assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("a directory that refuses the file is an I/O error: {other:?}"),
        }
        assert!(
            matches!(
                open_refusal(path, io::Error::from(io::ErrorKind::NotFound), true),
                Err(PortsError::Io { .. })
            ),
            "only a permission refusal is ever the answer a held port gives",
        );
    }
}
