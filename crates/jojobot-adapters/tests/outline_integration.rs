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
use std::sync::atomic::{AtomicU64, Ordering};

use jojobot_adapters::outline::{OutlineConfig, OutlineStore, Secret};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::contract as mailboxes;
use jojobot_domain::memory::testing::contract;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};
use jojobot_domain::session::testing::contract as sessions;
use jojobot_domain::session::{NewSession, Sessions, Sid};

/// The collection this test owns end to end. NOT the real `jojobot` collection.
const TEST_COLLECTION: &str = "jojobot-test";

/// Every throwaway collection the session contract creates is named under this
/// prefix — one per case, because the spec assumes a store that starts empty.
/// Deliberately distinct from both the real collection and [`TEST_COLLECTION`].
const SESSION_PREFIX: &str = "jojobot-sessions-itest-";
const MAILBOX_PREFIX: &str = "jojobot-mailboxes-itest-";
/// The disposable collection the handover reads from. Its own, so the board it
/// holds is one this test built and nothing else wrote into.
const HANDOVER_COLLECTION: &str = "jojobot-handover-itest";
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

/// Delete every throwaway collection the session contract created.
async fn drop_session_collections(http: &reqwest::Client, c: &Creds) {
    loop {
        let (_, page) = (
            (),
            http.post(format!("{}/api/collections.list", c.url))
                .bearer_auth(&c.token)
                .json(&serde_json::json!({ "limit": 100 }))
                .send()
                .await
                .expect("collections.list")
                .json::<serde_json::Value>()
                .await
                .expect("collections.list body"),
        );
        let mine: Vec<String> = page["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|c| {
                c["name"].as_str().is_some_and(|n| {
                    n.starts_with(SESSION_PREFIX)
                        || n.starts_with(MAILBOX_PREFIX)
                        || n == HANDOVER_COLLECTION
                })
            })
            .filter_map(|c| c["id"].as_str().map(str::to_string))
            .collect();
        if mine.is_empty() {
            return;
        }
        for id in mine {
            http.post(format!("{}/api/collections.delete", c.url))
                .bearer_auth(&c.token)
                .json(&serde_json::json!({ "id": id }))
                .send()
                .await
                .expect("collections.delete");
        }
    }
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

/// **The Sessions contract, against real Outline, with no edit to the spec.**
/// The same suite the fake satisfies.
///
/// A throwaway collection per case, because the spec assumes a store that
/// starts empty.
///
/// `all_sessions` earns its own assertion afterwards: it is what the handle
/// registry is rebuilt from at startup, and it is the one read that spans
/// pages — so a bug in it is a restart that silently forgets every session
/// of every bot but one.
/// **The Mailboxes contract, against real Outline.**
///
/// A break in body escaping, cell escaping, id minting over ragged rows or
/// notes flattening must never ship with the whole workspace green.
///
/// A collection per case, like the sessions run: the spec wants a fresh store
/// each time, and the owners are written into each because a box belongs to a
/// bot by construction and this store resolves that by reading Memory.
async fn assert_the_mailbox_contract_holds(http: &reqwest::Client, c: &Creds) {
    let next = AtomicU64::new(0);
    let url = c.url.clone();
    let token = c.token.clone();
    let client = http.clone();
    mailboxes::run_all(move || {
        let n = next.fetch_add(1, Ordering::SeqCst);
        let store = OutlineStore::with_collection(
            client.clone(),
            OutlineConfig {
                base_url: url.clone(),
                token: Secret::new(token.clone()),
            },
            format!("{MAILBOX_PREFIX}{n}"),
        );
        async move {
            for owner in mailboxes::OWNERS {
                store
                    .add_entity(NewEntity {
                        id: EntityId((*owner).to_string()),
                        name: owner.trim_start_matches("bot:").to_string(),
                        aliases: Vec::new(),
                        source: "user-named".into(),
                        crm: None,
                        parent: None,
                        boot: Default::default(),
                        override_token: None,
                    })
                    .await
                    .expect("the owner is written")
                    .written()
                    .expect("not blocked");
            }
            store.mailboxes()
        }
    })
    .await;
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

async fn assert_the_session_contract_holds(http: &reqwest::Client, c: &Creds) {
    let next = AtomicU64::new(0);
    let url = c.url.clone();
    let token = c.token.clone();
    let client = http.clone();
    let fresh = move || {
        let n = next.fetch_add(1, Ordering::SeqCst);
        OutlineStore::with_collection(
            client.clone(),
            OutlineConfig {
                base_url: url.clone(),
                token: Secret::new(token.clone()),
            },
            format!("{SESSION_PREFIX}{n}"),
        )
        .sessions()
    };
    sessions::run_all(fresh).await;

    // Two bots, two pages, one read. A registry rebuilt from this has to see
    // both — the failure it guards against is a restart in which every bot but
    // one loses its handles.
    let store = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        format!("{SESSION_PREFIX}across"),
    )
    .sessions();
    let mut begun = Vec::new();
    for (slug, handle) in [("gamma", "ab12"), ("delta", "cd34")] {
        begun.push(
            store
                .begin(NewSession {
                    bot: EntityId::new(EntityKind::Bot, slug),
                    sid: Sid(handle.into()),
                    focus: format!("what {slug} is doing"),
                    started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
                })
                .await
                .expect("begin should succeed"),
        );
    }

    let all = store
        .all_sessions()
        .await
        .expect("all_sessions should succeed");
    for session in &begun {
        let seen = all
            .iter()
            .find(|s| s.id == session.id)
            .unwrap_or_else(|| panic!("all_sessions must span pages, missing {}", session.id));
        assert_eq!(
            seen.sid, session.sid,
            "the handle rides on the row, or a restart cannot rebuild the registry"
        );
        assert_eq!(seen.bot, session.bot, "and each knows whose run it is");
    }
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
    drop_session_collections(&http, &c).await;

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
        assert_the_session_contract_holds(&http_for_spec, &creds_for_spec).await;
        assert_the_mailbox_contract_holds(&http_for_spec, &creds_for_spec).await;
        assert_the_owner_index_answers_from_real_memory(&http_for_spec, &creds_for_spec).await;
    })
    .await;

    drop_test_collection(&http, &c).await;
    drop_collection(&http, &c, OWNERS_COLLECTION).await;
    drop_session_collections(&http, &c).await;

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

