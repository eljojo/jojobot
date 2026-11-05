//! One snapshot of every log-owned projection, followed by complete tail batches.
//! The family interpreters and codecs are the same ones ordinary writes use.

use jojobot_domain::log::projection::{Member, State};
use jojobot_domain::log::{LogError, LogStore, Position, SealedBatch, ValidatedSnapshot, Validity};
use sha2::{Digest, Sha256};

use crate::{ledger_log::codec as ledger, teaching_log::codec as teaching};

include!(concat!(env!("OUT_DIR"), "/snapshot_shape.rs"));

const FORMAT: u32 = 1;
const READ_BOUND: usize = 256;

pub struct Boot {
    pub state: State,
    pub position: Position,
    batch_checksum: [u8; 32],
}

fn unreadable(error: impl std::fmt::Display) -> LogError {
    LogError::Store(error.to_string())
}

fn compatible(snapshot: &ValidatedSnapshot, validity: &Validity) -> bool {
    snapshot.snapshot_format_version == FORMAT
        && snapshot.projection_version == PROJECTION_FINGERPRINT
        && snapshot.log_identity == validity.log_identity
        && snapshot.representation_epoch == validity.representation_epoch
        && (snapshot.erasure_epoch == validity.erasure_epoch
            || (snapshot.erasure_epoch < validity.erasure_epoch
                && validity
                    .earliest_affected_position
                    .is_some_and(|first| snapshot.covered_position < first)))
        && snapshot.serialized_length == snapshot.blob.len() as u64
        && snapshot.checksum == <[u8; 32]>::from(Sha256::digest(&snapshot.blob))
}

async fn restored(log: &dyn LogStore, validity: &Validity) -> Result<Option<Boot>, LogError> {
    let Some(snapshot) = log.load_snapshot().await? else {
        return Ok(None);
    };
    if !compatible(&snapshot, validity) {
        return Ok(None);
    }
    // One bounded header read ties the snapshot to a completed batch. It does
    // not replay the prefix, even when that batch has many members.
    if snapshot.covered_position != Position(0) {
        let boundary = log
            .read_from(Position(snapshot.covered_position.0 - 1), 1)
            .await?;
        if !boundary.first().is_some_and(|batch| {
            batch.range().last == snapshot.covered_position
                && batch.header().checksum == snapshot.batch_checksum
        }) {
            return Ok(None);
        }
    }
    let Ok(state) = serde_json::from_slice(&snapshot.blob) else {
        return Ok(None);
    };
    Ok(Some(Boot {
        state,
        position: snapshot.covered_position,
        batch_checksum: snapshot.batch_checksum,
    }))
}

fn apply(state: &mut State, batch: &SealedBatch) -> Result<(), LogError> {
    for bytes in batch.members() {
        let member = if let Some(event) = teaching::decode_own(bytes).map_err(unreadable)? {
            Member::Teaching(event)
        } else if let Some(event) = ledger::decode_own(bytes).map_err(unreadable)? {
            Member::Ledger(event)
        } else {
            // A family not activated in this build has no snapshot member.
            continue;
        };
        state.apply(&member).map_err(unreadable)?;
    }
    Ok(())
}

/// Restore a compatible whole State, or replay from zero when it is disposable.
pub async fn boot(log: &dyn LogStore) -> Result<Boot, LogError> {
    let validity = log.validity().await?;
    let mut boot = restored(log, &validity).await?.unwrap_or(Boot {
        state: State::default(),
        position: Position(0),
        batch_checksum: [0; 32],
    });
    loop {
        let batches = log.read_from(boot.position, READ_BOUND).await?;
        if batches.is_empty() {
            return Ok(boot);
        }
        for batch in batches {
            apply(&mut boot.state, &batch)?;
            boot.position = batch.range().last;
            boot.batch_checksum = batch.header().checksum;
        }
    }
}

/// Capture every family at one completed position; later appends stay in the tail.
/// Publication checks the generation again atomically in the backend.
pub async fn take_snapshot(log: &dyn LogStore, at: jiff::Timestamp) -> Result<(), LogError> {
    let validity = log.validity().await?;
    let boot = boot(log).await?;
    let blob = serde_json::to_vec(&boot.state).map_err(unreadable)?;
    let snapshot = ValidatedSnapshot {
        snapshot_format_version: FORMAT,
        projection_version: PROJECTION_FINGERPRINT,
        log_identity: validity.log_identity,
        representation_epoch: validity.representation_epoch,
        erasure_epoch: validity.erasure_epoch,
        covered_position: boot.position,
        batch_checksum: boot.batch_checksum,
        created_at: at,
        checksum: Sha256::digest(&blob).into(),
        serialized_length: blob.len() as u64,
        blob,
    };
    log.store_snapshot(&snapshot).await
}

#[cfg(test)]
mod shape;
#[cfg(test)]
mod tests;
