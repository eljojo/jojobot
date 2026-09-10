//! **The shared contract, against the real store.**
//!
//! Not `#[ignore]` and not credential-gated: it needs a temporary directory
//! and the binary that is already in the toolchain, so it runs in an ordinary
//! `cargo test` and nobody has to remember it. That is what makes the
//! real-dependency gate cheap enough to have no excuse behind it.
//!
//! The contract is the specification. Nothing here restates what a session
//! does — it points the existing cases at the store and lets them say whether
//! it behaves.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::dolt::teaching::DoltTeachings;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::free_port;
use jojobot_domain::mailbox::testing::contract as mailboxes;
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::FormerHandle;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::search::{Hit, Search, SearchQuery};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityPatch, FactPatch, NewEntity, NewFact};
use jojobot_domain::session::testing::contract as sessions;
use jojobot_domain::teaching::testing::contract as teachings;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-contract-{}-{what}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **The two steps a boot takes after the schema**, in the order it takes them:
/// write the kinds this build ships, then parse handles against what the store
/// answered with.
///
/// A suite that stands a rail up is standing up a process, and a process that
/// never seeded cannot read a handle at all — which is the refusal a caller
/// meets and not a state a suite should be in silently. Opening a store does
/// not do this, deliberately: a load hidden inside `open` would make a suite
/// that touches memory look seeded while one that touches only the mail rail
/// does not.
async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

/// **What a handle's row is stored under**, for a case that reads the
/// substrate directly. `fact`/`fact_write` key on the badge now, never the
/// handle, so a raw query against them binds this rather than the handle a
/// fixture was created with.
async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

/// **The contract's cases, each against a store of its own.**
///
/// `fresh` is synchronous and opening a store is not, so the stores are opened
/// up front and handed out one per case. The count is deliberately larger than
/// the contract, and running out is a loud failure rather than a reused store:
/// two cases sharing one store would let one case's rows satisfy another's
/// assertions, which is the failure mode this isolation exists to prevent.
#[tokio::test]
async fn dolt_satisfies_the_session_contract() {
    let scratch = Scratch::new("sessions");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    const ROOM: usize = 32;
    let mut prepared = Vec::with_capacity(ROOM);
    for n in 0..ROOM {
        let pool = store
            .database(&format!("case{n}"))
            .await
            .expect("a database of this case's own");
        migrate::run(&pool).await.expect("the schema");
        booted(&pool).await;
        booted(&pool).await;
        prepared.push(DoltSessions::open(pool));
    }

    let handed = AtomicUsize::new(0);
    let fresh = || {
        let n = handed.fetch_add(1, Ordering::SeqCst);
        prepared.get(n).cloned().unwrap_or_else(|| {
            panic!(
                "the contract has more cases than this suite prepared stores for \
                 ({ROOM}). Raise ROOM — never let two cases share one store, or one \
                 case's rows start satisfying another's assertions."
            )
        })
    };

    sessions::run_all(fresh).await;

    store.stop().await;
}

