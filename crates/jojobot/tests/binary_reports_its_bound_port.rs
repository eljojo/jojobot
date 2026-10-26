//! **The binary picks the port it serves on and says which.** A caller that
//! chooses a free port and then starts the binary on it races every other
//! process on the machine for that number. With `JOJOBOT_BIND` on port 0 the
//! operating system hands the binary a port it holds from that moment, and the
//! serving line names it.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};

/// Start the binary on port 0 over a state directory of its own, which the returned
/// guard removes.
async fn start(name: &str) -> (tokio::process::Child, u16, Scratch) {
    let scratch = Scratch::new(name);
    let state_dir = scratch.0.clone();
    let mut child = tokio::process::Command::new(
        jojobot_exercise::room::server_binary().expect("a jojobot binary to run"),
    )
    .env("STATE_DIRECTORY", &state_dir)
    .env("JOJOBOT_STORE_PORT", free_port().to_string())
    .env("JOJOBOT_BIND", "127.0.0.1:0")
    .env("JOJOBOT_ALLOW_NO_AUTH", "1")
    .env_remove("JOJOBOT_ISSUER")
    .env("RUST_LOG", "info")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
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
    let port = serving.port();
    let mut lines = serving.stdout;
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    (child, port, scratch)
}

#[tokio::test]
async fn two_binaries_started_at_once_on_port_zero_serve_on_two_ports_they_report() {
    let ((mut one, first, _one_dir), (mut two, second, _two_dir)) =
        tokio::join!(start("own-port-one"), start("own-port-two"));
    assert_ne!(first, 0, "the serving line names the bound port");
    assert_ne!(first, second, "two binaries share one port");
    for port in [first, second] {
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap_or_else(|error| panic!("nothing serves on the reported port {port}: {error}"));
    }
    for child in [&mut one, &mut two] {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