/// **The battery, kept in step with the crate's own copy by name.** The text
/// lives in `outline::golden::BATTERY`, which the fast tests read; this is the
/// recorder's view of the same list. It is duplicated deliberately rather than
/// exported: the crate half is `#[cfg(test)]`, and widening a module's
/// visibility so a recorder can see it would put test scaffolding into the
/// shipped surface.
const GOLDEN_BATTERY: &[(&str, &str)] = &[
    ("tilde", "a ~ b ~ c"),
    ("lettered-list", "a) first\nb) second\nc) third"),
    ("numbered-list", "1. first\n2. second\n7. out of order"),
    ("bulleted-list", "- first\n* second\n+ third"),
    ("line-start-syntax", "# heading\n> quoted\n---"),
    ("emphasis", "_under_ *star* **bold** `tick`"),
    // **Snake case, because our own cells are full of it** — identifiers,
    // test names, function names. If the store reads a snake-cased token as an
    // emphasis run, the class of subjects that cannot be written is most of
    // what this project writes rather than an exotic corner.
    (
        "snake-case",
        "parse_bodies and same_cell_value in mailbox_codec",
    ),
    ("angle-brackets", "<b>bold</b> & an <email@example.test>"),
    ("backslash", "c:\\dir\\file and a trailing \\"),
    ("pipe", "a | b | c"),
    ("indented", "    four spaces\n\tand a tab"),
    ("unicode", "café — ünïcode ✓ 🎯"),
    ("blank-lines", "first\n\n\nlast"),
];

/// Put a page up verbatim and read back exactly what the store made of it.
///
/// **The raw API on purpose.** Going through a port would run the read-back
/// guard, and the guard refuses precisely the writes worth measuring — so the
/// interesting page would never reach disk and the fixture would record only
/// the cases nobody doubted.
async fn round_trip_page(http: &reqwest::Client, c: &Creds, doc_id: &str, text: &str) -> String {
    let resp = http
        .post(format!("{}/api/documents.update", c.url))
        .bearer_auth(&c.token)
        .json(&serde_json::json!({ "id": doc_id, "text": text }))
        .send()
        .await
        .expect("documents.update");
    assert!(
        resp.status().is_success(),
        "update failed: {}",
        resp.status()
    );
    let back: serde_json::Value = http
        .post(format!("{}/api/documents.info", c.url))
        .bearer_auth(&c.token)
        .json(&serde_json::json!({ "id": doc_id }))
        .send()
        .await
        .expect("documents.info")
        .json()
        .await
        .expect("documents.info json");
    back["data"]["text"]
        .as_str()
        .expect("the page comes back with text")
        .to_string()
}

