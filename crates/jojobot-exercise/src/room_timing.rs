//! **Where one room case's time goes before its first assertion**, one line per
//! room, written to the file the bar names.
//!
//! A passing case prints nothing the bar keeps, and `cargo test` reports only a
//! case's total. This line splits a room's life into what it spent copying the
//! migrated store, starting the server, furnishing the seed, and the rest of the
//! case, so a cut to the set-up is measured and not inferred.
//!
//! **It measures and does not judge**, and it never fails a case: a sink that
//! cannot be written is skipped.

use std::io::Write;
use std::time::Duration;

/// The variable that names the file the lines are appended to. The bar sets it
/// on the test processes it starts; with it unset, nothing is written.
pub const SINK_VARIABLE: &str = "JOJOBOT_ROOM_TIMING";

/// **What one room spent**, from the moment its open began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoomTime {
    /// Copying the migrated store template into the room's directory.
    pub copy: Duration,
    /// Spawning the server and waiting until it says it is listening.
    pub server: Duration,
    /// The seed's writes, summed over every `furnish` on this room's surface.
    pub furnish: Duration,
    /// From the start of the open to the room being dropped.
    pub life: Duration,
}

/// **The line for one room**, in `key=value` words so a reader can `grep` it and
/// `awk` it. The case is named by the thread it ran on, which `cargo test` names
/// for the test.
pub fn line(case: &str, time: &RoomTime) -> String {
    format!(
        "room-timing case={case} copy_ms={} server_ms={} furnish_ms={} life_ms={}",
        time.copy.as_millis(),
        time.server.as_millis(),
        time.furnish.as_millis(),
        time.life.as_millis(),
    )
}

/// Append one line to the file `path` names. One `write` call, so lines from
/// test processes running side by side stay whole.
pub fn append_to(path: &std::path::Path, line: &str) {
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let _ = file.write_all(format!("{line}\n").as_bytes());
}

/// Write the line for the case running on this thread to the sink, when there
/// is one.
pub fn record(time: &RoomTime) {
    let Some(path) = std::env::var_os(SINK_VARIABLE).filter(|path| !path.is_empty()) else {
        return;
    };
    let thread = std::thread::current();
    let case = thread.name().unwrap_or("unnamed");
    append_to(std::path::Path::new(&path), &line(case, time));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some_time() -> RoomTime {
        RoomTime {
            copy: Duration::from_millis(1_200),
            server: Duration::from_millis(3_400),
            furnish: Duration::from_millis(5_600),
            life: Duration::from_millis(12_000),
        }
    }

    /// **The line names the case and each of the four times in milliseconds**,
    /// on one line.
    #[test]
    fn a_room_line_names_the_case_and_its_four_times() {
        let said = line("vault_room::a_case", &some_time());
        assert_eq!(
            said,
            "room-timing case=vault_room::a_case copy_ms=1200 server_ms=3400 \
             furnish_ms=5600 life_ms=12000"
        );
        assert!(!said.contains('\n'), "{said}");
    }

    /// **The sink's name is stored in the bar's environment for the test
    /// processes**, and nothing else declares it, so the spelling is pinned.
    #[test]
    fn the_sink_variable_is_the_one_the_bar_sets() {
        assert_eq!(SINK_VARIABLE, "JOJOBOT_ROOM_TIMING");
    }

    /// **Two lines to one file are two lines**, the second after the first.
    #[test]
    fn two_rooms_append_two_lines_to_one_file() {
        let dir = std::env::temp_dir().join(format!("room-timing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        let file = dir.join("room-timing.log");
        append_to(&file, "first");
        append_to(&file, "second");
        let text = std::fs::read_to_string(&file).expect("the file");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(text, "first\nsecond\n");
    }

    /// A sink that cannot be opened is skipped; it never fails a case.
    #[test]
    fn a_sink_that_cannot_be_written_is_skipped() {
        append_to(
            std::path::Path::new("/nonexistent-dir/room-timing.log"),
            "x",
        );
    }
}
