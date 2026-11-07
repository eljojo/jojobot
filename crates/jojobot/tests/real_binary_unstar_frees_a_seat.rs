//! The configured binary frees a seat when the writer unstars the oldest rule.
//! A star into full seats is decided over the claims State, so the unstar the
//! refusal asks for has to reach that State and not only the rows.

use std::process::Stdio;

use jojobot_adapters::testing::{Scratch, free_port, wait_for_serving_line};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

#[tokio::test]
async fn the_real_binary_lets_a_star_in_after_the_oldest_rule_is_unstarred() {
    let scratch = Scratch::new("binary-unstar-seat");
    let http_port = free_port();
    let store_port = free_port();
    let binary = jojobot_exercise::room::server_binary().expect("the built jojobot binary");
    let mut child = tokio::process::Command::new(binary)
        .env("STATE_DIRECTORY", &scratch.0)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("JOJOBOT_TODAY", "2026-09-30")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    let _serving = wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the configured binary {failure}"));
    let surface = Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
        .await
        .expect("connecting to the binary");
    let started = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the shipped assistant boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot hands back a session: {started}"))
        .to_string();

    let rules = [
        "Nothing before 10am.",
        "Keep every reply short.",
        "Never book on a Sunday.",
        "Ask before spending.",
        "Say the day of the week.",
        "Write times in twenty-four hours.",
    ];
    let mut refused = None;
    for rule in rules {
        let said: Value = serde_json::from_str(
            &surface
                .call(
                    "capture",
                    json!({"subject": "bot:assistant", "content": rule,
                           "provenance": "testimony", "fields": {"starred": "true"},
                           "sid": sid}),
                )
                .await,
        )
        .expect("a capture answer");
        if said["status"] == "blocked" {
            refused = Some(said);
        }
    }
    let refused = refused.expect("the sixth star is refused");
    let oldest = refused["seated"][0]["address"]
        .as_str()
        .unwrap_or_else(|| panic!("the refusal names the seated rules: {refused}"))
        .to_string();
    let unstarred = surface
        .call(
            "update_fact",
            json!({"address": oldest, "fields": {"starred": "false"}, "sid": sid}),
        )
        .await;
    let again = surface
        .call(
            "capture",
            json!({"subject": "bot:assistant", "content": rules[5],
                   "provenance": "testimony", "fields": {"starred": "true"}, "sid": sid}),
        )
        .await;
    assert!(
        !again.contains("\"status\":\"blocked\""),
        "the star lands once the oldest is unstarred: {again}\nunstar said: {unstarred}"
    );
    surface.finish().await;
    child.start_kill().expect("the binary stops");
    child.wait().await.expect("the binary is reaped");
}
