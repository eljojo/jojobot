//! **The stored form of a ledger event**: the log envelope with a typed payload,
//! as bytes. `meaning_version` is fixed when an event is written; an unknown kind
//! or version is an error, never skipped (design section 3). The domain keeps no
//! wire format, so the encoding lives here, in private mirrors of the domain's
//! values.

use jiff::Timestamp;
use jojobot_domain::log::{Event, ObjectId, RequestId};
use jojobot_domain::request::Recorded;
use jojobot_domain::request::events::{
    AppAttempt, AppKey, Commit, LedgerEvent, LegacyRow, Part, Request, RequestKey,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MEANING_VERSION: u32 = 1;
/// What every kind of this family starts with.
const FAMILY: &str = "Ledger";
const PART_COMMITTED: &str = "LedgerRequestPartCommitted";
const COMMITTED: &str = "LedgerRequestCommitted";
const RECEIPT_RECORDED: &str = "LedgerRequestReceiptRecorded";
const COMPLETED: &str = "LedgerRequestCompleted";
const EXPIRED: &str = "LedgerRequestExpired";
const APP_ATTEMPT: &str = "LedgerAppAttemptCommitted";
const IMPORTED: &str = "LedgerImported";

/// Why stored bytes are not a ledger event this build understands.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("a ledger event could not be read: {0}")]
    Unreadable(String),
    #[error("a ledger event has a kind or meaning this build does not know: {0} v{1}")]
    Unknown(String, u32),
}

#[derive(Serialize, Deserialize)]
struct KeyWire {
    actor: ObjectId,
    id: RequestId,
    generation: u64,
}

#[derive(Serialize, Deserialize)]
struct RequestWire {
    key: KeyWire,
    verb: String,
    digest: String,
}

#[derive(Serialize, Deserialize)]
struct CommitWire {
    request: RequestWire,
    landed: String,
    receipt: Option<String>,
    written_at: Timestamp,
    expires_at: Timestamp,
}

#[derive(Serialize, Deserialize)]
struct PartWire {
    request: RequestWire,
    name: String,
    landed: String,
    note: Option<String>,
    written_at: Timestamp,
    expires_at: Timestamp,
}

#[derive(Serialize, Deserialize)]
struct AppWire {
    app_id: String,
    event_key: String,
    digest: String,
    landed: String,
    written_at: Timestamp,
}

#[derive(Serialize, Deserialize)]
struct ReceiptWire {
    key: KeyWire,
    receipt: String,
}

#[derive(Serialize, Deserialize)]
struct CompletedWire {
    commit: CommitWire,
    required_parts: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ExpiredWire {
    key: KeyWire,
}

#[derive(Serialize, Deserialize)]
struct ImportedWire {
    row: RowWire,
    generation: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "shape")]
enum RowWire {
    RequestReceipt {
        bot_key: ObjectId,
        request_id: RequestId,
        verb: String,
        args_digest: String,
        landed: String,
        receipt: Option<String>,
        written_at: Timestamp,
    },
    RequestPart {
        bot_key: ObjectId,
        request_id: RequestId,
        part: String,
        verb: String,
        args_digest: String,
        landed: String,
        note: Option<String>,
        written_at: Timestamp,
    },
    AppWrite {
        app_id: String,
        event_key: String,
        digest: String,
        landed: String,
        written_at: Timestamp,
    },
}

impl From<&RequestKey> for KeyWire {
    fn from(key: &RequestKey) -> Self {
        Self {
            actor: key.actor.clone(),
            id: key.id.clone(),
            generation: key.generation,
        }
    }
}

impl From<KeyWire> for RequestKey {
    fn from(key: KeyWire) -> Self {
        Self {
            actor: key.actor,
            id: key.id,
            generation: key.generation,
        }
    }
}

impl From<&Request> for RequestWire {
    fn from(request: &Request) -> Self {
        Self {
            key: (&request.key).into(),
            verb: request.verb.clone(),
            digest: request.digest.clone(),
        }
    }
}

impl From<RequestWire> for Request {
    fn from(request: RequestWire) -> Self {
        Self {
            key: request.key.into(),
            verb: request.verb,
            digest: request.digest,
        }
    }
}

