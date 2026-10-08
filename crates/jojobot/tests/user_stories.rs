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
/// "Is this value still good?" — the day the store learned a thing, and the day
/// of a value older than the thing.
#[path = "user_stories/age_of_a_value.rs"]
mod age_of_a_value;
/// How big an answer is allowed to get, and what a list does at the edge.
#[path = "user_stories/answer_ceiling.rs"]
mod answer_ceiling;
#[path = "user_stories/archiving.rs"]
mod archiving;
#[path = "user_stories/around_a_day.rs"]
mod around_a_day;
/// A run that is not happening now, and the day it says it is in.
#[path = "user_stories/backdated.rs"]
mod backdated;
#[path = "user_stories/bikes.rs"]
mod bikes;
#[path = "user_stories/boot.rs"]
mod boot;
#[path = "user_stories/carried_role.rs"]
mod carried_role;
#[path = "user_stories/catching_up.rs"]
mod catching_up;
#[path = "user_stories/challenge.rs"]
mod challenge;
#[path = "user_stories/chart.rs"]
mod chart;
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
#[path = "user_stories/creating_with_fields.rs"]
mod creating_with_fields;
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
#[path = "user_stories/first_post.rs"]
mod first_post;
/// A call that left a required argument off, and what it is told to send.
#[path = "user_stories/forgotten_arguments.rs"]
mod forgotten_arguments;
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
#[path = "user_stories/labels.rs"]
mod labels;
#[path = "user_stories/mentioning.rs"]
mod mentioning;
#[path = "user_stories/moving.rs"]
mod moving;
#[path = "user_stories/my_past_runs.rs"]
mod my_past_runs;
/// The boot names the operator, and what waits on them is one read away.
#[path = "user_stories/operator.rs"]
mod operator;
/// Any bot names the operator while nobody is named; then only the chart's head.
#[path = "user_stories/operator_key.rs"]
mod operator_key;
/// A bot leaves the operator a message, and no bot can read it back.
#[path = "user_stories/operators_mailbox.rs"]
mod operators_mailbox;
#[path = "user_stories/paragraphs.rs"]
mod paragraphs;
#[path = "user_stories/party.rs"]
mod party;
#[path = "user_stories/pets.rs"]
mod pets;
/// A server acting out a day, and the day it fills in when nobody types one.
#[path = "user_stories/pretending.rs"]
mod pretending;
/// What a project's dates, decisions and questions are written as.
#[path = "user_stories/projects.rs"]
mod projects;
#[path = "user_stories/promises.rs"]
mod promises;
#[path = "user_stories/quarantining.rs"]
mod quarantining;
#[path = "user_stories/quiet_answers.rs"]
mod quiet_answers;
#[path = "user_stories/receipts.rs"]
mod receipts;
#[path = "user_stories/refusal_words.rs"]
mod refusal_words;
#[path = "user_stories/rhythms.rs"]
mod rhythms;
#[path = "user_stories/role_owner.rs"]
mod role_owner;
#[path = "user_stories/rule_seats.rs"]
mod rule_seats;
/// A claim that sets keys on its thing from a bag of its own.
#[path = "user_stories/setting.rs"]
mod setting;
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
#[path = "user_stories/synthesis.rs"]
mod synthesis;
#[path = "user_stories/talkingpast.rs"]
mod talkingpast;
/// Words the operator said are not rewritten in place by a later run.
#[path = "user_stories/testimony.rs"]
mod testimony;
#[path = "user_stories/thought_room.rs"]
mod thought_room;
/// Only a bot above the bots that write into a thread sets its ceiling.
#[path = "user_stories/thread_ceiling.rs"]
mod thread_ceiling;
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
#[path = "user_stories/whats_next.rs"]
mod whats_next;
/// A wrap hands back a code that reopens the run for one last change.
#[path = "user_stories/wrap_code.rs"]
mod wrap_code;
