//! **A room opens again when the kept template was removed from under the
//! process.**
//!
//! A host can remove files the user holds in `/dev/shm` while a test binary runs:
//! systemd-logind clears a user's shared memory when the user's last login
//! session ends, and the bar's run root is in `/dev/shm`. In one `make rooms` run
//! that took the template directory of six binaries that were mid-run, and every
//! later case in each failed on the same missing directory (119 of 432). The
//! template is only files, so the harness remakes it once and the cases go on.
//!
//! This binary holds one case because the template is process-wide state.

use std::time::Duration;

use jojobot_exercise::room::{Room, kept_directories, server_binary};

#[tokio::test]
async fn a_room_opens_after_the_kept_template_is_removed_from_under_the_process() {
    let binary = server_binary().expect("a jojobot binary to run");

    // The first room builds the template and copies it.
    let first = Room::open(&binary).await.expect("a first room");
    drop(first);
    let kept = kept_directories();
    assert!(
        !kept.is_empty(),
        "the first room left no kept template to remove"
    );

    // What a host clearing the user's shared memory does to a directory nobody
    // is writing to: it is gone.
    for dir in &kept {
        std::fs::remove_dir_all(dir).expect("remove a kept directory");
    }

    let opened = tokio::time::timeout(Duration::from_secs(120), Room::open(&binary)).await;
    let second = opened
        .expect("the second room did not come up in two minutes")
        .expect("a room after the template was removed");
    drop(second);
}
