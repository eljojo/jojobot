//! The seam holds the claims state beside the ledgers', filled by the one replay.

use std::sync::Arc;

use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{Event, LogStore, ObjectId, RunId};

use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

fn event(payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
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
        payload,
    }
}

fn homer() -> ObjectId {
    ObjectId("k1".into())
}

fn claims_events() -> Vec<Event<ClaimEvent>> {
    vec![
        event(ClaimEvent::ObjectCreated {
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
        event(ClaimEvent::AddressBound {
            object: homer(),
            address: "person:homer".into(),
        }),
    ]
}

/// **One replay fills the claims state the seam holds.** The log holds a batch of
/// claims events with a member of another family on each side of it; the seam
/// opens over it and answers the address the batch bound, and its head is where
/// the batch left the log. The state is the one the claims log owner builds from
/// the same log, so the seam and the owner cannot read it two ways.
#[tokio::test]
async fn opening_the_seam_replays_the_claims_members_of_the_log() {
    use jojobot_domain::teaching::events::{RunId as TeachingRun, TeachingEvent};
    let scratch = Scratch::new("seam-claims");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();

    // Seed the log through the claims owner, then put a foreign member in a batch
    // of its own after it: the family test is by the kind of each member.
    {
        let log: Arc<dyn LogStore> = Arc::new(
            WriterLog::open(store.clone(), writer.clone())
                .await
                .unwrap(),
        );
        let owner = crate::claims_log::LoggedClaims::open(log.clone())
            .await
            .unwrap()
            .with_writer(writer.clone());
        owner
            .append(claims_events(), jiff::Timestamp::now())
            .await
            .unwrap();
        let foreign = crate::teaching_log::codec::encode(&TeachingEvent::Cleared {
            run: TeachingRun("marge".into()),
        });
        let batch = jojobot_domain::log::SealedBatch::seal(
            jojobot_domain::log::BatchId("foreign".into()),
            owner.head().await,
            jiff::Timestamp::now(),
            1,
            vec![foreign],
        )
        .unwrap();
        log.append(owner.head().await, &batch).await.unwrap();
    }
    let expected = {
        let log: Arc<dyn LogStore> = Arc::new(
            WriterLog::open(store.clone(), writer.clone())
                .await
                .unwrap(),
        );
        crate::claims_log::LoggedClaims::open(log)
            .await
            .unwrap()
            .state()
            .await
    };

    let pool = crate::writer::sql::Pool::from(store.clone());
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens over a log that holds claims events")
    };
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        Some(homer()),
        "the address the claims batch bound answers from the seam"
    );
    assert_eq!(
        seam.read_claims(Clone::clone),
        expected,
        "the seam and the claims owner read the same log to the same state"
    );
    writer.drain().await;
    server.stop().await;
}
