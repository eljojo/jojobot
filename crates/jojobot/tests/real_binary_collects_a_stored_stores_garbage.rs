//! **A boot collects the garbage of a store it did not just make, and says on a boot line of
//! its own what that took.** Every write leaves chunks nothing refers to, and the background
//! collection the pinned server reports on does not run, so a store's tree grows to several
//! times what it holds. The binary collects it with the command line's collection over the
//! stopped store, before it spawns the server, and logs a step in the form every boot step
//! uses. The first boot of a store that did not exist has nothing to collect and logs none.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};

/// Boot the real binary over the store, read every line it logged until it served, and stop it.
async fn boot_and_read_its_log(scratch: &Scratch, store_port: u16) -> Vec<String> {
    let http_port = free_port();
    let binary = jojobot_exercise::room::server_binary().expect("the built jojobot binary");
    let mut child = tokio::process::Command::new(binary)
        .env("STATE_DIRECTORY", &scratch.0)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let serving = wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the binary {failure}"));
    let seen = serving.seen.clone();
    child.start_kill().expect("the binary stops");
    child.wait().await.expect("the binary is reaped");
    seen
}

/// The collection's step line, if the boot logged one.
fn collection_line(seen: &[String]) -> Option<&String> {
    seen.iter()
        .find(|line| line.contains("boot step finished") && line.contains("store-collection"))
}

/// The number a line carries under `name`.
fn field(line: &str, name: &str) -> u64 {
    let from = line
        .find(&format!("{name}="))
        .unwrap_or_else(|| panic!("no {name} in {line}"))
        + name.len()
        + 1;
    line[from..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or_else(|_| panic!("{name} is no number in {line}"))
}

#[tokio::test]
async fn a_boot_collects_a_stored_stores_garbage_and_a_first_boot_makes_no_collection() {
    let scratch = Scratch::new("binary-collects-garbage");
    let store_port = free_port();

    let first = boot_and_read_its_log(&scratch, store_port).await;
    assert!(
        collection_line(&first).is_none(),
        "the first boot of a store that did not exist collected: {:?}",
        collection_line(&first)
    );

    let second = boot_and_read_its_log(&scratch, store_port).await;
    let line = collection_line(&second).unwrap_or_else(|| {
        panic!(
            "the second boot logged no collection:\n{}",
            second.join("\n")
        )
    });
    assert!(line.contains("elapsed_ms="), "{line}");
    let (before, after) = (field(line, "bytes_before"), field(line, "bytes_after"));
    assert!(
        after <= before,
        "the collection left the store larger: {before} to {after}"
    );
}
