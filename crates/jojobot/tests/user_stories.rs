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
/// The claim a retraction, a fold, a restore, a role move or a due clear leaves
/// names the run that made it.
#[path = "user_stories/acting_session.rs"]
mod acting_session;
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
/// A boot names the recovery read for mail delivered and never finished.
#[path = "user_stories/boot_owed_mail.rs"]
mod boot_owed_mail;
#[path = "user_stories/carried_role.rs"]
mod carried_role;
#[path = "user_stories/catching_up.rs"]
mod catching_up;
#[path = "user_stories/chain_of_trust.rs"]
mod chain_of_trust;
#[path = "user_stories/challenge.rs"]
mod challenge;
#[path = "user_stories/chart.rs"]
mod chart;
/// A rule settled by reading it at its address, not by taking somebody's word.
#[path = "user_stories/citing.rs"]
mod citing;
/// A claim must fit one ordinary answer, or it is refused when written.
#[path = "user_stories/claim_room.rs"]
mod claim_room;
#[path = "user_stories/clearing_a_key.rs"]
mod clearing_a_key;
#[path = "user_stories/colleagues.rs"]
mod colleagues;
/// A run that reports its context was compacted is taught and shown afresh and
/// keeps its key.
#[path = "user_stories/compaction.rs"]
mod compaction;
/// The days jojobot computes for a loop can be asked of every kind.
#[path = "user_stories/computed_dates.rs"]
mod computed_dates;
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
/// A key that stopped opening its run says what is in flight.
#[path = "user_stories/dead_handle.rs"]
mod dead_handle;
#[path = "user_stories/degraded.rs"]
mod degraded;
/// One thing filed twice, put back together — and what a repair costs.
/// A markdown file held by section, put whole and read back whole.
#[path = "user_stories/documents.rs"]
mod documents;
#[path = "user_stories/dsl.rs"]
mod dsl;
#[path = "user_stories/duplicates.rs"]
mod duplicates;
#[path = "user_stories/entitlements.rs"]
mod entitlements;
/// A thing's own writes: what it was called before, and which run changed it.
#[path = "user_stories/entity_history.rs"]
mod entity_history;
#[path = "user_stories/first_post.rs"]
mod first_post;
/// A call that left a required argument off, and what it is told to send.
#[path = "user_stories/forgotten_arguments.rs"]
mod forgotten_arguments;
/// A trace asked by a name the thing no longer wears.
#[path = "user_stories/former_handle_history.rs"]
mod former_handle_history;
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
/// What a record pointed at and stood for at each write of it.
#[path = "user_stories/kept_marks.rs"]
mod kept_marks;
/// Each change of state a message went through, and the run that made it.
#[path = "user_stories/message_history.rs"]
mod message_history;
#[path = "user_stories/override_reason.rs"]
mod override_reason;
/// A run taken over by its name is taught and shown afresh; a wrap code is not.
#[path = "user_stories/picked_up_by_name.rs"]
mod picked_up_by_name;
#[path = "user_stories/session_key.rs"]
mod session_key;
/// A key set on a record the software ships is found by that key.
#[path = "user_stories/supplied_search.rs"]
mod supplied_search;
#[path = "user_stories/supplied_sets.rs"]
mod supplied_sets;

/// A bot is told when another identity rewrites its charter.
#[path = "user_stories/charter_changed.rs"]
mod charter_changed;
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
/// A message in the operator's mailbox reaches their phone.
#[path = "user_stories/operators_phone.rs"]
mod operators_phone;
/// A write says what it made owed, in the answer to the write.
#[path = "user_stories/owed_receipt.rs"]
mod owed_receipt;
/// A thing whose schedule cannot be read is still owed, and says why.
#[path = "user_stories/owed_unreadable.rs"]
mod owed_unreadable;
#[path = "user_stories/paragraphs.rs"]
mod paragraphs;
/// An edit names the words to change, and only that span changes.
#[path = "user_stories/partial_edit.rs"]
mod partial_edit;
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
/// A bot that quotes the operator quotes a record.
#[path = "user_stories/quoting_the_operator.rs"]
mod quoting_the_operator;
#[path = "user_stories/ready.rs"]
mod ready;
#[path = "user_stories/receipts.rs"]
mod receipts;
#[path = "user_stories/refusal_words.rs"]
mod refusal_words;
#[path = "user_stories/request_ids.rs"]
mod request_ids;
#[path = "user_stories/rhythms.rs"]
mod rhythms;
#[path = "user_stories/role_owner.rs"]
mod role_owner;
#[path = "user_stories/rule_seats.rs"]
mod rule_seats;
/// What a run changed, and what an entry said before it was corrected.
#[path = "user_stories/run_history.rs"]
mod run_history;
/// A write is never stamped before the run that made it.
#[path = "user_stories/run_start.rs"]
mod run_start;
/// Search keeps the archive below the live claims, like a mail client.
#[path = "user_stories/search_like_a_mail_client.rs"]
mod search_like_a_mail_client;
/// A claim that sets keys on its thing from a bag of its own.
#[path = "user_stories/setting.rs"]
mod setting;
/// An empty search names what shares a word with it.
#[path = "user_stories/shared_word.rs"]
mod shared_word;
/// One charter, and a session never learns which half the build supplied.
#[path = "user_stories/shipping.rs"]
mod shipping;
/// The skills' worked calls, sent as printed.
#[path = "user_stories/skill_calls.rs"]
mod skill_calls;
#[path = "user_stories/sourcing.rs"]
mod sourcing;
#[path = "user_stories/spotlight.rs"]
mod spotlight;
/// An edit made against a revision the claim has moved past is refused.
#[path = "user_stories/stale_edit.rs"]
mod stale_edit;
#[path = "user_stories/stale_handle.rs"]
mod stale_handle;
#[path = "user_stories/statusbar.rs"]
mod statusbar;
#[path = "user_stories/superseding.rs"]
mod superseding;
#[path = "user_stories/synthesis.rs"]
mod synthesis;
#[path = "user_stories/talkingpast.rs"]
mod talkingpast;
/// Words the operator said are not rewritten in place by a later run.
#[path = "user_stories/testimony.rs"]
mod testimony;
/// What a testimony claim says includes its details.
#[path = "user_stories/testimony_details.rs"]
mod testimony_details;
#[path = "user_stories/thought_room.rs"]
mod thought_room;
/// Only a bot above the bots that write into a thread sets its ceiling.
#[path = "user_stories/thread_ceiling.rs"]
mod thread_ceiling;
/// A thread puts its standing decisions before the research around them.
#[path = "user_stories/thread_decisions.rs"]
mod thread_decisions;
/// One claim, two runs, two zones — and both answers are right.
#[path = "user_stories/timezones.rs"]
mod timezones;
/// A claim reads its settings and its own labels apart; one call moves a label.
#[path = "user_stories/two_bags.rs"]
mod two_bags;

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
/// Which things, said in one line: `where`, its scope, its order and its refusals.
#[path = "user_stories/where_refusals.rs"]
mod where_refusals;
#[path = "user_stories/which_things.rs"]
mod which_things;
/// A wrap hands back a code that reopens the run for one last change.
#[path = "user_stories/wrap_code.rs"]
mod wrap_code;
