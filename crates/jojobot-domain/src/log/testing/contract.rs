//! Backend-neutral witnesses. Each receives an empty log from its adapter's harness.

use super::*;

pub fn batch(id: &str, head: u64, members: &[&[u8]]) -> SealedBatch {
    SealedBatch::seal(
        BatchId(id.into()),
        Position(head),
        Timestamp::UNIX_EPOCH,
        1,
        members.iter().map(|s| s.to_vec()).collect(),
    )
    .unwrap()
}

pub async fn ordered_complete_batches(log: &impl LogStore) {
    let first = batch("alpha", 0, &[b"null", b"\"\"", b"[]"]);
    let second = batch("beta", 3, &[b"{}"]);
    assert_eq!(
        log.append(Position(0), &first).await.unwrap(),
        Append::Committed(first.range())
    );
    assert_eq!(
        log.append(Position(3), &second).await.unwrap(),
        Append::Committed(second.range())
    );
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap(),
        [first.clone(), second.clone()]
    );
    assert_eq!(
        log.read_from(Position(1), 1).await.unwrap().as_slice(),
        std::slice::from_ref(&first)
    );
    assert_eq!(
        log.read_from(Position(2), 10).await.unwrap(),
        [first, second.clone()]
    );
    assert_eq!(log.read_from(Position(3), 10).await.unwrap(), [second]);
    assert!(log.read_from(Position(4), 10).await.unwrap().is_empty());
    assert!(log.read_from(Position(0), 0).await.unwrap().is_empty());
}

pub async fn retries_keep_identity_and_bytes(log: &impl LogStore) {
    let first = batch("alpha", 0, &[b"null", b""]);
    assert_eq!(
        log.append(Position(0), &first).await.unwrap(),
        Append::Committed(first.range())
    );
    assert_eq!(
        log.append(Position(0), &first).await.unwrap(),
        Append::Committed(first.range())
    );
    let changed = batch("alpha", 0, &[b"", b""]);
    assert_eq!(
        log.append(Position(0), &changed).await,
        Err(LogError::Conflict)
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [first]);
}

pub async fn stale_heads_never_append(log: &impl LogStore) {
    let first = batch("alpha", 0, &[b"a"]);
    assert_eq!(
        log.append(Position(0), &first).await.unwrap(),
        Append::Committed(first.range())
    );
    assert_eq!(
        log.append(Position(0), &batch("beta", 0, &[b"b"])).await,
        Err(LogError::Conflict)
    );
    assert_eq!(
        log.append(Position(2), &batch("gamma", 1, &[b"c"])).await,
        Err(LogError::Conflict)
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [first]);
}

