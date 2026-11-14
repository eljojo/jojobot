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
    /// **The position of the last batch whose claims members `state.claims` applied.** It
    /// follows `position` while every claims member reads. A claims member this build cannot
    /// read (a declaration naming a kind the set does not hold yet) stops it, and
    /// `claims_unreadable` says why: the ledgers and the teaching go on, the claims State
    /// stays at the last batch it could read whole.
    pub claims_applied: Position,
    /// Why the claims State stopped behind the log, while it is behind.
    pub claims_unreadable: Option<String>,
    /// **The replay met the batch of the claims import**, by the id its batch carries. A State
    /// restored from a snapshot says the same through its `ImportCompleted` event, so the seam
    /// reads both.
    pub claims_import_batch: bool,
    /// What the boot did, for its line and for a case that reads it.
    pub report: BootReport,
}

impl Boot {
    /// The boot of a log with no history: an empty State at position zero.
    fn empty() -> Self {
        Boot {
            state: State::default(),
            position: Position(0),
            batch_checksum: [0; 32],
            claims_applied: Position(0),
            claims_unreadable: None,
            claims_import_batch: false,
            report: BootReport::default(),
        }
    }
}

/// **What one boot did**: whether it started from a snapshot and how far the snapshot
/// reached, what loading it cost, and the batches replayed after it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BootReport {
    pub from_snapshot: bool,
    pub restored_at: u64,
    /// The bytes of the snapshot the boot read, and the time its State took to decode.
    pub snapshot_bytes: u64,
    pub decode_ms: u64,
    pub tail_batches: u64,
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

/// **A boot that did not use a snapshot is counted once, and says so.** Every reason
/// is a warning, the absence of a snapshot included, with one exception: a log that
/// holds no history. There the boot had nothing a snapshot could have covered, so no
/// snapshot is the ordinary state of a first boot and says nothing about a
/// pre-build that failed. A log with history and no snapshot is the case the
/// warning is for: a deploy whose pre-build failed lands there, and must not land
/// quietly.
fn skipped(skip: Skip, nothing_to_replay: bool) {
    SKIPS[skip.index()].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if skip == Skip::NoneHeld && nothing_to_replay {
        tracing::info!(
            reason = skip.name(),
            "no snapshot held and no history to replay; the boot starts from an empty log"
        );
    } else {
        tracing::warn!(
            reason = skip.name(),
            "SNAPSHOT NOT USED — the boot replays the whole log, not a fast boot"
        );
    }
}

/// **Whether `snapshot` is of the shape this build reads.** The one place the
/// rule is written: the boot refuses a snapshot that fails it, and the keeper
/// counts no coverage from one.
pub(crate) fn is_this_builds(snapshot: &ValidatedSnapshot) -> bool {
    snapshot.projection_version == PROJECTION_FINGERPRINT
}

