//! Whether a real use case is reachable at all — not whether a part behaves.
//!
//! Run through the served surface as a real client, because the last field
//! failure was invisible to every in-process test.
//!
//! What cannot be done yet is commented out in place, marked `// GAP —`. Those
//! blocks are the roadmap's raw material. A story that runs clean tells us less
//! than one that stops.
//!
//! Handles come from the fixture roster, which scans commented-out code too.

// The issuer and the login round trip, shared with the listing's own suite:
// one story reads the operator's page, and it logs in the way his browser does.
mod support;

// A test target's root file does not get the `foo.rs` + `foo/` convention, so
// the folder is named explicitly.
#[path = "user_stories/bikes.rs"]
mod bikes;
#[path = "user_stories/boot.rs"]
mod boot;
#[path = "user_stories/challenge.rs"]
mod challenge;
#[path = "user_stories/colleagues.rs"]
mod colleagues;
#[path = "user_stories/contracts.rs"]
mod contracts;
#[path = "user_stories/coordinating.rs"]
mod coordinating;
#[path = "user_stories/counting.rs"]
mod counting;
#[path = "user_stories/curveball.rs"]
mod curveball;
#[path = "user_stories/degraded.rs"]
mod degraded;
#[path = "user_stories/dsl.rs"]
mod dsl;
#[path = "user_stories/investigating.rs"]
mod investigating;
#[path = "user_stories/kitchensink.rs"]
mod kitchensink;
#[path = "user_stories/moving.rs"]
mod moving;
#[path = "user_stories/party.rs"]
mod party;
#[path = "user_stories/pets.rs"]
mod pets;
#[path = "user_stories/sourcing.rs"]
mod sourcing;
#[path = "user_stories/spotlight.rs"]
mod spotlight;
#[path = "user_stories/stale_handle.rs"]
mod stale_handle;
#[path = "user_stories/statusbar.rs"]
mod statusbar;
#[path = "user_stories/talkingpast.rs"]
mod talkingpast;
#[path = "user_stories/typing.rs"]
mod typing;
#[path = "user_stories/unprompted.rs"]
mod unprompted;
#[path = "user_stories/unsourced.rs"]
mod unsourced;
#[path = "user_stories/unsure.rs"]
mod unsure;
#[path = "user_stories/vocabulary.rs"]
mod vocabulary;
