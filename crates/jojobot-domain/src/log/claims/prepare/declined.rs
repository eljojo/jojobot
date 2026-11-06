//! **A refusal of [`prepare`](super::prepare), as the caller is answered.** One
//! map, read inside the job against the State that refused, so the facts a full
//! room names and the near names a missing name resembles are the State's at the
//! moment of the refusal. The port renders the answer; nothing here chooses
//! words for a verb.

use super::super::State;
use super::{Refusal, candidates};
use crate::log::RecordId;
use crate::memory::guard::EntityMatch;
use crate::memory::{EntityId, Fact, MemoryError};

/// What a refused capture is answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Declined {
    /// A name nothing holds, and the names it resembles: no write happened.
    Blocked {
        attempted: EntityId,
        candidates: Vec<EntityMatch>,
    },
    /// Any other refusal, as the error the caller reads.
    Refused(MemoryError),
}

impl Refusal {
    /// This refusal as the caller is answered, with the facts and names it
    /// carries read from `state`.
    pub fn declined(self, state: &State) -> Declined {
        let facts = |records: &[RecordId]| -> Vec<Fact> {
            records.iter().filter_map(|id| state.fact(id)).collect()
        };
        let blocked = |name: String, candidates: Vec<EntityMatch>| Declined::Blocked {
            attempted: EntityId(name),
            candidates,
        };
        let handle = |object: &crate::log::ObjectId| {
            state
                .handle_of(object)
                .map_or_else(|| object.0.clone(), |handle| handle.0)
        };
        match self {
            Refusal::UnknownSubject(name, near)
            | Refusal::UnknownEdgeObject(name, near)
            | Refusal::UnknownRef(name, near) => blocked(name, near),
            Refusal::UnknownFieldTarget(name) => {
                let near = candidates::of(state, &name);
                blocked(name, near)
            }
            Refusal::UnknownSourceHome(address) => {
                let home = address.split('#').next().unwrap_or(&address).to_string();
                let nearest = candidates::of(state, &home);
                Declined::Refused(MemoryError::UnknownEntity {
                    attempted: home,
                    nearest,
                })
            }
            Refusal::UnknownSourceClaim(address) => {
                let home = address.split('#').next().unwrap_or(&address);
                let nearest = state
                    .resolve(home)
                    .map(|object| {
                        state
                            .records_homed_on(object)
                            .into_iter()
                            .filter_map(|id| state.records.get(id))
                            .map(|held| format!("{}#{}", handle(object), held.claim.label))
                            .collect()
                    })
                    .unwrap_or_default();
                Declined::Refused(MemoryError::UnknownFact {
                    attempted: address,
                    nearest,
                })
            }
            Refusal::OwnedKeyWritten { subject, key } => {
                Declined::Refused(MemoryError::InvalidFact(format!(
                    "'{key}' on {subject} belongs to the app the thing is bound to, and only \
                     that app writes it"
                )))
            }
            Refusal::OwnerRevoked { subject, key } => {
                Declined::Refused(MemoryError::InvalidFact(format!(
                    "'{key}' on {subject} belongs to an app whose grant is revoked, so nobody \
                     writes it"
                )))
            }
            Refusal::RoleFieldGuarded { role, key } => {
                Declined::Refused(MemoryError::RoleFieldGuarded { role, key })
            }
            Refusal::ServerWritten { key } => Declined::Refused(MemoryError::InvalidFact(format!(
                "'{key}' is written by jojobot, not by a caller"
            ))),
            Refusal::Unlicensed(error)
            | Refusal::Numbering(error)
            | Refusal::BreaksKind(error)
            | Refusal::Invalid(error) => Declined::Refused(error),
            Refusal::Malformed(words) => Declined::Refused(MemoryError::InvalidFact(words)),
            Refusal::SeatsFull {
                subject,
                seats,
                starred,
                seated,
            } => {
                let records: Vec<RecordId> = seated.into_iter().map(|rule| rule.record).collect();
                Declined::Refused(MemoryError::SeatsFull {
                    subject,
                    seats,
                    starred,
                    seated: facts(&records),
                })
            }
            Refusal::RoomFull {
                subject,
                live,
                capacity,
                aged_out,
                room,
            } => {
                let records: Vec<RecordId> = room.into_iter().map(|held| held.record).collect();
                Declined::Refused(MemoryError::RoomFull {
                    subject,
                    live,
                    capacity,
                    room: facts(&records),
                    aged_out,
                })
            }
            Refusal::OverCapacity {
                subject,
                limit,
                adding,
                from_setting,
                records,
            } => {
                let records: Vec<RecordId> = records.into_iter().map(|held| held.record).collect();
                Declined::Refused(MemoryError::OverCapacity {
                    subject,
                    live: records.len(),
                    limit,
                    adding,
                    from_setting,
                    records: facts(&records),
                })
            }
            Refusal::NumberHeld {
                container,
                key,
                number,
                holder,
                next,
            } => Declined::Refused(MemoryError::NumberTaken {
                collection: handle(&container),
                key,
                number,
                holder: handle(&holder),
                next,
            }),
            Refusal::NumberRetired {
                container,
                key,
                number,
                next,
            } => Declined::Refused(MemoryError::NumberRetired {
                collection: handle(&container),
                key,
                number,
                next,
            }),
        }
    }
}
