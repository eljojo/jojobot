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

use std::path::Path;
use std::sync::Mutex;

static GATE: Mutex<()> = Mutex::new(());

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
