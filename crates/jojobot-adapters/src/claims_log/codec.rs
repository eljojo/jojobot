//! **The stored form of a claims event**: the log envelope with a typed payload,
//! as bytes. `meaning_version` is fixed when an event is written; an unknown kind
//! or version is an error, never skipped (design section 3). The domain keeps no
//! wire format, so the encoding lives here, in private mirrors of the domain's
//! values. The whole envelope is kept, because the State reads the actor, the run
//! and the moments off it.
//!
//! **Reading a declared type needs the shipped kinds loaded**, as the process loads
//! them at start: a field that points at kinds is stored by their names and read
//! back through the registry.

use std::collections::BTreeMap;

use jiff::{Timestamp, civil::Date};
use jojobot_domain::app::{AppId, AppStamp};
use jojobot_domain::log::claims::{
    ClaimBody, ClaimEvent, FieldValue, KeyId, KindId, MovedRecord, MovedWrite, ObjectBody,
    ObjectSnapshot,
};
use jojobot_domain::log::{Event, ObjectId, RecordId};
use jojobot_domain::memory::types::{DeclaredType, Field, Fold, Kinds, Origin, ValueType};
use jojobot_domain::memory::{FactStatus, Provenance, Receipt, Standing, WriteClass};
use serde::{Deserialize, Serialize};

const MEANING_VERSION: u32 = 1;
/// What every kind of this family starts with.
const FAMILY: &str = "Claims";

/// Why stored bytes are not a claims event this build understands.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("a claims event could not be read: {0}")]
    Unreadable(String),
    #[error("a claims event has a kind or meaning this build does not know: {0} v{1}")]
    Unknown(String, u32),
}

fn unreadable(what: impl std::fmt::Display) -> DecodeError {
    DecodeError::Unreadable(what.to_string())
}

#[derive(Serialize, Deserialize)]
struct ObjectBodyWire {
    kind: String,
    parent: Option<ObjectId>,
    name: String,
    source: String,
    aliases: Vec<String>,
    boot: Option<String>,
    metadata: BTreeMap<String, String>,
    archived: bool,
    bound_to: Option<AppId>,
}

#[derive(Serialize, Deserialize)]
struct SnapshotWire {
    kind: String,
    name: String,
    source: String,
    crm: Option<String>,
    parent: Option<ObjectId>,
    boot: String,
    archived_reason: Option<String>,
    archived_at: Option<String>,
    merged_into: Option<ObjectId>,
    aliases: Vec<String>,
    prose: String,
}

#[derive(Serialize, Deserialize)]
struct ClaimBodyWire {
    label: String,
    home: ObjectId,
    content: String,
    details: Option<String>,
    provenance: Provenance,
    standing: Standing,
    status: FactStatus,
    recorded_at: Date,
    happened_at: Option<Date>,
    happened_through: Option<Date>,
    stale_after: Option<Date>,
    edge: Option<(String, ObjectId)>,
    derived_from: Option<RecordId>,
    refs: Vec<ObjectId>,
    stands_for: Vec<RecordId>,
    receipt: Option<Receipt>,
    app: Option<AppStamp>,
    inserted_at: Option<Timestamp>,
}

#[derive(Serialize, Deserialize)]
struct MovedRecordWire {
    record: RecordId,
    label: String,
}

#[derive(Serialize, Deserialize)]
struct MovedWriteWire {
    record: RecordId,
    key: String,
    from_ordinal: u64,
    ordinal: u64,
}

