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
//! nothing behind. Missing either variable → it PANICS: a run that reached no
//! store has verified nothing, and a green bar that says otherwise is the one
//! failure this suite cannot afford. It never scans for or hardcodes a token;
//! the token comes from the env the operator sets.

use std::sync::Arc;

use jojobot_adapters::outline::{OutlineConfig, OutlineStore, Secret};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::memory::testing::contract;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};

/// The collection this test owns end to end. NOT the real `jojobot` collection.
const TEST_COLLECTION: &str = "jojobot-test";

/// The disposable collection the owner index is asked about. Its own for the
/// same reason, and more sharply: the answer to "who is nearly this handle" is
/// a function of the WHOLE index, so a roster anything else writes into would
/// change what the screen is allowed to say. **Deliberately outside the prefix
/// sweep** — the collection belongs to one case, and it drops it itself.
const OWNERS_COLLECTION: &str = "jojobot-owners-itest";

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
        let new = NewEntity {
            parent: under,
            ..NewEntity::new(id.clone(), name, "integration-fixture")
        };
        // The child's handle contains its parent's — the natural shape of a
        // tree, and a containment near miss. It is deliberate here, so it goes
        // over the screen the way a caller says so: read the refusal, hand back
        // the token it minted.
        let written = match store
            .add_entity(new.clone())
            .await
            .expect("add_entity should succeed")
        {
            jojobot_domain::memory::Guarded::Written(entity) => entity,
            jojobot_domain::memory::Guarded::Blocked {
                attempted,
                candidates,
            } => store
                .add_entity(NewEntity {
                    override_token: Some(jojobot_domain::memory::guard::override_token(
                        &attempted,
                        &candidates,
                    )),
                    ..new
                })
                .await
                .expect("add_entity should succeed")
                .written()
                .unwrap_or_else(|| panic!("the refusal's own token must let {id} through")),
        };
        assert_eq!(&written.id, id);
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
    drop_collection(http, c, TEST_COLLECTION).await;
}

/// Delete one collection by name, and every doc in it, if it exists.
async fn drop_collection(http: &reqwest::Client, c: &Creds, name: &str) {
    if let Some(id) = find_collection(http, c, name).await {
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

/// **The owner index, answering out of real Outline.**
///
/// `create_mailbox` asks this on every box it opens — including the default
/// identity's seed on a first boot — so what it says about a handle decides
/// whether a box can be opened at all. The unit tests answer it out of a fake
/// index; here the index is documents in a real collection, read back over the
/// real API.
///
/// A collection of its own, holding exactly two entities. The screen's answer is
/// a function of the whole index, so a roster this case does not control would
/// make the candidate assertion below a statement about whatever else was
/// written that day.
async fn assert_the_owner_index_answers_from_real_memory(http: &reqwest::Client, c: &Creds) {
    use jojobot_adapters::owners::MemoryOwners;
    use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
    use jojobot_domain::memory::guard;

    let store = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        OWNERS_COLLECTION,
    );
    for (handle, name) in [("bot:gamma", "Gamma"), ("person:alpha", "Alpha")] {
        store
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "integration-fixture",
            ))
            .await
            .expect("the entity is written")
            .written()
            .expect("the roster's two handles do not collide");
    }
    let owners = MemoryOwners::new(Arc::new(store));

    assert_eq!(
        owners
            .look_up(&EntityId("bot:gamma".into()))
            .await
            .expect("a reachable entity world answers"),
        OwnerLookup::Known,
        "a handle whose page is really in the collection resolves"
    );

    // **The candidates are the assertion, not the variant.** `Unknown` alone
    // would pass over an index that screened nothing, and an empty list reads as
    // "nothing even resembles this" — which is the one thing that is false here,
    // and the answer a caller would act on by minting a second bot.
    let found = owners
        .look_up(&EntityId("bot:gamm".into()))
        .await
        .expect("a reachable entity world answers");
    let OwnerLookup::Unknown(candidates) = found else {
        panic!("a handle nobody holds does not resolve: {found:?}");
    };
    assert_eq!(
        candidates
            .iter()
            .map(|m| m.handle.as_str())
            .collect::<Vec<_>>(),
        vec!["bot:gamma"],
        "the near miss comes back off the real index, and the unrelated entity does not"
    );
    assert_eq!(candidates[0].reason, guard::MatchReason::NearSlug);

    // **An entity world that cannot be reached is an error, never "no such
    // owner".** Provoked the way it really arrives — the real host, and a token
    // it will not accept — so the refusal is the adapter's own HTTP failure
    // making the whole trip, not a double standing in for one. Reporting that
    // silence as absence would refuse a legitimate box, and do it with an empty
    // candidate list that says nothing resembles a handle nothing was read about.
    let unreachable = MemoryOwners::new(Arc::new(OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new("no-token-this-workspace-ever-minted".to_string()),
        },
        OWNERS_COLLECTION,
    )));
    let outcome = unreachable.look_up(&EntityId("bot:gamma".into())).await;
    assert!(
        matches!(outcome, Err(MailboxError::Store(_))),
        "a store that will not answer is a failure, not a verdict about the owner: {outcome:?}"
    );
}

