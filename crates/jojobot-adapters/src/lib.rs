//! Anti-corruption layer — the concrete clients for each fronted life-layer
//! service. Each adapter implements a port defined in `jojobot-domain::life`
//! and quarantines that service's quirks. No adapters are implemented yet; this
//! crate exists to fix the seam so the domain never grows a dependency on a
//! wire format.
//!
//! The Outline store — the Memory port's real adapter — is the first one
//! landed, with the search projection over it ([`search`]). Vikunja, CalDAV, and
//! Raindrop clients are still pending.

pub mod outline;
pub mod search;
pub mod vikunja;
