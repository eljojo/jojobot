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

/// **Why a boot did not use the snapshot the log holds.** A skip is never silent:
/// each one is counted, logged with its name, and the boot replays from zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// The log holds no snapshot.
    NoneHeld,
    /// The snapshot's format is not the one this build reads.
    Format,
    /// The shape of the State changed: the build reads a different projection.
    Fingerprint,
    /// The snapshot is of another log.
    Identity,
    /// The log's representation epoch moved on.
    Epoch,
    /// An erasure reaches the snapshot's covered positions.
    Erasure,
    /// The stored length is not the blob's length.
    Length,
    /// The blob does not match its checksum.
    Checksum,
    /// The snapshot is not tied to a completed batch of the log.
    Boundary,
    /// The blob does not read as this build's State.
    Unreadable,
}

impl Skip {
    pub const ALL: [Skip; 10] = [
        Skip::NoneHeld,
        Skip::Format,
        Skip::Fingerprint,
        Skip::Identity,
        Skip::Epoch,
        Skip::Erasure,
        Skip::Length,
        Skip::Checksum,
        Skip::Boundary,
        Skip::Unreadable,
    ];

    /// The name the boot line and the metric carry.
    pub const fn name(self) -> &'static str {
        match self {
            Skip::NoneHeld => "none_held",
            Skip::Format => "format",
            Skip::Fingerprint => "fingerprint",
            Skip::Identity => "identity",
            Skip::Epoch => "epoch",
            Skip::Erasure => "erasure",
            Skip::Length => "length",
            Skip::Checksum => "checksum",
            Skip::Boundary => "boundary",
            Skip::Unreadable => "unreadable",
        }
    }

    /// **Where this reason sits in [`Skip::ALL`] and in the counters.** An
    /// exhaustive match, so a variant added to the enum has to be placed here
    /// before the crate builds, and a place past the end of `ALL` fails the count
    /// instead of passing unseen.
    const fn index(self) -> usize {
        match self {
            Skip::NoneHeld => 0,
            Skip::Format => 1,
            Skip::Fingerprint => 2,
            Skip::Identity => 3,
            Skip::Epoch => 4,
            Skip::Erasure => 5,
            Skip::Length => 6,
            Skip::Checksum => 7,
            Skip::Boundary => 8,
            Skip::Unreadable => 9,
        }
    }
}

static SKIPS: [std::sync::atomic::AtomicU64; Skip::ALL.len()] =
    [const { std::sync::atomic::AtomicU64::new(0) }; Skip::ALL.len()];

/// How many boots of this process skipped a snapshot, by reason.
pub fn skip_counts() -> Vec<(&'static str, u64)> {
    Skip::ALL
        .iter()
        .zip(&SKIPS)
        .map(|(skip, count)| {
            (
                skip.name(),
                count.load(std::sync::atomic::Ordering::Relaxed),
            )
        })
        .collect()
}

fn skipped(skip: Skip) {
    SKIPS[skip.index()].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if skip == Skip::NoneHeld {
        tracing::info!(
            reason = skip.name(),
            "no snapshot held; the boot replays the log"
        );
    } else {
        tracing::warn!(
            reason = skip.name(),
            "SNAPSHOT NOT USED — the boot replays the whole log, not a fast boot"
        );
    }
}

/// **The reason `snapshot` cannot be used against `validity`**, or none when it
/// can. The checks run in a fixed order, so one defect has one name.
fn refusal(snapshot: &ValidatedSnapshot, validity: &Validity) -> Option<Skip> {
    if snapshot.snapshot_format_version != FORMAT {
        return Some(Skip::Format);
    }
    if snapshot.projection_version != PROJECTION_FINGERPRINT {
        return Some(Skip::Fingerprint);
    }
    if snapshot.log_identity != validity.log_identity {
        return Some(Skip::Identity);
    }
    if snapshot.representation_epoch != validity.representation_epoch {
        return Some(Skip::Epoch);
    }
    let erased_after = snapshot.erasure_epoch < validity.erasure_epoch
        && validity
            .earliest_affected_position
            .is_some_and(|first| snapshot.covered_position < first);
    if snapshot.erasure_epoch != validity.erasure_epoch && !erased_after {
        return Some(Skip::Erasure);
    }
    if snapshot.serialized_length != snapshot.blob.len() as u64 {
        return Some(Skip::Length);
    }
    if snapshot.checksum != <[u8; 32]>::from(Sha256::digest(&snapshot.blob)) {
        return Some(Skip::Checksum);
    }
    None
}

async fn restored(log: &dyn LogStore, validity: &Validity) -> Result<Result<Boot, Skip>, LogError> {
    let Some(snapshot) = log.load_snapshot().await? else {
        return Ok(Err(Skip::NoneHeld));
    };
    if let Some(skip) = refusal(&snapshot, validity) {
        return Ok(Err(skip));
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
            return Ok(Err(Skip::Boundary));
        }
    }
    let Ok(state) = serde_json::from_slice(&snapshot.blob) else {
        return Ok(Err(Skip::Unreadable));
    };
    Ok(Ok(Boot {
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
    let began = std::time::Instant::now();
    let validity = log.validity().await?;
    let (mut boot, from_snapshot) = match restored(log, &validity).await? {
        Ok(boot) => (boot, true),
        Err(skip) => {
            skipped(skip);
            (
                Boot {
                    state: State::default(),
                    position: Position(0),
                    batch_checksum: [0; 32],
                },
                false,
            )
        }
    };
    let (restored_at, loaded_ms) = (boot.position, began.elapsed().as_millis() as u64);
    let mut tail_batches = 0u64;
    loop {
        let batches = log.read_from(boot.position, READ_BOUND).await?;
        if batches.is_empty() {
            tracing::info!(
                from_snapshot,
                restored_at = restored_at.0,
                snapshot_load_ms = loaded_ms,
                tail_batches,
                head = boot.position.0,
                total_ms = began.elapsed().as_millis() as u64,
                "event log boot"
            );
            return Ok(boot);
        }
        for batch in batches {
            apply(&mut boot.state, &batch)?;
            boot.position = batch.range().last;
            boot.batch_checksum = batch.header().checksum;
            tail_batches += 1;
        }
    }
}

/// Capture every family at one completed position; later appends stay in the tail.
/// Publication checks the generation again atomically in the backend. Returns the
/// position the snapshot covers.
pub async fn take_snapshot(log: &dyn LogStore, at: jiff::Timestamp) -> Result<Position, LogError> {
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
    log.store_snapshot(&snapshot).await?;
    Ok(snapshot.covered_position)
}

#[cfg(test)]
mod shape;
#[cfg(test)]
mod tests;
