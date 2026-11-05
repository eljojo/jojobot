//! The seam replays lazily, at the first writer job, and a declaration whose field
//! points at kinds is read back through the kinds the process has loaded. These
//! cases pin what the open does with it: it opens when the kinds are held and
//! fails, as an unreadable log, when one is not.

use std::sync::Arc;

use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::log::{BatchId, Event, LogStore, ObjectId, Position, RunId, SealedBatch};
use jojobot_domain::memory::EntityKind;
use jojobot_domain::memory::types::{DeclaredType, Field, Fold, Kinds, Origin, ValueType};

use crate::dolt::Dolt;
use crate::ledger_log::LedgerError;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};
use crate::writer_log::WriterLog;

/// The stored member of a declaration whose field points at `person` and `bot`.
fn declaration_member() -> Vec<u8> {
    jojobot_domain::memory::kinds::load_shipped();
    let event = Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(jiff::Timestamp::from_second(1_700_000_000).expect("a moment")),
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
    };
    crate::claims_log::codec::encode(&event)
}

/// What opening the seam says over a log whose one batch holds `member`.
async fn open_over(member: Vec<u8>) -> Result<(), LedgerError> {
    let scratch = Scratch::new("seam-kind-set");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    {
        let log: Arc<dyn LogStore> = Arc::new(
            WriterLog::open(store.clone(), writer.clone())
                .await
                .unwrap(),
        );
        let batch = SealedBatch::seal(
            BatchId("declared".into()),
            Position(0),
            jiff::Timestamp::now(),
            1,
            vec![member],
        )
        .unwrap();
        log.append(Position(0), &batch).await.unwrap();
    }
    let pool = sql::Pool::from(store.clone());
    let opened = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await.map(|_| ()) })
            .await
    };
    writer.drain().await;
    server.stop().await;
    opened
}

/// **The kinds a declaration names must be held when the seam opens.** A
/// declaration of `person` and `bot` opens when the shipped kinds are loaded; the
/// same declaration with a kind no set holds fails the open as an unreadable log,
/// and does not open to a claims state that lacks it.
#[tokio::test]
async fn a_declaration_naming_a_kind_nobody_holds_fails_the_open() {
    assert!(
        open_over(declaration_member()).await.is_ok(),
        "a declaration of kinds the process holds opens"
    );

    let stored = String::from_utf8(declaration_member()).expect("json is text");
    assert!(stored.contains("person|bot"), "{stored}");
    let unknown = stored.replace("person|bot", "walrus|sloth").into_bytes();
    let opened = open_over(unknown).await;
    assert!(
        matches!(opened, Err(LedgerError::Store(_))),
        "a declaration of a kind no set holds does not open: {opened:?}"
    );
}
