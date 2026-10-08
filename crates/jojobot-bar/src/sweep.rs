//! **Scratch directories whose owner is gone.**
//!
//! A test process that is killed cannot remove the directories it made. Each
//! kind the tests make carries the number of its maker in its name, and this
//! reads that number strictly: a name it does not fully recognise is not
//! touched. A directory is removed only when the maker is certainly gone and no
//! live process still works inside it. Whatever is uncertain is kept, because a
//! kept directory costs disk and a removed one under a live process costs a
//! run.
//!
//! **A shell's own scratch directory is never removed here.** Its children are
//! reached, but nothing in its name says whether its shell has ended.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// What a sweep did.
#[derive(Debug, Default)]
pub struct Swept {
    /// The directories removed.
    pub removed: Vec<PathBuf>,
    /// The directories of a gone owner that a live process still works in.
    pub in_use: Vec<PathBuf>,
    /// The directories that could not be removed, and are left as they were.
    pub failed: Vec<PathBuf>,
}

/// **Whether a process is certainly gone.** Its entry under `/proc` is absent,
/// and `/proc` itself can be read. Where `/proc` cannot be read, nothing is
/// known and nothing is gone.
pub fn process_is_gone(pid: u32) -> bool {
    Path::new("/proc/self").exists() && !Path::new("/proc").join(pid.to_string()).exists()
}

/// **The number of the process that made a directory, read from its name.**
///
/// The four kinds the tests make:
///
/// - `jojobot-room-<pid>-<stamp>-<nth>`
/// - `jojobot-ports-<tag>-<pid>-<nanos>`, where the tag may hold hyphens
/// - `jojobot-dolt-<pid>-<what>-<nanos>`, where the part between may hold hyphens
/// - `jojobot-store-template-<pid>`
/// - `jojobot-bar-<pid>-<nanos>`, the directory a bar run gives its children
///   (see [`run_root_name`])
///
/// Anything else, including a name with a missing or non-numeric part, is
/// `None`.
fn owner_of(name: &str) -> Option<u32> {
    let number = |part: &str| -> Option<u32> {
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if let Some(rest) = name.strip_prefix("jojobot-room-") {
        let parts: Vec<&str> = rest.split('-').collect();
        return match parts.as_slice() {
            [pid, stamp, nth] if all_digits(stamp) && all_digits(nth) => number(pid),
            _ => None,
        };
    }
    if let Some(rest) = name.strip_prefix("jojobot-ports-") {
        let mut from_the_right = rest.rsplitn(3, '-');
        let (nanos, pid, tag) = (
            from_the_right.next()?,
            from_the_right.next()?,
            from_the_right.next()?,
        );
        return (all_digits(nanos) && !tag.is_empty())
            .then(|| number(pid))
            .flatten();
    }
    if let Some(rest) = name.strip_prefix("jojobot-dolt-") {
        let (pid, rest) = rest.split_once('-')?;
        let (what, nanos) = rest.rsplit_once('-')?;
        return (all_digits(nanos) && !what.is_empty())
            .then(|| number(pid))
            .flatten();
    }
    if let Some(pid) = name.strip_prefix("jojobot-store-template-") {
        return number(pid);
    }
    if let Some(rest) = name.strip_prefix("jojobot-bar-") {
        let (pid, nanos) = rest.split_once('-')?;
        return all_digits(nanos).then(|| number(pid)).flatten();
    }
    None
}

/// **What the directory a bar run gives its children is called.** Everything a
/// run's test processes make in the temp directory goes under it, and the run
/// removes it when it ends. A run that is killed leaves it, and the name says
/// whose it was, so the next run's sweep can tell.
pub fn run_root_name(pid: u32, nanos: u128) -> String {
    format!("jojobot-bar-{pid}-{nanos}")
}

/// Every directory in `dir` that this sweep may judge, with its owner's number:
/// a real directory (a link is not followed) made by the same user as `dir`.
fn candidates_in(dir: &Path, user: u32, found: &mut Vec<(PathBuf, u32)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        if !meta.is_dir() || meta.uid() != user {
            continue;
        }
        let name = entry.file_name();
        if let Some(pid) = owner_of(&name.to_string_lossy()) {
            found.push((entry.path(), pid));
        }
    }
}

/// What every live process is working in or has been told about: its working
/// directory and each word of its command line. One read of `/proc`, however
/// many directories are judged against it.
fn where_processes_work() -> Vec<String> {
    let mut places = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return places;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(cwd) = std::fs::read_link(entry.path().join("cwd")) {
            places.push(cwd.to_string_lossy().into_owned());
        }
        if let Ok(command) = std::fs::read(entry.path().join("cmdline")) {
            for word in command.split(|b| *b == 0) {
                if !word.is_empty() {
                    places.push(String::from_utf8_lossy(word).into_owned());
                }
            }
        }
    }
    places
}

/// **Remove the scratch directories under `parent`, and one level inside each
/// `nix-shell.*` directory there, whose owner `gone` says is gone and that no
/// live process works in.**
pub fn sweep_orphans(parent: &Path, gone: &dyn Fn(u32) -> bool) -> Swept {
    let mut swept = Swept::default();
    let Ok(parent_meta) = parent.metadata() else {
        return swept;
    };
    let user = parent_meta.uid();
    let mut found = Vec::new();
    candidates_in(parent, user, &mut found);
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let is_shell = entry
                .file_name()
                .to_string_lossy()
                .starts_with("nix-shell.");
            let real = entry
                .path()
                .symlink_metadata()
                .is_ok_and(|meta| meta.is_dir());
            if is_shell && real {
                candidates_in(&entry.path(), user, &mut found);
            }
        }
    }
    found.retain(|(_, pid)| gone(*pid));
    if found.is_empty() {
        return swept;
    }
    let places = where_processes_work();
    for (dir, _) in found {
        let shown = dir.to_string_lossy();
        if places.iter().any(|place| place.contains(shown.as_ref())) {
            swept.in_use.push(dir);
            continue;
        }
        // A directory a case made unwritable cannot be emptied until it can be.
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        match std::fs::remove_dir_all(&dir) {
            // Another sweep, or its owner, got there first.
            Ok(()) => swept.removed.push(dir),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => swept.failed.push(dir),
        }
    }
    swept
}
