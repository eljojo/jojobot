//! One batch per transaction: the ledger and claims events of a write are sealed
//! into one batch, commit or roll back as one, and are applied to the seam's two
//! states together.

use jojobot_domain::log::Position;

use super::fixtures::{actor, at, claims_events, homer, request_id, spent};
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, imported_database};
use crate::writer::{Writer, sql};

/// **A write's ledger and claims events are one batch**, at consecutive
/// positions, and the seam applies both of them when the commit is acknowledged.
/// A second seam over the same store replays both from the log.
#[tokio::test]
async fn a_batch_carries_both_families_and_the_seam_applies_both() {
    let scratch = Scratch::new("seam-two-families");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
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
                    .stage_with(&mut tx, vec![spent()], claims_events(), at(1))
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
    // The first batch is the import the database was opened with.
    assert_eq!(batches.len(), 2, "one batch for one transaction");
    assert_eq!(
        batches[1].members().len(),
        3,
        "a ledger member and two claims"
    );
    assert_eq!(batches[1].range().last, Position(4));
    assert!(
        seam.read(|state| state.lookup(&actor(), &request_id(), at(1)).is_some()),
        "the spent id answers from the ledgers"
    );
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        Some(homer()),
        "the address answers from the claims"
    );

    // The first writer holds the store's connection, so it is let go before a
    // restarted process's writer opens a seam over the same log.
    let applied = seam.read_claims(Clone::clone);
    drop(seam);
    writer.drain().await;
    drop(writer);

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
        applied,
        "the replay builds the claims the commit applied"
    );
    assert!(reopened.read(|state| state.lookup(&actor(), &request_id(), at(1)).is_some()));
    second.drain().await;
    server.stop().await;
}

/// **A transaction that rolls back leaves neither family's event**: the log holds
/// nothing and neither state moved.
#[tokio::test]
async fn a_rolled_back_transaction_leaves_neither_family() {
    let scratch = Scratch::new("seam-rolled-back");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
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
                    .stage_with(&mut tx, vec![spent()], claims_events(), at(1))
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
            .read_from(Position(1), 10)
            .await
            .unwrap()
            .is_empty(),
        "the rolled-back batch is not in the log"
    );
    assert_eq!(seam.read_claims(Clone::clone), before);
    assert!(!seam.read(|state| state.lookup(&actor(), &request_id(), at(1)).is_some()));
    writer.drain().await;
    server.stop().await;
}