impl From<&Commit> for CommitWire {
    fn from(commit: &Commit) -> Self {
        Self {
            request: (&commit.request).into(),
            landed: commit.landed.clone(),
            receipt: commit.receipt.clone(),
            written_at: commit.written_at,
            expires_at: commit.expires_at,
        }
    }
}

impl From<CommitWire> for Commit {
    fn from(commit: CommitWire) -> Self {
        Self {
            request: commit.request.into(),
            landed: commit.landed,
            receipt: commit.receipt,
            written_at: commit.written_at,
            expires_at: commit.expires_at,
        }
    }
}

impl From<&LegacyRow> for RowWire {
    fn from(row: &LegacyRow) -> Self {
        match row.clone() {
            LegacyRow::RequestReceipt {
                bot_key,
                request_id,
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
            } => Self::RequestReceipt {
                bot_key,
                request_id,
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
            },
            LegacyRow::RequestPart {
                bot_key,
                request_id,
                part,
                verb,
                args_digest,
                landed,
                note,
                written_at,
            } => Self::RequestPart {
                bot_key,
                request_id,
                part,
                verb,
                args_digest,
                landed,
                note,
                written_at,
            },
            LegacyRow::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            } => Self::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            },
        }
    }
}

impl From<RowWire> for LegacyRow {
    fn from(row: RowWire) -> Self {
        match row {
            RowWire::RequestReceipt {
                bot_key,
                request_id,
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
            } => Self::RequestReceipt {
                bot_key,
                request_id,
                verb,
                args_digest,
                landed,
                receipt,
                written_at,
            },
            RowWire::RequestPart {
                bot_key,
                request_id,
                part,
                verb,
                args_digest,
                landed,
                note,
                written_at,
            } => Self::RequestPart {
                bot_key,
                request_id,
                part,
                verb,
                args_digest,
                landed,
                note,
                written_at,
            },
            RowWire::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            } => Self::AppWrite {
                app_id,
                event_key,
                digest,
                landed,
                written_at,
            },
        }
    }
}

fn to_value<T: Serialize>(wire: &T) -> Value {
    serde_json::to_value(wire).expect("a ledger event is plain data")
}

pub fn encode(event: &LedgerEvent) -> Vec<u8> {
    // The envelope names the request and the part the event is about, so a
    // reader that wants no payload can still tell what it concerns.
    let (kind, request, part, source, payload) = match event {
        LedgerEvent::RequestPartCommitted(part) => (
            PART_COMMITTED,
            Some(part.request.key.id.clone()),
            Some(part.name.clone()),
            None,
            to_value(&PartWire {
                request: (&part.request).into(),
                name: part.name.clone(),
                landed: part.recorded.landed.clone(),
                note: part.recorded.note.clone(),
                written_at: part.written_at,
                expires_at: part.expires_at,
            }),
        ),
        LedgerEvent::RequestCommitted(commit) => (
            COMMITTED,
            Some(commit.request.key.id.clone()),
            None,
            None,
            to_value(&CommitWire::from(commit)),
        ),
        LedgerEvent::RequestReceiptRecorded { key, receipt } => (
            RECEIPT_RECORDED,
            Some(key.id.clone()),
            None,
            None,
            to_value(&ReceiptWire {
                key: key.into(),
                receipt: receipt.clone(),
            }),
        ),
        LedgerEvent::RequestCompleted {
            commit,
            required_parts,
        } => (
            COMPLETED,
            Some(commit.request.key.id.clone()),
            None,
            None,
            to_value(&CompletedWire {
                commit: commit.into(),
                required_parts: required_parts.clone(),
            }),
        ),
        LedgerEvent::RequestExpired { key } => (
            EXPIRED,
            Some(key.id.clone()),
            None,
            None,
            to_value(&ExpiredWire { key: key.into() }),
        ),
        LedgerEvent::AppAttemptCommitted(attempt) => (
            APP_ATTEMPT,
            None,
            None,
            None,
            to_value(&AppWire {
                app_id: attempt.key.app_id.clone(),
                event_key: attempt.key.event_key.clone(),
                digest: attempt.digest.clone(),
                landed: attempt.landed.clone(),
                written_at: attempt.written_at,
            }),
        ),
        LedgerEvent::Imported { row, generation } => (
            IMPORTED,
            None,
            None,
            Some(imported_time(row)),
            to_value(&ImportedWire {
                row: row.into(),
                generation: *generation,
            }),
        ),
    };
    let envelope = Event {
        event_kind: kind.to_string(),
        meaning_version: MEANING_VERSION,
        actor: None,
        run: None,
        session_card: None,
        request,
        part,
        occurred_at: None,
        written_at: None,
        imported_source_time: source,
        payload,
    };
    serde_json::to_vec(&envelope).expect("a ledger event is plain data")
}

