//! One batch per transaction: the ledger and claims events of a write are sealed
//! into one batch, commit or roll back as one, and are applied to the seam's two
//! states together.

use jiff::Timestamp;
use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{Event, ObjectId, Position, RequestId, RunId};
use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};

use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn actor() -> ObjectId {
    ObjectId("bot:homer".into())
}

fn id() -> RequestId {
    RequestId("alpha".into())
}

fn spent() -> LedgerEvent {
    LedgerEvent::RequestCommitted(Commit {
        request: Request {
            key: RequestKey {
                actor: actor(),
                id: id(),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest-alpha".into(),
        },
        landed: "{\"address\":\"topic:alpha#f1\"}".into(),
        receipt: None,
        written_at: at(0),
        expires_at: at(10),
    })
}

fn claim(payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(at(0)),
        imported_source_time: None,
        payload,
    }
}

fn homer() -> ObjectId {
    ObjectId("k1".into())
}

fn claims() -> Vec<Event<ClaimEvent>> {
    vec![
        claim(ClaimEvent::ObjectCreated {
            object: homer(),
            body: ObjectBody {
                kind: KindId("person".into()),
                parent: None,
                name: "person:homer".into(),
                source: "operator".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        claim(ClaimEvent::AddressBound {
            object: homer(),
            address: "person:homer".into(),
        }),
    ]
}

/// **A write's ledger and claims events are one batch**, at consecutive
/// positions, and the seam applies both of them when the commit is acknowledged.
/// A second seam over the same store replays both from the log.
#[tokio::test]
async fn a_batch_carries_both_families_and_the_seam_applies_both() {
    let scratch = Scratch::new("seam-two-families");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };

    {
        let (seam, pool) = (seam.clone(), pool.clone());
        writer
            .run(async move {
                let mut tx = pool.writer().begin().await.unwrap();
                let pending = seam
                    .stage_with(&mut tx, vec![spent()], claims(), at(1))
                    .await
                    .unwrap();
                sql::commit(tx).await.unwrap();
                pending.committed();
            })
            .await;
    }

    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 1, "one batch for one transaction");
    assert_eq!(
        batches[0].members().len(),
        3,
        "a ledger member and two claims"
    );
    assert_eq!(batches[0].range().last, Position(3));
    assert!(
        seam.read(|state| state.lookup(&actor(), &id(), at(1)).is_some()),
        "the spent id answers from the ledgers"
    );
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        Some(homer()),
        "the address answers from the claims"
    );

    let second = Writer::default();
    let reopened = {
        let (opener, pool) = (second.clone(), pool.clone());
        second
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("a second seam replays the log")
    };
    assert_eq!(
        reopened.read_claims(Clone::clone),
        seam.read_claims(Clone::clone),
        "the replay builds the claims the commit applied"
    );
    assert!(reopened.read(|state| state.lookup(&actor(), &id(), at(1)).is_some()));
    writer.drain().await;
    second.drain().await;
    server.stop().await;
}

/// **A transaction that rolls back leaves neither family's event**: the log holds
/// nothing and neither state moved.
#[tokio::test]
async fn a_rolled_back_transaction_leaves_neither_family() {
    let scratch = Scratch::new("seam-rolled-back");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    let before = seam.read_claims(Clone::clone);

    {
        let (seam, pool) = (seam.clone(), pool.clone());
        writer
            .run(async move {
                let mut tx = pool.writer().begin().await.unwrap();
                let pending = seam
                    .stage_with(&mut tx, vec![spent()], claims(), at(1))
                    .await
                    .unwrap();
                tx.rollback().await.unwrap();
                drop(pending);
            })
            .await;
    }

    assert!(
        writer
            .shared_log(&store)
            .read_from(Position(0), 10)
            .await
            .unwrap()
            .is_empty(),
        "the rolled-back batch is not in the log"
    );
    assert_eq!(seam.read_claims(Clone::clone), before);
    assert!(!seam.read(|state| state.lookup(&actor(), &id(), at(1)).is_some()));
    writer.drain().await;
    server.stop().await;
}
