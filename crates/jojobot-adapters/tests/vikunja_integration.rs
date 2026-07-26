//! Gated integration test: the shared Mailboxes contract, run against real
//! Vikunja. This is the "prove the real adapter conforms" half of the contract
//! strategy — the fast tier already ran the same spec against the in-memory fake
//! and against the store over an API double.
//!
//! It is `#[ignore]` and env-gated on **credentials only** (no ids — that is the
//! whole point of convention over configuration), so a default `cargo test` and
//! CI never touch the network:
//!
//! ```sh
//! JOJOBOT_VIKUNJA_URL=https://tasks.example.org \
//! JOJOBOT_VIKUNJA_TOKEN=... \
//!   cargo test -p jojobot-adapters --test vikunja_integration -- --ignored
//! ```
//!
//! **The operator's real task boards live on this Vikunja.** Three things keep
//! this test away from them:
//!
//! * every project it uses is named with the [`TEST_PREFIX`] and stamped with
//!   jojobot's owner tag;
//! * each test uses ONE fixed, persistent project (created on the first run,
//!   adopted ever after) — the operator's decision, 2026-07-25: the API tokens
//!   deliberately cannot delete anything (DELETE → 401), and archive-per-run
//!   would silt the instance, so nothing here creates per-case projects or
//!   tears anything down. The delete-based teardown below is dormant until a
//!   disposable test Vikunja exists (the operator's chosen endgame);
//! * the run fingerprints every project and label it does not own, before and
//!   after, and asserts the set is unchanged.
//!
//! Known, accepted limitation of the shared persistent project: the contract
//! spec assumes a store that starts empty, so a REPEAT run fails on leftovers
//! from the previous one (e.g. `create_mailbox("inbox")` blocks as existing).
//! Until the disposable instance lands, this gate's job is the first-run truth:
//! does the adapter work against real Vikunja at all.

use std::sync::Arc;

use jojobot_adapters::vikunja::{Secret, VikunjaConfig, VikunjaStore};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::mailbox::testing::contract;

/// Every project this file creates is titled under this prefix. Deliberately
/// distinct from [`VikunjaStore::DEFAULT_PROJECT`], so a run can never adopt or
/// delete the real mailbox board.
const TEST_PREFIX: &str = "jojobot-mailboxes-itest-";

/// **Each test owns one fixed project.** The two tests live in one binary and
/// the harness runs them in parallel, so they must not share a board — mailbox
/// labels are namespaced by project title, so distinct titles isolate the boxes
/// too. These are full project titles now, not minting prefixes.
const CONTRACT_PROJECT: &str = "jojobot-mailboxes-itest-c";
/// The adoption test's own fixed project. See [`CONTRACT_PROJECT`].
const ADOPT_PROJECT: &str = "jojobot-mailboxes-itest-a";

/// The tag jojobot stamps on what it creates. Teardown requires it as well as
/// the name: two independent conditions, because one of them being wrong must
/// not be enough to delete a real board.
const OWNER_TAG: &str = "[jojobot:owned]";

struct Creds {
    url: String,
    token: String,
}

fn creds() -> Option<Creds> {
    Some(Creds {
        url: std::env::var("JOJOBOT_VIKUNJA_URL").ok().filter(|s| !s.is_empty())?,
        token: std::env::var("JOJOBOT_VIKUNJA_TOKEN").ok().filter(|s| !s.is_empty())?,
    })
}

