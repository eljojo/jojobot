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
//! Once explicitly invoked (`--ignored`), absent credentials **panic**: a gate
//! that prints "skipping" and exits green is a run that verified nothing while
//! reading as if it had. The tests also **serialize themselves** on a shared
//! lock — green must not depend on the invoker remembering `--test-threads=1`,
//! and real Vikunja 500s under concurrent writes (SQLite-lock class).
//!
//! **The operator's real task boards live on this Vikunja.** Five things keep
//! this test away from them — starting with a **disposability probe**: the
//! suite's first act is to create and delete a canary project, and an instance
//! that refuses the delete is not a disposable test server, so the run panics
//! before any contract case exists. Then:
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
//!   after, and asserts the set is unchanged. The fingerprint is a **hash**: a
//!   mismatch must fail the run without dumping the operator's board names
//!   into a log.

use std::hash::{DefaultHasher, Hash, Hasher};
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

/// The gate's shared lock. The two tests live in one binary and the harness
/// runs them in parallel by default; real Vikunja (SQLite) answers concurrent
/// writes with 500s, so each test holds this for its whole body — green never
/// depends on the invoker passing `--test-threads=1`.
static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Creds {
    url: String,
    token: String,
}

/// The credentials, or a **panic**. This test only runs when explicitly asked
/// for (`--ignored`); if the credentials are then absent, printing "skipping"
/// and exiting green would report a verification that never happened.
fn creds() -> Creds {
    let require = |key: &str| {
        std::env::var(key)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set. This gate was explicitly invoked, and a skipped gate \
                     must never read as green — set JOJOBOT_VIKUNJA_URL and \
                     JOJOBOT_VIKUNJA_TOKEN to the DISPOSABLE test instance."
                )
            })
    };
    Creds {
        url: require("JOJOBOT_VIKUNJA_URL"),
        token: require("JOJOBOT_VIKUNJA_TOKEN"),
    }
}

/// **The disposability probe — the suite's first act.** Create a canary
/// project and immediately delete it. An instance whose token cannot delete is
/// the operator's production instance (its tokens cannot delete by design), or
/// otherwise not a server this suite may litter — either way the run stops
/// before a single contract case exists, making the real instance mechanically
/// unreachable by the destructive path.
/// The canary is titled under the CALLING test's own `prefix`, so a canary
/// leaked by a crash between its create and its delete is cleaned up by that
/// test's next clean-slate teardown — by construction, not by a prefix
/// coincidence.
async fn assert_disposable(http: &reqwest::Client, c: &Creds, prefix: &str) {
    let resp = http
        .put(format!("{}/api/v1/projects", c.url.trim_end_matches('/')))
        .bearer_auth(&c.token)
        .json(&serde_json::json!({
            "title": format!("{prefix}-canary"),
            "description": format!("disposability probe — safe to delete. {OWNER_TAG}"),
        }))
        .send()
        .await
        .unwrap_or_else(|e| panic!("disposability probe: creating the canary failed: {e}"));
    assert!(
        resp.status().is_success(),
        "disposability probe: creating the canary returned {}",
        resp.status()
    );
    let body: serde_json::Value = resp
        .json()
        .await
        .unwrap_or_else(|e| panic!("disposability probe: canary body: {e}"));
    let id = body["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("disposability probe: the canary came back with no id"));

    let deleted = http
        .delete(format!("{}/api/v1/projects/{id}", c.url.trim_end_matches('/')))
        .bearer_auth(&c.token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("disposability probe: deleting the canary failed: {e}"));
    assert!(
        deleted.status().is_success(),
        "this instance is not disposable — the canary project could not be deleted \
         (HTTP {}). Point the gate at the test server.",
        deleted.status()
    );
}

/// A client that fails a stuck request instead of hanging the gate forever —
/// reqwest has no default timeout, so without one a wedged request reads as a
/// dead server until someone kills the run.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("building the gate's HTTP client cannot fail")
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
///
/// Stops on an **empty** page, never a short one — Vikunja serves the page
/// size it decides, not the one requested, so "fewer than 50" can be true on
/// every page. The same rule the store's own loops follow; a teardown or
/// fingerprint that read only page one would silently narrow the safety net.
///
/// Emptiness is judged AFTER filtering: Vikunja appends pseudo-projects with
/// negative ids (Favorites, "My Open Tasks") to every page of `/projects`, so
/// a raw page is never empty and an until-raw-empty loop never terminates.
/// The `as_u64` filter is what drops them; the store's `owned_projects`
/// terminates by the same mechanism (`project_rec` rejects pseudo ids before
/// its empty check).
async fn all_projects(http: &reqwest::Client, c: &Creds) -> Vec<(u64, String, String)> {
    let mut found = Vec::new();
    let mut page = 1;
    loop {
        let body = get(http, c, &format!("/projects?page={page}&per_page=50")).await;
        let items: Vec<(u64, String, String)> = body
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| {
                Some((
                    p["id"].as_u64()?,
                    p["title"].as_str().unwrap_or_default().to_string(),
                    p["description"].as_str().unwrap_or_default().to_string(),
                ))
            })
            .collect();
        if items.is_empty() {
            break;
        }
        found.extend(items);
        page += 1;
    }
    found
}

/// Every label, paged in full, as `(id, title, description)`. Stops on an
/// empty page, never a short one, judged after the same filter — see
/// [`all_projects`] for both gotchas.
async fn all_labels(http: &reqwest::Client, c: &Creds) -> Vec<(u64, String, String)> {
    let mut found = Vec::new();
    let mut page = 1;
    loop {
        let body = get(http, c, &format!("/labels?page={page}&per_page=50")).await;
        let items: Vec<(u64, String, String)> = body
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|l| {
                Some((
                    l["id"].as_u64()?,
                    l["title"].as_str().unwrap_or_default().to_string(),
                    l["description"].as_str().unwrap_or_default().to_string(),
                ))
            })
            .collect();
        if items.is_empty() {
            break;
        }
        found.extend(items);
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
///
/// Returned as a **hash**, deliberately: a mismatch must fail the run without
/// printing every project and label title on the instance — a gate failure is
/// not a licence to dump an operator's board names into a log.
async fn foreign_fingerprint(http: &reqwest::Client, c: &Creds) -> u64 {
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
    let mut hasher = DefaultHasher::new();
    seen.hash(&mut hasher);
    hasher.finish()
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
    let _serialized = GATE.lock().await;
    let c = creds();

    let http = http_client();
    assert_disposable(&http, &c, CONTRACT_PREFIX).await;
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
        "the set of projects and labels this test does not own changed \
         (fingerprint {before:x} → {after:x}). Inspect the instance directly; \
         titles are deliberately not printed here."
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
    let _serialized = GATE.lock().await;
    let c = creds();
    let http = http_client();
    assert_disposable(&http, &c, ADOPT_PREFIX).await;
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
