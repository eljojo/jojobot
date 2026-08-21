//! **The escape hatches, and every one of them is a finding.**
//!
//! A room's document names a check here when the claim it makes cannot be
//! written as a query and an assertion. **That is not a convenience**: each
//! entry names a specific thing jojobot's query surface cannot say, the run
//! counts and prints the ones a room took, and a room that cannot reach zero
//! is telling you something about the surface rather than about the harness.
//!
//! **The check answers the verdict and never the sentence.** The prose a reader
//! sees is written beside the lock, in the room, by somebody who knows what the
//! room is about.

use crate::run::Checks;

/// One hatch: the name a document calls it, and what it does.
type Hatch = (&'static str, fn() -> Box<dyn Checks>);

/// **Every named check this build ships.** A room adds one line here and one
/// `check` line in its document, and both are visible in the count.
pub const CHECKS: [Hatch; 0] = [];
