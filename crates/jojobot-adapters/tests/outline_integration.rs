//! Gated integration test: the shared Memory contract, run against real
//! Outline. This is the "prove the real adapter conforms" half of the contract
//! strategy — the fast tier already ran the same spec against the fake.
//!
//! It is `#[ignore]` and env-gated on **credentials only** (no ids — the whole
//! point of the rework), so default `cargo test` and CI never touch the network:
//!
//! ```sh
//! JOJOBOT_OUTLINE_URL=https://wiki.example.org \
//! JOJOBOT_OUTLINE_TOKEN=... \
//!   cargo test -p jojobot-adapters --test outline_integration -- --ignored
//! ```
//!
//! Convention over configuration: the adapter discovers/creates a collection by
//! name. This test points it at a dedicated **`jojobot-test`** collection, which
//! it deletes (collection + all its docs) before and after the run — so it never
//! touches the real `jojobot` collection or any of the user’s own docs, and it leaves
//! nothing behind. Missing either variable → it skips. It never scans for or
//! hardcodes a token; the token comes from the env the operator sets.

use std::sync::Arc;

use jojobot_adapters::outline::{OutlineConfig, OutlineStore, Secret};
use jojobot_adapters::search::IndexedMemory;
use jojobot_domain::memory::testing::contract;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};

/// The collection this test owns end to end. NOT the real `jojobot` collection.
const TEST_COLLECTION: &str = "jojobot-test";

struct Creds {
    url: String,
    token: String,
}

fn creds() -> Option<Creds> {
    Some(Creds {
        url: std::env::var("JOJOBOT_OUTLINE_URL")
            .ok()
            .filter(|s| !s.is_empty())?,
        token: std::env::var("JOJOBOT_OUTLINE_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())?,
    })
}

/// Find a collection id by name, paging through `collections.list`.
async fn find_collection(http: &reqwest::Client, c: &Creds, name: &str) -> Option<String> {
    let mut offset = 0u64;
    loop {
        let page: serde_json::Value = http
            .post(format!("{}/api/collections.list", c.url))
            .bearer_auth(&c.token)
            .json(&serde_json::json!({ "offset": offset, "limit": 100 }))
            .send()
            .await
            .expect("collections.list")
            .json()
            .await
            .expect("collections.list body");
        let items = page["data"].as_array().cloned().unwrap_or_default();
        if let Some(found) = items.iter().find(|c| c["name"].as_str() == Some(name)) {
            return found["id"].as_str().map(str::to_string);
        }
        if items.len() < 100 {
            return None;
        }
        offset += 100;
    }
}

/// The sorted document ids in a collection — a fingerprint to prove it's
/// untouched. Empty if the collection doesn't exist.
async fn doc_id_fingerprint(http: &reqwest::Client, c: &Creds, name: &str) -> Vec<String> {
    let Some(id) = find_collection(http, c, name).await else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let mut offset = 0u64;
    loop {
        let page: serde_json::Value = http
            .post(format!("{}/api/documents.list", c.url))
            .bearer_auth(&c.token)
            .json(&serde_json::json!({ "collectionId": id, "offset": offset, "limit": 100 }))
            .send()
            .await
            .expect("documents.list")
            .json()
            .await
            .expect("documents.list body");
        let items = page["data"].as_array().cloned().unwrap_or_default();
        let n = items.len();
        ids.extend(
            items
                .iter()
                .filter_map(|d| d["id"].as_str().map(str::to_string)),
        );
        if n < 100 {
            break;
        }
        offset += 100;
    }
    ids.sort();
    ids
}

/// Every document in a collection, raw, as the API returns it — so an
/// assertion about a doc's **position** reads Outline's own answer rather than
/// anything the adapter believes.
async fn raw_documents(http: &reqwest::Client, c: &Creds, name: &str) -> Vec<serde_json::Value> {
    let Some(id) = find_collection(http, c, name).await else {
        return Vec::new();
    };
    let mut docs = Vec::new();
    let mut offset = 0u64;
    loop {
        let page: serde_json::Value = http
            .post(format!("{}/api/documents.list", c.url))
            .bearer_auth(&c.token)
            .json(&serde_json::json!({ "collectionId": id, "offset": offset, "limit": 100 }))
            .send()
            .await
            .expect("documents.list")
            .json()
            .await
            .expect("documents.list body");
        let items = page["data"].as_array().cloned().unwrap_or_default();
        let n = items.len();
        docs.extend(items);
        if n < 100 {
            break;
        }
        offset += 100;
    }
    docs
}

/// **A child entity's page is really nested under its parent's page.** The
/// contract proves the tree round-trips; it cannot prove *where the page sits*,
/// because that is Outline's word and no other store has one. This reads the
/// raw `parentDocumentId` back off the API.
///
/// It also pins the assumption the whole index rests on: **`documents.list`
/// returns nested documents too.** If it did not, every child would be missing
/// from `entity_index` and jojobot would quietly forget half its store.
async fn assert_a_child_page_is_nested(http: &reqwest::Client, c: &Creds, store: &OutlineStore) {
    let parent = EntityId::new(EntityKind::Project, "integration-monorail");
    let child = EntityId::new(EntityKind::Project, "integration-monorail-track");
    for (id, name, under) in [
        (&parent, "Integration Monorail", None),
        (&child, "Integration Monorail Track", Some(parent.clone())),
    ] {
        store
            .add_entity(NewEntity {
                parent: under,
                ..NewEntity::new(id.clone(), name, "integration-fixture")
            })
            .await
            .expect("add_entity should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block {id}"));
    }

    let docs = raw_documents(http, c, TEST_COLLECTION).await;
    // The marker is matched as a WHOLE LINE. One handle here is a prefix of the
    // other — which is the natural shape of a tree, a detail page named after
    // the thing it details — so a `contains` picks the child's page when asked
    // for the parent's, and the real store is where that showed up.
    let doc_for = |handle: &EntityId| {
        docs.iter()
            .find(|d| {
                d["text"]
                    .as_str()
                    .is_some_and(|t| t.lines().any(|l| l.trim() == format!("id: {handle}")))
            })
            .unwrap_or_else(|| {
                panic!("documents.list must return {handle}'s page — nested pages included")
            })
    };

    assert_eq!(
        doc_for(&parent)["parentDocumentId"].as_str(),
        None,
        "a root sits at the top of the collection"
    );
    assert_eq!(
        doc_for(&child)["parentDocumentId"].as_str(),
        doc_for(&parent)["id"].as_str(),
        "the child's page hangs off the parent's, by Outline's own account"
    );
    assert_eq!(
        store.children(&parent).await.expect("children"),
        vec![child.clone()],
        "…and the tree reads back from the real store"
    );
}