/// **The reason `snapshot` cannot be used against `validity`**, or none when it
/// can. The checks run in a fixed order, so one defect has one name.
fn refusal(snapshot: &ValidatedSnapshot, validity: &Validity) -> Option<Skip> {
    if snapshot.snapshot_format_version != FORMAT {
        return Some(Skip::Format);
    }
    if !is_this_builds(snapshot) {
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
    let decoding = std::time::Instant::now();
    let Ok(state) = serde_json::from_slice::<State>(&snapshot.blob) else {
        return Ok(Err(Skip::Unreadable));
    };
    // **The one door a loaded State comes through**: what a family leaves out of its
    // serialized form is made again here.
    let state = state.restored();
    Ok(Ok(Boot {
        state,
        position: snapshot.covered_position,
        batch_checksum: snapshot.batch_checksum,
        claims_applied: snapshot.covered_position,
        claims_unreadable: None,
        claims_import_batch: false,
        report: BootReport {
            from_snapshot: true,
            restored_at: snapshot.covered_position.0,
            snapshot_bytes: snapshot.blob.len() as u64,
            decode_ms: decoding.elapsed().as_millis() as u64,
            tail_batches: 0,
        },
    }))
}

/// **Apply one batch to the boot's State.** A teaching or ledger member that cannot be read
/// fails the boot. A claims member that cannot be read does not: the claims are applied from
/// the batch whole or not at all, and the first batch with one the build cannot read stops them
/// there and says why, as the ledger seam always has, so the ledgers and the teaching go on.
fn apply(boot: &mut Boot, batch: &SealedBatch) -> Result<(), LogError> {
    if batch
        .header()
        .batch_id
        .0
        .starts_with(crate::ledger_seam::CLAIMS_IMPORT_PREFIX)
    {
        boot.claims_import_batch = true;
    }
    let mut claims = Vec::new();
    let mut unreadable_claim = None;
    for bytes in batch.members() {
        let member = if let Some(event) = teaching::decode_own(bytes).map_err(unreadable)? {
            Member::Teaching(event)
        } else if let Some(event) = ledger::decode_own(bytes).map_err(unreadable)? {
            Member::Ledger(event)
        } else {
            match crate::claims_log::codec::decode_own(bytes) {
                Ok(Some(claim)) => claims.push(claim),
                Ok(None) => {}
                Err(error) => {
                    unreadable_claim.get_or_insert_with(|| error.to_string());
                }
            }
            continue;
        };
        boot.state.apply(&member).map_err(unreadable)?;
    }
    if boot.claims_unreadable.is_none() {
        match unreadable_claim {
            None => {
                for claim in &claims {
                    crate::claims_log::apply(&mut boot.state.claims, claim);
                }
                boot.claims_applied = batch.range().last;
            }
            Some(why) => {
                boot.claims_unreadable = Some(format!(
                    "the batch ending at {}: {why}",
                    batch.range().last.0
                ));
            }
        }
    }
    Ok(())
}

/// Restore a compatible whole State, or replay from zero when it is disposable.
/// **A boot that cannot use the snapshot the log holds says so and is counted.**
pub async fn boot(log: &dyn LogStore) -> Result<Boot, LogError> {
    replay(log, true).await
}

/// The replay a boot makes, and the one a snapshot is taken from. **Only a boot
/// reports a skipped snapshot**: taking a snapshot reads the State the same way and
/// is not a boot, so it must not warn that the boot replayed the log, and the boots
/// the scrape counts are the boots the process made.
async fn replay(log: &dyn LogStore, report_skip: bool) -> Result<Boot, LogError> {
    let began = std::time::Instant::now();
    let validity = log.validity().await?;
    // The skip is reported once the replay is done: whether the log held any history
    // is known only then.
    let mut skip_to_report = None;
    let (mut boot, from_snapshot) = match restored(log, &validity).await? {
        Ok(boot) => (boot, true),
        Err(skip) => {
            skip_to_report = report_skip.then_some(skip);
            (Boot::empty(), false)
        }
    };
    let (restored_at, loaded_ms) = (boot.position, began.elapsed().as_millis() as u64);
    let mut tail_batches = 0u64;
    loop {
        let batches = log.read_from(boot.position, READ_BOUND).await?;
        if batches.is_empty() {
            if let Some(skip) = skip_to_report {
                skipped(skip, boot.position == Position(0));
            }
            boot.report.tail_batches = tail_batches;
            tracing::info!(
                from_snapshot,
                restored_at = restored_at.0,
                snapshot_load_ms = loaded_ms,
                snapshot_bytes = boot.report.snapshot_bytes,
                snapshot_decode_ms = boot.report.decode_ms,
                tail_batches,
                head = boot.position.0,
                total_ms = began.elapsed().as_millis() as u64,
                "event log boot"
            );
            return Ok(boot);
        }
        for batch in batches {
            apply(&mut boot, &batch)?;
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
    let boot = replay(log, false).await?;
    // **A snapshot never leaves a claims member out.** A State whose claims stopped at a member
    // this build cannot read would be published as the State of the whole log, and a boot that
    // restored it would serve claims the log has moved past. Nothing is published; the boot of
    // such a log replays from the last snapshot it can use, as it does without this change.
    if let Some(why) = &boot.claims_unreadable {
        tracing::warn!(
            reason = %why,
            "NO SNAPSHOT TAKEN — a claims member of the log cannot be read by this build"
        );
        return Err(LogError::Store(format!(
            "no snapshot is taken while a claims member is unreadable: {why}"
        )));
    }
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