async fn get(http: &reqwest::Client, c: &Creds, path: &str) -> serde_json::Value {
    http.get(format!("{}/api/v1{path}", c.url.trim_end_matches('/')))
        .bearer_auth(&c.token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {path}: {e}"))
        .json()
        .await
        .unwrap_or_else(|e| panic!("GET {path} body: {e}"))
}

/// Dormant: the operator's API tokens cannot delete (401 by design), and the
/// persistent-project mode above no longer tears down. Kept for the disposable
/// test instance, where delete-based teardown becomes possible again.
#[allow(dead_code)]
async fn delete(http: &reqwest::Client, c: &Creds, path: &str) {
    let resp = http
        .delete(format!("{}/api/v1{path}", c.url.trim_end_matches('/')))
        .bearer_auth(&c.token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("DELETE {path}: {e}"));
    assert!(resp.status().is_success(), "DELETE {path} → {}", resp.status());
}

/// Every project, paged in full, as `(id, title, description)`.
async fn all_projects(http: &reqwest::Client, c: &Creds) -> Vec<(u64, String, String)> {
    let mut found = Vec::new();
    let mut page = 1;
    loop {
        let body = get(http, c, &format!("/projects?page={page}&per_page=50")).await;
        let items = body.as_array().cloned().unwrap_or_default();
        let count = items.len();
        found.extend(items.iter().filter_map(|p| {
            Some((
                p["id"].as_u64()?,
                p["title"].as_str().unwrap_or_default().to_string(),
                p["description"].as_str().unwrap_or_default().to_string(),
            ))
        }));
        if count < 50 {
            break;
        }
        page += 1;
    }
    found
}

/// Every label, paged in full, as `(id, title, description)`.
async fn all_labels(http: &reqwest::Client, c: &Creds) -> Vec<(u64, String, String)> {
    let mut found = Vec::new();
    let mut page = 1;
    loop {
        let body = get(http, c, &format!("/labels?page={page}&per_page=50")).await;
        let items = body.as_array().cloned().unwrap_or_default();
        let count = items.len();
        found.extend(items.iter().filter_map(|l| {
            Some((
                l["id"].as_u64()?,
                l["title"].as_str().unwrap_or_default().to_string(),
                l["description"].as_str().unwrap_or_default().to_string(),
            ))
        }));
        if count < 50 {
            break;
        }
        page += 1;
    }
    found
}

/// A fingerprint of everything this test does **not** own: the operator's
/// projects and their titles, and every label that is not one of jojobot's.
///
/// Labels are in it because they are **global** in Vikunja — the one thing a
/// mailbox run creates that is not confined to its own project, and therefore
/// the one thing a leak would leave in the operator's face.
///
/// Honest about its reach: this compares the shape of the instance, not the
/// contents of the operator's cards. A write landing on a card inside one of
/// their boards would not show up here. That case is covered where it can
/// actually be proven — the unit-level invariant test, against a fake that
/// records every project and every card any call touched.
async fn foreign_fingerprint(http: &reqwest::Client, c: &Creds) -> Vec<String> {
    let mut seen: Vec<String> = all_projects(http, c)
        .await
        .into_iter()
        .filter(|(_, title, _)| !title.starts_with(TEST_PREFIX))
        .map(|(id, title, _)| format!("project {id} {title}"))
        .chain(
            all_labels(http, c)
                .await
                .into_iter()
                .filter(|(_, title, _)| !title.starts_with(TEST_PREFIX))
                .map(|(id, title, _)| format!("label {id} {title}")),
        )
        .collect();
    seen.sort();
    seen
}

/// Delete every project and label this test created — and **nothing else**.
/// Both conditions are required: the test-only name, and jojobot's owner tag.
/// Dormant — see [`delete`].
#[allow(dead_code)]
async fn teardown(http: &reqwest::Client, c: &Creds, prefix: &str) {
    for (id, title, description) in all_projects(http, c).await {
        if title.starts_with(prefix) && description.contains(OWNER_TAG) {
            delete(http, c, &format!("/projects/{id}")).await;
        }
    }
    for (id, title, description) in all_labels(http, c).await {
        if title.starts_with(prefix) && description.contains(OWNER_TAG) {
            delete(http, c, &format!("/labels/{id}")).await;
        }
    }
}

#[tokio::test]
#[ignore = "hits real Vikunja; set JOJOBOT_VIKUNJA_URL and JOJOBOT_VIKUNJA_TOKEN"]
async fn real_vikunja_satisfies_the_contract() {
    let Some(c) = creds() else {
        eprintln!("skipping: set JOJOBOT_VIKUNJA_URL and JOJOBOT_VIKUNJA_TOKEN");
        return;
    };

    let http = reqwest::Client::new();

    let before = foreign_fingerprint(&http, &c).await;

    // Every contract case shares the ONE persistent project (see the module
    // doc): no per-case minting, no teardown. First-run truth only.
    let url = c.url.clone();
    let token = c.token.clone();
    let client = http.clone();
    let fresh = move || {
        VikunjaStore::with_project(
            client.clone(),
            VikunjaConfig {
                base_url: url.clone(),
                token: Secret::new(token.clone()),
            },
            CONTRACT_PROJECT.to_string(),
        )
    };

    // Run the shared spec in a task so a panic is caught — the fingerprint
    // check below runs either way.
    let outcome = tokio::spawn(async move { contract::run_all(fresh).await }).await;

    let after = foreign_fingerprint(&http, &c).await;
    assert_eq!(
        before, after,
        "every project and label this test does not own must be untouched"
    );

    outcome.expect("the contract must hold against real Vikunja");
}

/// The other half of the write-scope invariant, against the real API: a store is
/// only ever pointed at a project it owns, so a run against real Vikunja must
/// never so much as read the operator's boards into its own state.
///
/// Cheap and worth pinning separately: it is the assertion that would have
/// caught a discovery bug that adopted a same-named board.
#[tokio::test]
#[ignore = "hits real Vikunja; set JOJOBOT_VIKUNJA_URL and JOJOBOT_VIKUNJA_TOKEN"]
async fn a_store_never_adopts_a_board_it_did_not_create() {
    let Some(c) = creds() else {
        eprintln!("skipping: set JOJOBOT_VIKUNJA_URL and JOJOBOT_VIKUNJA_TOKEN");
        return;
    };
    let http = reqwest::Client::new();

    let store = Arc::new(VikunjaStore::with_project(
        http.clone(),
        VikunjaConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        ADOPT_PROJECT.to_string(),
    ));

    let outcome = {
        let store = store.clone();
        tokio::spawn(async move {
            // The persistent project carries prior runs' state: "inbox" may
            // already exist and hold messages, so accept a guard block on the
            // existing box and assert floors, not exact counts.
            store
                .create_mailbox(&jojobot_domain::mailbox::MailboxName("inbox".into()))
                .await
                .expect("create_mailbox call must succeed (written or blocked-as-existing)");
            contract::post(store.as_ref(), "inbox", "alpha", "the shipment landed", 0).await;
            let boxes = store.list_mailboxes().await.expect("list ok");
            assert_eq!(
                boxes.len(),
                1,
                "a store sees only the boxes of its own project: {boxes:?}"
            );
            assert!(boxes[0].counts.new >= 1);
        })
        .await
    };

    outcome.expect("the store must stay inside its own project");
}
