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
//! **The operator's real task boards live on this Vikunja.** Four things keep
//! this test away from them:
//!
//! * every project it uses is named with the [`TEST_PREFIX`] and stamped with
//!   jojobot's owner tag, and teardown deletes **only** what matches both — a
//!   board without the tag is somebody's real one and is never touched;
//! * each contract case gets its **own** throwaway project, because the spec
//!   assumes a store that starts empty; mailbox labels are namespaced by project
//!   title, so the cases cannot see each other's boxes either;
//! * each *test* owns a sub-prefix and tears down only that, because the two
//!   run in parallel in one binary;
//! * the run fingerprints every project and label it does not own, before and
//!   after, and asserts the set is unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jojobot_adapters::vikunja::{Secret, VikunjaConfig, VikunjaStore};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::mailbox::testing::contract;

/// Every project this file creates is titled under this prefix. Deliberately
/// distinct from [`VikunjaStore::DEFAULT_PROJECT`], so a run can never adopt or
/// delete the real mailbox board.
const TEST_PREFIX: &str = "jojobot-mailboxes-itest-";

/// **Each test owns a sub-prefix, and tears down only that one.** The two tests
/// live in one binary and the harness runs them in parallel, so a teardown
/// scoped to the whole file would delete the other test's projects and labels
/// out from under it, mid-run — a flake that looks exactly like a real failure.
const CONTRACT_PREFIX: &str = "jojobot-mailboxes-itest-c";
/// The adoption test's own namespace. See [`CONTRACT_PREFIX`].
const ADOPT_PREFIX: &str = "jojobot-mailboxes-itest-a";

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
    // Clean slate, in case a prior run aborted before teardown.
    teardown(&http, &c, CONTRACT_PREFIX).await;

    let before = foreign_fingerprint(&http, &c).await;

    // Each contract case gets its own throwaway project: the spec assumes a
    // store that starts empty, and mailbox labels are namespaced by project
    // title, so this isolates the boxes as well as the boards.
    let next = AtomicU64::new(0);
    let url = c.url.clone();
    let token = c.token.clone();
    let client = http.clone();
    let fresh = move || {
        let n = next.fetch_add(1, Ordering::SeqCst);
        VikunjaStore::with_project(
            client.clone(),
            VikunjaConfig {
                base_url: url.clone(),
                token: Secret::new(token.clone()),
            },
            format!("{CONTRACT_PREFIX}{n}"),
        )
    };

    // Run the shared spec in a task so a panic is caught — teardown happens
    // either way, so nothing is left behind on a failure.
    let outcome = tokio::spawn(async move { contract::run_all(fresh).await }).await;

    teardown(&http, &c, CONTRACT_PREFIX).await;

    let after = foreign_fingerprint(&http, &c).await;
    assert_eq!(
        before, after,
        "every project and label this test does not own must be untouched"
    );

    let leftovers: Vec<String> = all_labels(&http, &c)
        .await
        .into_iter()
        .filter(|(_, title, _)| title.starts_with(CONTRACT_PREFIX))
        .map(|(_, title, _)| title)
        .collect();
    assert!(
        leftovers.is_empty(),
        "labels are global in Vikunja, so a run that leaves any behind pollutes \
         the operator's label list forever: {leftovers:?}"
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
    teardown(&http, &c, ADOPT_PREFIX).await;

    let store = Arc::new(VikunjaStore::with_project(
        http.clone(),
        VikunjaConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        format!("{ADOPT_PREFIX}dopt"),
    ));

    let outcome = {
        let store = store.clone();
        tokio::spawn(async move {
            contract::create(store.as_ref(), "inbox").await;
            contract::post(store.as_ref(), "inbox", "alpha", "the shipment landed", 0).await;
            let boxes = store.list_mailboxes().await.expect("list ok");
            assert_eq!(
                boxes.len(),
                1,
                "a store sees only the boxes of its own project: {boxes:?}"
            );
            assert_eq!(boxes[0].counts.new, 1);
        })
        .await
    };

    teardown(&http, &c, ADOPT_PREFIX).await;
    outcome.expect("the store must stay inside its own project");
}
