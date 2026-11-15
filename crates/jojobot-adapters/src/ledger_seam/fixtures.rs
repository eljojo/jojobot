//! **The fixtures the seam's test files share**: one moment, one actor, one request, the
//! event wrapper and the two events that create a thing, and the committed request they
//! spend. A file that needs another shape writes its own beside its cases.

use jiff::Timestamp;
use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{Event, ObjectId, RequestId, RunId};
use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};

pub(super) fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

pub(super) fn actor() -> ObjectId {
    ObjectId("bot:homer".into())
}

pub(super) fn request_id() -> RequestId {
    RequestId("alpha".into())
}

pub(super) fn homer() -> ObjectId {
    ObjectId("k1".into())
}

/// An event carrying `payload`, written at the first moment, by the run "run-a".
pub(super) fn event(payload: ClaimEvent) -> Event<ClaimEvent> {
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

/// The events that create a person and bind the address it is known by.
pub(super) fn claims_events() -> Vec<Event<ClaimEvent>> {
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

/// A request that committed, and the receipt it landed.
pub(super) fn spent() -> LedgerEvent {
    LedgerEvent::RequestCommitted(Commit {
        request: Request {
            key: RequestKey {
                actor: actor(),
                id: request_id(),
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
