//! **Defects the product has today, written down as cases that fail.**
//!
//! A case here states how the product SHOULD answer and is marked `#[ignore]`
//! at both stores, with the reason. It is red when run on purpose, and it stays
//! out of the bar until a fix lands, so a defect nobody has scheduled is still
//! one a person can run and read rather than a sentence on a card.
//!
//! **There are none today.** The module stays so the next defect has somewhere to be
//! written down, and so a case moves out of it the day it passes. The glob
//! re-export in `contract.rs` is added back with the first case, because a glob
//! of an empty module is an unused import.
