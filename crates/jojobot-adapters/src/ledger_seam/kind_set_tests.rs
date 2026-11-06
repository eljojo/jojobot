//! The seam replays lazily, at the first writer job, and a declaration whose field
//! points at kinds is read back through the kinds the process has loaded. The
//! ledger serves requests and app attempts for every write, so a claims member the
//! seam cannot read must not stop it: the claims state stays behind the log, the
//! ledger keeps answering, and the claims state catches up once the kinds are held.
//!
//! Each case names kinds of its own, because the kind set is the process's and the
//! cases run side by side.

use std::sync::Arc;

use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::log::{
    BatchId, Event, LogStore, ObjectId, Position, RequestId, RunId, SealedBatch,
};
use jojobot_domain::memory::EntityKind;
use jojobot_domain::memory::types::{DeclaredType, Field, Fold, Kinds, Origin, ValueType};
use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};

use super::Seam;
use crate::dolt::Dolt;
use crate::ledger_log::LedgerError;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};
use crate::writer_log::WriterLog;

/// **One case at a time in this module**: each loads or replaces the process-wide kind
/// set, which the others read.
static KINDS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn at() -> jiff::Timestamp {
    jiff::Timestamp::from_second(1_700_000_000).expect("a moment")
}

fn declaration_event() -> Event<ClaimEvent> {
    jojobot_domain::memory::kinds::load_shipped();
    Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(at()),
        imported_source_time: None,
        payload: ClaimEvent::TypeDeclared {
            kind: ObjectId("k5".into()),
            declaration: DeclaredType {
                name: "tally".into(),
                fields: vec![Field {
                    key: "owner".into(),
                    holds: ValueType::Reference,
                    points_at: Kinds::any_of(&[EntityKind::PERSON, EntityKind::BOT]),
                    folds: Fold::Newest,
                    required: false,
                    list: false,
                    one_of: None,
                }],
                origin: Origin::Declared,
                must: None,
            },
        },
    }
}

/// The stored member of a declaration whose field points at `person` and `bot`.
fn declaration_member() -> Vec<u8> {
    crate::claims_log::codec::encode(&declaration_event())
}

/// The same declaration, pointing at the two kinds in `pair` (`first|second`).
fn declaration_naming(pair: &str) -> Vec<u8> {
    let stored = String::from_utf8(declaration_member()).expect("json is text");
    assert!(stored.contains("person|bot"), "{stored}");
    stored.replace("person|bot", pair).into_bytes()
}

fn actor() -> ObjectId {
    ObjectId("bot:homer".into())
}

fn request_id() -> RequestId {
    RequestId("alpha".into())
}

/// A ledger member: a request that spent its id, kept for the next minute.
fn commit_member() -> Vec<u8> {
    let now = jiff::Timestamp::now();
    crate::ledger_log::codec::encode(&LedgerEvent::RequestCommitted(Commit {
        request: Request {
            key: RequestKey {
                actor: actor(),
                id: request_id(),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest-alpha".into(),
        },
        landed: "{}".into(),
        receipt: None,
        written_at: now,
        expires_at: now + std::time::Duration::from_secs(60),
    }))
}

/// A seam opened over a log that already holds the given batches, one member list
/// each. The store stays up until [`Opened::finish`].
struct Opened {
    _scratch: Scratch,
    server: Dolt,
    writer: Writer,
    pool: sql::Pool,
    seam: Arc<Seam>,
}

impl Opened {
    async fn finish(mut self) {
        self.writer.drain().await;
        self.server.stop().await;
    }
}

async fn open_over(batches: Vec<Vec<Vec<u8>>>) -> Result<Opened, LedgerError> {
    let scratch = Scratch::new("seam-kind-set");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    // **The batches go in through a writer of their own.** Opening a writer's log
    // opens its ledger seam as a side effect, so the writer that opens the seam
    // below must not have touched the log before the batches are in it.
    let loader = Writer::default();
    {
        let log: Arc<dyn LogStore> = Arc::new(
            WriterLog::open(store.clone(), loader.clone())
                .await
                .unwrap(),
        );
        let mut head = 0_u64;
        for (n, members) in batches.into_iter().enumerate() {
            let count = members.len() as u64;
            let batch = SealedBatch::seal(
                BatchId(format!("preset-{n}")),
                Position(head),
                jiff::Timestamp::now(),
                1,
                members,
            )
            .unwrap();
            log.append(Position(head), &batch).await.unwrap();
            head += count;
        }
    }
    loader.drain().await;
    drop(loader);
    let writer = Writer::default();
    let pool = sql::Pool::from(store.clone());
    let opened = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
    };
    match opened {
        Ok(seam) => Ok(Opened {
            _scratch: scratch,
            server,
            writer,
            pool,
            seam,
        }),
        Err(error) => {
            writer.drain().await;
            server.stop().await;
            Err(error)
        }
    }
}

