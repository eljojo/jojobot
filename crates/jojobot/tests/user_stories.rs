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
// one story reads the operator's page, and it logs in the way a browser does.
mod support;

// A test target's root file does not get the `foo.rs` + `foo/` convention, so
// the folder is named explicitly.
#[path = "user_stories/around_a_day.rs"]
mod around_a_day;
/// A run that is not happening now, and the day it says it is in.
#[path = "user_stories/backdated.rs"]
mod backdated;
#[path = "user_stories/bikes.rs"]
mod bikes;
#[path = "user_stories/boot.rs"]
mod boot;
#[path = "user_stories/challenge.rs"]
mod challenge;
/// A rule settled by reading it at its address, not by taking somebody's word.
#[path = "user_stories/citing.rs"]
mod citing;
#[path = "user_stories/colleagues.rs"]
mod colleagues;
#[path = "user_stories/contracts.rs"]
mod contracts;
#[path = "user_stories/coordinating.rs"]
mod coordinating;
/// Whether a record was always right, or was wrong and got fixed.
#[path = "user_stories/corrected.rs"]
mod corrected;
#[path = "user_stories/counting.rs"]
mod counting;
#[path = "user_stories/curveball.rs"]
mod curveball;
#[path = "user_stories/degraded.rs"]
mod degraded;
#[path = "user_stories/dsl.rs"]
mod dsl;
/// One thing filed twice, put back together — and what a repair costs.
#[path = "user_stories/duplicates.rs"]
mod duplicates;
#[path = "user_stories/entitlements.rs"]
mod entitlements;
#[path = "user_stories/gigs.rs"]
mod gigs;
#[path = "user_stories/handover.rs"]
mod handover;
#[path = "user_stories/handshake.rs"]
mod handshake;
#[path = "user_stories/investigating.rs"]
mod investigating;
/// The three loops a person actually keeps, and the one with no cadence.
#[path = "user_stories/keeping_up.rs"]
mod keeping_up;

#[path = "user_stories/kitchensink.rs"]
mod kitchensink;
#[path = "user_stories/mentioning.rs"]
mod mentioning;
#[path = "user_stories/moving.rs"]
mod moving;
#[path = "user_stories/party.rs"]
mod party;
#[path = "user_stories/pets.rs"]
mod pets;
/// A server acting out a day, and the day it fills in when nobody types one.
#[path = "user_stories/pretending.rs"]
mod pretending;
#[path = "user_stories/receipts.rs"]
mod receipts;
#[path = "user_stories/rhythms.rs"]
mod rhythms;
/// One charter, and a session never learns which half the build supplied.
#[path = "user_stories/shipping.rs"]
mod shipping;
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
/// One claim, two runs, two zones — and both answers are right.
#[path = "user_stories/timezones.rs"]
mod timezones;

#[path = "user_stories/typing.rs"]
mod typing;
#[path = "user_stories/unprompted.rs"]
mod unprompted;
#[path = "user_stories/unsourced.rs"]
mod unsourced;
#[path = "user_stories/unsure.rs"]
mod unsure;
/// An upgrade takes back the rows the binary owns and no longer ships.
#[path = "user_stories/upgrading.rs"]
mod upgrading;
/// A question you ask by name, whether the software shipped it or you did.
#[path = "user_stories/views.rs"]
mod views;
#[path = "user_stories/vocabulary.rs"]
mod vocabulary;
