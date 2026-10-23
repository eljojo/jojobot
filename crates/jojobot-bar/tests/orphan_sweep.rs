//! **The sweep at the start of a bar run removes scratch directories whose
//! owner is gone, and nothing else.**
//!
//! A test process that is killed cannot remove the directories it made, so they
//! stay. Each name carries the number of the process that made it. The sweep
//! reads that number strictly, removes a directory only when the process is
//! certainly gone and no live process still works inside it, and leaves every
//! name it does not fully recognise.

use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_bar::sweep::{is_gone_by_signal, process_is_gone, sweep_orphans, sweep_orphans_of};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-sweep-case-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// A process number that is certainly free: a child that ran and was waited on.
fn a_dead_pid() -> u32 {
    let mut child = Command::new("true").spawn().expect("a child to start");
    let pid = child.id();
    child.wait().expect("the child ends");
    pid
}

fn make(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    std::fs::create_dir_all(dir.join("inside")).expect("a directory");
    std::fs::write(dir.join("inside").join("file"), b"x").expect("a file");
    dir
}

/// Every kind of directory the tests make, named for the process that made it.
fn names_for(pid: u32) -> Vec<String> {
    vec![
        format!("jojobot-room-{pid}-1791471848664039428-4"),
        format!("jojobot-ports-exhaust-{pid}-1791465454594751433"),
        format!("jojobot-ports-outsider-around-{pid}-1791465454594751433"),
        format!("jojobot-dolt-{pid}-migrate-copy-short-1791465489513247132"),
        format!("jojobot-store-template-{pid}"),
        format!("jojobot-bar-{pid}-1791471848664039428"),
    ]
}