#[derive(Serialize, Deserialize)]
struct FieldWire {
    key: String,
    holds: String,
    points_at: Option<String>,
    folds: String,
    required: bool,
    list: bool,
    one_of: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct DeclaredTypeWire {
    name: String,
    fields: Vec<FieldWire>,
    origin: String,
    must: Option<String>,
}

/// A written value is `Some`, and a cleared key is `None`.
type ValueWire = Option<String>;

/// **Every shape this family stores**, as `Payload` names them. Kept beside the
/// enum so a new shape is added in both places, and checked against the events and
/// the pinned literals in the tests.
pub const SHAPES: [&str; 13] = [
    "ObjectCreated",
    "ObjectRevised",
    "ObjectBaselined",
    "ObjectWriteObserved",
    "AddressBound",
    "AddressRetired",
    "ObjectsMerged",
    "ObjectRedirected",
    "MergeNumbersLeftEmpty",
    "RecordCreated",
    "RecordRevised",
    "FieldWritten",
    "TypeDeclared",
];

#[derive(Serialize, Deserialize)]
#[serde(tag = "shape")]
enum Payload {
    ObjectCreated {
        object: ObjectId,
        body: ObjectBodyWire,
    },
    ObjectRevised {
        object: ObjectId,
        body: ObjectBodyWire,
    },
    ObjectBaselined {
        object: ObjectId,
        body: ObjectBodyWire,
    },
    ObjectWriteObserved {
        object: ObjectId,
        filed_under: String,
        ordinal: u64,
        snapshot: Option<SnapshotWire>,
    },
    AddressBound {
        object: ObjectId,
        address: String,
    },
    AddressRetired {
        object: ObjectId,
        address: String,
        day: Date,
    },
    ObjectsMerged {
        folded: ObjectId,
        survivor: ObjectId,
        records: Vec<MovedRecordWire>,
        writes: Vec<MovedWriteWire>,
    },
    ObjectRedirected {
        folded: ObjectId,
        survivor: ObjectId,
    },
    MergeNumbersLeftEmpty {
        count: u64,
    },
    RecordCreated {
        record: RecordId,
        claim: ClaimBodyWire,
    },
    RecordRevised {
        record: RecordId,
        claim: ClaimBodyWire,
    },
    FieldWritten {
        record: RecordId,
        subject: ObjectId,
        key: String,
        ordinal: u64,
        class: String,
        write: ValueWire,
        targets: Vec<ObjectId>,
    },
    TypeDeclared {
        kind: ObjectId,
        declaration: DeclaredTypeWire,
    },
}

impl Payload {
    /// The kind the envelope names, which says which payload to expect.
    fn kind(&self) -> String {
        let name = match self {
            Payload::ObjectCreated { .. } => "ObjectCreated",
            Payload::ObjectRevised { .. } => "ObjectRevised",
            Payload::ObjectBaselined { .. } => "ObjectBaselined",
            Payload::ObjectWriteObserved { .. } => "ObjectWriteObserved",
            Payload::AddressBound { .. } => "AddressBound",
            Payload::AddressRetired { .. } => "AddressRetired",
            Payload::ObjectsMerged { .. } => "ObjectsMerged",
            Payload::ObjectRedirected { .. } => "ObjectRedirected",
            Payload::MergeNumbersLeftEmpty { .. } => "MergeNumbersLeftEmpty",
            Payload::RecordCreated { .. } => "RecordCreated",
            Payload::RecordRevised { .. } => "RecordRevised",
            Payload::FieldWritten { .. } => "FieldWritten",
            Payload::TypeDeclared { .. } => "TypeDeclared",
        };
        format!("{FAMILY}{name}")
    }
}

fn body_wire(body: &ObjectBody) -> ObjectBodyWire {
    ObjectBodyWire {
        kind: body.kind.0.clone(),
        parent: body.parent.clone(),
        name: body.name.clone(),
        source: body.source.clone(),
        aliases: body.aliases.clone(),
        boot: body.boot.clone(),
        metadata: body.metadata.clone(),
        archived: body.archived,
        bound_to: body.bound_to.clone(),
    }
}

fn body_of(wire: ObjectBodyWire) -> ObjectBody {
    ObjectBody {
        kind: KindId(wire.kind),
        parent: wire.parent,
        name: wire.name,
        source: wire.source,
        aliases: wire.aliases,
        boot: wire.boot,
        metadata: wire.metadata,
        archived: wire.archived,
        bound_to: wire.bound_to,
    }
}

fn snapshot_wire(snapshot: &ObjectSnapshot) -> SnapshotWire {
    SnapshotWire {
        kind: snapshot.kind.clone(),
        name: snapshot.name.clone(),
        source: snapshot.source.clone(),
        crm: snapshot.crm.clone(),
        parent: snapshot.parent.clone(),
        boot: snapshot.boot.clone(),
        archived_reason: snapshot.archived_reason.clone(),
        archived_at: snapshot.archived_at.clone(),
        merged_into: snapshot.merged_into.clone(),
        aliases: snapshot.aliases.clone(),
        prose: snapshot.prose.clone(),
    }
}

fn snapshot_of(wire: SnapshotWire) -> ObjectSnapshot {
    ObjectSnapshot {
        kind: wire.kind,
        name: wire.name,
        source: wire.source,
        crm: wire.crm,
        parent: wire.parent,
        boot: wire.boot,
        archived_reason: wire.archived_reason,
        archived_at: wire.archived_at,
        merged_into: wire.merged_into,
        aliases: wire.aliases,
        prose: wire.prose,
    }
}

fn claim_wire(claim: &ClaimBody) -> ClaimBodyWire {
    ClaimBodyWire {
        label: claim.label.clone(),
        home: claim.home.clone(),
        content: claim.content.clone(),
        details: claim.details.clone(),
        provenance: claim.provenance,
        standing: claim.standing,
        status: claim.status,
        recorded_at: claim.recorded_at,
        happened_at: claim.happened_at,
        happened_through: claim.happened_through,
        stale_after: claim.stale_after,
        edge: claim.edge.clone(),
        derived_from: claim.derived_from.clone(),
        refs: claim.refs.clone(),
        stands_for: claim.stands_for.clone(),
        receipt: claim.receipt.clone(),
        app: claim.app.clone(),
        inserted_at: claim.inserted_at,
    }
}

fn claim_of(wire: ClaimBodyWire) -> ClaimBody {
    ClaimBody {
        label: wire.label,
        home: wire.home,
        content: wire.content,
        details: wire.details,
        provenance: wire.provenance,
        standing: wire.standing,
        status: wire.status,
        recorded_at: wire.recorded_at,
        happened_at: wire.happened_at,
        happened_through: wire.happened_through,
        stale_after: wire.stale_after,
        edge: wire.edge,
        derived_from: wire.derived_from,
        refs: wire.refs,
        stands_for: wire.stands_for,
        receipt: wire.receipt,
        app: wire.app,
        inserted_at: wire.inserted_at,
    }
}

fn class_token(class: WriteClass) -> String {
    class.as_token().to_string()
}

/// **A class this build does not name is an error.** `WriteClass::from_token`
/// reads every unknown word as the legacy class, which would turn a newer
/// build's class into an older one silently.
fn class_of(token: &str) -> Result<WriteClass, DecodeError> {
    match token {
        "legacy" => Ok(WriteClass::Legacy),
        "own" => Ok(WriteClass::Own),
        "sets" => Ok(WriteClass::Sets),
        "stays" => Ok(WriteClass::Stays),
        other => Err(unreadable(format!("a write class named {other}"))),
    }
}

fn declaration_wire(declaration: &DeclaredType) -> DeclaredTypeWire {
    DeclaredTypeWire {
        name: declaration.name.clone(),
        fields: declaration
            .fields
            .iter()
            .map(|field| FieldWire {
                key: field.key.clone(),
                holds: field.holds.as_token().to_string(),
                points_at: field.points_at.as_ref().map(Kinds::as_token),
                folds: field.folds.as_token().to_string(),
                required: field.required,
                list: field.list,
                one_of: field.one_of.clone(),
            })
            .collect(),
        origin: declaration.origin.as_token().to_string(),
        must: declaration.must.clone(),
    }
}

fn declaration_of(wire: DeclaredTypeWire) -> Result<DeclaredType, DecodeError> {
    let fields = wire
        .fields
        .into_iter()
        .map(|field| {
            Ok(Field {
                holds: ValueType::of_token(&field.holds)
                    .ok_or_else(|| unreadable(format!("a value type named {}", field.holds)))?,
                points_at: match field.points_at {
                    Some(token) => Some(
                        Kinds::of_token(&token)
                            .ok_or_else(|| unreadable(format!("kinds named {token}")))?,
                    ),
                    None => None,
                },
                folds: Fold::of_token(&field.folds)
                    .ok_or_else(|| unreadable(format!("a fold named {}", field.folds)))?,
                key: field.key,
                required: field.required,
                list: field.list,
                one_of: field.one_of,
            })
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    Ok(DeclaredType {
        name: wire.name,
        fields,
        origin: Origin::of_token(&wire.origin)
            .ok_or_else(|| unreadable(format!("an origin named {}", wire.origin)))?,
        must: wire.must,
    })
}

fn payload_wire(event: &ClaimEvent) -> Payload {
    match event {
        ClaimEvent::ObjectCreated { object, body } => Payload::ObjectCreated {
            object: object.clone(),
            body: body_wire(body),
        },
        ClaimEvent::ObjectRevised { object, body } => Payload::ObjectRevised {
            object: object.clone(),
            body: body_wire(body),
        },
        ClaimEvent::ObjectBaselined { object, body } => Payload::ObjectBaselined {
            object: object.clone(),
            body: body_wire(body),
        },
        ClaimEvent::ObjectWriteObserved {
            object,
            filed_under,
            ordinal,
            snapshot,
        } => Payload::ObjectWriteObserved {
            object: object.clone(),
            filed_under: filed_under.clone(),
            ordinal: *ordinal,
            snapshot: snapshot.as_ref().map(snapshot_wire),
        },
        ClaimEvent::AddressBound { object, address } => Payload::AddressBound {
            object: object.clone(),
            address: address.clone(),
        },
        ClaimEvent::AddressRetired {
            object,
            address,
            day,
        } => Payload::AddressRetired {
            object: object.clone(),
            address: address.clone(),
            day: *day,
        },
        ClaimEvent::ObjectsMerged {
            folded,
            survivor,
            records,
            writes,
        } => Payload::ObjectsMerged {
            folded: folded.clone(),
            survivor: survivor.clone(),
            records: records
                .iter()
                .map(|moved| MovedRecordWire {
                    record: moved.record.clone(),
                    label: moved.label.clone(),
                })
                .collect(),
            writes: writes
                .iter()
                .map(|moved| MovedWriteWire {
                    record: moved.record.clone(),
                    key: moved.key.0.clone(),
                    from_ordinal: moved.from_ordinal,
                    ordinal: moved.ordinal,
                })
                .collect(),
        },
        ClaimEvent::MergeNumbersLeftEmpty { count } => {
            Payload::MergeNumbersLeftEmpty { count: *count }
        }
        ClaimEvent::ObjectRedirected { folded, survivor } => Payload::ObjectRedirected {
            folded: folded.clone(),
            survivor: survivor.clone(),
        },
        ClaimEvent::RecordCreated { record, claim } => Payload::RecordCreated {
            record: record.clone(),
            claim: claim_wire(claim),
        },
        ClaimEvent::RecordRevised { record, claim } => Payload::RecordRevised {
            record: record.clone(),
            claim: claim_wire(claim),
        },
        ClaimEvent::FieldWritten {
            record,
            subject,
            key,
            ordinal,
            class,
            write,
            targets,
        } => Payload::FieldWritten {
            record: record.clone(),
            subject: subject.clone(),
            key: key.0.clone(),
            ordinal: *ordinal,
            class: class_token(*class),
            write: match write {
                FieldValue::Set(value) => Some(value.clone()),
                FieldValue::Clear => None,
            },
            targets: targets.clone(),
        },
        ClaimEvent::TypeDeclared { kind, declaration } => Payload::TypeDeclared {
            kind: kind.clone(),
            declaration: declaration_wire(declaration),
        },
    }
}

fn event_of(payload: Payload) -> Result<ClaimEvent, DecodeError> {
    Ok(match payload {
        Payload::ObjectCreated { object, body } => ClaimEvent::ObjectCreated {
            object,
            body: body_of(body),
        },
        Payload::ObjectRevised { object, body } => ClaimEvent::ObjectRevised {
            object,
            body: body_of(body),
        },
        Payload::ObjectBaselined { object, body } => ClaimEvent::ObjectBaselined {
            object,
            body: body_of(body),
        },
        Payload::ObjectWriteObserved {
            object,
            filed_under,
            ordinal,
            snapshot,
        } => ClaimEvent::ObjectWriteObserved {
            object,
            filed_under,
            ordinal,
            snapshot: snapshot.map(snapshot_of),
        },
        Payload::AddressBound { object, address } => ClaimEvent::AddressBound { object, address },
        Payload::AddressRetired {
            object,
            address,
            day,
        } => ClaimEvent::AddressRetired {
            object,
            address,
            day,
        },
        Payload::ObjectsMerged {
            folded,
            survivor,
            records,
            writes,
        } => ClaimEvent::ObjectsMerged {
            folded,
            survivor,
            records: records
                .into_iter()
                .map(|moved| MovedRecord {
                    record: moved.record,
                    label: moved.label,
                })
                .collect(),
            writes: writes
                .into_iter()
                .map(|moved| MovedWrite {
                    record: moved.record,
                    key: KeyId(moved.key),
                    from_ordinal: moved.from_ordinal,
                    ordinal: moved.ordinal,
                })
                .collect(),
        },
        Payload::ObjectRedirected { folded, survivor } => {
            ClaimEvent::ObjectRedirected { folded, survivor }
        }
        Payload::MergeNumbersLeftEmpty { count } => ClaimEvent::MergeNumbersLeftEmpty { count },
        Payload::RecordCreated { record, claim } => ClaimEvent::RecordCreated {
            record,
            claim: claim_of(claim),
        },
        Payload::RecordRevised { record, claim } => ClaimEvent::RecordRevised {
            record,
            claim: claim_of(claim),
        },
        Payload::FieldWritten {
            record,
            subject,
            key,
            ordinal,
            class,
            write,
            targets,
        } => ClaimEvent::FieldWritten {
            record,
            subject,
            key: KeyId(key),
            ordinal,
            class: class_of(&class)?,
            write: match write {
                Some(value) => FieldValue::Set(value),
                None => FieldValue::Clear,
            },
            targets,
        },
        Payload::TypeDeclared { kind, declaration } => ClaimEvent::TypeDeclared {
            kind,
            declaration: declaration_of(declaration)?,
        },
    })
}

/// **The stored form of one event**: its envelope, with the kind and the meaning
/// version this build writes, and the payload as typed values. The envelope's own
/// `event_kind` and `meaning_version` are replaced, so a caller cannot store a
/// kind the payload does not match.
pub fn encode(event: &Event<ClaimEvent>) -> Vec<u8> {
    let payload = payload_wire(&event.payload);
    let envelope = Event {
        event_kind: payload.kind(),
        meaning_version: MEANING_VERSION,
        actor: event.actor.clone(),
        run: event.run.clone(),
        session_card: event.session_card.clone(),
        request: event.request.clone(),
        part: event.part.clone(),
        occurred_at: event.occurred_at,
        written_at: event.written_at,
        imported_source_time: event.imported_source_time,
        payload,
    };
    serde_json::to_vec(&envelope).expect("a claims event is plain data")
}

/// **The claims family's own events, out of a log that holds every family's.** The
/// family is the one the envelope's `event_kind` names: a kind that starts with
/// [`FAMILY`] is this family's and must decode (an unknown one is an error, never
/// skipped); any other kind belongs to another family's reader and gives `None`.
pub fn decode_own(bytes: &[u8]) -> Result<Option<Event<ClaimEvent>>, DecodeError> {
    #[derive(Deserialize)]
    struct Named {
        event_kind: String,
    }
    let named: Named = serde_json::from_slice(bytes).map_err(unreadable)?;
    if !named.event_kind.starts_with(FAMILY) {
        return Ok(None);
    }
    decode(bytes).map(Some)
}

pub fn decode(bytes: &[u8]) -> Result<Event<ClaimEvent>, DecodeError> {
    #[derive(Deserialize)]
    struct Named {
        event_kind: String,
        meaning_version: u32,
    }
    let named: Named = serde_json::from_slice(bytes).map_err(unreadable)?;
    if named.meaning_version != MEANING_VERSION {
        return Err(DecodeError::Unknown(
            named.event_kind,
            named.meaning_version,
        ));
    }
    let envelope: Event<serde_json::Value> = serde_json::from_slice(bytes).map_err(unreadable)?;
    let payload: Payload = match serde_json::from_value(envelope.payload) {
        Ok(payload) => payload,
        Err(_) => {
            return Err(DecodeError::Unknown(
                named.event_kind,
                named.meaning_version,
            ));
        }
    };
    if payload.kind() != named.event_kind {
        return Err(unreadable(format!(
            "the kind {} does not match its payload",
            named.event_kind
        )));
    }
    Ok(Event {
        event_kind: named.event_kind,
        meaning_version: named.meaning_version,
        actor: envelope.actor,
        run: envelope.run,
        session_card: envelope.session_card,
        request: envelope.request,
        part: envelope.part,
        occurred_at: envelope.occurred_at,
        written_at: envelope.written_at,
        imported_source_time: envelope.imported_source_time,
        payload: event_of(payload)?,
    })
}