/// 🚨 **An entity keeps its badge through every path that rewrites it, and no
/// two entities share one.**
///
/// A badge is the name a row keeps when its handle changes. It is minted here
/// rather than in the domain because it is a column, never accepted from a
/// caller and never serialised outward — so the store is the only place that
/// can say whether it survived.
///
/// **Writing an entity is a whole-row `REPLACE`**, which deletes the row before
/// inserting the new one. So preservation is not something the schema does for
/// us: every path that rewrites a row has to carry the badge across, and this
/// asserts it PER PATH rather than once, because a single case passes against a
/// build where one caller stopped going through the helper that carries it.
///
/// **Uniqueness is asserted over drawn badges rather than over the column**,
/// because the column is nullable until a backfill lands: a row written before
/// it existed carries none, and that is honestly absent rather than a value
/// shared with every other such row.
#[tokio::test]
async fn an_entity_keeps_its_badge_through_every_rewrite() {
    let scratch = Scratch::new("badge");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("badge")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let badge = |handle: &str| {
        let pool = pool.clone();
        let handle = handle.to_string();
        async move {
            sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
                .bind(&handle)
                .fetch_one(&pool)
                .await
                .expect("the row is there")
        }
    };

    // ── the path that creates ───────────────────────────────────────────────
    let alpha = EntityId::person("person:badge-alpha");
    memory
        .add_entity(NewEntity::new(
            alpha.clone(),
            "Badge Alpha",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let minted = badge("person:badge-alpha")
        .await
        .expect("a created entity is given a badge");

    // ⭐ **The receipt says what the row says.** A write that handed back a
    // record wearing no badge, or a different one, would be the answer and the
    // row disagreeing about the same thing — and the caller has only the
    // answer.
    let created = memory
        .add_entity(NewEntity::new(
            EntityId::person("person:badge-gamma"),
            "Badge Gamma",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    assert_eq!(
        created.badge,
        badge("person:badge-gamma").await,
        "the answer to a creation and the row disagree about the badge",
    );

    // ── the path that edits ─────────────────────────────────────────────────
    let renamed = memory
        .update_entity(
            &alpha,
            EntityPatch {
                name: Some("Badge Alpha, renamed".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_entity ok")
        .written()
        .expect("the guard waves it through");
    assert_eq!(
        badge("person:badge-alpha").await.as_deref(),
        Some(minted.as_str()),
        "the row was rewritten and stopped being the thing anything else pointed at",
    );
    assert_eq!(
        renamed.badge.as_deref(),
        Some(minted.as_str()),
        "the answer to a rename carries a different badge from the row it renamed",
    );

    // ── the path that writes prose ──────────────────────────────────────────
    memory
        .set_prose(&alpha, "a page somebody wrote")
        .await
        .expect("set_prose ok");
    assert_eq!(
        badge("person:badge-alpha").await.as_deref(),
        Some(minted.as_str()),
        "writing a page took the row's badge with it",
    );

    // ── and no two share one ────────────────────────────────────────────────
    let beta = EntityId::person("person:badge-beta");
    memory
        .add_entity(NewEntity::new(beta, "Badge Beta", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let other = badge("person:badge-beta")
        .await
        .expect("a created entity is given a badge");
    assert_ne!(
        minted, other,
        "two entities wear one badge, so it names neither of them",
    );

    store.stop().await;
}

/// 🚨 **A row written before the badge column existed is given one at startup,
/// and a second pass gives out none.**
///
/// A row gains a badge when it is next rewritten, which reaches what something
/// touches and no more. **A store where nothing is edited would keep unbadged
/// rows for ever**, so the fill reaches the rest.
///
/// **The rows here carry NULL, written straight through SQL**, because that is
/// what a row predating the column really looks like — an entity created
/// through the port would already have one, and a fixture that used the port
/// would be testing the mint again rather than the fill.
///
/// **Three reads.** The fill gives one to every waiting row; a second pass
/// gives out none, which is what makes it safe to run at every startup; and the
/// badges it gave out differ, because a fill that gave one badge to everybody
/// would satisfy the first two.
#[tokio::test]
async fn the_fill_badges_rows_written_before_the_column_and_repeats_nothing() {
    let scratch = Scratch::new("badgefill");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("badgefill")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    for handle in ["person:fill-alpha", "person:fill-beta"] {
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge)
             VALUES (?, 'person', 'Fill', 'contract-fixture', NULL, NULL, 'on-demand', '', NULL)",
        )
        .bind(handle)
        .execute(&pool)
        .await
        .expect("a row from before the column");
    }

    assert_eq!(
        memory.badge_the_unbadged().await.expect("the fill runs"),
        2,
        "the rows waiting for a badge were not given one",
    );
    let worn: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT badge FROM entity WHERE id IN ('person:fill-alpha', 'person:fill-beta')          ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("the rows are readable");
    assert!(
        worn.iter().all(Option::is_some),
        "a row came back still waiting: {worn:?}",
    );
    assert_ne!(
        worn[0], worn[1],
        "two rows wear one badge, so it names neither of them",
    );

    assert_eq!(
        memory
            .badge_the_unbadged()
            .await
            .expect("the fill runs again"),
        0,
        "the fill gave out badges a second time, so running it at every startup would not be \
         safe",
    );

    store.stop().await;
}

/// 🚨 **The bug the boot order makes possible: a badge fill that runs, and a
/// row that stays filed under the handle it wore before.**
///
/// `badge_the_unbadged` gives an unbadged entity a badge. Nothing else moves —
/// `fact`, `fact_write` and `field_write` still hold that entity's OLD handle
/// in every badge-keyed column. `recall` and every other read resolve the
/// handle to the NEW badge before querying, so from the moment the fill runs
/// until the backfill does, the row is filed under one key and read under
/// another: unreachable, not merely stale.
///
/// **The pre-upgrade state is built with the port, not with raw SQL**, unlike
/// its neighbour above: an entity inserted with `badge` NULL and then written
/// through `capture` is exactly what every row here looked like before
/// `ccc926f` — `capture`'s own resolve step keys a row by the handle whenever
/// the subject wears no badge, so this is the real shape rather than an
/// invented one.
///
/// **Watched failing first.** The middle read, taken after the fill and
/// before the backfill, is the assertion that reproduces the bug: it must
/// come back empty. A backfill that ran too early, or a fill that changed
/// nothing, would both make this pass by accident, so the case would say
/// nothing about the fix if this read were left out.
#[tokio::test]
async fn the_backfill_rekeys_rows_a_badge_reached_after_they_were_written() {
    let scratch = Scratch::new("rekey");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("rekey")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let handle = "person:rekey-alpha";
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge)
         VALUES (?, 'person', 'Rekey Alpha', 'contract-fixture', NULL, NULL, 'on-demand', '', \
         NULL)",
    )
    .bind(handle)
    .execute(&pool)
    .await
    .expect("a row from before the badge column");
    let subject = EntityId::person(handle);

    // Written through the port, while the subject still wears no badge — so
    // this lands exactly where a pre-`ccc926f` capture would have: keyed by
    // the plain handle, because `capture`'s own resolve step falls back to it
    // when there is no badge to prefer.
    let source = memory
        .capture(NewFact::about(
            subject.clone(),
            "the first claim",
            date(2026, 3, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    let claim = memory
        .capture(NewFact {
            details: Some("a nuance worth keeping".into()),
            derived_from: Some(source.address()),
            fields: [("note".to_string(), "kept".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the second claim", date(2026, 3, 2))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    // **The fill runs, and nothing else does yet.** This is the exact window
    // the boot order opens: a badge exists, and every row naming this subject
    // still names the handle.
    assert_eq!(
        memory.badge_the_unbadged().await.expect("the fill runs"),
        1,
        "the waiting row was not given a badge",
    );

    // **Watched failing first.** Unreachable, not merely stale: the handle
    // now resolves to a badge nothing was rekeyed onto.
    let stranded = memory.recall(&subject).await.expect("a plain read");
    assert!(
        stranded.is_empty(),
        "the claims read back after the fill and before the backfill, so the bug the backfill \
         exists to fix does not reproduce here: {stranded:?}",
    );

    assert_eq!(
        memory
            .backfill_handle_keyed_rows()
            .await
            .expect("the backfill runs"),
        1,
        "the one entity with stranded rows was not rekeyed",
    );

    let found = memory.recall(&subject).await.expect("a plain read");
    let source_read = found
        .iter()
        .find(|f| f.id == source.id)
        .expect("the source claim reads back after the rekey");
    assert_eq!(source_read.content, "the first claim");
    let claim_read = found
        .iter()
        .find(|f| f.id == claim.id)
        .expect("the derived claim reads back after the rekey");
    assert_eq!(claim_read.content, "the second claim");
    assert_eq!(
        claim_read.details.as_deref(),
        Some("a nuance worth keeping"),
        "the nuance did not survive the rekey",
    );
    assert_eq!(
        claim_read.derived_from,
        Some(source.address()),
        "the lineage did not survive the rekey",
    );
    assert_eq!(
        claim_read.fields.get("note").map(String::as_str),
        Some("kept"),
        "a field write did not survive the rekey",
    );

    // **Paired: a store already converted is untouched.**
    assert_eq!(
        memory
            .backfill_handle_keyed_rows()
            .await
            .expect("the backfill runs again"),
        0,
        "the backfill rekeyed a row a second time, so running it at every startup would not be \
         safe",
    );

    store.stop().await;
}

/// 🚨 **An alias row is its own entity's foreign key, and a rename must not
/// sever it.**
///
/// `entity_alias.entity` names the row it belongs to — the same shape
/// `fact.entity` was before the badge conversion, not an edge's object. A
/// rename here is the real store's own way of moving a handle: the row is
/// rewritten in place, exactly as [`DoltRehandles`] does it, because no
/// production verb exists yet.
///
/// **The consequence that matters is search, not the join alone**: a
/// nickname the index cannot resolve to the entity's current handle is a
/// nickname that finds nothing, silently. `Retrieval::search` refreshes from
/// the store before it answers, so this reaches the path a caller actually
/// uses rather than stopping at `list_entities`.
#[tokio::test]
async fn an_alias_survives_a_rename_and_a_search_still_finds_it_by_nickname() {
    let scratch = Scratch::new("alias-rename");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("aliasrename")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(DoltMemory::open(pool.clone())))
            .expect("the search index opens"),
    );
    let retrieval = Retrieval::new(indexed.index(), vec![indexed.clone()]);

    let was = EntityId::person("person:alias-rename-was");
    indexed
        .add_entity(NewEntity {
            aliases: vec!["Nicky".into()],
            ..NewEntity::new(was.clone(), "Alias Rename Was", "contract-fixture")
        })
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    let before = retrieval
        .search(&SearchQuery::text("Nicky"))
        .await
        .expect("search ok");
    assert!(
        before
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == was)),
        "the nickname does not find the entity before any rename: {before:?}",
    );

    let now = EntityId("work:alias-rename-now".into());
    sqlx::query("UPDATE entity SET id = ?, kind = ? WHERE id = ?")
        .bind(now.as_str())
        .bind(now.kind_token())
        .bind(was.as_str())
        .execute(&pool)
        .await
        .expect("the row moves");

    let entities = indexed.list_entities(None).await.expect("list_entities ok");
    let renamed = entities
        .iter()
        .find(|e| e.id == now)
        .expect("the renamed row is there");
    assert_eq!(
        renamed.aliases,
        vec!["Nicky".to_string()],
        "the alias did not follow the rename: {renamed:?}",
    );

    let after = retrieval
        .search(&SearchQuery::text("Nicky"))
        .await
        .expect("search ok");
    assert!(
        after
            .iter()
            .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == now)),
        "the nickname does not find the renamed entity: {after:?}",
    );

    store.stop().await;
}

/// 🚨 **The bug the boot order makes possible, over an alias row: a badge
/// fill that runs, and an alias that stays filed under the handle it wore
/// before.**
///
/// The same window `the_backfill_rekeys_rows_a_badge_reached_after_they_were_written`
/// proves for a claim, over `entity_alias` instead: the join in
/// `DoltMemory::index` prefers the badge, so once the fill hands out one, an
/// alias row still under the old handle stops joining — the entity reads
/// back with no aliases at all, and a search on its nickname finds nothing.
#[tokio::test]
async fn the_backfill_rekeys_an_alias_row_a_badge_reached_after_it_was_written() {
    let scratch = Scratch::new("alias-rekey");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("aliasrekey")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let handle = "person:alias-rekey-alpha";
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge)
         VALUES (?, 'person', 'Alias Rekey Alpha', 'contract-fixture', NULL, NULL, \
         'on-demand', '', NULL)",
    )
    .bind(handle)
    .execute(&pool)
    .await
    .expect("a row from before the badge column");
    sqlx::query("INSERT INTO entity_alias (entity, ordinal, alias) VALUES (?, 1, 'Rekeyed')")
        .bind(handle)
        .execute(&pool)
        .await
        .expect("an alias row from before the badge column");

    assert_eq!(
        memory.badge_the_unbadged().await.expect("the fill runs"),
        1,
        "the waiting row was not given a badge",
    );

    let subject = EntityId::person(handle);
    let stranded = memory
        .list_entities(None)
        .await
        .expect("list_entities ok")
        .into_iter()
        .find(|e| e.id == subject)
        .expect("the entity itself still reads back");
    assert!(
        stranded.aliases.is_empty(),
        "the alias read back after the fill and before the backfill, so the bug the backfill \
         exists to fix does not reproduce here: {stranded:?}",
    );

    assert_eq!(
        memory
            .backfill_handle_keyed_rows()
            .await
            .expect("the backfill runs"),
        1,
        "the one entity with a stranded alias was not rekeyed",
    );

    let found = memory
        .list_entities(None)
        .await
        .expect("list_entities ok")
        .into_iter()
        .find(|e| e.id == subject)
        .expect("the entity reads back");
    assert_eq!(
        found.aliases,
        vec!["Rekeyed".to_string()],
        "the alias did not survive the rekey",
    );

    store.stop().await;
}

/// 🚨 **A correction is kept: the substrate holds what the claim used to say,
/// and a claim nobody corrected has one write and no more.**
///
/// A claim's row was rewritten in place, so a correction overwrote its words
/// and nothing anywhere remembered them. A session could not tell *we never
/// recorded this* from *we recorded it and we were wrong*.
///
/// **The whole claim is kept, not its content and edge.** The fold that
/// projects a thing's fields keeps only writes whose record is ACTIVE, so a
/// status left in a column above a versioned content is a filter reading the
/// value it is meant to be deciding.
///
/// ⚠️ **The negative is what gives it meaning**: an uncorrected claim must
/// carry ONE write. A substrate that kept a chain for everything would satisfy
/// the positive and be useless — and it is what a reader would meet on every
/// claim they ever looked at.
///
/// ⛔️ **Nothing reads this yet**, so both halves are asserted against the
/// substrate directly. The claim's own row still answers every read, and this
/// case says the two agree.
#[tokio::test]
async fn the_substrate_keeps_what_a_correction_overwrote() {
    let scratch = Scratch::new("factwrite");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factwrite")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:kept-alpha");
    let untouched = EntityId::person("person:kept-beta");
    for (who, called) in [(&subject, "Kept Alpha"), (&untouched, "Untouched Beta")] {
        memory
            .add_entity(NewEntity::new(who.clone(), called, "contract-fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("the guard waves it through");
    }

    let claim = memory
        .capture(NewFact::about(
            subject.clone(),
            "was at the fair",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    memory
        .capture(NewFact::about(
            untouched.clone(),
            "stayed home",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    memory
        .update_fact(
            &claim.address(),
            FactPatch {
                content: Some("was never at the fair".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("the guard waves it through");

    let kept: Vec<String> = sqlx::query_scalar(
        "SELECT content FROM fact_write WHERE entity = ? AND fact_id = ? ORDER BY ordinal",
    )
    .bind(badge_of(&pool, subject.as_str()).await)
    .bind(claim.id.as_str())
    .fetch_all(&pool)
    .await
    .expect("the substrate is readable");
    assert_eq!(
        kept,
        vec![
            "was at the fair".to_string(),
            "was never at the fair".to_string(),
        ],
        "the correction overwrote what the claim used to say and nothing kept it",
    );

    // **The negative.** A claim nobody corrected carries one write.
    let alone: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact_write WHERE entity = ?")
        .bind(badge_of(&pool, untouched.as_str()).await)
        .fetch_one(&pool)
        .await
        .expect("the substrate is readable");
    assert_eq!(
        alone, 1,
        "a claim nobody corrected carries a chain, so every claim a reader meets would",
    );

    // **And the row still answers as itself**, which is what makes this inert.
    let read = memory
        .recall(&subject)
        .await
        .expect("a plain read")
        .into_iter()
        .map(|f| f.content)
        .collect::<Vec<_>>();
    assert_eq!(read, vec!["was never at the fair".to_string()]);

    store.stop().await;
}

/// **The memory contract, against the real store.**
///
/// One store for every case, which is what this contract is written for: its
/// assertions are subset-based — what was captured comes back, never an exact
/// total — because it also runs against a shared, pre-populated real
/// collection. Handing each case its own database here would prove less than
/// the suite is designed to prove, not more.
#[tokio::test]
async fn dolt_satisfies_the_memory_contract() {
    let scratch = Scratch::new("memory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::run_all(&DoltMemory::open(pool)).await;

    store.stop().await;
}

/// **…and what sits under a supplied record answers over the real store too.**
///
/// The read is derived from `list_entities` rather than implemented by either
/// store, so the claim is about which layer answers it — and that is worth
/// asking of the real store, because the row half is where a supplied record is
/// genuinely absent.
#[tokio::test]
async fn what_sits_under_a_supplied_record_answers_over_the_real_store() {
    let scratch = Scratch::new("supplied-children");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("suppliedchildren")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let shipped = EntityId("view:loops".into());
    let under = EntityId("view:my-week".into());
    let supplied = Provisions::new(vec![Provision::record(
        jojobot_domain::memory::Entity {
            id: shipped.clone(),
            kind: jojobot_domain::memory::EntityKind::VIEW,
            name: "The Loops".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
        },
        std::collections::BTreeMap::new(),
    )]);
    let served = Provisioned::new(
        DoltMemory::open(pool.clone()).knowing(supplied.clone()),
        supplied,
    );

    assert_eq!(
        served.children(&shipped).await.expect("the read answers"),
        Vec::<EntityId>::new(),
        "a supplied record nothing sits under answers with none rather than a miss",
    );

    let mut child = NewEntity::new(under.clone(), "My Week", "user-named");
    child.parent = Some(shipped.clone());
    served
        .add_entity(child)
        .await
        .expect("the operator files one under it")
        .written()
        .expect("nothing collides with it");
    assert_eq!(
        served.children(&shipped).await.expect("the read answers"),
        vec![under],
        "the child under a supplied record came back empty over the real store",
    );

    store.stop().await;
}

/// **The mention contract against the real store**, over the layer that
/// resolves and renders and over the same store read bare.
///
/// The two handles address one database: the claim is that what is KEPT and
/// what is SERVED differ, and a case holding only one of them cannot make it.
#[tokio::test]
async fn the_dolt_store_keeps_a_mention_as_a_badge_and_serves_it_as_a_handle() {
    let scratch = Scratch::new("mentions");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("mentions")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let bare: Arc<dyn Memory> = Arc::new(DoltMemory::open(pool.clone()));
    memory::run_all_mentioning(
        &jojobot_domain::memory::mention::Mentioning::new(bare.clone()),
        &*bare,
        &DoltRehandles(pool.clone()),
    )
    .await;

    store.stop().await;
}

/// **The real store's way of moving a handle**: the row is rewritten in place,
/// badge and all.
///
/// There is no rename verb and this does not add one. What the mention layer
/// claims is that text survives a handle moving, and the only way that state
/// arises today is an edit made outside jojobot — so that is how the case
/// produces it.
struct DoltRehandles(sqlx::MySqlPool);

#[async_trait::async_trait]
impl memory::Rehandles for DoltRehandles {
    async fn rehandle(&self, from: &EntityId, to: &EntityId) {
        sqlx::query("UPDATE entity SET id = ?, kind = ? WHERE id = ?")
            .bind(to.as_str())
            .bind(to.kind_token())
            .bind(from.as_str())
            .execute(&self.0)
            .await
            .expect("the row moves");
    }

    async fn note_former_handle(&self, event: FormerHandle) {
        sqlx::query(
            "INSERT INTO entity_former_handle (former_handle, badge, changed_at) VALUES (?, ?, ?)",
        )
        .bind(event.former.as_str())
        .bind(&event.badge)
        .bind(event.changed_at.to_string())
        .execute(&self.0)
        .await
        .expect("the rename event is recorded");
    }
}

/// **…and the supplied-record guard specs**, over a real store wired with
/// the same shipped view [`memory::SUPPLIED_VIEW_FOR_THE_GUARD_SPECS`]
/// names — a near-miss is caught and its own override lifts it, an exact
/// collision never clears, a claim on it reads back, and a rename of it is
/// refused rather than a silent no-op, on the real store exactly as on the
/// fake.
#[tokio::test]
async fn dolt_satisfies_the_supplied_record_guard_contract() {
    let scratch = Scratch::new("supplied-guard");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("supplied-guard")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let supplied = Provisions::new(vec![Provision::record(
        jojobot_domain::memory::Entity {
            id: EntityId(memory::SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into()),
            kind: jojobot_domain::memory::EntityKind::VIEW,
            name: "The Loops".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
        },
        std::collections::BTreeMap::new(),
    )]);
    let known = DoltMemory::open(pool).knowing(supplied);

    memory::a_near_miss_against_a_supplied_record_is_caught_and_its_override_lifts_it(&known).await;
    memory::an_exact_collision_with_a_supplied_handle_is_never_forceable(&known).await;
    memory::a_claim_on_a_supplied_record_reads_back(&known).await;
    memory::a_rename_of_a_supplied_handle_is_refused_not_a_silent_no_op(&known).await;

    store.stop().await;
}

/// **…and every entity read counted in one place, over the real store.**
///
/// A database of its own, because the case is the one a sabotage is aimed at:
/// sharing a run with the specs above would let their assertions answer for
/// it, and a verdict about somebody else's case measures nothing.
#[tokio::test]
async fn dolt_answers_every_entity_read_for_a_supplied_record() {
    let scratch = Scratch::new("supplied-reads");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("suppliedreads")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let supplied = Provisions::new(vec![Provision::record(
        jojobot_domain::memory::Entity {
            id: EntityId(memory::SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into()),
            kind: jojobot_domain::memory::EntityKind::VIEW,
            name: "The Loops".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
        },
        std::collections::BTreeMap::new(),
    )]);
    let known = DoltMemory::open(pool).knowing(supplied);

    memory::every_entity_read_answers_for_a_supplied_record(&known).await;

    store.stop().await;
}

/// **…and the same contract including retrieval**, with the search projection
/// over this store.
///
/// It is the one thing `run_all` cannot reach: `scan` feeds the projection and
/// no verb a caller calls returns it, so a scan that came back empty would
/// leave every case above green and every search answer wrong. The projection
/// itself is unchanged — it sits above the port and does not care which store
/// answers, which is the claim this case actually tests.
#[tokio::test]
async fn the_indexed_dolt_store_satisfies_the_whole_contract() {
    let scratch = Scratch::new("memory-indexed");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memoryindexed")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(DoltMemory::open(pool))).expect("the search index opens"),
    );
    memory::run_all_searchable(
        indexed.as_ref(),
        &Retrieval::new(indexed.index(), vec![indexed.clone()]),
    )
    .await;

    store.stop().await;
}

/// **What the REAL store keeps when the build supplies half the text.**
///
/// The decorator sits above the port and does not care which store answers, so
/// its behaviour is proven against the fake. **What only a store can answer is
/// what ends up in the column** — and that is the claim that matters here,
/// because the whole design rests on the build's half never being written down.
/// A store that quietly kept it would freeze this instance on this build, and
/// every case above would stay green.
///
/// **Three answers, and each covers how the others pass on a build that is
/// wrong.** Without the first, a store that wrote nothing at all would pass.
/// Without the second, a store that kept everything would. Without the third,
/// the case says nothing about what a reader actually gets.
#[tokio::test]
async fn the_real_store_keeps_the_operators_half_and_not_the_builds() {
    let scratch = Scratch::new("provisioned");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("provisioned")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    const SHIPPED: &str = "You answer in one line unless asked otherwise.";
    let bot = EntityId("bot:gamma".into());
    let bare = DoltMemory::open(pool.clone());
    bare.add_entity(jojobot_domain::memory::NewEntity::new(
        bot.clone(),
        "Gamma",
        "jojobot",
    ))
    .await
    .expect("the identity is created")
    .written()
    .expect("an empty board blocks nothing");

    let served = Provisioned::new(
        bare,
        Provisions::new(vec![Provision::prose(bot.clone(), SHIPPED)]),
    );
    served
        .set_prose(&bot, "Gamma files the weekly note.")
        .await
        .expect("the operator's own half lands");

    // ① What the store kept, read straight out of the column rather than
    // through the layer that would resolve it.
    let kept: String = sqlx::query_scalar("SELECT prose FROM entity WHERE id = ?")
        .bind(bot.as_str())
        .fetch_one(&pool)
        .await
        .expect("the column reads");
    assert_eq!(
        kept.trim(),
        "Gamma files the weekly note.",
        "the operator's half is what the store holds",
    );
    // ② …and the build's half is not in it, which is what makes an upgrade
    // free and this instance unfrozen.
    assert!(
        !kept.contains(SHIPPED),
        "the build's half was written down, so this instance is frozen on this build: {kept}",
    );
    // ③ …while a reader gets both.
    let read = served
        .scan_entity(&bot)
        .await
        .expect("the scan reads")
        .expect("the entity is there")
        .prose;
    assert!(read.contains(SHIPPED) && read.contains("files the weekly note"));

    store.stop().await;
}

/// **A record the build ships is in no table of the real store**, and it still
/// answers the reads a stored one answers.
///
/// The same claim as the case above, for the other shape a provision takes, and
/// the same reason for asking it of a real store: the whole design rests on the
/// build's data never being written down. **Three answers** — the record
/// answers, the store holds no row for it, and a row the operator really wrote
/// is there beside it. Without the third, a store that had lost everything
/// would pass.
#[tokio::test]
async fn a_record_the_build_ships_is_in_no_table_of_the_real_store() {
    let scratch = Scratch::new("supplied");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("supplied")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let shipped = EntityId("view:loops".into());
    let theirs = EntityId("view:my-people".into());
    let supplied = Provisions::new(vec![jojobot_domain::memory::owned::Provision::record(
        jojobot_domain::memory::Entity {
            id: shipped.clone(),
            kind: jojobot_domain::memory::EntityKind::VIEW,
            name: "The Loops".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
        },
        std::collections::BTreeMap::from([("selects".to_string(), "rhythm".to_string())]),
    )]);
    // **Both halves are told the same set**, as the binary wires it: the layer
    // above resolves supplied records into answers, and the store below sees
    // them when its guard asks what exists.
    let bare = DoltMemory::open(pool.clone()).knowing(supplied.clone());
    bare.add_entity(jojobot_domain::memory::NewEntity::new(
        theirs.clone(),
        "My People",
        "user-named",
    ))
    .await
    .expect("the operator declares their own")
    .written()
    .expect("an empty board blocks nothing");

    let served = Provisioned::new(bare, supplied);

    // ① The shipped record answers, over the real store.
    assert_eq!(
        served
            .fields(&shipped)
            .await
            .expect("the keys read")
            .get("selects"),
        Some(&"rhythm".to_string()),
    );
    // ② …and the store has no row for it.
    let rows: Vec<String> = sqlx::query_scalar("SELECT id FROM entity WHERE kind = 'view'")
        .fetch_all(&pool)
        .await
        .expect("the table reads");
    assert!(
        !rows.contains(&shipped.to_string()),
        "the build's record was written down, so this instance is frozen on this build: {rows:?}",
    );
    // ③ …while the one the operator declared is a row like any other.
    assert!(
        rows.contains(&theirs.to_string()),
        "the operator's own record is in the table: {rows:?}",
    );

    // 🚨 ④ **A claim may point at it**, over the real store. The guard reads
    // what the store holds, and a guard that could not see what the build
    // supplies refused a claim pointing at a record every read answers for.
    let who = EntityId::person("person:milhouse");
    served
        .add_entity(jojobot_domain::memory::NewEntity::new(
            who.clone(),
            "Milhouse",
            "user-named",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("an empty board blocks nothing");
    let written = served
        .capture(NewFact {
            edge: Some(jojobot_domain::memory::Edge {
                shape: jojobot_domain::memory::EdgeShape::About,
                object: shipped.clone(),
            }),
            ..NewFact::about(
                who,
                "asked for that view twice this week",
                date(2026, 4, 18),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("a claim may point at a record the build ships");
    // **The edge landed rather than being dropped on the way in.**
    assert_eq!(
        written.edge.map(|edge| edge.object),
        Some(shipped),
        "the write was allowed and the link was lost",
    );

    store.stop().await;
}

/// **An owner index that answers for the contract's roster and nothing else.**
///
/// Strict on purpose. A resolver that says yes to everything makes
/// `Guarded::UnknownOwner` unreachable, so the case that proves a box cannot be
/// opened for a stranger would pass over a store that never checks — a green
/// bar wearing a costume. This one holds exactly the handles the suite's
/// precondition names.
struct RosterOnly;

#[async_trait::async_trait]
impl OwnerIndex for RosterOnly {
    async fn look_up(&self, owner: &EntityId) -> Result<OwnerLookup, MailboxError> {
        if mailboxes::OWNERS.contains(&owner.as_str()) {
            return Ok(OwnerLookup::Known);
        }
        // The near misses a real index would find, from the roster it does
        // hold — so a typo comes back with the handle it probably meant rather
        // than an empty list that says nothing.
        let index: Vec<jojobot_domain::memory::Entity> = mailboxes::OWNERS
            .iter()
            .map(|handle| {
                let id = EntityId((*handle).to_string());
                jojobot_domain::memory::Entity {
                    kind: id.kind().expect("the roster's handles are well-formed"),
                    name: id.slug().to_string(),
                    id,
                    aliases: Vec::new(),
                    source: "contract-fixture".into(),
                    crm: None,
                    parent: None,
                    boot: Default::default(),
                    merged_into: None,
                    badge: None,
                }
            })
            .collect();
        Ok(OwnerLookup::Unknown(jojobot_domain::memory::guard::screen(
            owner,
            &[],
            &index,
        )))
    }
}

/// The mailbox contract's cases, each against a store of its own.
#[tokio::test]
async fn dolt_satisfies_the_mailbox_contract() {
    let scratch = Scratch::new("mailboxes");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    const ROOM: usize = 48;
    let mut prepared = Vec::with_capacity(ROOM);
    for n in 0..ROOM {
        let pool = store
            .database(&format!("mail{n}"))
            .await
            .expect("a database of this case's own");
        migrate::run(&pool).await.expect("the schema");
        booted(&pool).await;
        booted(&pool).await;
        prepared.push(DoltMailboxes::open(pool, Arc::new(RosterOnly)));
    }

    let handed = AtomicUsize::new(0);
    mailboxes::run_all(|| {
        let n = handed.fetch_add(1, Ordering::SeqCst);
        let store = prepared.get(n).cloned().unwrap_or_else(|| {
            panic!(
                "the contract has more cases than this suite prepared stores for \
                 ({ROOM}). Raise ROOM — never let two cases share one store, or one \
                 case's rows start satisfying another's assertions."
            )
        });
        async move { store }
    })
    .await;

    store.stop().await;
}

/// The teaching contract's cases, each against a store of its own.
#[tokio::test]
async fn dolt_satisfies_the_teaching_contract() {
    let scratch = Scratch::new("teaching");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    const ROOM: usize = 8;
    let mut prepared = Vec::with_capacity(ROOM);
    for n in 0..ROOM {
        let pool = store
            .database(&format!("teach{n}"))
            .await
            .expect("a database of this case's own");
        migrate::run(&pool).await.expect("the schema");
        prepared.push(DoltTeachings::open(pool));
    }

    let handed = AtomicUsize::new(0);
    teachings::run_all(|| {
        let n = handed.fetch_add(1, Ordering::SeqCst);
        let store = prepared.get(n).cloned().unwrap_or_else(|| {
            panic!(
                "the contract has more cases than this suite prepared stores for \
                 ({ROOM}). Raise ROOM — never let two cases share one store, or one \
                 case's rows start satisfying another's assertions."
            )
        });
        async move { store }
    })
    .await;

    store.stop().await;
}

/// 🚨 **The claim's own column keeps the moment it was taken in, and an edit
/// does not touch it — asserted AT THE COLUMN.**
///
/// The contract asserts this through a read, which is the behaviour a caller
/// sees and the right thing to pin there. **It no longer reaches this column**:
/// a claim read projects from the write table, so the row's `inserted_at` could
/// be re-stamped by every edit and every read would still answer correctly.
///
/// ⚠️ **The column is not dead**, which is why it earns a case of its own: the
/// backfill copies from it, so a store repaired after an interrupted migration
/// would take whatever this row holds. A re-stamp here would silently move the
/// taken-in moment of every claim the backfill touches.
///
/// **Read straight out of the table** rather than through any verb, because a
/// read that projects cannot see what this case is about.
#[tokio::test]
async fn an_edit_does_not_re_stamp_the_claims_own_column() {
    let scratch = Scratch::new("restamp");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("restamp")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:kept-alpha");
    memory
        .add_entity(NewEntity::new(subject.clone(), "Kept Alpha", "fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let claim = memory
        .capture(NewFact::about(
            subject.clone(),
            "was at the fair",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let badge = badge_of(&pool, "person:kept-alpha").await;
    let stamped_at = |pool: sqlx::MySqlPool, badge: String, id: String| async move {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT inserted_at FROM fact WHERE entity = ? AND id = ?",
        )
        .bind(badge)
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("the row is readable")
    };
    let taken_in = stamped_at(pool.clone(), badge.clone(), claim.id.as_str().to_string()).await;
    assert!(
        taken_in.is_some(),
        "the store did not stamp when it took the record in, so this case pins nothing",
    );

    memory
        .update_fact(
            &claim.address(),
            FactPatch {
                content: Some("was never at the fair".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("the guard waves it through");

    assert_eq!(
        stamped_at(pool.clone(), badge.clone(), claim.id.as_str().to_string()).await,
        taken_in,
        "an edit re-stamped the column that says when jojobot took the record in, which the \
         backfill copies from",
    );

    store.stop().await;
}

/// **A folded value's backing carries the note the record it came from
/// carries.**
///
/// 🚨 **The real store answers this over a DIFFERENT path from the double.**
/// `DoltMemory` overrides `backing` and folds `KeyWrite`s read off a join; a
/// store with no override walks `FieldWrite`s instead. **Both produce a
/// `FieldBacking` and only one of them is exercised by any in-process case**,
/// so a note that reached the second and not the first would be invisible
/// everywhere but production.
///
/// **Both halves**, because a build that always emits the note satisfies the
/// positive on its own.
#[tokio::test]
async fn a_folded_values_backing_carries_the_note_its_record_carries() {
    let scratch = Scratch::new("noted");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("noted")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:noted-alpha");
    memory
        .add_entity(NewEntity::new(subject.clone(), "Noted Alpha", "fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    let with_a_note = NewFact {
        details: Some("exact day not given, approximated as mid-summer".into()),
        fields: [("moved_in".to_string(), "2026-08-15".to_string())]
            .into_iter()
            .collect(),
        ..NewFact::about(subject.clone(), "when they moved in", date(2026, 8, 15))
    };
    memory
        .capture(with_a_note)
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let plain = NewFact {
        fields: [("rent".to_string(), "950".to_string())]
            .into_iter()
            .collect(),
        ..NewFact::about(subject.clone(), "what the rent is", date(2026, 8, 15))
    };
    memory
        .capture(plain)
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let backing = memory.backing(&subject).await.expect("backing reads");

    let moved_in = backing
        .get("moved_in")
        .expect("the value that has a note is backed");
    assert_eq!(
        moved_in.note.as_deref(),
        Some("exact day not given, approximated as mid-summer"),
        "the real store drops the note its record carries",
    );
    let rent = backing.get("rent").expect("the plain value is backed");
    assert_eq!(
        rent.note, None,
        "a record with no note produced one anyway: {:?}",
        rent.note,
    );

    store.stop().await;
}

/// **A store acting out a day stamps that day**, in the two columns that say
/// when jojobot took a record in and when a write of it happened.
///
/// ⚠️ **The fake cannot answer this.** Both columns are written by this
/// adapter, in SQL, and no caller can name either — so a case over the double
/// would only prove the double stamps what the double stamps. This is the
/// half that says the real store moved.
///
/// **Read straight out of the tables**, for the same reason the case above
/// does: what is under test is what the columns hold, not what a read projects
/// from them.
///
/// **Both columns**, because they are stamped in different statements: the
/// claim's own moment is set where the record is assembled, and the write's is
/// bound where the substrate row is appended.
#[tokio::test]
async fn a_store_acting_out_a_day_stamps_that_day() {
    const JUNE: &str = "2026-06-01";

    let scratch = Scratch::new("acting");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("acting")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone()).on_clock(jojobot_domain::clock::Clock::stating(
        JUNE.parse().expect("a day"),
    ));

    let subject = EntityId::person("person:acting-beta");
    memory
        .add_entity(NewEntity::new(subject.clone(), "Acting Beta", "fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let claim = memory
        .capture(NewFact::about(
            subject.clone(),
            "ate an apple",
            date(2026, 6, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let badge = badge_of(&pool, subject.as_str()).await;
    let taken_in: Option<String> =
        sqlx::query_scalar("SELECT inserted_at FROM fact WHERE entity = ? AND id = ?")
            .bind(&badge)
            .bind(claim.id.as_str())
            .fetch_one(&pool)
            .await
            .expect("the row is readable");
    let taken_in = taken_in.expect("the store stamps when it took the record in");
    assert!(
        taken_in.starts_with(JUNE),
        "the store stamped when it took the record in on the wall clock: {taken_in}",
    );

    let written_at: Option<String> = sqlx::query_scalar(
        "SELECT written_at FROM fact_write WHERE entity = ? AND fact_id = ? ORDER BY ordinal",
    )
    .bind(&badge)
    .bind(claim.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("the write row is readable");
    let written_at = written_at.expect("the store stamps when the write happened");
    assert!(
        written_at.starts_with(JUNE),
        "the store stamped the write itself on the wall clock: {written_at}",
    );

    store.stop().await;
}

/// 🚨 **The backfill is what keeps a claim written before the substrate
/// readable, and this is what goes red without it.**
///
/// A claim read is projected from `fact_write`. A claim's own row is still
/// written and nothing reads it, so a claim with no write behind it is not a
/// claim that reads partially — **it does not read at all**. Every claim in
/// every store that predates the substrate is in exactly that state until the
/// backfill runs, which makes the backfill load-bearing rather than tidy.
///
/// **The state is reached by taking the writes off a claim the store wrote
/// itself**, so the row under test is a row this store produced rather than
/// one this case hand-assembled. Its neighbour keeps its writes and stays
/// readable, which says the projection went dark for THAT claim rather than
/// for the read.
///
/// ⚠️ **The positive is the half that gives it meaning**: the same claim comes
/// back WHOLE once the backfill has run — whole meaning every column, asserted
/// against the claim as it read before its writes were taken away. A negative
/// alone passes against a build where every read is broken.
///
/// **The backfill runs through the runner rather than as loose SQL.** The
/// ledger row goes and `migrate::run` is called again, so what fills the
/// substrate here is the migration this build ships, on the path a start takes.
#[tokio::test]
async fn the_backfill_is_what_makes_a_claim_older_than_the_substrate_readable() {
    let scratch = Scratch::new("backfill");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("backfill")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:backfill-alpha");
    memory
        .add_entity(NewEntity::new(subject.clone(), "Backfill Alpha", "fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    // **A claim carrying more than its words**, because "whole" is every column
    // the backfill copies and a bare content would be satisfied by a fill that
    // dropped the rest.
    let claim = memory
        .capture(NewFact {
            details: Some("and stayed for the whole afternoon".into()),
            provenance: jojobot_domain::memory::Provenance::Testimony,
            stale_after: Some(date(2027, 1, 1)),
            ..NewFact::about(subject.clone(), "was at the fair", date(2026, 8, 10))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    let neighbour = memory
        .capture(NewFact::about(
            subject.clone(),
            "walked home afterwards",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let before = memory
        .recall(&subject)
        .await
        .expect("a plain read")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the claim reads before anything is taken away");

    // **The state every claim written before the substrate is in**: a row with
    // no write behind it.
    let badge = badge_of(&pool, subject.as_str()).await;
    sqlx::query("DELETE FROM fact_write WHERE entity = ? AND fact_id = ?")
        .bind(&badge)
        .bind(claim.id.as_str())
        .execute(&pool)
        .await
        .expect("the substrate is writable");
    let row: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ? AND id = ?")
        .bind(&badge)
        .bind(claim.id.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row is readable");
    assert_eq!(
        row, 1,
        "the claim's own row went with its writes, so the case is about a deleted row rather \
         than an empty substrate",
    );

    let dark = memory.recall(&subject).await.expect("a plain read");
    assert!(
        !dark.iter().any(|f| f.id == claim.id),
        "a claim with no write behind it read back, so the reads are not projected and the \
         backfill gates nothing: {dark:?}",
    );
    assert!(
        dark.iter().any(|f| f.id == neighbour.id),
        "the claim that kept its writes went dark too, so the read is broken rather than the \
         substrate empty: {dark:?}",
    );

    // **The backfill, through the runner, with its own shipped SQL.** The
    // ledger row is what makes a migration already-run, so taking it off is
    // what asks for one again.
    //
    // 🚨 **A step older than a rename has to be replayed against the schema it
    // was written for.** `0035` selects the claim's date under the name it had
    // then, and `0039`/`0040` renamed it since. **So the replay puts the two
    // columns back, asks for all three steps, and the renames carry the schema
    // forward again** — the store ends where it started and the backfill ran
    // its own text, which is the whole point of going through the runner.
    //
    // ⚠️ **This is a test technique and not a recovery procedure.** Nothing
    // outside `#[cfg(test)]` deletes a row from `schema_migration`; the running
    // code only ever clears the interruption marker beside it. An ordinary
    // start applies the set in order, so `0035` meets the old name where it
    // really runs and never meets the new one.
    for table in ["fact", "fact_write"] {
        sqlx::query(&format!(
            "ALTER TABLE {table} RENAME COLUMN recorded_at TO date"
        ))
        .execute(&pool)
        .await
        .expect("the schema goes back to what the frozen step was written for");
    }
    for version in [
        "0035_fact_write_backfill",
        "0039_fact_recorded_at",
        "0040_fact_write_recorded_at",
    ] {
        for table in ["schema_migration", "schema_migration_begun"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE version = ?"))
                .bind(version)
                .execute(&pool)
                .await
                .expect("the ledger is writable");
        }
    }
    let applied = migrate::run(&pool).await.expect("the backfill runs again");
    assert!(
        applied.contains(&"0035_fact_write_backfill".to_string()),
        "the backfill did not run, so what follows says nothing about it: {applied:?}",
    );
    // **And the schema came forward again**, so every assertion below reads the
    // store the rest of this suite reads rather than a half-migrated one.
    assert!(
        applied.contains(&"0039_fact_recorded_at".to_string())
            && applied.contains(&"0040_fact_write_recorded_at".to_string()),
        "the renames did not replay, so the store is left on the old names: {applied:?}",
    );

    let after = memory
        .recall(&subject)
        .await
        .expect("a plain read")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the backfill did not make the claim readable again");
    assert_eq!(
        after, before,
        "the claim came back changed, so the backfill fills the substrate with less than the \
         row holds",
    );

    store.stop().await;
}