/// The time the old store gave a retained row, kept as the import's source time.
fn imported_time(row: &LegacyRow) -> Timestamp {
    match row {
        LegacyRow::RequestReceipt { written_at, .. }
        | LegacyRow::RequestPart { written_at, .. }
        | LegacyRow::AppWrite { written_at, .. } => *written_at,
    }
}

/// **The ledger's own events, out of a log that holds every family's.** The
/// family is the one the envelope's `event_kind` names: a kind that starts with
/// [`FAMILY`] is the ledger's and must decode (an unknown one is an error, never
/// skipped); any other kind belongs to another family's reader and gives `None`.
pub fn decode_own(bytes: &[u8]) -> Result<Option<LedgerEvent>, DecodeError> {
    #[derive(Deserialize)]
    struct Named {
        event_kind: String,
    }
    let named: Named =
        serde_json::from_slice(bytes).map_err(|e| DecodeError::Unreadable(e.to_string()))?;
    if !named.event_kind.starts_with(FAMILY) {
        return Ok(None);
    }
    decode(bytes).map(Some)
}

fn payload<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, DecodeError> {
    serde_json::from_value(value).map_err(|e| DecodeError::Unreadable(e.to_string()))
}

pub fn decode(bytes: &[u8]) -> Result<LedgerEvent, DecodeError> {
    let envelope: Event<Value> =
        serde_json::from_slice(bytes).map_err(|e| DecodeError::Unreadable(e.to_string()))?;
    if envelope.meaning_version != MEANING_VERSION {
        return Err(DecodeError::Unknown(
            envelope.event_kind,
            envelope.meaning_version,
        ));
    }
    let value = envelope.payload;
    Ok(match envelope.event_kind.as_str() {
        PART_COMMITTED => {
            let part: PartWire = payload(value)?;
            LedgerEvent::RequestPartCommitted(Part {
                request: part.request.into(),
                name: part.name,
                recorded: Recorded {
                    landed: part.landed,
                    note: part.note,
                },
                written_at: part.written_at,
                expires_at: part.expires_at,
            })
        }
        COMMITTED => LedgerEvent::RequestCommitted(payload::<CommitWire>(value)?.into()),
        RECEIPT_RECORDED => {
            let wire: ReceiptWire = payload(value)?;
            LedgerEvent::RequestReceiptRecorded {
                key: wire.key.into(),
                receipt: wire.receipt,
            }
        }
        COMPLETED => {
            let wire: CompletedWire = payload(value)?;
            LedgerEvent::RequestCompleted {
                commit: wire.commit.into(),
                required_parts: wire.required_parts,
            }
        }
        EXPIRED => LedgerEvent::RequestExpired {
            key: payload::<ExpiredWire>(value)?.key.into(),
        },
        APP_ATTEMPT => {
            let wire: AppWire = payload(value)?;
            LedgerEvent::AppAttemptCommitted(AppAttempt {
                key: AppKey {
                    app_id: wire.app_id,
                    event_key: wire.event_key,
                },
                digest: wire.digest,
                landed: wire.landed,
                written_at: wire.written_at,
            })
        }
        IMPORTED => {
            let wire: ImportedWire = payload(value)?;
            LedgerEvent::Imported {
                row: wire.row.into(),
                generation: wire.generation,
            }
        }
        kind => return Err(DecodeError::Unknown(kind.to_string(), MEANING_VERSION)),
    })
}
