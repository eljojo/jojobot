//! **The third tier: a run that costs money.**
//!
//! The fast suites ask whether the code works. The store suites ask whether it
//! works against the real thing it fronts. Neither can ask the question this
//! tier exists for: **does the surface teach itself?** A scripted client is
//! told which verb to call, so it proves the verb and nothing about the
//! descriptions, the orientation or the refusals a session actually reads. Only
//! a real model reading the surface as shipped can be wrong about it.
//!
//! A run is: a **room** brought up from nothing, a **playbook** somebody wrote
//! in English, a **model** driven through it over MCP, and **assertions over
//! what is in the store afterwards**. The run reports; it does not judge. There
//! is no rubric and no score, because whether jojobot did well is the
//! operator's reading of a transcript, not a number this crate invents.
//!
//! **Nothing here runs from `cargo test`.** The paid half is a binary somebody
//! invokes on purpose. What is testable without spending anything — the room,
//! the reading of a playbook, the shape of a result — is tested the ordinary
//! way, in this library, and `make check` runs those.
//!
//! ⚠️ **A bare `cargo test -p jojobot-exercise` does not rebuild the server
//! binary the rooms spawn** — [`room::refuse_a_stale_server`] catches it, but
//! it catches it PER TEST, so a stale binary reads as a wall of failures
//! across seemingly unrelated cases rather than one clear error. That wall is
//! one root cause, not dozens. `make check` and `make narrow` already build
//! the workspace first; run one of those, or `cargo build --workspace`
//! yourself, before reaching for `cargo test -p jojobot-exercise` directly.

pub mod agent;
pub mod calls;
pub mod checks;
pub mod expectations;
pub mod failability;
pub mod isolate;
pub mod lock;
pub mod playbook;
pub mod room;
pub mod run;
pub mod surface;
pub mod world;
