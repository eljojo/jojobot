//! **A room does not depend on the log filter it inherits.**
//!
//! A room is ready when its server prints the serving line, which is an info
//! event. The server is a child of the process that opens the room, so it
//! inherits that process's `RUST_LOG`. A sandbox that exports `warn` would hide
//! the line, and the room would wait out its deadline against a server that is
//! healthy. The harness sets the filter of the child itself.
//!
//! This binary holds one case because the case writes the environment of its
//! own process, and nothing else here may read it while that happens.

use std::time::Duration;

use jojobot_exercise::room::{Room, server_binary};

#[tokio::test]
async fn a_room_comes_up_when_the_inherited_log_filter_hides_info_events() {
    // SAFETY: this binary holds one test, so nothing else is reading the
    // environment while it is written.
    unsafe {
        std::env::set_var("RUST_LOG", "warn");
    }

    let binary = server_binary().expect("a jojobot binary to run");
    // A room that cannot see its server's line waits out a two-minute deadline.
    // A healthy start takes seconds, so a minute separates the two.
    let opened = tokio::time::timeout(Duration::from_secs(60), Room::open(&binary)).await;
    let room = opened
        .expect("the room did not come up: its server's serving line never reached the harness")
        .expect("a room");
    drop(room);
}