/// **Record the mail and session rails' golden pages.** Same gate and same
/// reasoning as the fact-table recorder above: a golden that re-records itself
/// inside the checking command is not a golden.
#[tokio::test]
#[ignore]
async fn record_the_rail_goldens() {
    if std::env::var("JOJOBOT_RECORD_GOLDENS")
        .ok()
        .filter(|v| !v.is_empty())
        .is_none()
    {
        println!("SKIPPED: set JOJOBOT_RECORD_GOLDENS=1 to rewrite the checked-in rail fixtures.");
        return;
    }
    let c = creds().expect("the recorder needs credentials");
    let http = reqwest::Client::new();
    drop_test_collection(&http, &c).await;
    drop_session_collections(&http, &c).await;

    let store = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        TEST_COLLECTION,
    );
    let owner = EntityId::new(EntityKind::Bot, "gamma");
    store
        .add_entity(NewEntity::new(owner.clone(), "Gamma", "golden"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");

    // A benign message and a benign session, written through the PORTS — so
    // the page SHAPE is the one production really produces, and only the text
    // inside it is the battery's.
    use jojobot_domain::mailbox::Mailboxes as _;
    let boxes = store.mailboxes();
    let sessions = store.sessions();
    let anchor_subject = "SUBJECT-ANCHOR";
    let anchor_body = "BODY-ANCHOR";
    let anchor_focus = "FOCUS-ANCHOR";
    let anchor_entry = "ENTRY-ANCHOR";

    boxes
        .create_mailbox(
            &jojobot_domain::mailbox::MailboxName("gamma".into()),
            &owner,
            None,
        )
        .await
        .expect("the box opens")
        .written()
        .expect("the owner exists, so it is not blocked");
    boxes
        .post_message(jojobot_domain::mailbox::NewMessage {
            mailbox: jojobot_domain::mailbox::MailboxName("gamma".into()),
            body: anchor_body.into(),
            sender: owner.to_string(),
            subject: Some(anchor_subject.into()),
            sent_at: jiff::Timestamp::UNIX_EPOCH,
            in_reply_to: None,
        })
        .await
        .expect("the anchor message posts")
        .written()
        .expect("not blocked");

    let session = sessions
        .begin(NewSession {
            bot: owner.clone(),
            sid: Sid("aa11".into()),
            focus: anchor_focus.into(),
            started_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .await
        .expect("the anchor session begins");
    sessions
        .append(
            &session.id,
            jojobot_domain::session::NewEntry {
                text: anchor_entry.into(),
                at: jiff::Timestamp::UNIX_EPOCH,
                beat: None,
            },
        )
        .await
        .expect("the anchor entry lands");

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    for (rail, anchors) in [
        ("mailboxes", [anchor_subject, anchor_body]),
        ("sessions", [anchor_focus, anchor_entry]),
    ] {
        std::fs::create_dir_all(dir.join(rail)).expect("the fixture directory");
        let docs = raw_documents(&http, &c, TEST_COLLECTION).await;
        let page = docs
            .iter()
            .find(|d| {
                d["text"]
                    .as_str()
                    .is_some_and(|t| t.contains(anchors[0]) && t.contains(anchors[1]))
            })
            .unwrap_or_else(|| panic!("{rail}'s anchor page must be on the board"));
        let doc_id = page["id"].as_str().expect("a doc id").to_string();
        let clean = page["text"].as_str().expect("text").to_string();

        for (name, text) in GOLDEN_BATTERY {
            // A cell is one line, so the cell position gets the text flattened
            // — which is what the domain does to it anyway.
            let flat = text.replace('\n', " ");
            let put = clean.replace(anchors[0], &flat).replace(anchors[1], text);
            let got = round_trip_page(&http, &c, &doc_id, &put).await;

            std::fs::write(dir.join(rail).join(format!("{name}.md")), &got).expect("write");
            println!("RECORDED {rail}/{name} ({} bytes)", got.len());
        }
        // Put the page back the way it was found, so the next rail's lookup is
        // not confused by a page still wearing the last battery entry.
        round_trip_page(&http, &c, &doc_id, &clean).await;
    }

    drop_test_collection(&http, &c).await;
    drop_session_collections(&http, &c).await;
}

/// **The one-time handover, against a real board.**
///
/// The fast tests prove the carry over a fake source. This one proves it over
/// the store the records actually live in — where a body has been through a
/// document editor, a state is a column on a page, and a chronology is rows
/// somebody's session wrote. A migration proven only against a fake is a
/// migration whose verification has never itself been verified.
///
/// **It enters where a boot enters — `carry_over`, never `run`.** The record,
/// its two states and the already-carried answer all sit above `run`, and a
/// suite that stepped past them would prove the carrying and nothing about the
/// decision to do it. The one exception is marked where it stands.
///
/// **It reads a disposable collection this test builds, and never the real
/// one.** The operator's own board is not this suite's to read, and a handover
/// is exactly the operation where that would matter most.
#[tokio::test]
#[ignore = "hits real Outline; set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN"]
async fn the_handover_carries_a_real_board_across() {
    use jojobot_adapters::dolt::{
        Dolt, handover, mailboxes::DoltMailboxes, migrate, sessions::DoltSessions,
    };
    use jojobot_domain::mailbox::{
        Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
        MessageId, MessageState, NewMessage, OwnerIndex, OwnerLookup, StateCounts,
    };
    use jojobot_domain::session::{
        EntryId, JournalEntry, NewEntry, Session, SessionError, SessionId, SessionState,
    };

    let c = creds().expect(
        "this suite needs JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN, and a run without them \
         has verified nothing",
    );
    let http = reqwest::Client::new();
    drop_session_collections(&http, &c).await;

    let old = OutlineStore::with_collection(
        http.clone(),
        OutlineConfig {
            base_url: c.url.clone(),
            token: Secret::new(c.token.clone()),
        },
        HANDOVER_COLLECTION,
    );

    // --- the old board, built through the real store's own verbs -----------
    let owner = EntityId("bot:gamma".into());
    old.add_entity(NewEntity {
        id: owner.clone(),
        name: "gamma".into(),
        aliases: Vec::new(),
        source: "user-named".into(),
        crm: None,
        parent: None,
        boot: Default::default(),
        override_token: None,
    })
    .await
    .expect("the owner is written")
    .written()
    .expect("not blocked");

    let old_mail = old.mailboxes();
    old_mail
        .create_mailbox(&MailboxName("gamma".into()), &owner, None)
        .await
        .expect("the box opens")
        .written()
        .expect("not blocked");

    let at = |offset: i64| {
        jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant")
            + jiff::SignedDuration::from_secs(offset)
    };
    let mut posted = Vec::new();
    for (n, body) in [
        "nobody has taken this one",
        "somebody took this one",
        // Prose that has been through a document editor, which is the whole
        // reason this runs against the real store rather than a fake.
        "somebody finished this one\n\n| a | table |\n| - | - |\n| in | a body |\n\n```\na fence\n```",
    ]
    .into_iter()
    .enumerate()
    {
        posted.push(
            old_mail
                .post_message(NewMessage {
                    mailbox: MailboxName("gamma".into()),
                    body: body.into(),
                    subject: Some("a subject that must survive".into()),
                    sender: "gamma".into(),
                    sent_at: at(n as i64),
                    in_reply_to: None,
                })
                .await
                .expect("post ok")
                .written()
                .expect("not blocked"),
        );
    }
    old_mail.read_message(&posted[1].id).await.expect("read ok");
    old_mail
        .mark_processed(&posted[2].id, Some("the outcome, recorded"))
        .await
        .expect("processed ok");

    let old_sessions = old.sessions();
    let run = old_sessions
        .begin(NewSession {
            bot: owner.clone(),
            sid: Sid("hndv".into()),
            focus: "the board this handover carries".into(),
            started_at: at(0),
        })
        .await
        .expect("begin ok");
    for (n, text) in ["what I set out to do", "what I found"]
        .into_iter()
        .enumerate()
    {
        old_sessions
            .append(&run.id, NewEntry::manual(text, at(n as i64 + 1)))
            .await
            .expect("append ok");
    }

    // --- the source, wrapped so this test can say whether it was READ -------
    //
    // The steady state's whole claim is that a boot with a verified record never
    // touches the old store. `AlreadyCarried` on its own is equally true of a
    // boot that scanned the entire remote board first and threw the answer away
    // — a full Outline scan, every start, to learn there is nothing to do. So
    // the reads are counted rather than inferred from the outcome.
    //
    // **Only the verbs the handover's SOURCE side calls are implemented.** The
    // handover reads and never writes; anything else arriving here is this
    // test's own bug, and a delegation that quietly answered it would hide that.
    struct WatchedMail<'a>(&'a dyn Mailboxes, AtomicU64);
    struct WatchedRuns<'a>(&'a dyn Sessions, AtomicU64);

    impl WatchedMail<'_> {
        fn reads(&self) -> u64 {
            self.1.load(Ordering::Relaxed)
        }
        fn forget(&self) {
            self.1.store(0, Ordering::Relaxed);
        }
    }
    impl WatchedRuns<'_> {
        fn reads(&self) -> u64 {
            self.1.load(Ordering::Relaxed)
        }
        fn forget(&self) {
            self.1.store(0, Ordering::Relaxed);
        }
    }

    #[async_trait::async_trait]
    impl Mailboxes for WatchedMail<'_> {
        async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.list_mailboxes().await
        }
        async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.scan_messages().await
        }
        async fn create_mailbox(
            &self,
            _: &MailboxName,
            _: &EntityId,
            _: Option<&str>,
        ) -> Result<Guarded<Mailbox>, MailboxError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn post_message(&self, _: NewMessage) -> Result<Guarded<Message>, MailboxError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn read_mailbox(&self, _: &MailboxName) -> Result<Guarded<Delivery>, MailboxError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn read_message(&self, _: &MessageId) -> Result<Delivered, MailboxError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn mark_processed(
            &self,
            _: &MessageId,
            _: Option<&str>,
        ) -> Result<Message, MailboxError> {
            unimplemented!("the handover only ever reads its source")
        }
    }

    // Counted separately from the mail half: the two sources are two remote
    // boards, and a start that touched only one of them still touched a source.
    #[async_trait::async_trait]
    impl Sessions for WatchedRuns<'_> {
        async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.all_sessions().await
        }
        async fn sessions_of(&self, _: &EntityId) -> Result<Vec<Session>, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn read_session(&self, _: &SessionId) -> Result<Session, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn begin(&self, _: NewSession) -> Result<Session, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn append(&self, _: &SessionId, _: NewEntry) -> Result<JournalEntry, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn amend_last(&self, _: &SessionId, _: &str) -> Result<JournalEntry, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn amend_beat(
            &self,
            _: &SessionId,
            _: &EntryId,
            _: &str,
            _: jiff::Timestamp,
        ) -> Result<JournalEntry, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn set_focus(&self, _: &SessionId, _: &str) -> Result<Session, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn close(&self, _: &SessionId, _: SessionState) -> Result<Session, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
        async fn reopen(&self, _: &SessionId) -> Result<Session, SessionError> {
            unimplemented!("the handover only ever reads its source")
        }
    }

    let source_mail = WatchedMail(&old_mail, AtomicU64::new(0));
    let source_runs = WatchedRuns(&old_sessions, AtomicU64::new(0));

    // --- the new store ------------------------------------------------------
    struct AnyOwner;
    #[async_trait::async_trait]
    impl OwnerIndex for AnyOwner {
        async fn look_up(
            &self,
            _: &EntityId,
        ) -> Result<OwnerLookup, jojobot_domain::mailbox::MailboxError> {
            Ok(OwnerLookup::Known)
        }
    }

    let dir = std::env::temp_dir().join(format!("jojobot-handover-itest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free port")
        .local_addr()
        .expect("a bound address")
        .port();
    let mut store = Dolt::start(&dir, port).await.expect("the store comes up");
    migrate::run(store.pool()).await.expect("the schema");
    let new_mail = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
    let new_sessions = DoltSessions::open(store.pool().clone());

    // --- carry, and check by comparison ------------------------------------
    //
    // **Through `carry_over`, because that is the verb a boot calls.** The
    // record and its two states, the already-carried answer and the refusals
    // all sit above `run`, and against fake sources they are proven elsewhere.
    // This is the one place they meet a real document store.
    let carried = handover::carry_over(
        &source_mail,
        &source_runs,
        &new_mail,
        &new_sessions,
        store.pool(),
    )
    .await;
    let handover::Carryover::Carried(report) = carried else {
        panic!("the first boot carries the board across: {carried:?}");
    };
    // The positive twin of the steady state below: without it, the zeroes there
    // would pass just as well over a double nothing ever calls.
    assert!(
        source_mail.reads() > 0 && source_runs.reads() > 0,
        "the carrying boot DID read both sources — {} mail reads, {} session reads",
        source_mail.reads(),
        source_runs.reads()
    );

    assert!(report.whole(), "every kind came through whole: {report:?}");
    assert_eq!(report.messages.read, 3, "{report:?}");
    assert_eq!(
        report.messages.verified, 3,
        "the comparison ran: {report:?}"
    );
    assert_eq!(report.sessions.read, 1, "{report:?}");
    assert_eq!(report.entries.read, 2, "{report:?}");

    // The states survived the crossing — the half no count can show.
    let landed = new_mail.list_mailboxes().await.expect("list ok");
    let gamma = landed
        .iter()
        .find(|b| b.name.as_str() == "gamma")
        .expect("the box came across");
    assert_eq!(
        gamma.counts,
        StateCounts {
            new: 1,
            read: 1,
            processed: 1
        },
        "one message in each state, as the real board had them"
    );

    // The body that went through the document editor came across as the old
    // store hands it back — table, fence and all.
    let carried = new_mail.scan_messages().await.expect("scan ok");
    let handled = carried
        .iter()
        .find(|m| m.state == MessageState::Processed)
        .expect("the handled message came across handled");
    let was = old_mail
        .scan_messages()
        .await
        .expect("scan ok")
        .into_iter()
        .find(|m| m.id == handled.id)
        .expect("it is still on the old board");
    assert_eq!(handled.body, was.body, "byte for byte, against the source");
    assert_eq!(handled.notes.as_deref(), Some("the outcome, recorded"));

    // **The old board is untouched.** The source is read-only, and a handover
    // that quietly moved records instead of copying them is the one mistake
    // here that reading a diff cannot undo.
    assert_eq!(
        old_mail.scan_messages().await.expect("scan ok").len(),
        3,
        "the old board still holds everything it held"
    );
    assert_eq!(
        old_sessions.all_sessions().await.expect("list ok").len(),
        1,
        "…and its sessions"
    );

    // ---- the record says the store may be served from ----------------------
    //
    // Read the way an operator would — a plain `SELECT` — rather than through
    // the module's own helper: a verify that shares the reader it is checking is
    // not a verify. Only this token lets a later boot serve mail from here.
    let recorded: Option<String> =
        sqlx::query_scalar("SELECT state FROM handover WHERE what = 'mail-and-sessions'")
            .fetch_optional(store.pool())
            .await
            .expect("the record is readable");
    assert_eq!(
        recorded.as_deref(),
        Some("verified"),
        "the read-back passed against the real board, so the record is promoted"
    );

    // ---- the steady state: every later boot asks, and carries nothing ------
    //
    // What the boot path actually does after the first start. The claim is not
    // merely the outcome: the old store must not be TOUCHED, or every start
    // pays a full Outline scan to learn it has nothing to do.
    let before = new_mail.scan_messages().await.expect("scan ok").len();
    source_mail.forget();
    source_runs.forget();
    let again = handover::carry_over(
        &source_mail,
        &source_runs,
        &new_mail,
        &new_sessions,
        store.pool(),
    )
    .await;
    assert!(
        matches!(again, handover::Carryover::AlreadyCarried),
        "a verified record means an earlier boot already did this: {again:?}"
    );
    assert_eq!(
        (source_mail.reads(), source_runs.reads()),
        (0, 0),
        "and it was answered from the record — the old board was not read at all"
    );

    // ---- and the doubling guard underneath it ------------------------------
    //
    // **`run` on purpose, and it is the one call here that cannot go through
    // `carry_over`**: with a verified record on this store the boot path answers
    // from the record and never reaches the guard. What it holds is that the
    // guard itself refuses a populated target rather than doubling the board —
    // the last thing standing between a re-run and two of every message.
    let twice = handover::run(
        &source_mail,
        &source_runs,
        &new_mail,
        &new_sessions,
        store.pool(),
    )
    .await;
    assert!(
        matches!(twice, Err(handover::HandoverError::Populated { .. })),
        "a second run must refuse rather than double the board: {twice:?}"
    );
    assert_eq!(
        new_mail.scan_messages().await.expect("scan ok").len(),
        before,
        "and the refusal wrote nothing"
    );

    // ---- the first write of each kind lands on an id nothing carried wears -
    //
    // ⚠️ **Against THIS source the counters are a no-op, and that is safe by
    // shape rather than by the advance working.** The document store mints
    // `gamma-1`, `e1` — prefixed — and this store mints bare decimals, so
    // `highest` reads none of them, no counter moves, and nothing can collide
    // because the two shapes are disjoint.
    //
    // So the assertions below cannot fail for an Outline source. They are here
    // because they are the property the cutover actually needs, and they would
    // fail the day either side's id shape changed. **The counter logic itself is
    // proven in the fast suite**, where the source mints numeric ids and each of
    // the three counters reddens when its kind is misspelled.
    let fresh_message = new_mail
        .post_message(NewMessage {
            mailbox: MailboxName("gamma".into()),
            body: "the first message after the move".into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: at(20),
            in_reply_to: None,
        })
        .await
        .expect("the store takes a message after the handover")
        .written()
        .expect("not blocked");
    assert_eq!(
        new_mail
            .scan_messages()
            .await
            .expect("scan ok")
            .iter()
            .filter(|m| m.id == fresh_message.id)
            .count(),
        1,
        "the new message got an id nothing carried wears"
    );

    let fresh_session = new_sessions
        .begin(NewSession {
            bot: owner.clone(),
            sid: Sid("post".into()),
            focus: "the first run after the move".into(),
            started_at: at(20),
        })
        .await
        .expect("the store takes a session after the handover");
    let appended = new_sessions
        .append(
            &fresh_session.id,
            NewEntry::manual("the first beat after the move", at(21)),
        )
        .await
        .expect("the store takes an entry after the handover");
    let landed = new_sessions.all_sessions().await.expect("list ok");
    assert_eq!(
        landed.iter().filter(|s| s.id == fresh_session.id).count(),
        1,
        "the new session got an id nothing carried wears"
    );
    assert_eq!(
        landed
            .iter()
            .flat_map(|s| s.entries.iter())
            .filter(|e| e.id == appended.id)
            .count(),
        1,
        "and so did the new entry"
    );

    store.stop().await;
    let _ = std::fs::remove_dir_all(&dir);

    // ---- THE ORDINARY TARGET: one that already holds sessions --------------
    // Not an empty database. A target jojobot has been up on at all has
    // sessions and nothing else, and only the session check sees it. This is
    // what the cutover will actually meet if the handover is ever run after
    // the adapters are flipped, and refusing is what stops it doubling a board.
    let lived_in =
        std::env::temp_dir().join(format!("jojobot-handover-livedin-{}", std::process::id()));
    std::fs::create_dir_all(&lived_in).expect("a scratch directory");
    let mut second = Dolt::ready(&lived_in, {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("a free port")
            .local_addr()
            .expect("a bound address")
            .port()
    })
    .await
    .expect("the store comes up and migrates")
    .0;
    let lived_mail = DoltMailboxes::open(second.pool().clone(), Arc::new(AnyOwner));
    let lived_sessions = DoltSessions::open(second.pool().clone());
    lived_sessions
        .begin(NewSession {
            bot: owner.clone(),
            sid: Sid("live".into()),
            focus: "a run that happened before anybody migrated".into(),
            started_at: at(0),
        })
        .await
        .expect("the store takes a session");

    //
    // Through `carry_over`, because this is the one refusal a boot really
    // reaches: rows and no record, which is the store answering that somebody
    // else wrote them. The composition root turns it into a dead start.
    let onto_lived_in = handover::carry_over(
        &source_mail,
        &source_runs,
        &lived_mail,
        &lived_sessions,
        second.pool(),
    )
    .await;
    let handover::Carryover::Refused(handover::HandoverError::Populated { what, .. }) =
        &onto_lived_in
    else {
        panic!("a target already holding sessions must refuse: {onto_lived_in:?}");
    };
    assert_eq!(
        *what, "sessions",
        "and it names what it found, so a person knows what to clear"
    );
    assert!(
        lived_mail
            .list_mailboxes()
            .await
            .expect("list ok")
            .is_empty(),
        "the refusal carried no board across"
    );

    second.stop().await;
    let _ = std::fs::remove_dir_all(&lived_in);
    drop_session_collections(&http, &c).await;
}
