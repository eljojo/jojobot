//! **No process is forked while a file that will be executed is open for
//! writing.**
//!
//! A cut-down stand-in agent or server is a shell script a case writes and then
//! runs. Rust opens the file close-on-exec, so the descriptor never leaks into a
//! child that has finished starting — but a fork on ANOTHER thread, started
//! between this thread's open and its close, copies the table, and its child
//! holds the script open for writing until it execs. Executing the script in
//! that window fails with `ETXTBSY` ("Text file busy"), and the case that did it
//! never forked at all. It reads as a failure about the product and passes on
//! the next run.
//!
//! **The cause is two threads, and the removal is one lock.** Every write of a
//! script and every fork in this process take it, so a fork can never find a
//! script open for writing. There is no retry in it and no wait: the lock is
//! held for the length of one write or one `spawn` call.
//!
//! **The lock cannot reach a fork it does not hold.** A store the adapters start
//! on another thread forks without it, and its child can hold a script open for
//! writing for the moment between its fork and its exec. A start that reads
//! "Text file busy" is tried once more after a short wait
//! ([`spawn_retrying_when_busy`]); the window is the child's fork-to-exec and ends
//! on its own.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

static GATE: Mutex<()> = Mutex::new(());

/// How long a start that read "Text file busy" waits before it tries once more.
const BUSY_WAIT: Duration = Duration::from_millis(50);

/// **Start a child with the lock held, and once more after a short wait when the
/// program it runs is "Text file busy".** Anything else that fails is the caller's
/// to read, and a second busy answer is returned as it is.
pub fn spawn_retrying_when_busy<C>(
    mut start: impl FnMut() -> std::io::Result<C>,
) -> std::io::Result<C> {
    match guarded(&mut start) {
        Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
            std::thread::sleep(BUSY_WAIT);
            guarded(start)
        }
        done => done,
    }
}

/// Run `act` with no other fork or script write in this process in flight.
/// A panicking holder does not wedge the rest.
pub fn guarded<R>(act: impl FnOnce() -> R) -> R {
    let _held = GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    act()
}

/// Write an executable script, closed and marked runnable before any fork may
/// run.
pub fn write_script(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    guarded(|| {
        std::fs::write(path, text)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    })
}
