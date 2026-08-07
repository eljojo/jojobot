//! Anti-corruption layer — the concrete clients for each fronted life-layer
//! service. Each adapter implements a port defined in `jojobot-domain::life`
//! and quarantines that service's quirks, so the domain never grows a
//! dependency on a wire format.
//!
//! **Memory fronts on [`outline`]** — one collection behind one write lock, an
//! entity a page and a fact a row — and the search projection sits over it
//! ([`search`]). CalDAV and Raindrop clients are still pending.
//!
//! **Mailboxes and sessions front on [`dolt`]**, a SQL store jojobot spawns and
//! supervises itself. They moved because a document editor was the wrong shape
//! for them: rows with states and an append-only order are what a table is for,
//! and everything that existed only to survive prose being rewritten stopped
//! earning its place when they left.
//!
//! **The two stores meet at one narrow port** ([`owners`]): a box is created for
//! somebody, entities do not live where the mail does, and "does this handle
//! resolve" is the whole of what crosses.

pub mod dolt;
pub mod outline;
pub mod owners;
pub mod search;

#[cfg(test)]
mod log_capture;