pub async fn partial_batch_is_invisible(log: &impl FaultLog) {
    log.inject(Fault::BetweenMembers).await;
    let next = batch("alpha", 0, &[b"null", b"", b"[]"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::DefinitelyNotCommitted
    );
    assert!(log.read_from(Position(0), 10).await.unwrap().is_empty());
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

pub async fn lost_acknowledgement_is_recovered_once(log: &impl FaultLog) {
    log.inject(Fault::LostAcknowledgement).await;
    let next = batch("alpha", 0, &[b"null", b""]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap().as_slice(),
        std::slice::from_ref(&next)
    );
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

pub async fn absence_without_a_terminal_fence_is_uncertain(log: &impl FaultLog) {
    log.inject(Fault::DelayedCommit).await;
    let next = batch("alpha", 0, &[b"a", b"b"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    assert_eq!(
        log.read_from(Position(0), 10).await,
        Err(LogError::Indeterminate)
    );
    assert_eq!(
        log.append(Position(0), &batch("beta", 0, &[b"c"]))
            .await
            .unwrap(),
        Append::Indeterminate
    );
    log.finish_delayed(true).await;
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

pub async fn a_fenced_abort_leaves_the_same_head(log: &impl FaultLog) {
    let next = batch("alpha", 0, &[b"a", b"b"]);
    log.inject(Fault::DelayedCommit).await;
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Indeterminate
    );
    log.finish_delayed(false).await;
    assert!(log.read_from(Position(0), 10).await.unwrap().is_empty());
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
}

pub async fn restart_excludes_a_late_predecessor(log: &impl FaultLog) {
    log.inject(Fault::DelayedCommit).await;
    let old = batch("alpha", 0, &[b"old"]);
    assert_eq!(
        log.append(Position(0), &old).await.unwrap(),
        Append::Indeterminate
    );
    log.restart().await;
    let next = batch("beta", 0, &[b"new"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    log.finish_delayed(true).await;
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

pub async fn cancelling_an_acknowledgement_does_not_cancel_the_commit(log: &impl FaultLog) {
    log.inject(Fault::CancelledAcknowledgement).await;
    let next = batch("alpha", 0, &[b"null", b""]);
    let mut append = Box::pin(log.append(Position(0), &next));
    tokio::select! {
        result = &mut append => panic!("the acknowledgement was not held: {result:?}"),
        () = log.acknowledgement_pending() => {}
    }
    drop(append);
    assert_eq!(
        log.read_from(Position(0), 10).await.unwrap().as_slice(),
        std::slice::from_ref(&next)
    );
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

pub async fn snapshot_blob_round_trips(log: &impl LogStore) {
    assert_eq!(log.load_snapshot().await.unwrap(), None);
    let next = batch("alpha", 0, &[b"a"]);
    assert_eq!(
        log.append(Position(0), &next).await.unwrap(),
        Append::Committed(next.range())
    );
    let blob = vec![0, 255, 0];
    let snapshot = ValidatedSnapshot {
        snapshot_format_version: 1,
        projection_version: 1,
        log_identity: "alpha".into(),
        representation_epoch: 0,
        erasure_epoch: 0,
        covered_position: Position(1),
        batch_checksum: next.header().checksum,
        created_at: Timestamp::UNIX_EPOCH,
        checksum: Sha256::digest(&blob).into(),
        serialized_length: blob.len() as u64,
        blob,
    };
    log.store_snapshot(&snapshot).await.unwrap();
    assert_eq!(log.load_snapshot().await.unwrap(), Some(snapshot));
    assert_eq!(log.read_from(Position(0), 10).await.unwrap(), [next]);
}

/// **An event keeps the actor it was appended with.** The log keeps the bytes of a
/// member, so the actor must survive the encoding a family gives the event.
#[cfg(test)]
pub async fn an_events_actor_reads_back_as_it_was_appended(log: &impl LogStore) {
    use crate::actor::Actor;
    let actors = [
        Actor::Bot(crate::memory::EntityId("bot:lambda".into())),
        Actor::App(crate::app::AppId::parse("springfield-watch").unwrap()),
        Actor::Operator,
        Actor::System("sweep".into()),
    ];
    let encoded: Vec<Vec<u8>> = actors
        .iter()
        .map(|actor| {
            serde_json::to_vec(&Event {
                event_kind: "probe".into(),
                meaning_version: 1,
                actor: Some(actor.clone()),
                run: None,
                session_card: None,
                request: None,
                part: None,
                occurred_at: None,
                written_at: None,
                imported_source_time: None,
                payload: serde_json::Value::Null,
            })
            .unwrap()
        })
        .collect();
    let members: Vec<&[u8]> = encoded.iter().map(Vec::as_slice).collect();
    let sealed = batch("alpha", 0, &members);
    assert_eq!(
        log.append(Position(0), &sealed).await.unwrap(),
        Append::Committed(sealed.range())
    );
    let read = log.read_from(Position(0), 10).await.unwrap();
    let read_back: Vec<Option<Actor>> = read
        .iter()
        .flat_map(SealedBatch::members)
        .map(|member| {
            serde_json::from_slice::<Event<serde_json::Value>>(member)
                .unwrap()
                .actor
        })
        .collect();
    assert_eq!(read_back, actors.map(Some));
}

#[cfg(test)]
mod log_store_contract {
    use super::*;
    macro_rules! case {
        ($($name:ident),+ $(,)?) => {$(
            #[tokio::test]
            async fn $name() { super::$name(&InMemoryLog::default()).await; }
        )+};
    }
    case!(
        ordered_complete_batches,
        retries_keep_identity_and_bytes,
        stale_heads_never_append,
        partial_batch_is_invisible,
        lost_acknowledgement_is_recovered_once,
        absence_without_a_terminal_fence_is_uncertain,
        a_fenced_abort_leaves_the_same_head,
        restart_excludes_a_late_predecessor,
        cancelling_an_acknowledgement_does_not_cancel_the_commit,
        snapshot_blob_round_trips,
        an_events_actor_reads_back_as_it_was_appended
    );
}
