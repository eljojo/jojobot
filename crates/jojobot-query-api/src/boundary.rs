//! A consumer can submit the shared question through the handle.
//!
//! ```
//! use jojobot_query_api::{QueryHandle, Question};
//! fn ask(handle: &QueryHandle, question: Question) {
//!     drop(handle.ask(question));
//! }
//! ```
//!
//! The same consumer cannot name a store port through this boundary.
//!
//! ```compile_fail,E0432
//! use jojobot_query_api::Memory;
//! ```
