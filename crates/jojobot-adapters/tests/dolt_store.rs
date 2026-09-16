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
use jojobot_adapters::fold::Folded;
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
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityPatch, FactAddress, FactPatch, MemoryError, NewEntity, NewFact,
};
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

/// 🚨 **The pointer migration, rule 268: a row written before this build
/// stored the badge at write time still holds whatever handle it was
/// given, in all four columns at once.**
///
/// **The pre-upgrade state is built with the port and then reverted with raw
/// SQL**, because the port itself now always writes the badge — there is no
/// window left to catch it in, unlike the badge fill above. Rewriting the
/// column back to the plain handle after a normal write is what a row from
/// before this slice actually looked like.
///
/// **Paired with the unresolvable case below**: this one must migrate clean,
/// rewriting every stale value and reporting zero left to fix on a second
/// run.
#[tokio::test]
async fn resolve_stale_pointer_columns_rewrites_every_stale_value() {
    let scratch = Scratch::new("pointer-migration-clean");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("pointer_migration_clean")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let object = EntityId("thing:pointer-migration-object".into());
    memory
        .add_entity(NewEntity::new(object.clone(), "Object", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let child = EntityId("thing:pointer-migration-child".into());
    memory
        .add_entity(NewEntity {
            parent: Some(object.clone()),
            ..NewEntity::new(child.clone(), "Child", "contract-fixture")
        })
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let subject = EntityId::person("person:pointer-migration-subject");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Subject",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let written = memory
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::About, object.clone())),
            refs: vec![object.clone()],
            ..NewFact::about(subject.clone(), "drew an edge", date(2026, 5, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");

    // **Reverted to the plain handle** — what every one of these columns
    // held before this build resolved them at write time.
    for statement in [
        "UPDATE fact SET edge_object = ? WHERE edge_object != ?",
        "UPDATE fact_write SET edge_object = ? WHERE edge_object != ?",
        "UPDATE fact_event_ref SET entity = ? WHERE entity != ?",
        "UPDATE entity SET parent = ? WHERE parent != ?",
    ] {
        sqlx::query(statement)
            .bind(object.as_str())
            .bind(object.as_str())
            .execute(&pool)
            .await
            .expect("the pre-upgrade shape is written");
    }

    // **Watched failing first.** Read bare, off the raw columns: still the
    // handle, not the badge, because nothing has resolved it yet.
    let stale: Option<String> = sqlx::query_scalar("SELECT edge_object FROM fact WHERE id = ?")
        .bind(written.id.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row reads");
    assert_eq!(
        stale.as_deref(),
        Some(object.as_str()),
        "the fixture was not reverted to the pre-upgrade shape",
    );

    let rewritten = memory
        .resolve_stale_pointer_columns()
        .await
        .expect("nothing is unresolvable here");
    assert_eq!(
        rewritten, 4,
        "one stale value in each of the four columns should have been rewritten",
    );

    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(object.as_str())
        .fetch_one(&pool)
        .await
        .expect("the object wears a badge");
    for (table, column) in [
        ("fact", "edge_object"),
        ("fact_write", "edge_object"),
        ("fact_event_ref", "entity"),
        ("entity", "parent"),
    ] {
        let now: String = sqlx::query_scalar(&format!(
            "SELECT {column} FROM {table} WHERE {column} IS NOT NULL LIMIT 1"
        ))
        .fetch_one(&pool)
        .await
        .expect("a rewritten row is there");
        assert_eq!(now, badge, "{table}.{column} still holds the plain handle");
    }

    // Served under the handle, exactly as before the fixture reverted it.
    let held = memory.recall(&subject).await.expect("recall reads");
    let fact = held
        .iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(fact.edge.as_ref().map(|e| &e.object), Some(&object));
    assert_eq!(fact.refs, vec![object.clone()]);
    let kids = memory
        .list_entities(None)
        .await
        .expect("list_entities reads")
        .into_iter()
        .find(|e| e.id == child)
        .expect("the child is there");
    assert_eq!(kids.parent.as_ref(), Some(&object));

    assert_eq!(
        memory
            .resolve_stale_pointer_columns()
            .await
            .expect("nothing left to fix"),
        0,
        "the migration rewrote a value a second time, so running it at every startup would not \
         be safe",
    );

    store.stop().await;
}

/// 🚨 **The pointer migration refuses to guess.** A stored value that
/// resolves through no current handle and no former one is not silently
/// dropped and not silently kept — the whole call comes back an error
/// naming the count and the row, and nothing for that value is rewritten.
///
/// **Paired with the clean case above**: a fixture built the same way, with
/// one value nothing has ever answered to mixed in.
#[tokio::test]
async fn resolve_stale_pointer_columns_refuses_to_guess_at_an_unresolvable_row() {
    let scratch = Scratch::new("pointer-migration-blocked");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("pointer_migration_blocked")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:pointer-migration-blocked-subject");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Subject",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    memory
        .capture(NewFact::about(
            subject.clone(),
            "names nothing that ever existed",
            date(2026, 5, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");
    let never_existed = "thing:pointer-migration-never-existed";
    sqlx::query(
        "UPDATE fact SET edge_shape = 'connection', edge_object = ? WHERE entity IN \
                 (SELECT badge FROM entity WHERE id = ?)",
    )
    .bind(never_existed)
    .bind(subject.as_str())
    .execute(&pool)
    .await
    .expect("a dangling edge is written directly");

    let err = memory
        .resolve_stale_pointer_columns()
        .await
        .expect_err("an unresolvable value must refuse rather than guess");
    let message = err.to_string();
    assert!(
        message.contains(never_existed),
        "the refusal does not name the row it could not resolve: {message}",
    );
    assert!(
        message.contains('1'),
        "the refusal does not carry the count: {message}",
    );

    // Nothing was silently dropped: the dangling value is exactly where it
    // was, not blanked and not guessed at.
    let still_there: String = sqlx::query_scalar(
        "SELECT edge_object FROM fact WHERE entity IN (SELECT badge FROM entity WHERE id = ?)",
    )
    .bind(subject.as_str())
    .fetch_one(&pool)
    .await
    .expect("the row reads");
    assert_eq!(still_there, never_existed);

    store.stop().await;
}

/// 🚨 **The reference-field migration rewrites a value stored as plain
/// handle text onto the permanent ids it names**, exactly the shape every
/// row held before this build lowered a reference-typed field value at
/// write time.
///
/// **Reverted with raw SQL, the same way `resolve_stale_pointer_columns`'s
/// own clean case is**: this feature never lowered a value before this
/// slice, so there is no earlier commit whose real output would differ from
/// what reading the pre-fix source already proves the shape to be — a
/// captured fixture from a checked-out commit would capture nothing this
/// revert does not already know.
#[tokio::test]
async fn migrate_reference_fields_rewrites_a_value_stored_as_plain_handle_text() {
    let scratch = Scratch::new("reference-field-migration-clean");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("reference_field_migration_clean")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    memory
        .declare_type(DeclaredType::new(
            "contract-field-migration-pointer",
            vec![Field::listing("friends", ValueType::Reference)],
        ))
        .await
        .expect("declare_type ok");

    let alpha = EntityId::person("person:contract-field-migration-alpha");
    memory
        .add_entity(NewEntity::new(alpha.clone(), "Alpha", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let beta = EntityId::person("person:contract-field-migration-beta");
    memory
        .add_entity(NewEntity::new(beta.clone(), "Beta", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let pet = EntityId("pet:contract-field-migration-pet".into());
    memory
        .add_entity(NewEntity::new(pet.clone(), "The Pet", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let written = memory
        .capture(NewFact {
            fields: [("friends".to_string(), format!("{alpha}, {beta}"))]
                .into_iter()
                .collect(),
            ..NewFact::about(pet.clone(), "made two friends", date(2026, 5, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");

    // **Reverted to the plain handle text** — what every reference-typed
    // field value held before this build lowered it at write time.
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(pet.as_str())
        .fetch_one(&pool)
        .await
        .expect("the pet wears a badge");
    sqlx::query(
        "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'friends' AND fact_id = ?",
    )
    .bind(format!("{alpha},{beta}"))
    .bind(&badge)
    .bind(written.id.as_str())
    .execute(&pool)
    .await
    .expect("the pre-lowering shape is written");

    // **Watched failing first.** Read bare, off the raw column: still the
    // plain handle text, not a permanent id, because nothing has resolved
    // it yet.
    let stale: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'friends' AND fact_id = ?",
    )
    .bind(&badge)
    .bind(written.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("the row reads");
    assert_eq!(
        stale,
        format!("{alpha},{beta}"),
        "the fixture was not reverted to the pre-lowering shape",
    );

    let rewritten = memory
        .migrate_reference_fields()
        .await
        .expect("nothing is unresolvable here");
    assert_eq!(
        rewritten, 1,
        "the one stale write should have been rewritten"
    );

    let now: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'friends' AND fact_id = ?",
    )
    .bind(&badge)
    .bind(written.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("the rewritten row is there");
    assert!(
        !now.contains(':'),
        "the migrated value still holds a plain handle rather than a permanent id: {now}",
    );

    // Served under the handle, exactly as before the fixture reverted it.
    let held = memory.fields(&pet).await.expect("fields reads");
    let served = held.get("friends").cloned().unwrap_or_default();
    assert!(
        served.contains(alpha.as_str()) && served.contains(beta.as_str()),
        "the migrated value did not compose back to today's handles: {served}",
    );

    assert_eq!(
        memory
            .migrate_reference_fields()
            .await
            .expect("nothing left to fix"),
        0,
        "the migration rewrote a value a second time, so running it at every startup would not \
         be safe",
    );

    store.stop().await;
}

/// 🚨 **The reference-field migration refuses to guess.** A stored value
/// that resolves through no current handle and no former one is not
/// silently dropped and not silently kept — the whole call comes back an
/// error naming the count and the row, and nothing for that value is
/// rewritten.
///
/// **Paired with the clean case above**: a fixture built the same way, with
/// one item nothing has ever answered to mixed in.
#[tokio::test]
async fn migrate_reference_fields_refuses_to_guess_at_an_unresolvable_value() {
    let scratch = Scratch::new("reference-field-migration-blocked");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("reference_field_migration_blocked")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    memory
        .declare_type(DeclaredType::new(
            "contract-field-migration-blocked-pointer",
            vec![Field::new("friend", ValueType::Reference)],
        ))
        .await
        .expect("declare_type ok");

    let pet = EntityId("pet:contract-field-migration-blocked-pet".into());
    memory
        .add_entity(NewEntity::new(pet.clone(), "The Pet", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    memory
        .capture(NewFact::about(
            pet.clone(),
            "names nothing that ever existed",
            date(2026, 5, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");
    let never_existed = "person:contract-field-migration-never-existed";
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(pet.as_str())
        .fetch_one(&pool)
        .await
        .expect("the pet wears a badge");
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id) VALUES (?, 'friend', \
         1, ?, (SELECT id FROM fact WHERE entity = ? LIMIT 1))",
    )
    .bind(&badge)
    .bind(never_existed)
    .bind(&badge)
    .execute(&pool)
    .await
    .expect("a dangling reference-field value is written directly");

    let err = memory
        .migrate_reference_fields()
        .await
        .expect_err("an unresolvable value must refuse rather than guess");
    let message = err.to_string();
    assert!(
        message.contains(never_existed),
        "the refusal does not name the value it could not resolve: {message}",
    );
    assert!(
        message.contains('1'),
        "the refusal does not carry the count: {message}",
    );

    // Nothing was silently dropped: the dangling value is exactly where it
    // was, not blanked and not guessed at.
    let still_there: String =
        sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = 'friend'")
            .bind(&badge)
            .fetch_one(&pool)
            .await
            .expect("the row reads");
    assert_eq!(still_there, never_existed);

    store.stop().await;
}

/// 🚨 **The migration batch, against rows the previous build actually wrote.**
///
/// Every other case in this module builds its "before" state either with the
/// current port (for a badge fill, still reachable through a real code path)
/// or by reverting the current port's own output with raw SQL (for the
/// pointer columns, because the current port always resolves them now). Both
/// are informed guesses about what a genuinely old row looked like.
///
/// **This fixture is neither.** `tests/fixtures/a334e84_pre_batch.sql` is a
/// literal dump of rows written by `a334e84` — the last pushed commit before
/// this batch, checked out and built on its own, driven through its own
/// `DoltMemory` port with no raw SQL involved in producing the rows
/// themselves. At that commit: `fact.entity`/`fact_write.entity` already hold
/// the plain handle rather than a badge (the resolve step that prefers a
/// badge postdates it), `fact.edge_object`/`entity.parent`/
/// `fact_event_ref.entity` do too, and retraction still worked by writing the
/// old `retracted`/`superseded` status words rather than `archived`.
///
/// **What this proves that the synthetic versions cannot**: that the actual
/// bytes on disk before this batch take the shape every migration and
/// backfill here assumes they take. A hand-authored or reverted fixture can
/// only be wrong in the same way its author already believed; a captured one
/// cannot.
#[tokio::test]
async fn the_batch_migrates_rows_a334e84_actually_wrote() {
    let scratch = Scratch::new("batch-real-fixture");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("batch_real_fixture")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let fixture = include_str!("fixtures/a334e84_pre_batch.sql");
    for statement in fixture.lines().filter(|line| line.starts_with("INSERT")) {
        sqlx::raw_sql(statement)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("the captured fixture loads: {e}: {statement}"));
    }

    // **Loaded and holding what it should, before anything acts on it.** A
    // case that skipped this could pass identically on a fixture that failed
    // to load at all.
    let before_status: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM fact WHERE entity = 'person:pointer-migration-subject' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("the fixture's own rows read");
    assert_eq!(
        before_status,
        vec!["active", "retracted", "active", "superseded"],
        "the captured fixture did not load in the shape it was captured in: {before_status:?}",
    );
    let before_pointer: Option<String> =
        sqlx::query_scalar("SELECT edge_object FROM fact WHERE entity = ? AND id = 'f1'")
            .bind("person:pointer-migration-subject")
            .fetch_one(&pool)
            .await
            .expect("the edge row reads");
    assert_eq!(
        before_pointer.as_deref(),
        Some("thing:pointer-migration-object"),
        "the fixture's edge_object is not the plain handle it was captured with",
    );

    // **The remaining migrations, over rows that already exist** — the shape
    // a real upgrade takes: 0046/0047 ran once already against an empty
    // database when `migrate::run` created the schema above, so this
    // re-issues their own statements, verbatim from the shipped files,
    // against the fixture that landed afterward.
    sqlx::raw_sql(include_str!("../migrations/0046_fact_status_archived.sql"))
        .execute(&pool)
        .await
        .expect("0046 runs");
    sqlx::raw_sql(include_str!(
        "../migrations/0047_fact_write_status_archived.sql"
    ))
    .execute(&pool)
    .await
    .expect("0047 runs");

    let memory = DoltMemory::open(pool.clone());
    let rekeyed = memory
        .backfill_handle_keyed_rows()
        .await
        .expect("the handle-keyed rekey runs");
    assert!(
        rekeyed > 0,
        "the fixture's own entity is handle-keyed in fact/fact_write/field_write/\
         fact_event_ref, and nothing was rekeyed",
    );
    let resolved = memory
        .resolve_stale_pointer_columns()
        .await
        .expect("nothing here is unresolvable");
    assert!(
        resolved > 0,
        "the fixture's own edge_object and parent are stale handles, and nothing was resolved",
    );

    // ── key by key ──

    // A collapsed status arrives as the archive status, on both the record
    // and its own history.
    let after_status: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM fact WHERE entity IN (SELECT badge FROM entity WHERE id = ?) \
         ORDER BY id",
    )
    .bind("person:pointer-migration-subject")
    .fetch_all(&pool)
    .await
    .expect("the migrated rows read");
    assert_eq!(
        after_status,
        vec!["active", "archived", "active", "archived"],
        "a collapsed status did not arrive as archived: {after_status:?}",
    );
    let write_statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM fact_write WHERE entity IN (SELECT badge FROM entity WHERE id = ?) \
         ORDER BY fact_id, written_at",
    )
    .bind("person:pointer-migration-subject")
    .fetch_all(&pool)
    .await
    .expect("the write history reads");
    assert_eq!(
        write_statuses,
        vec![
            "active", "active", "archived", "active", "active", "archived"
        ],
        "a write carrying a collapsed status was not rewritten: {write_statuses:?}",
    );

    // The retraction's own reason and pointer survive the status rewrite
    // intact — its `retracts` value still names the record it took back.
    let retracts: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE `key` = 'retracts' AND entity IN \
         (SELECT badge FROM entity WHERE id = ?)",
    )
    .bind("person:pointer-migration-subject")
    .fetch_one(&pool)
    .await
    .expect("the retraction pointer reads");
    assert_eq!(
        retracts, "person:pointer-migration-subject#f2",
        "the retraction's own pointer did not survive the status collapse",
    );

    // A pointer stored as a raw handle arrives lowered: the edge, the parent,
    // and the ref all now name the object's badge rather than its handle.
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind("thing:pointer-migration-object")
        .fetch_one(&pool)
        .await
        .expect("the object wears a badge");
    let edge_now: String = sqlx::query_scalar(
        "SELECT edge_object FROM fact WHERE id = 'f1' AND edge_object IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("the edge row reads");
    assert_eq!(
        edge_now, badge,
        "fact.edge_object still holds the plain handle"
    );
    let parent_now: String = sqlx::query_scalar("SELECT parent FROM entity WHERE id = ?")
        .bind("thing:pointer-migration-child")
        .fetch_one(&pool)
        .await
        .expect("the child reads");
    assert_eq!(
        parent_now, badge,
        "entity.parent still holds the plain handle"
    );
    let ref_now: String =
        sqlx::query_scalar("SELECT entity FROM fact_event_ref WHERE fact_id = 'f1'")
            .fetch_one(&pool)
            .await
            .expect("the ref row reads");
    assert_eq!(
        ref_now, badge,
        "fact_event_ref.entity still holds the plain handle",
    );

    // A reference-typed field value — the one shape this migration batch does
    // not touch at all. `retracts` is not a declared reference field, but it
    // is the same structural shape: a field's own VALUE embeds a handle-based
    // address, and no backfill here rewrites `field_write.value`. Documented
    // as current behaviour rather than asserted as a defect: nothing in this
    // slice's scope says it should change, and the value read back above is
    // exactly the unrewritten handle-based address the fixture was captured
    // with.

    // Nothing readable before is unreadable after: every claim the fixture
    // carried still reads, under the object's current handle.
    let subject = EntityId::person("person:pointer-migration-subject");
    let read = memory.recall(&subject).await.expect("recall reads");
    assert_eq!(
        read.len(),
        4,
        "not every pre-batch claim survived: {read:?}"
    );
    assert!(
        read.iter().any(|f| f.content == "drew an edge and a ref"
            && f.edge.as_ref().map(|e| &e.object)
                == Some(&EntityId("thing:pointer-migration-object".into()))),
        "the edge-bearing claim did not read back correctly: {read:?}",
    );

    // Idempotent: a second run of both finds nothing left to do.
    assert_eq!(
        memory
            .backfill_handle_keyed_rows()
            .await
            .expect("the rekey runs again"),
        0,
        "the rekey found handle-keyed rows a second time",
    );
    assert_eq!(
        memory
            .resolve_stale_pointer_columns()
            .await
            .expect("nothing left to fix"),
        0,
        "the pointer migration found stale rows a second time",
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

/// 🚨 **Renaming a row from before the badge column exists is a readable
/// error, never a panic.**
///
/// `rename_entity` writes a forwarding row keyed on the badge the renamed
/// entity wears — but a row from before that column existed, or one the
/// startup fill has not reached yet, wears none. `scan` and `scan_entity`
/// already answer this exact condition without crashing; this is the same
/// condition on the write side of a rename.
#[tokio::test]
async fn renaming_a_row_with_no_badge_is_a_readable_error_not_a_panic() {
    let scratch = Scratch::new("rename-no-badge");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("renamenobadge")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let was = EntityId::person("person:rename-no-badge-was");
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge)
         VALUES (?, 'person', 'No Badge Yet', 'contract-fixture', NULL, NULL, 'on-demand', '', \
         NULL)",
    )
    .bind(was.as_str())
    .execute(&pool)
    .await
    .expect("a row from before the column");

    let now = EntityId("work:rename-no-badge-now".into());
    let result = memory
        .rename_entity(&was, &now, None, date(2026, 1, 1), None)
        .await;
    assert!(
        matches!(result, Err(MemoryError::Store(_))),
        "renaming a badgeless row did not come back a readable store error: {result:?}",
    );

    let unchanged: (String,) = sqlx::query_as("SELECT id FROM entity WHERE id = ?")
        .bind(was.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row is still there, untouched");
    assert_eq!(
        unchanged.0,
        was.to_string(),
        "the refused rename moved the row anyway",
    );

    store.stop().await;
}

/// 🚨 **A rename to or from a handle near the domain's own limit does not
/// fail on the forwarding row's account.**
///
/// The domain accepts a handle up to 128 characters
/// ([`jojobot_domain::memory::validate_subject`]). `entity_former_handle`
/// used to hold `former_handle` at `VARCHAR(64)` — half that — so a rename
/// past 64 characters failed outright under the store's strict mode,
/// naming no cause a caller could act on.
#[tokio::test]
async fn a_rename_near_the_handle_length_limit_still_lands() {
    let scratch = Scratch::new("rename-long-handle");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("renamelonghandle")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    // "person:" is 7 characters, so 115 of these keeps the whole handle at
    // 122 — under the domain's 128-character limit and well past the
    // forwarding row's former 64-character one.
    let long_slug = "x".repeat(115);
    let was = EntityId(format!("person:{long_slug}"));
    memory
        .add_entity(NewEntity::new(
            was.clone(),
            "Long Handle",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    let now = EntityId(format!("work:{}", "y".repeat(115)));
    memory
        .rename_entity(&was, &now, None, date(2026, 1, 1), None)
        .await
        .expect("a rename past 64 characters must not fail on the forwarding row")
        .written()
        .expect("nothing collides with the destination");

    let stored: (String,) =
        sqlx::query_as("SELECT former_handle FROM entity_former_handle WHERE former_handle = ?")
            .bind(was.as_str())
            .fetch_one(&pool)
            .await
            .expect("the forwarding row was written whole, not truncated");
    assert_eq!(
        stored.0,
        was.to_string(),
        "the forwarding row does not hold the handle it was given",
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

/// **The same contract, wrapped in a fold, against the real store.** `Folded`
/// forwards every guard, every read and every write it does not itself
/// refresh straight to Dolt, so this is the regression test for that claim
/// against the store the fold exists to spare, not only against the fake.
#[tokio::test]
async fn dolt_satisfies_the_memory_contract_when_folded() {
    let scratch = Scratch::new("memory_folded");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::run_all(&Folded::new(Arc::new(DoltMemory::open(pool)))).await;

    store.stop().await;
}

/// **`scan_entity` must answer exactly what `scan` would answer for the same
/// row, over a real corpus rather than a hand-picked one.**
///
/// The contract's own run leaves behind entities with aliases, parents,
/// merges, retractions and folded fields — every shape `scan`'s per-entity
/// loop builds a document out of. Comparing `scan_entity`'s answer against
/// `scan`'s own for every one of them, whole value against whole value, is
/// what catches an override that gets the easy fields right and drops the
/// harder ones — a per-field check would only catch what this case's author
/// thought to name.
#[tokio::test]
async fn scan_entity_agrees_with_scan_for_every_document_the_real_store_holds() {
    let scratch = Scratch::new("scan-entity");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("scanentity")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    memory::run_all(&memory).await;

    let whole = memory.scan().await.expect("the full scan answers");
    assert!(
        whole.len() > 50,
        "the contract should have left far more than {} documents behind",
        whole.len()
    );
    for doc in &whole {
        let Some(entity) = &doc.entity else { continue };
        let by_id = memory
            .scan_entity(&entity.id)
            .await
            .expect("scan_entity answers for a row scan just found");
        assert_eq!(
            by_id.as_ref(),
            Some(doc),
            "scan_entity disagreed with scan over {}",
            entity.id
        );
    }

    let missing = EntityId("person:contract-nobody".into());
    assert_eq!(
        memory
            .scan_entity(&missing)
            .await
            .expect("the read answers"),
        None,
        "an entity scan never heard of is a miss, not an error",
    );

    store.stop().await;
}

/// **The mark's own contract, against the real store.**
#[tokio::test]
async fn dolt_satisfies_the_stands_for_contract() {
    let scratch = Scratch::new("stands-for");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("standsfor")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::run_all_stands_for(&DoltMemory::open(pool)).await;

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
            archived: None,
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
            archived: None,
        },
        std::collections::BTreeMap::new(),
    )]);
    let known = DoltMemory::open(pool.clone()).knowing(supplied);
    let bare = DoltMemory::open(pool);

    memory::add_entity_guards_hold_for_stored_and_supplied(&bare, &known).await;
    memory::a_claim_on_a_supplied_record_reads_back(&known).await;
    memory::a_rename_of_a_supplied_handle_is_refused_not_a_silent_no_op(&known).await;
    memory::an_archive_of_a_supplied_handle_is_refused_not_a_silent_no_op(&known).await;
    memory::a_merge_naming_a_supplied_handle_is_refused_not_a_silent_no_op(&known).await;

    store.stop().await;
}

/// **A whole-record provision colliding with a real stored row is refused,
/// over the real store** — the class the original defect actually shipped
/// as: a crate-narrow suite runs only the fake, and this is the case the
/// fake alone could not have proven wrong about the real thing.
///
/// On its own database: the collision is checked against `list_entities`
/// directly, and a row left behind by another case would be a row this case
/// did not create the collision against.
#[tokio::test]
async fn dolt_refuses_a_supplied_record_colliding_with_a_stored_row() {
    let scratch = Scratch::new("provision-collision");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("provision-collision")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::a_supplied_record_colliding_with_a_stored_row_is_refused(&DoltMemory::open(pool)).await;

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
            archived: None,
        },
        std::collections::BTreeMap::new(),
    )]);
    let known = DoltMemory::open(pool).knowing(supplied);

    memory::every_entity_read_answers_for_a_supplied_record(&known).await;

    store.stop().await;
}

/// **…and a fact address minted before a fold, over the real store's own
/// renumbering** — a database of its own, not the shared `run_all` mount,
/// because the fold's renumbering is exactly the state a case sharing a
/// database with dozens of others should not have to reason about.
#[tokio::test]
async fn dolt_names_the_survivor_for_an_address_stale_after_a_fold() {
    let scratch = Scratch::new("stale-after-fold");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("staleafterfold")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    memory::a_stale_address_after_a_fold_says_where_it_went(&DoltMemory::open(pool)).await;

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
            archived: None,
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
                    archived: None,
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

/// 🚨 **A row carrying a retired status is rewritten to `archived` on disk**,
/// not only read that way.
///
/// `FactStatus::from_token` already maps `superseded`, `retracted` and
/// `negated` to `archived` on the way in, so a read is correct without this.
/// The backfill is what makes the SPELLING on disk say what the store now
/// means, over both tables that carry a status: `fact`, the current
/// snapshot, and `fact_write`, which keeps its own copy per write.
#[tokio::test]
async fn the_backfill_rewrites_every_retired_status_to_archived() {
    let scratch = Scratch::new("status-archived");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("statusarchived")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());

    let subject = EntityId::person("person:status-archived-alpha");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Status Archived Alpha",
            "fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let claim = memory
        .capture(NewFact::about(
            subject.clone(),
            "was a member",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    // **The badge, because the fact tables key on it, not on the handle.**
    let badge: (String,) = sqlx::query_as("SELECT badge FROM entity WHERE id = ?")
        .bind(subject.as_str())
        .fetch_one(&pool)
        .await
        .expect("the entity is readable");

    // **A row from before the two marks collapsed**, staged past the domain
    // layer exactly as the rename-replay case above stages a schema from
    // before a rename — this is what a real pre-migration row looks like.
    sqlx::query("UPDATE fact SET status = 'superseded' WHERE entity = ? AND id = ?")
        .bind(&badge.0)
        .bind(claim.id.as_str())
        .execute(&pool)
        .await
        .expect("the row is writable");
    sqlx::query("UPDATE fact_write SET status = 'superseded' WHERE entity = ? AND fact_id = ?")
        .bind(&badge.0)
        .bind(claim.id.as_str())
        .execute(&pool)
        .await
        .expect("the row is writable");

    for version in [
        "0046_fact_status_archived",
        "0047_fact_write_status_archived",
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
        applied.contains(&"0046_fact_status_archived".to_string())
            && applied.contains(&"0047_fact_write_status_archived".to_string()),
        "the backfill did not run, so what follows says nothing about it: {applied:?}",
    );

    let fact_status: (String,) =
        sqlx::query_as("SELECT status FROM fact WHERE entity = ? AND id = ?")
            .bind(&badge.0)
            .bind(claim.id.as_str())
            .fetch_one(&pool)
            .await
            .expect("the row is readable");
    assert_eq!(
        fact_status.0, "archived",
        "fact still carries the retired spelling on disk",
    );
    let write_status: (String,) =
        sqlx::query_as("SELECT status FROM fact_write WHERE entity = ? AND fact_id = ?")
            .bind(&badge.0)
            .bind(claim.id.as_str())
            .fetch_one(&pool)
            .await
            .expect("the row is readable");
    assert_eq!(
        write_status.0, "archived",
        "fact_write still carries the retired spelling on disk",
    );

    store.stop().await;
}

/// **A `fields` hit never pays for a listing of every entity.** `resolve`
/// answers a live handle from one targeted row; the full listing exists only
/// to say what a name that answered to nothing resembles, and a hit never
/// needs that question asked.
#[tokio::test]
async fn a_fields_hit_answers_with_no_full_listing() {
    let scratch = Scratch::new("resolve_hit");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact {
            fields: std::collections::BTreeMap::from([(
                "due_on".to_string(),
                "2026-09-14".to_string(),
            )]),
            ..NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let fields = memory.fields(&gamma).await.expect("fields ok");
    assert_eq!(
        fields.get("due_on"),
        Some(&"2026-09-14".to_string()),
        "the hit still answers correctly"
    );
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    store.stop().await;
}

/// **A `fields` miss still comes back with its near candidates.** The fix
/// above defers the full listing to the miss branch rather than removing it
/// — the refusal's whole value is naming a way forward, and that still needs
/// the picture a hit does not.
#[tokio::test]
async fn a_fields_miss_still_answers_with_its_near_candidates() {
    let scratch = Scratch::new("resolve_miss");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");

    // One letter off — near enough for the screen to find it, far enough
    // that the direct row lookups in resolve_by_id/resolve_by_badge miss.
    let near_miss = EntityId::person("person:gama");
    let before = memory.index_listings();
    let err = memory
        .fields(&near_miss)
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => assert!(
            !nearest.is_empty(),
            "a near-miss must still come back with candidates, not an empty list"
        ),
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "a miss must still build the listing, which is what the candidates come from"
    );

    store.stop().await;
}

/// **`recall` deferred the same way `fields` did.** Same read, shared with
/// the store's own read-then-fix over every site that built the listing
/// eagerly before a resolve.
#[tokio::test]
async fn a_recall_hit_answers_with_no_full_listing_and_a_miss_still_gets_candidates() {
    let scratch = Scratch::new("resolve_recall");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let facts = memory.recall(&gamma).await.expect("recall ok");
    assert_eq!(facts.len(), 1, "the hit still answers correctly");
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    let before = memory.index_listings();
    let err = memory
        .recall(&EntityId::person("person:gama"))
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "a miss must still build the listing"
    );

    store.stop().await;
}

/// **`history` deferred the same way.**
#[tokio::test]
async fn a_history_hit_answers_with_no_full_listing_and_a_miss_still_gets_candidates() {
    let scratch = Scratch::new("resolve_history");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact {
            fields: std::collections::BTreeMap::from([(
                "due_on".to_string(),
                "2026-09-14".to_string(),
            )]),
            ..NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let writes = memory.history(&gamma, "due_on").await.expect("history ok");
    assert_eq!(writes.len(), 1, "the hit still answers correctly");
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    let before = memory.index_listings();
    let err = memory
        .history(&EntityId::person("person:gama"), "due_on")
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "a miss must still build the listing"
    );

    store.stop().await;
}

/// **`backing` deferred the same way.**
#[tokio::test]
async fn a_backing_hit_answers_with_no_full_listing_and_a_miss_still_gets_candidates() {
    let scratch = Scratch::new("resolve_backing");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact {
            fields: std::collections::BTreeMap::from([(
                "due_on".to_string(),
                "2026-09-14".to_string(),
            )]),
            ..NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let backing = memory.backing(&gamma).await.expect("backing ok");
    assert_eq!(backing.len(), 1, "the hit still answers correctly");
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    let before = memory.index_listings();
    let err = memory
        .backing(&EntityId::person("person:gama"))
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "a miss must still build the listing"
    );

    store.stop().await;
}

/// **`claim_histories` deferred the same way.**
#[tokio::test]
async fn a_claim_histories_hit_answers_with_no_full_listing_and_a_miss_still_gets_candidates() {
    let scratch = Scratch::new("resolve_claim_histories");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let histories = memory
        .claim_histories(&gamma)
        .await
        .expect("claim_histories ok");
    assert_eq!(histories.len(), 1, "the hit still answers correctly");
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    let before = memory.index_listings();
    let err = memory
        .claim_histories(&EntityId::person("person:gama"))
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "a miss must still build the listing"
    );

    store.stop().await;
}

/// **`claim_history` deferred, and it has TWO miss branches**: a handle
/// that does not resolve, and a resolved handle with no writes at that fact
/// id. Both are checked, because the fix moved the listing into both.
#[tokio::test]
async fn a_claim_history_hit_answers_with_no_full_listing_and_both_misses_still_get_candidates_or_repair()
 {
    let scratch = Scratch::new("resolve_claim_history");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let claim = memory
        .capture(NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    let writes = memory
        .claim_history(&FactAddress::new(gamma.clone(), claim.id.clone()))
        .await
        .expect("claim_history ok");
    assert_eq!(writes.len(), 1, "the hit still answers correctly");
    assert_eq!(
        memory.index_listings(),
        before,
        "a hit must not build the full listing"
    );

    // First miss branch: the handle itself does not resolve.
    let before = memory.index_listings();
    let err = memory
        .claim_history(&FactAddress::new(
            EntityId::person("person:gama"),
            claim.id.clone(),
        ))
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "the handle-miss branch must still build the listing"
    );

    // Second miss branch: the handle resolves, the fact id does not.
    let before = memory.index_listings();
    let err = memory
        .claim_history(&FactAddress::new(
            gamma.clone(),
            jojobot_domain::memory::FactId("f99".into()),
        ))
        .await
        .expect_err("an address with no writes must not resolve");
    assert!(
        matches!(err, MemoryError::UnknownFact { .. }),
        "expected UnknownFact, got {err:?}"
    );
    assert!(
        memory.index_listings() > before,
        "the fact-miss branch must still build the listing, which is what the already-merged \
         check reads"
    );

    store.stop().await;
}

/// **`retract` deferred, and it has the same two miss branches
/// `claim_history` does.**
#[tokio::test]
async fn a_retract_hit_answers_with_no_full_listing_and_both_misses_still_build_the_listing() {
    let scratch = Scratch::new("resolve_retract");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;

    let memory = DoltMemory::open(pool);
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    let claim = memory
        .capture(NewFact::about(gamma.clone(), "seeded", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let before = memory.index_listings();
    memory
        .retract(
            &FactAddress::new(gamma.clone(), claim.id.clone()),
            Some("no longer true"),
            date(2026, 9, 8),
        )
        .await
        .expect("retract ok");
    assert_eq!(
        memory.index_listings(),
        before,
        "the hit must not build the full listing"
    );

    // First miss branch: the handle itself does not resolve.
    let before = memory.index_listings();
    let err = memory
        .retract(
            &FactAddress::new(EntityId::person("person:gama"), claim.id.clone()),
            None,
            date(2026, 9, 8),
        )
        .await
        .expect_err("an unwritten near-miss handle must not resolve");
    match err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                !nearest.is_empty(),
                "a near-miss must still come back with candidates"
            )
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }
    assert!(
        memory.index_listings() > before,
        "the handle-miss branch must still build the listing"
    );

    // Second miss branch: the handle resolves, the fact id does not (and is
    // no longer active besides, having just been retracted above).
    let before = memory.index_listings();
    let err = memory
        .retract(
            &FactAddress::new(gamma.clone(), jojobot_domain::memory::FactId("f99".into())),
            None,
            date(2026, 9, 8),
        )
        .await
        .expect_err("an address with no fact must not resolve");
    assert!(
        matches!(err, MemoryError::UnknownFact { .. }),
        "expected UnknownFact, got {err:?}"
    );
    assert!(
        memory.index_listings() > before,
        "the fact-miss branch must still build the listing, which is what the already-merged \
         check reads"
    );

    store.stop().await;
}

/// **The cheap signal [`Memory::write_summary`] answers, against the real
/// store.**
///
/// The pair a caller must be able to trust: unmoved across two calls with
/// nothing written between them, and moved by every kind of write this slice
/// wires the log into — a fact write, and every shape of entity write
/// (creation, prose, rename, archive, merge).
#[tokio::test]
async fn write_summary_answers_the_real_store() {
    let scratch = Scratch::new("write-summary");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("write_summary")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool);

    let empty = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the dolt store offers the signal");
    assert_eq!(empty.entities.0, 0, "an empty store has written no entity");
    assert_eq!(empty.facts.0, 0, "an empty store has written no fact");

    // **The unchanged half of the pair.** Nothing wrote between these two
    // calls, so a caller comparing them must see no difference.
    let still_empty = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert_eq!(
        still_empty, empty,
        "nothing was written, so the signal must not move"
    );

    // `add_entity` moves the entity half and not the fact half.
    let homer = EntityId::person("person:homer");
    memory
        .add_entity(NewEntity::new(homer.clone(), "Homer", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let after_add = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_add.entities.0 > empty.entities.0,
        "add_entity did not move the entity count: {after_add:?}"
    );
    assert_eq!(
        after_add.facts, empty.facts,
        "add_entity must not move the fact half"
    );

    // `capture` moves the fact half and not the entity half.
    memory
        .capture(NewFact::about(
            homer.clone(),
            "keeps a duff in the fridge",
            date(2026, 1, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");
    let after_capture = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_capture.facts.0 > after_add.facts.0,
        "capture did not move the fact count: {after_capture:?}"
    );
    assert_eq!(
        after_capture.entities, after_add.entities,
        "capture must not move the entity half"
    );

    // `set_prose` moves the entity half.
    memory
        .set_prose(&homer, "a page somebody wrote")
        .await
        .expect("set_prose ok");
    let after_prose = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_prose.entities.0 > after_capture.entities.0,
        "set_prose did not move the entity count: {after_prose:?}"
    );

    // `rename_entity` moves the entity half.
    let renamed = EntityId::person("person:homer-simpson");
    memory
        .rename_entity(&homer, &renamed, None, date(2026, 1, 1), None)
        .await
        .expect("rename_entity ok")
        .written()
        .expect("nothing collides with it");
    let after_rename = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_rename.entities.0 > after_prose.entities.0,
        "rename_entity did not move the entity count: {after_rename:?}"
    );

    // `archive_entity` moves the entity half.
    let alpha = EntityId::person("person:alpha");
    memory
        .add_entity(NewEntity::new(alpha.clone(), "Alpha", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let before_archive = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    memory
        .archive_entity(&alpha, "a mistaken write")
        .await
        .expect("archive_entity ok");
    let after_archive = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_archive.entities.0 > before_archive.entities.0,
        "archive_entity did not move the entity count: {after_archive:?}"
    );

    // `merge` moves the entity half — one write, whatever the sweep's fan-out
    // was (see append_entity_write's own call site in `merge`).
    let beta = EntityId::person("person:beta");
    let gamma = EntityId::person("person:gamma");
    memory
        .add_entity(NewEntity::new(beta.clone(), "Beta", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    memory
        .add_entity(NewEntity::new(gamma.clone(), "Gamma", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides with it");
    let before_merge = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    memory
        .merge(&beta, &gamma, None, date(2026, 1, 1))
        .await
        .expect("merge ok");
    let after_merge = memory
        .write_summary()
        .await
        .expect("write_summary ok")
        .expect("the signal");
    assert!(
        after_merge.entities.0 > before_merge.entities.0,
        "merge did not move the entity count: {after_merge:?}"
    );

    store.stop().await;
}
