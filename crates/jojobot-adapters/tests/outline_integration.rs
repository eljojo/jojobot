//! Gated integration test: the shared Memory contract, run against a real
//! Outline doc. This is the "prove the real adapter conforms" half of the
//! contract strategy — the fast tier already ran the same spec against the fake.
//!
//! It is `#[ignore]` and env-gated, so default `cargo test` (and CI) never touch
//! the network. Run it explicitly with the three variables set:
//!
//! ```sh
//! JOJOBOT_OUTLINE_URL=https://wiki.example.org \
//! JOJOBOT_OUTLINE_TOKEN=... \
//! JOJOBOT_TEST_DOC=<scratch-doc-id> \
//!   cargo test -p jojobot-adapters --test outline_integration -- --ignored
//! ```
//!
//! It writes ONLY to `JOJOBOT_TEST_DOC` (a scratch doc the operator points it
//! at — never a real jojobot doc, and it creates no collections), and it
//! restores that doc's original text afterward, so it self-cleans whatever it
//! wrote. Missing any variable → it skips.

use jojobot_adapters::outline::{OutlineConfig, OutlineStore};
use jojobot_domain::memory::testing::contract;

struct Env {
    url: String,
    token: String,
    doc: String,
}

fn env() -> Option<Env> {
    Some(Env {
        url: std::env::var("JOJOBOT_OUTLINE_URL").ok()?,
        token: std::env::var("JOJOBOT_OUTLINE_TOKEN").ok()?,
        doc: std::env::var("JOJOBOT_TEST_DOC").ok()?,
    })
}

async fn doc_text(http: &reqwest::Client, e: &Env) -> String {
    let body: serde_json::Value = http
        .post(format!("{}/api/documents.info", e.url))
        .bearer_auth(&e.token)
        .json(&serde_json::json!({ "id": e.doc }))
        .send()
        .await
        .expect("documents.info")
        .json()
        .await
        .expect("documents.info body");
    body["data"]["text"]
        .as_str()
        .expect("data.text")
        .to_string()
}

async fn set_doc_text(http: &reqwest::Client, e: &Env, text: &str) {
    let resp = http
        .post(format!("{}/api/documents.update", e.url))
        .bearer_auth(&e.token)
        .json(&serde_json::json!({ "id": e.doc, "text": text }))
        .send()
        .await
        .expect("documents.update");
    assert!(resp.status().is_success(), "restore failed: {}", resp.status());
}

#[tokio::test]
#[ignore = "hits real Outline; set JOJOBOT_OUTLINE_URL/TOKEN and JOJOBOT_TEST_DOC"]
async fn real_outline_satisfies_the_contract() {
    let Some(e) = env() else {
        eprintln!("skipping: set JOJOBOT_OUTLINE_URL, JOJOBOT_OUTLINE_TOKEN, JOJOBOT_TEST_DOC");
        return;
    };

    let http = reqwest::Client::new();
    let original = doc_text(&http, &e).await;

    let store = OutlineStore::new(
        http.clone(),
        OutlineConfig {
            base_url: e.url.clone(),
            token: e.token.clone(),
            doc_id: e.doc.clone(),
        },
    );

    // Run the shared spec in a task so a panic is caught — the doc is restored
    // to its original text either way, self-cleaning every row we wrote.
    let outcome = {
        let store = store.clone();
        tokio::spawn(async move { contract::run_all(&store).await }).await
    };

    set_doc_text(&http, &e, &original).await;
    outcome.expect("the contract must hold against real Outline");
}
