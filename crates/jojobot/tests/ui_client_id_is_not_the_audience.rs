//! **The server does not start when the UI's client id is the agent surface's
//! audience.** Configuration is read first, before the issuer is asked anything
//! and before the store comes up, so the refusal is the process's own exit and
//! not a line in a log.
//!
//! Driven through the real binary, because a refusal that the config unit cases
//! see and `main` swallows would still serve. The pair: the same environment
//! with a client id of its own gets past configuration and stops later, at an
//! issuer that is not there, which is what shows the refusal above is about the
//! equal values and not about the rest of the environment.

use std::process::Command;

/// Run the binary with an issuer nobody answers at, the given audience and the
/// given UI client id, and return `(exited ok, stderr)`.
fn run(audience: &str, ui_client_id: &str) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_jojobot"))
        .env_clear()
        .env("JOJOBOT_BIND", "127.0.0.1:0")
        .env("JOJOBOT_ISSUER", "http://127.0.0.1:9")
        .env("JOJOBOT_AUDIENCE", audience)
        .env("JOJOBOT_UI_CLIENT_ID", ui_client_id)
        .env("JOJOBOT_UI_BASE_URL", "http://127.0.0.1:1")
        .env("RUST_LOG", "error")
        .output()
        .expect("the binary runs");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn equal_values_stop_the_process_naming_both_and_distinct_ones_get_past_configuration() {
    let (ok, said) = run("shared-id", "shared-id");
    assert!(!ok, "the server started with one value for both: {said}");
    assert!(
        said.contains("JOJOBOT_UI_CLIENT_ID") && said.contains("JOJOBOT_AUDIENCE"),
        "the refusal names both variables: {said}"
    );
    assert!(
        said.contains("loading configuration"),
        "the refusal is the configuration's own, before anything else is tried: {said}"
    );

    let (ok, said) = run("https://jojobot.test/mcp", "jojobot-ui");
    assert!(
        !ok,
        "no issuer answers here, so this must still stop: {said}"
    );
    assert!(
        !said.contains("loading configuration"),
        "distinct values must get past configuration: {said}"
    );
}
