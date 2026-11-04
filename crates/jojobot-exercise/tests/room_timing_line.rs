//! **A room that is opened, furnished and dropped writes one timing line whose
//! parts are the ones that happened.** The line is read back from the file the
//! sink variable names, so the clock boundaries of `Room` are observed through
//! what a lane would keep, and not through the formatter alone.
//!
//! This binary holds one case, because the case names the sink in the process
//! environment and nothing else here may read the environment at the same time.

use jojobot_exercise::expectations;
use jojobot_exercise::room::{Room, server_binary};

/// The number after `key=` on a timing line.
fn millis(line: &str, key: &str) -> u128 {
    let from = line
        .find(key)
        .unwrap_or_else(|| panic!("{key} is not in {line:?}"));
    line[from + key.len()..]
        .split_whitespace()
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("{key} holds no number in {line:?}"))
}

#[tokio::test]
async fn a_room_writes_a_line_whose_parts_are_the_ones_that_happened() {
    let file = std::env::temp_dir().join(format!("room-timing-line-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&file);
    // SAFETY: this binary holds one case, and the sink is read only when a room
    // is dropped, which happens below on this thread.
    unsafe { std::env::set_var(jojobot_exercise::room_timing::SINK_VARIABLE, &file) };
    let binary = server_binary().expect("a jojobot binary");
    let seed = Room::snapshot_after(&binary, |surface| async move {
        expectations::seed_for(expectations::BIKE_ROOM)
            .expect("the room has furniture")
            .furnish(&surface)
            .await
            .expect("the room is furnished");
        Ok(())
    })
    .await
    .expect("a furnished seed");
    let text = std::fs::read_to_string(&file).expect("the sink was written");
    let _ = std::fs::remove_file(&file);
    let case = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string();
    let mine: Vec<&str> = text
        .lines()
        .filter(|line| line.contains(&format!("case={case} ")) && millis(line, "furnish_ms=") > 0)
        .collect();
    assert_eq!(
        mine.len(),
        1,
        "one furnished room, one line with a furnish: {text}"
    );
    let line = mine[0];
    let (template, copy, server, furnish, life) = (
        millis(line, "template_ms="),
        millis(line, "copy_ms="),
        millis(line, "server_ms="),
        millis(line, "furnish_ms="),
        millis(line, "life_ms="),
    );
    assert!(server > 0 && furnish > 0, "{line}");
    assert!(
        template + copy + server + furnish <= life,
        "the parts of a room's open are inside its life: {line}"
    );
    assert!(
        template > copy,
        "the first room of a process builds the template, which costs more than copying it: {line}"
    );

    {
        let (_room, _surface) = Room::open_with_client_from(&binary, &seed)
            .await
            .expect("a room copied from the caller's seed");
    }
    let text = std::fs::read_to_string(&file).expect("the caller-seed sink was written");
    let _ = std::fs::remove_file(&file);
    let line = text
        .lines()
        .rfind(|line| line.contains(&format!("case={case} ")))
        .expect("the caller-seed room writes its timing");
    let copy = millis(line, "copy_ms=");
    let life = millis(line, "life_ms=");
    assert!(copy > 0, "the caller's seed copy is recorded: {line}");
    assert!(copy <= life, "the seed copy is inside the open: {line}");
    assert_eq!(millis(line, "template_ms="), 0, "{line}");
}
