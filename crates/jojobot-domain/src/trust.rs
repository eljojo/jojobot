//! Trust — cross-cutting policy, not a context. Every claim is tagged with a
//! source class; tool output is untrusted until vetted; the write path refuses
//! an actionable specific without a verified source. Provenance rides the
//! graph so any claim can be traced to its source span.
//!
//! TODO: skeleton only — no types defined yet.
