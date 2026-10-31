//! **The stored form of a teaching event**: the log envelope with a typed payload,
//! as bytes. `meaning_version` is fixed when an event is written; an unknown kind
//! or version is an error, never skipped (design section 3). The domain keeps no
//! wire format, so the encoding lives here.

use jojobot_domain::log::Event;
use jojobot_domain::teaching::events::{RunId, TeachingEvent};
use serde::{Deserialize, Serialize};

const MEANING_VERSION: u32 = 1;
const RECORDED: &str = "TeachingRecorded";
const CLEARED: &str = "TeachingCleared";

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    Recorded {
        domain: String,
        taught_at: jiff::Timestamp,
    },
    Cleared {},
}

/// Why stored bytes are not a teaching event this build understands.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("a teaching event could not be read: {0}")]
    Unreadable(String),
    #[error("a teaching event has a kind or meaning this build does not know: {0} v{1}")]
    Unknown(String, u32),
}

pub fn encode(event: &TeachingEvent) -> Vec<u8> {
    let (kind, run, payload, source) = match event {
        TeachingEvent::Recorded {
            run,
            domain,
            taught_at,
        } => (
            RECORDED,
            run,
            Payload::Recorded {
                domain: domain.clone(),
                taught_at: *taught_at,
            },
            Some(*taught_at),
        ),
        TeachingEvent::Cleared { run } => (CLEARED, run, Payload::Cleared {}, None),
    };
    let envelope = Event {
        event_kind: kind.to_string(),
        meaning_version: MEANING_VERSION,
        actor: None,
        run: Some(jojobot_domain::log::RunId(run.0.clone())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: source,
        written_at: None,
        imported_source_time: None,
        payload,
    };
    serde_json::to_vec(&envelope).expect("a teaching event is plain data")
}

pub fn decode(bytes: &[u8]) -> Result<TeachingEvent, DecodeError> {
    let envelope: Event<Payload> =
        serde_json::from_slice(bytes).map_err(|e| DecodeError::Unreadable(e.to_string()))?;
    let run = envelope
        .run
        .map(|run| RunId(run.0))
        .ok_or_else(|| DecodeError::Unreadable("no run".to_string()))?;
    match (
        envelope.event_kind.as_str(),
        envelope.meaning_version,
        envelope.payload,
    ) {
        (RECORDED, MEANING_VERSION, Payload::Recorded { domain, taught_at }) => {
            Ok(TeachingEvent::Recorded {
                run,
                domain,
                taught_at,
            })
        }
        (CLEARED, MEANING_VERSION, Payload::Cleared {}) => Ok(TeachingEvent::Cleared { run }),
        (kind, version, _) => Err(DecodeError::Unknown(kind.to_string(), version)),
    }
}
