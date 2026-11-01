//! **A grant that names kinds is read after the kinds are loaded.**
//!
//! The apps file is read at configuration time and the kind set is loaded when
//! the store comes up, which is later. A grant of `capture` on kinds is resolved
//! against that set, so resolving it at configuration time refused every such
//! grant and the server never started. Each case spawns the real binary under a
//! cleared environment, in the order `main` starts it, with an apps file and its
//! credential.

use std::io::{BufRead, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jojobot_adapters::testing::{Scratch, free_port, scratch_path};

/// What one start of the binary did.
struct Started {
    /// The server printed its `serving http://` line.
    served: bool,
    /// Everything the server printed on either stream.
    printed: String,
}

/// Run the real binary with one app registered by `file`, until it serves or
/// ends.
fn start_with_apps(name: &str, file: &str) -> Started {
    let state_dir = scratch_path(name);
    std::fs::create_dir_all(state_dir.join("creds")).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    std::fs::write(state_dir.join("apps.json"), file).expect("the apps file");
    std::fs::write(
        state_dir.join("creds").join("app-token-hash-watchbot"),
        jojobot_domain::app::hash_token(&jojobot_domain::app::mint_token()),
    )
    .expect("the token hash credential");

    let mut server = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").expect("a PATH to find the store"),
        )
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", free_port().to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{}", free_port()))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("JOJOBOT_APPS_FILE", state_dir.join("apps.json"))
        .env("CREDENTIALS_DIRECTORY", state_dir.join("creds"))
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");

    let printed = Arc::new(Mutex::new(String::new()));
    let mut readers = Vec::new();
    let out = server.stdout.take().expect("stdout was piped");
    let err = server.stderr.take().expect("stderr was piped");
    {
        let printed = Arc::clone(&printed);
        readers.push(std::thread::spawn(move || {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                printed.lock().unwrap().push_str(&format!("{line}\n"));
            }
        }));
    }
    {
        let printed = Arc::clone(&printed);
        readers.push(std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::BufReader::new(err).read_to_string(&mut text);
            printed.lock().unwrap().push_str(&text);
        }));
    }

    let deadline = Instant::now() + jojobot_exercise::room::READY_TIMEOUT;
    let mut served = false;
    while Instant::now() < deadline {
        if printed.lock().unwrap().contains("serving http://") {
            served = true;
            break;
        }
        if server.try_wait().expect("checking the server").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = server.kill();
    let _ = server.wait();
    for reader in readers {
        let _ = reader.join();
    }
    let printed = printed.lock().unwrap().clone();
    Started { served, printed }
}

fn capture_on(kinds: &str) -> String {
    format!(
        r#"[{{"name":"watchbot","operations":[{{"operation":"capture","kinds":[{kinds}],"keys":["watch_state"]}}]}}]"#
    )
}

/// **A write grant on kinds the instance holds starts the server.** Today the
/// grant is refused at configuration time with `"role" is not a kind`.
#[test]
fn a_write_grant_naming_kinds_the_instance_holds_starts_the_server() {
    let started = start_with_apps("app-grant-kinds-hold", &capture_on(r#""role","work""#));
    assert!(
        started.served,
        "the server did not start on a grant of capture on role and work: {}",
        started.printed
    );
}

/// **The pair of the case above: a kind nothing holds still refuses the start**,
/// naming the token, so the case above passes because the kinds were known and
/// not because the grant stopped being checked.
#[test]
fn a_write_grant_naming_no_kind_still_refuses_the_start_and_names_the_token() {
    let started = start_with_apps("app-grant-kinds-none", &capture_on(r#""nonesuch""#));
    assert!(
        !started.served,
        "the server started on a grant of a kind that does not exist: {}",
        started.printed
    );
    assert!(
        started.printed.contains("nonesuch") && started.printed.contains("is not a kind"),
        "the refusal names the token: {}",
        started.printed
    );
}