/// **A directory of a dead process is removed and one of a live process is
/// kept, for every kind.** Both are in the same parent, so a sweep that removed
/// everything, or nothing, would not pass.
#[test]
fn a_dead_owners_directory_is_removed_and_a_live_owners_is_kept() {
    let parent = scratch("kinds");
    let dead = a_dead_pid();
    let live = std::process::id();
    let gone: Vec<PathBuf> = names_for(dead).iter().map(|n| make(&parent, n)).collect();
    let kept: Vec<PathBuf> = names_for(live).iter().map(|n| make(&parent, n)).collect();

    let swept = sweep_orphans(&parent, &process_is_gone);

    for dir in &gone {
        assert!(
            !dir.exists(),
            "{} of a dead process was kept",
            dir.display()
        );
    }
    for dir in &kept {
        assert!(
            dir.is_dir(),
            "{} of a live process was removed",
            dir.display()
        );
    }
    assert_eq!(swept.removed.len(), gone.len(), "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **A name that is not fully recognised is left alone**, whatever it begins
/// with: a missing number, a number that is not one, a stray file, a link to a
/// directory, and the shared port directory the rooms use.
#[test]
fn a_name_that_is_not_fully_recognised_is_left_alone() {
    let parent = scratch("names");
    let dead = a_dead_pid();
    let odd = [
        "jojobot-room-notapid-1-2".to_string(),
        format!("jojobot-room-{dead}-not-numbers"),
        format!("jojobot-ports-tag-{dead}-stamp"),
        format!("jojobot-dolt-{dead}-what-not-a-stamp"),
        format!("jojobot-room-{dead}"),
        format!("jojobot-dolt-{dead}"),
        "jojobot-ports-exhaust".to_string(),
        "jojobot-store-template-abc".to_string(),
        "jojobot-room-ports".to_string(),
        format!("jojobot-bar-{dead}"),
        format!("jojobot-bar-{dead}-not-a-stamp"),
        "something-else".to_string(),
    ];
    let dirs: Vec<PathBuf> = odd.iter().map(|n| make(&parent, n)).collect();
    // A file carrying a dead owner's name is not a directory.
    let file = parent.join(&names_for(dead)[0]);
    std::fs::write(&file, b"x").expect("a file");
    // A link to a directory carrying a dead owner's name is not followed.
    let target = make(&parent, "target-of-a-link");
    let link = parent.join(&names_for(dead)[3]);
    std::os::unix::fs::symlink(&target, &link).expect("a link");

    let swept = sweep_orphans(&parent, &process_is_gone);

    for dir in dirs.iter().chain([&file, &target]) {
        assert!(dir.exists(), "{} was removed", dir.display());
    }
    assert!(
        link.symlink_metadata().is_ok() && target.join("inside").is_dir(),
        "a link was followed"
    );
    assert!(swept.removed.is_empty(), "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **A dead owner's directory that a live process still works in is kept.** The
/// process is started inside it, so it is the directory a leftover server would
/// hold. Beside it, the same dead owner's directory with nobody inside is
/// removed, so the keeping is about the process and not about the owner.
#[test]
fn a_directory_a_live_process_works_in_is_kept() {
    let parent = scratch("in-use");
    let dead = a_dead_pid();
    let names = names_for(dead);
    let busy = make(&parent, &names[3]);
    let idle = make(&parent, &names[0]);
    let mut worker = Command::new("sleep")
        .arg("60")
        .current_dir(busy.join("inside"))
        .spawn()
        .expect("a process to start");

    let swept = sweep_orphans(&parent, &process_is_gone);

    let _ = worker.kill();
    let _ = worker.wait();
    assert!(busy.is_dir(), "a directory in use was removed");
    assert!(!idle.exists(), "an idle directory of a dead owner was kept");
    assert_eq!(swept.removed.len(), 1, "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **A dead owner's directory that a live process holds as its temp directory
/// is kept**, though the process works elsewhere and names it nowhere on its
/// command line. A server a run started keeps the temp directory the run gave
/// it, and a build fails the moment that directory is gone. Beside it, the same
/// dead owner's directory with no such process is removed, and once the process
/// ends the held directory is removed too.
#[test]
fn a_directory_a_live_process_holds_as_its_temp_directory_is_kept() {
    let parent = scratch("held-temp");
    let dead = a_dead_pid();
    let names = names_for(dead);
    let held = make(&parent, &names[5]);
    let idle = make(&parent, &names[0]);
    let mut server = Command::new("sleep")
        .arg("60")
        .env("TMPDIR", &held)
        .current_dir("/")
        .spawn()
        .expect("a process to start");

    let swept = sweep_orphans(&parent, &process_is_gone);

    assert!(
        held.is_dir(),
        "a directory held as a temp directory was removed"
    );
    assert!(!idle.exists(), "an idle directory of a dead owner was kept");
    assert_eq!(swept.in_use, vec![held.clone()], "{swept:?}");
    assert_eq!(swept.removed, vec![idle.clone()], "{swept:?}");

    let _ = server.kill();
    let _ = server.wait();
    let after = sweep_orphans(&parent, &process_is_gone);
    assert!(
        !held.exists(),
        "a directory nobody holds any more was kept: {after:?}"
    );
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **The directories one level inside a shell's scratch directory are reached,
/// and the shell's directory is not removed**, even when the sweep empties it:
/// nothing says its shell is gone.
#[test]
fn a_shells_children_are_swept_and_the_shell_directory_stays() {
    let parent = scratch("shell");
    let shell = make(&parent, "nix-shell.abc123");
    let dead = a_dead_pid();
    let inside = make(&shell, &names_for(dead)[1]);
    let live_inside = make(&shell, &names_for(std::process::id())[1]);

    let swept = sweep_orphans(&parent, &process_is_gone);

    assert!(
        !inside.exists(),
        "a dead owner's directory in a shell was kept"
    );
    assert!(live_inside.is_dir(), "a live owner's directory was removed");
    assert!(shell.is_dir(), "the shell's own directory was removed");
    assert_eq!(swept.removed.len(), 1, "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **Nothing is removed when nothing is known.** A check that cannot say a
/// process is gone says it is not, and a directory is kept.
#[test]
fn a_check_that_cannot_tell_removes_nothing() {
    let parent = scratch("unknown");
    let dir = make(&parent, &names_for(a_dead_pid())[0]);

    let swept = sweep_orphans(&parent, &|_pid| false);

    assert!(dir.is_dir(), "a directory was removed on no evidence");
    assert!(swept.removed.is_empty(), "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **The real check answers for this process and for one that has ended.**
#[test]
fn the_real_check_knows_a_live_process_from_a_gone_one() {
    assert!(
        !process_is_gone(std::process::id()),
        "this process is alive"
    );
    assert!(process_is_gone(a_dead_pid()), "a waited-for child is gone");
}

/// **The sweep removes the directories of the user it is told, and no other
/// user's.** A shared directory such as the memory-backed one belongs to root,
/// so the user who made the scratch directories in it is named, not read from
/// the parent. Both users are tried on the same directory.
#[test]
fn the_sweep_judges_the_directories_of_the_user_it_is_told() {
    use std::os::unix::fs::MetadataExt;
    let parent = scratch("user");
    let me = parent.metadata().expect("the parent's owner").uid();
    let pid = a_dead_pid();
    let name = format!("jojobot-bar-{pid}-1791471848664039428");
    let dir = make(&parent, &name);

    let other = sweep_orphans_of(&parent, me + 1, &|candidate| candidate == pid);
    assert!(other.removed.is_empty(), "another user's: {other:?}");
    assert!(dir.exists(), "a directory of another user was removed");

    let mine = sweep_orphans_of(&parent, me, &|candidate| candidate == pid);
    assert_eq!(mine.removed, std::slice::from_ref(&dir), "{mine:?}");
    assert!(!dir.exists());
    let _ = std::fs::remove_dir_all(&parent);
}

/// **A process this one cannot signal is not gone, so its directory stays.**
/// Process 1 belongs to another user, always exists and answers `EPERM`, which
/// is what a job in a child user namespace gets for every process outside it.
/// The directory of a dead maker beside it goes, so a check that kept
/// everything would not pass.
#[test]
fn a_directory_whose_maker_exists_but_cannot_be_signalled_is_kept() {
    let parent = scratch("unsignalled");
    let kept = make(&parent, "jojobot-room-1-1791471848664039428-4");
    let gone = make(
        &parent,
        &format!("jojobot-room-{}-1791471848664039428-5", a_dead_pid()),
    );

    let swept = sweep_orphans(&parent, &process_is_gone);

    assert!(
        kept.is_dir(),
        "the directory of an existing process was swept"
    );
    assert!(!gone.exists(), "the directory of a dead process was kept");
    assert_eq!(swept.removed.len(), 1, "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

/// **A maker that answers `EPERM` is kept and one that answers `ESRCH` goes, whoever
/// runs the case.** The case above leans on process 1 belonging to another user,
/// and run by root it can signal process 1, so the `EPERM` answer is never met.
/// Here the answers come from the signal's own mapping, so the sweep meets both.
#[test]
fn the_sweep_keeps_a_maker_that_answers_eperm_and_removes_one_that_answers_esrch() {
    let parent = scratch("answers");
    let kept = make(&parent, "jojobot-room-1-1791471848664039428-6");
    let gone = make(&parent, "jojobot-room-2-1791471848664039428-7");

    let swept = sweep_orphans(&parent, &|pid| {
        let error = if pid == 1 { libc::EPERM } else { libc::ESRCH };
        is_gone_by_signal(-1, Some(error))
    });

    assert!(kept.is_dir(), "a maker that answered EPERM was swept");
    assert!(!gone.exists(), "a maker that answered ESRCH was kept");
    assert_eq!(swept.removed.len(), 1, "{swept:?}");
    std::fs::remove_dir_all(&parent).expect("cleanup");
}

mod orphan_sweep {
    use super::*;

    /// Run only as the child fixture: its real helper name outlives its owner.
    #[test]
    #[ignore = "child fixture for scratch ownership"]
    fn scratch_child() {
        let parent = std::env::var_os("JOJOBOT_SCRATCH_CASE_PARENT").expect("parent");
        let name = jojobot_ports::scratch::name("jojobot-contract-alpha-with-hyphens");
        make(Path::new(&parent), &name);
        println!("scratch-name:{name}");
    }

    fn dead_helper_directory(parent: &Path) -> PathBuf {
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--ignored",
                "--exact",
                "orphan_sweep::scratch_child",
                "--nocapture",
            ])
            .env("JOJOBOT_SCRATCH_CASE_PARENT", parent)
            .output()
            .expect("child runs");
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("child output");
        let name = text
            .lines()
            .find_map(|line| line.strip_prefix("scratch-name:"))
            .expect("the name made by the child");
        parent.join(name)
    }

    /// Actual names identify the owner, including when the label has hyphens.
    #[test]
    fn helper_names_remove_an_exited_owner_and_keep_a_live_owner() {
        let parent = scratch("shared-owners");
        let dead = dead_helper_directory(&parent);
        let live = make(
            &parent,
            &jojobot_ports::scratch::name("jojobot-contract-alpha-with-hyphens"),
        );
        let swept = sweep_orphans(&parent, &process_is_gone);
        assert!(
            !dead.exists(),
            "a real helper name was not swept: {swept:?}"
        );
        assert!(live.is_dir(), "a live helper owner was swept: {swept:?}");
        assert_eq!(swept.removed, vec![dead]);
        std::fs::remove_dir_all(parent).expect("cleanup");
    }

    /// Lookalikes must not turn digits in a hyphenated label into an owner.
    #[test]
    fn malformed_helper_lookalikes_are_left_alone() {
        let parent = scratch("shared-lookalikes");
        let real = dead_helper_directory(&parent);
        let pid = a_dead_pid();
        let odd = [
            format!("jojobot-scratch-label-{pid}-1"),
            format!("jojobot-scratch-0{pid}-1-alpha"),
            format!("jojobot-scratch-{pid}-01-alpha"),
            format!("jojobot-scratch-{pid}-1-"),
            format!("jojobot-scratch-{pid}-alpha-with-hyphens"),
            format!("jojobot-contract-alpha-with-hyphens-{pid}-1"),
            format!("jojobot-scratch-{pid}-18446744073709551616-alpha"),
            "jojobot-scratch-0-1-alpha".to_string(),
            "jojobot-scratch-4294967296-1-alpha".to_string(),
        ];
        let kept: Vec<_> = odd.iter().map(|name| make(&parent, name)).collect();
        let swept = sweep_orphans(&parent, &process_is_gone);
        assert!(
            !real.exists(),
            "the positive helper control was kept: {swept:?}"
        );
        assert_eq!(swept.removed, vec![real]);
        for path in kept {
            assert!(
                path.is_dir(),
                "a malformed lookalike was swept: {}",
                path.display()
            );
        }
        std::fs::remove_dir_all(parent).expect("cleanup");
    }
}