/// **A declaration of kinds the process holds opens, and its claims state holds the
/// declaration.** The control for the cases below: the same log with held kinds is
/// not behind.
#[tokio::test]
async fn a_declaration_of_held_kinds_opens_with_the_claims_state_current() {
    let _kinds = KINDS.lock().await;
    let opened = open_over(vec![vec![declaration_member()]])
        .await
        .expect("a declaration of kinds the process holds opens");
    assert!(!opened.seam.claims_behind());
    assert!(
        opened
            .seam
            .read_claims(|claims| claims.declared(&ObjectId("k5".into())).is_some()),
        "the declaration is in the claims state"
    );
    opened.finish().await;
}

/// **A claims member the seam cannot read leaves the ledger serving.** The log holds
/// a request that spent its id and then a declaration of kinds no set holds. The
/// open succeeds, the ledger answers the request, and the claims state is behind
/// the log and says so; it is not served as if it were whole.
#[tokio::test]
async fn a_claims_member_the_seam_cannot_read_leaves_the_ledger_serving() {
    let _kinds = KINDS.lock().await;
    let opened = open_over(vec![
        vec![commit_member()],
        vec![declaration_naming("lemur|stoat")],
    ])
    .await
    .expect("an unreadable claims member does not fail the ledger's open");
    let answer = opened.seam.read(|state| {
        state
            .lookup(&actor(), &request_id(), jiff::Timestamp::now())
            .cloned()
    });
    assert!(
        answer.is_some(),
        "the request that spent its id is still found"
    );
    assert!(
        opened.seam.claims_behind(),
        "the claims state is behind the log"
    );
    assert!(
        opened
            .seam
            .read_claims(|claims| claims.declared(&ObjectId("k5".into())).is_none()),
        "the declaration it could not read is not in the claims state"
    );
    opened.finish().await;
}

/// **The claims state catches up once the kinds are held.** The seam opens with a
/// declaration of two kinds no set holds, the kinds load afterwards, and settling
/// the claims reads the member again from where the claims state stopped: it is no
/// longer behind and holds the declaration. Settling before the kinds load says
/// the claims are still unreadable and changes nothing.
#[tokio::test]
async fn the_claims_state_catches_up_when_the_kinds_load_after_the_open() {
    let _kinds = KINDS.lock().await;
    let opened = open_over(vec![
        vec![commit_member()],
        vec![declaration_naming("otter|badger")],
    ])
    .await
    .expect("the open does not fail");
    assert!(opened.seam.claims_behind());
    let refused = opened.seam.settle_claims().await;
    assert!(
        matches!(refused, Err(LedgerError::Store(_))),
        "the kinds are still not held: {refused:?}"
    );
    assert!(opened.seam.claims_behind());

    // **The kind set is the process's**, and a test elsewhere in the crate that loads
    // the shipped kinds replaces it, so the load and the settle are tried together a
    // few times: the claims catching up once the kinds are held is what is asked.
    let mut settled = Err(LedgerError::Store("never tried".into()));
    for _ in 0..20 {
        let mut held = jojobot_domain::memory::kinds::all();
        held.extend(["otter".to_string(), "badger".to_string()]);
        jojobot_domain::memory::kinds::load(held);
        settled = opened.seam.settle_claims().await;
        if settled.is_ok() {
            break;
        }
    }
    jojobot_domain::memory::kinds::load_shipped();
    settled.expect("with the kinds held the claims catch up");
    assert!(!opened.seam.claims_behind());
    assert!(
        opened
            .seam
            .read_claims(|claims| claims.declared(&ObjectId("k5".into())).is_some()),
        "the declaration is in the claims state now"
    );
    opened.finish().await;
}

/// **While the claims are behind, a write of the ledgers alone still lands and a
/// write that carries claims is refused.** The ledger is what every id-spending
/// write needs; a claims event prepared against a state that is behind the log
/// would be built on what the log has moved past.
#[tokio::test]
async fn while_the_claims_are_behind_ledger_writes_land_and_claims_writes_are_refused() {
    let _kinds = KINDS.lock().await;
    let opened = open_over(vec![vec![declaration_naming("marten|ferret")]])
        .await
        .expect("the open does not fail");
    assert!(opened.seam.claims_behind());

    let landed = opened
        .seam
        .append_event_batch(
            &opened.pool,
            vec![LedgerEvent::RequestCommitted(Commit {
                request: Request {
                    key: RequestKey {
                        actor: actor(),
                        id: RequestId("beta".into()),
                        generation: 1,
                    },
                    verb: "capture".into(),
                    digest: "digest-beta".into(),
                },
                landed: "{}".into(),
                receipt: None,
                written_at: jiff::Timestamp::now(),
                expires_at: jiff::Timestamp::now() + std::time::Duration::from_secs(60),
            })],
            jiff::Timestamp::now(),
        )
        .await;
    assert!(landed.is_ok(), "a ledger write lands: {landed:?}");

    let mut tx = opened.pool.writer().begin().await.expect("a transaction");
    let refused = opened
        .seam
        .stage_with(
            &mut tx,
            Vec::new(),
            vec![declaration_event()],
            jiff::Timestamp::now(),
        )
        .await;
    assert!(
        matches!(refused, Err(LedgerError::Store(_))),
        "a claims write is refused while the claims are behind: {:?}",
        refused.err()
    );
    let _ = tx.rollback().await;
    opened.finish().await;
}
