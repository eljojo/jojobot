//! Anti-corruption layer — the concrete clients for each fronted life-layer
//! service. Each adapter implements a port defined in `jojobot-domain::life`
//! and quarantines that service's quirks. No adapters are implemented yet; this
//! crate exists to fix the seam so the domain never grows a dependency on a
//! wire format.
//!
//! TODO: skeleton — no adapters implemented yet (Vikunja, Outline, CalDAV,
//! Raindrop clients all pending).
