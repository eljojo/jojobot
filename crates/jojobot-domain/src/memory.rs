//! Memory — facts, portraits, rules-and-receipts.
//!
//! Provenance is a *type*, not a convention: a fact the user stated is
//! testimony; anything derived is inference, and inference is the default.
//! Making this an enum means every place that consumes a fact must decide how
//! it treats the two — the compiler lists the sites.

/// Where a claim came from. The default is [`Provenance::Inference`]: anything
/// not tied to the user's own words is a hypothesis until confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// The user said or confirmed it.
    Testimony,
    /// jojobot (or Claude) derived it. Carries no more authority than a guess.
    Inference,
}

impl Default for Provenance {
    fn default() -> Self {
        Provenance::Inference
    }
}
