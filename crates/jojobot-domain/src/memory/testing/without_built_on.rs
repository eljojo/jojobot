//! A store that forwards every required method of the port and nothing else.

use std::sync::Arc;

use super::Memory;

/// **A store that does not override `built_on`, or any other read with a
/// default.** It forwards the required methods to `inner` and leaves every
/// defaulted one to the port's own body, so a case run against it reads what
/// the default reads and nothing the inner store answers by itself.
pub struct WithoutBuiltOn(pub Arc<dyn Memory>);

#[crate::forward(forward_memory, via = 0, required_only)]
impl Memory for WithoutBuiltOn {}