#[tokio::test]
#[ignore = "hits real Outline; set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN"]
async fn real_outline_satisfies_the_contract() {
    // **A gate with no way to fail is not a gate.** This test returned green
    // when the credentials were absent, so a `.env` holding neither name — the
    // case the Makefile's own `.env` check does not cover — produced a passing
    // run that reached no store and proved nothing about the adapter. The
    // absence is a misconfiguration and it fails loud (rule 38). Skipping is
    // right for the recorders below, which verify nothing by design; it is
    // wrong here, where verifying is the whole job.
    let c = creds().expect(
        "this suite needs JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN, and a run without them \
         has verified nothing — see the Makefile's integration rule",
    );

    let http = reqwest::Client::new();
    // Clean slate, in case a prior run aborted before teardown.
    drop_test_collection(&http, &c).await;
    drop_collection(&http, &c, OWNERS_COLLECTION).await;

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
    let indexed =
        Arc::new(IndexedMemory::new(Arc::new(store.clone())).expect("the search index opens"));
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
        contract::run_all_searchable(
            indexed.as_ref(),
            &Retrieval::new(indexed.index(), vec![indexed.clone()]),
        )
        .await;
        assert_a_child_page_is_nested(&http_for_spec, &creds_for_spec, &store).await;
        assert_the_owner_index_answers_from_real_memory(&http_for_spec, &creds_for_spec).await;
    })
    .await;

    drop_test_collection(&http, &c).await;
    drop_collection(&http, &c, OWNERS_COLLECTION).await;

    let jojobot_after = doc_id_fingerprint(&http, &c, "jojobot").await;
    assert_eq!(
        jojobot_before, jojobot_after,
        "the real `jojobot` collection must be untouched by the test"
    );

    outcome.expect("the contract must hold against real Outline");
}

/// **The golden fixture recorder — run by hand, never by the suite.**
///
/// It writes a battery of records through real Outline, reads the raw document
/// text back exactly as the store returns it, and saves that text beside the
/// records it should parse into. The fast tier then asserts those bytes parse
/// forever ([`codec::tests::the_golden_fixtures_still_parse`]).
///
/// **Deliberately not part of `real_outline_satisfies_the_contract`.** A
/// recorder that ran with the suite would rewrite the goldens to match
/// whatever the store does today — so the day the store starts mangling
/// something, the fixture would quietly move and every test would stay green.
/// A golden that re-records itself is not a golden. Running this is a decision,
/// and the diff it produces is the thing a reviewer reads.
///
/// **`#[ignore]` is not enough to hold that line**, which is why there is an
/// env gate too: `make integration` runs the ignored tests, so on its first
/// run the recorder rode along — rewriting the checked-in goldens inside the
/// very command meant to be checking them, and racing the contract suite for
/// the same collection while it did.
///
/// ```sh
/// nix develop -c bash -c 'set -a; . ./.env; set +a; JOJOBOT_RECORD_GOLDENS=1 \
///   cargo test -p jojobot-adapters --test outline_integration \
///   record_the_golden_fixtures -- --ignored --nocapture'
/// ```
#[tokio::test]
#[ignore]
async fn record_the_golden_fixtures() {
    // **Skipping is the right answer here and nowhere else in this file.**
    // This is a TOOL, not a check: it verifies nothing, so a run that does not
    // perform it has not failed to verify anything — which is the opposite of
    // the suites the Makefile rule refuses to let skip.
    if std::env::var("JOJOBOT_RECORD_GOLDENS")
        .ok()
        .filter(|v| !v.is_empty())
        .is_none()
    {
        println!(
            "SKIPPED: the golden recorder rewrites checked-in fixtures, so it runs only when \
             asked for by name — set JOJOBOT_RECORD_GOLDENS=1. Nothing was recorded."
        );
        return;
    }
    let c =
        creds().expect("the recorder needs credentials — a skipped recording is not a recording");
    let http = reqwest::Client::new();
    drop_test_collection(&http, &c).await;

    let store = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        TEST_COLLECTION,
    );

    let mut recorded: Vec<(String, EntityId)> = Vec::new();
    for (name, subject) in golden_cases(&store).await {
        recorded.push((name, subject));
    }

    let docs = raw_documents(&http, &c, TEST_COLLECTION).await;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/facts");
    std::fs::create_dir_all(&dir).expect("the fixture directory");

    for (name, subject) in &recorded {
        let text = docs
            .iter()
            .find(|d| {
                d["text"]
                    .as_str()
                    .is_some_and(|t| t.lines().any(|l| l.trim() == format!("id: {subject}")))
            })
            .and_then(|d| d["text"].as_str())
            .unwrap_or_else(|| panic!("{subject}'s page must come back"));

        let facts = store.recall(subject).await.expect("recall should succeed");
        std::fs::write(dir.join(format!("{name}.md")), text).expect("write the page");
        std::fs::write(
            dir.join(format!("{name}.json")),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&facts).expect("facts serialize")
            ),
        )
        .expect("write the expectation");
        println!("RECORDED {name} ({} rows)", facts.len());
    }

    drop_test_collection(&http, &c).await;
}

