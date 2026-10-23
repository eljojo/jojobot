//! **Where a bar run puts the directory it gives its children as their temp
//! directory.**
//!
//! The test stores hold nothing that must outlive the run, and many Dolt servers
//! syncing to one disk at once stall each other's commits past a server's
//! readiness deadline. A memory-backed directory has no disk to wait for, so the
//! run root goes there when the host has room, and on disk otherwise. A host
//! without memory-backed scratch, or with too little of it, still runs the bar.

use std::path::{Path, PathBuf};

pub use jojobot_ports::scratch::name;

/// The memory-backed directory every Linux host offers.
pub const MEMORY_DIR: &str = "/dev/shm";

/// **The free bytes the memory-backed directory needs before the run root goes
/// there.** Below it the run falls back to disk. A report run on a build runner
/// held about 330 MiB there at its peak; the smaller runner has an 8 GiB
/// memory-backed directory, which clears this.
pub const MINIMUM_FREE_BYTES: u64 = 2 << 30;

/// Where the run root goes, and why it is on disk when it is.
#[derive(Debug, PartialEq, Eq)]
pub enum Placement {
    /// Under the memory-backed directory.
    Memory(PathBuf),
    /// Under the disk's temp directory, for the reason given.
    Disk { base: PathBuf, reason: String },
}

impl Placement {
    /// The directory the run root is made in.
    pub fn base(&self) -> &Path {
        match self {
            Placement::Memory(base) | Placement::Disk { base, .. } => base,
        }
    }
}

/// **Choose the base of the run root.** `free` is what the memory-backed
/// directory has free, or none when it is missing or cannot be read.
pub fn place(memory: &Path, free: Option<u64>, minimum: u64, disk: &Path) -> Placement {
    match free {
        Some(free) if free >= minimum => Placement::Memory(memory.to_path_buf()),
        Some(free) => Placement::Disk {
            base: disk.to_path_buf(),
            reason: format!(
                "{} has {free} bytes free and the bar needs {minimum}",
                memory.display()
            ),
        },
        None => Placement::Disk {
            base: disk.to_path_buf(),
            reason: format!("{} is missing or cannot be read", memory.display()),
        },
    }
}

/// **The bytes a process may still write under `dir`**, or none when `dir` is
/// missing or the system will not say.
// The fields are wider on some hosts than on others, so the conversion is
// needed where it is not on this one.
#[allow(clippy::useless_conversion)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a NUL-terminated string that outlives the call, and
    // `stat` is written by the call before it is read.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let read = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    (read == 0).then(|| u64::from(stat.f_bavail).saturating_mul(u64::from(stat.f_frsize)))
}
