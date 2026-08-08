//! Anti-corruption layer — the concrete clients for each fronted life-layer
//! service. Each adapter implements a port defined in `jojobot-domain::life`
//! and quarantines that service's quirks, so the domain never grows a
//! dependency on a wire format.
//!
//! **Memory, mailboxes and sessions all front on [`dolt`]**, a SQL store
//! jojobot spawns and supervises itself: an entity is a row, a fact is a row
//! under it, a message is a row in a box. The search projection ([`search`])
//! sits over the same port. CalDAV and Raindrop clients are still pending.
//!
//! **One store, so [`owners`] no longer spans two.** It is still the port a
//! mailbox asks whether a handle resolves, because the question crosses a
//! context rather than a store: mail does not know what an entity is.

pub mod dolt;
pub mod owners;
pub mod search;

#[cfg(test)]
mod log_capture;