/// The battery, written through the store so that whatever it does to them is
/// what lands in the fixture.
///
/// **Every case is something the code actually writes**, and the punctuation
/// ones are not decoration: each character in them was mangled by real Outline
/// at some point in this record's short life.
async fn golden_cases(store: &OutlineStore) -> Vec<(String, EntityId)> {
    use jojobot_domain::memory::event::Event;
    use jojobot_domain::memory::{Edge, EdgeShape, NewFact, Provenance};

    let mut out = Vec::new();
    let ensure = |slug: &str| EntityId::person(slug);

    // 1. An event carrying the punctuation the store has opinions about.
    let punctuated = ensure("golden-punctuation");
    store
        .add_entity(NewEntity::new(
            punctuated.clone(),
            "Golden Punctuation",
            "golden",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("spaced".into(), "a value with spaces".to_string());
    metadata.insert("equals".into(), "a = b".to_string());
    metadata.insert("backslash".into(), "c:\\dir\\file".to_string());
    metadata.insert("tilde".into(), "a~b~c".to_string());
    metadata.insert(
        "markup".into(),
        "<b>bold</b> & *starred* _under_".to_string(),
    );
    metadata.insert("quoted".into(), "\"double\" and 'single'".to_string());
    metadata.insert("unicode".into(), "café — ünïcode ✓".to_string());
    metadata.insert("percent".into(), "already %20 encoded".to_string());
    metadata.insert("empty".into(), String::new());
    store
        .capture(NewFact {
            event: Some(Event {
                kind: "a type with spaces".into(),
                metadata,
                refs: vec![punctuated.clone()],
            }),
            ..NewFact::about(
                punctuated.clone(),
                "an event whose payload is made of the store's own syntax",
                jiff::civil::date(2026, 7, 29),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    out.push(("event-punctuation".to_string(), punctuated));

    // 2. A retraction: two rows, one marked, in one page.
    let taken_back = ensure("golden-retraction");
    store
        .add_entity(NewEntity::new(
            taken_back.clone(),
            "Golden Retraction",
            "golden",
        ))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let event = store
        .capture(NewFact {
            event: Some(Event::of("an-appointment")),
            ..NewFact::about(
                taken_back.clone(),
                "moved to the 14th",
                jiff::civil::date(2026, 7, 28),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    store
        .retract(
            &event.address(),
            Some("it was rebooked twice | and the pipe is deliberate"),
            jiff::civil::date(2026, 7, 29),
        )
        .await
        .expect("retract ok");
    out.push(("retraction".to_string(), taken_back));

    // 3. An ordinary fact table: an edge, testimony, details, a pipe in the
    //    content — the shapes that predate events and must not regress.
    let plain = ensure("golden-plain");
    let place = EntityId::new(EntityKind::Place, "golden-north-trail");
    for (id, name) in [(&plain, "Golden Plain"), (&place, "Golden North Trail")] {
        store
            .add_entity(NewEntity::new(id.clone(), name, "golden"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }
    store
        .capture(NewFact {
            details: Some("with details | carrying a pipe".into()),
            provenance: Provenance::Testimony,
            edge: Some(Edge::new(EdgeShape::Location, place)),
            ..NewFact::about(
                plain.clone(),
                "a claim | with a pipe in it",
                jiff::civil::date(2026, 7, 27),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    out.push(("plain-fact".to_string(), plain));

    out
}
