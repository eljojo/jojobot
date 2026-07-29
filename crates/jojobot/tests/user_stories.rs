//! Whether a real use case is reachable at all — not whether a part behaves.
//!
//! Run through the served surface as a real client, because the last field
//! failure was invisible to every in-process test.
//!
//! What cannot be done yet is commented out in place, marked `// GAP:`. Those
//! blocks are the roadmap's raw material. A story that runs clean tells us less
//! than one that stops.
//!
//! Handles come from the fixture roster, which scans commented-out code too.

// A test target's root file does not get the `foo.rs` + `foo/` convention, so
// the folder is named explicitly.
#[path = "user_stories/bikes.rs"]
mod bikes;
#[path = "user_stories/dsl.rs"]
mod dsl;
#[path = "user_stories/moving.rs"]
mod moving;
#[path = "user_stories/party.rs"]
mod party;
