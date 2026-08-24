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
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::free_port;
use jojobot_domain::mailbox::testing::contract as mailboxes;
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityPatch, FactPatch, NewEntity, NewFact};
use jojobot_domain::session::testing::contract as sessions;

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

    // ── the path that edits ─────────────────────────────────────────────────
    memory
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
    .bind(subject.as_str())
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
        .bind(untouched.as_str())
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
    let bare = DoltMemory::open(pool.clone());
    bare.add_entity(jojobot_domain::memory::NewEntity::new(
        theirs.clone(),
        "My People",
        "user-named",
    ))
    .await
    .expect("the operator declares their own")
    .written()
    .expect("an empty board blocks nothing");

    let served = Provisioned::new(
        bare,
        Provisions::new(vec![jojobot_domain::memory::owned::Provision::record(
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
            },
            std::collections::BTreeMap::from([("selects".to_string(), "rhythm".to_string())]),
        )]),
    );

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
    sqlx::query("DELETE FROM fact_write WHERE entity = ? AND fact_id = ?")
        .bind(subject.as_str())
        .bind(claim.id.as_str())
        .execute(&pool)
        .await
        .expect("the substrate is writable");
    let row: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ? AND id = ?")
        .bind(subject.as_str())
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

    // **The backfill, through the runner.** The ledger row is what makes a
    // migration already-run, so taking it off is what asks for this one again.
    for table in ["schema_migration", "schema_migration_begun"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE version = ?"))
            .bind("0035_fact_write_backfill")
            .execute(&pool)
            .await
            .expect("the ledger is writable");
    }
    let applied = migrate::run(&pool).await.expect("the backfill runs again");
    assert!(
        applied.contains(&"0035_fact_write_backfill".to_string()),
        "the backfill did not run, so what follows says nothing about it: {applied:?}",
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
