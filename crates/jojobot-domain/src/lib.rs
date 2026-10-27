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

pub mod app;
pub mod attention;
pub mod challenge;
pub mod clock;
pub mod handle;
pub mod life;
pub mod mailbox;
pub mod memory;
pub mod override_token;
pub mod push;
pub mod recurrence;
pub mod request;
pub mod session;
pub mod storage_check;
pub mod teaching;
pub mod text;
pub mod trust;
pub mod usage;
pub mod voucher;

#[doc(hidden)]
pub use jojobot_forward::__finish as __forward_finish;
/// **The generator of a test double's forwarding** (see `jojobot-forward`): the
/// attribute that marks a double's impl, and the step the port's macro calls.
pub use jojobot_forward::forward;
