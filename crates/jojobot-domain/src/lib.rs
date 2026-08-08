//! jojobot's domain — pure, deterministic, no I/O and no MCP.
//!
//! The bounded contexts below are the assistant's method expressed as a
//! ubiquitous language. This crate is a skeleton: each context is seeded with
//! only the types that are already decided, so the compiler starts enforcing
//! them from day one. Everything undecided (the full tool surface, the graph
//! ontology, the journal schema) is deliberately absent, not stubbed with
//! guesses.
//!
//! TODO: skeleton — only the Memory `Provenance` seed type exists so far.

pub mod attention;
pub mod handle;
pub mod life;
pub mod mailbox;
pub mod memory;
pub mod override_token;
pub mod session;
pub mod text;
pub mod trust;
