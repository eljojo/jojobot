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
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jojobot_adapters::testing::free_port;

use jojobot_adapters::testing::Scratch;

/// **Whether process `pid` holds the listening socket on loopback `port`.**
///
/// The kernel's table of listening sockets names each one by an inode, and a
/// process's open descriptors name the inodes they hold. Only the process that
/// bound the port holds the inode of its listener.
#[cfg(target_os = "linux")]
fn listens_on(pid: u32, port: u16) -> bool {
    let Ok(table) = std::fs::read_to_string("/proc/net/tcp") else {
        return false;
    };
    let wanted = format!("0100007F:{port:04X}");
    let listening: Vec<String> = table
        .lines()
        .skip(1)
        .filter_map(|row| {
            let columns: Vec<&str> = row.split_whitespace().collect();
            // Column 1 is the local address, 3 the state (0A is listening) and
            // 9 the socket's inode.
            (columns.get(1) == Some(&wanted.as_str()) && columns.get(3) == Some(&"0A"))
                .then(|| format!("socket:[{}]", columns[9]))
        })
        .collect();
    let Ok(held) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    held.filter_map(Result::ok)
        .filter_map(|fd| std::fs::read_link(fd.path()).ok())
        .any(|target| {
            listening
                .iter()
                .any(|socket| target.to_string_lossy() == *socket)
        })
}

/// Off Linux the table is not there to read, and a connection is taken as it
/// was before.
#[cfg(not(target_os = "linux"))]
fn listens_on(_pid: u32, _port: u16) -> bool {
    true
}

/// What a server printed and did while it ran under one `RUST_LOG`.
struct Served {
    /// The server printed its `serving http://` line.
    said_it_was_serving: bool,
    /// The address accepted a connection.
    answered: bool,
    /// How long the server was given before the wait ended.
    waited: Duration,
}

/// Run the real binary with `RUST_LOG` set to exactly `filter` and nothing else
/// inherited, until its address answers and a short grace has passed for the
/// line that follows the bind.
fn serve_under(filter: &str, name: &str) -> Served {
    serve_under_on(filter, name, free_port())
}

/// [`serve_under`] on a port the caller chose.
fn serve_under_on(filter: &str, name: &str, http_port: u16) -> Served {
    let state_dir = jojobot_adapters::testing::scratch_path(name);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
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

    let boot_began = Instant::now();
    let deadline = boot_began + jojobot_exercise::room::READY_TIMEOUT;
    let mut answered = false;
    while Instant::now() < deadline {
        if server.try_wait().expect("checking the server").is_some() {
            break;
        }
        // A connection is the server's only when the server holds the listener:
        // any process can take a port that was released before the server bound
        // it, and it would answer too.
        if TcpStream::connect(("127.0.0.1", http_port)).is_ok()
            && listens_on(server.id(), http_port)
        {
            answered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let waited = boot_began.elapsed();
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
        waited,
    }
}

/// An empty `RUST_LOG` is no filter, so the server logs at info and says it is
/// serving.
#[test]
fn an_empty_log_filter_leaves_the_serving_line_in_place() {
    let served = serve_under("", "jojobot-empty-log-filter");
    assert!(
        served.answered,
        "the server never answered after {:?} (a boot is given {:?}), so the line cannot be judged",
        served.waited,
        jojobot_exercise::room::READY_TIMEOUT
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
        "the server never answered after {:?} (a boot is given {:?}), so the absence below says \
         nothing",
        served.waited,
        jojobot_exercise::room::READY_TIMEOUT
    );
    assert!(
        !served.said_it_was_serving,
        "RUST_LOG=warn is a filter and the info line got through it"
    );
}

/// Another process holds the port the server was told to bind. The server
/// cannot bind it, and the connection that succeeds is the holder's. A case
/// that counts a connection as the server answering is wrong here.
#[test]
fn a_listener_that_is_not_the_server_is_not_taken_for_it() {
    let http_port = free_port();
    let _holder = TcpListener::bind(("127.0.0.1", http_port)).expect("the port is ours to hold");

    let served = serve_under_on("", "jojobot-foreign-listener", http_port);

    assert!(
        !served.answered,
        "a listener the test holds itself was taken for the server"
    );
}