/// Delete the test collection (and every doc in it), if it exists.
async fn drop_test_collection(http: &reqwest::Client, c: &Creds) {
    if let Some(id) = find_collection(http, c, TEST_COLLECTION).await {
        let resp = http
            .post(format!("{}/api/collections.delete", c.url))
            .bearer_auth(&c.token)
            .json(&serde_json::json!({ "id": id }))
            .send()
            .await
            .expect("collections.delete");
        assert!(
            resp.status().is_success(),
            "teardown failed: {}",
            resp.status()
        );
    }
}

#[tokio::test]
#[ignore = "hits real Outline; set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN"]
async fn real_outline_satisfies_the_contract() {
    let Some(c) = creds() else {
        eprintln!("skipping: set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN");
        return;
    };

    let http = reqwest::Client::new();
    // Clean slate, in case a prior run aborted before teardown.
    drop_test_collection(&http, &c).await;

    // Fingerprint the real `jojobot` collection: the test must not touch it.
    let jojobot_before = doc_id_fingerprint(&http, &c, "jojobot").await;

    let store = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        TEST_COLLECTION,
    );

    // The spec runs against the store **behind the search projection**, so the
    // retrieval half is proven against real Outline too: the index is fed by the
    // real scan (real prose, real fact tables), not by a fake's approximation.
    let indexed = IndexedMemory::new(Arc::new(store.clone())).expect("the search index opens");
    indexed.rebuild().await.expect("the boot scan must succeed");

    // Run the shared spec in a task so a panic is caught — the test collection
    // is dropped either way, so nothing is left behind. The page-nesting check
    // rides in the same task, and after the spec: it is the one assertion the
    // store-agnostic contract cannot make, because where a page SITS is
    // Outline's word and no other store has one.
    let http_for_spec = http.clone();
    let creds_for_spec = Creds {
        url: c.url.clone(),
        token: c.token.clone(),
    };
    let outcome = tokio::spawn(async move {
        contract::run_all_searchable(&indexed).await;
        assert_a_child_page_is_nested(&http_for_spec, &creds_for_spec, &store).await;
    })
    .await;

    drop_test_collection(&http, &c).await;

    let jojobot_after = doc_id_fingerprint(&http, &c, "jojobot").await;
    assert_eq!(
        jojobot_before, jojobot_after,
        "the real `jojobot` collection must be untouched by the test"
    );

    outcome.expect("the contract must hold against real Outline");
}
