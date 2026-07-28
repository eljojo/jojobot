//! Anti-corruption layer — the concrete clients for each fronted life-layer
//! service. Each adapter implements a port defined in `jojobot-domain::life`
//! and quarantines that service's quirks, so the domain never grows a
//! dependency on a wire format.
//!
//! **One store fronts all three contexts.** Memory, Sessions and Mailboxes all
//! live in [`outline`], over one collection and behind one write lock — an
//! entity is a page, a bot's sessions are a page under it, a mailbox is a page,
//! and a session or a message is a row. The search projection sits over the
//! same store ([`search`]). CalDAV and Raindrop clients are still pending.

pub mod outline;
pub mod search;

#[cfg(test)]
mod log_capture;
