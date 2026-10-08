//! **The serving line survives the log filter a build sandbox exports.**
//!
//! The package build exports `RUST_LOG=""`. A filter parsed from an empty
//! string has no directives, so it drops every info event, and the serving line
//! is an info event. The server is healthy and says nothing, and whatever waits
//! for the line waits out its deadline.
//!
//! Each case spawns the real binary under a cleared environment with `RUST_LOG`
//! set to the one value under test, the shape `env -i RUST_LOG=` gives, and
//! reads what the server prints. An empty value is no filter at all and the
//! server logs at info. A non-empty value is a real filter and is honoured, so
//! the second case pairs the first: the line is absent under `warn`, and the
//! server is up and answering, which is what makes the absence mean something.

use std::io::BufRead;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jojobot_adapters::testing::free_port;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a server printed and did while it ran under one `RUST_LOG`.
struct Served {
    /// The server printed its `serving http://` line.
    said_it_was_serving: bool,
    /// The address accepted a connection.
    answered: bool,
}

/// Run the real binary with `RUST_LOG` set to exactly `filter` and nothing else
/// inherited, until its address answers and a short grace has passed for the
/// line that follows the bind.
fn serve_under(filter: &str, name: &str) -> Served {
    let state_dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let http_port = free_port();
    let store_port = free_port();

    let mut server = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        // The store's own binary is found on the path.
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("RUST_LOG", filter)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary runs");

    let said = Arc::new(AtomicBool::new(false));
    let log = server.stdout.take().expect("stdout was piped");
    let reader = {
        let said = Arc::clone(&said);
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(log).lines().map_while(Result::ok) {
                if line.contains("serving http://") {
                    said.store(true, Ordering::SeqCst);
                }
            }
        })
    };

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut answered = false;
    while Instant::now() < deadline {
        if server.try_wait().expect("checking the server").is_some() {
            break;
        }
        if TcpStream::connect(("127.0.0.1", http_port)).is_ok() {
            answered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // The line is printed right after the listener binds. A short grace lets it
    // arrive, and the wait ends the moment it does.
    let grace = Instant::now() + Duration::from_secs(3);
    while Instant::now() < grace && !said.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = server.kill();
    let _ = server.wait();
    let _ = reader.join();
    Served {
        said_it_was_serving: said.load(Ordering::SeqCst),
        answered,
    }
}

/// An empty `RUST_LOG` is no filter, so the server logs at info and says it is
/// serving.
#[test]
fn an_empty_log_filter_leaves_the_serving_line_in_place() {
    let served = serve_under("", "jojobot-empty-log-filter");
    assert!(
        served.answered,
        "the server never answered, so the line cannot be judged"
    );
    assert!(
        served.said_it_was_serving,
        "the server answered under RUST_LOG=\"\" and never printed its serving line"
    );
}

/// A non-empty filter is a real one: under `warn` the server is up and the info
/// line is not printed. The pair of the case above.
#[test]
fn a_non_empty_log_filter_is_still_honoured() {
    let served = serve_under("warn", "jojobot-warn-log-filter");
    assert!(
        served.answered,
        "the server never answered, so the absence below says nothing"
    );
    assert!(
        !served.said_it_was_serving,
        "RUST_LOG=warn is a filter and the info line got through it"
    );
}
