//! **Teaching a session, once, the first time it touches a domain.**
//!
//! The problem: a rule an agent needs is sometimes true only at the moment it
//! decides whether to write — "a further claim does not destroy the one
//! already there" has to reach a session before it decides not to write a
//! second, contradicting claim. A footer on every read reaches that session
//! too, but reaches every other one forever. A boot-time essay never reaches
//! a session already mid-run. What is wanted is once, on the call that
//! actually raised the question — see [`jojobot_domain::teaching`].
//!
//! **The mechanism is generic; the content is the caller's.** `first_contact`
//! knows nothing about domains or wording — a caller names a domain string
//! and gets back whether this is the first time. Composing that into the
//! answer, and deciding WHAT counts as "touching" a domain, is each verb's
//! own call: a search that matched nothing never touched claims; a search
//! that surfaced a fact did.

use super::*;

/// The first domain this slice teaches. A caller-declared string, not a
/// compiled set — a second domain is a second constant beside this one, not
/// a variant added here.
pub(crate) const CLAIMS_DOMAIN: &str = "claims";

/// **Ships in the binary, composed into the answer — never stored as a
/// row.** An upgrade improves the wording for every instance with nothing to
/// migrate.
pub(crate) const CLAIMS_TEACHING: &str = "A further claim does not destroy the one already \
    there — capture a second claim about the same thing and it does not erase the first, even \
    when the two contradict each other. jojobot runs no inference and settles nothing, so two \
    accounts that disagree are both allowed to stand: recording the new one is not a judgment \
    that it is the true one, and deciding between them was never the job. Reach for capture when \
    a new thing happened; reach for update_fact only to correct what the record already says — \
    it rewrites a claim in place, and recall with a history argument reads the earlier wording \
    back.\n\n\
    Three corrections, three different moves. Mistyped it just now? update_fact in place — the \
    record becomes what it should have said, and the wrong wording stays readable through \
    history. It changed, or it was never true at all? Archive the old claim with update_fact \
    (status: archived, details saying why) and capture the new one, naming the archived claim as \
    the new one's derived_from when there is a direct replacement. Archive it, never negate it: \
    rewriting a claim into its own denial — 'the club does NOT meet on Tuesdays' replacing 'the \
    club meets on Tuesdays' — leaves a sentence about what is not so where a claim about what is \
    true belongs, and the archived claim already says it stopped standing. A negative is \
    still an ordinary fact when it is not correcting anything: 'he did not attend' stands on its \
    own.";

/// **The second domain — a convention, not a rule about claims themselves.**
/// A different string from [`CLAIMS_DOMAIN`], so a session already taught one
/// has not been taught the other: they are independent rows on the same
/// ledger, and the mechanism did not have to change to add this one.
pub(crate) const CLAIM_SUBJECT_DOMAIN: &str = "claim-subject";

/// **Ships in the binary, exactly as [`CLAIMS_TEACHING`] does.** No column,
/// no migration, no verb: `fields` already takes any key a caller writes, so
/// what was missing was agreement on the key, not a place to hold it.
///
/// **`purpose`'s values are open, deliberately.** No fixed list ships beside
/// it and none is named here — a closed set is a later move, made only if a
/// vocabulary actually stabilizes from use, not proposed in advance. The
/// loop is the design: this teaching names the key and says what it is for,
/// `answers_type`/`fits_type` already read back whatever values are in use,
/// and the convention settles by use rather than by enforcement.
pub(crate) const CLAIM_SUBJECT_TEACHING: &str = "A claim's fields may carry two keys nothing \
    enforces. `subject` is a one-line label, the way an email has a subject. `purpose` says \
    what the claim is FOR — its value is open, read back from whatever is already in use rather \
    than chosen from a fixed list, so it settles by use rather than by a list somebody \
    maintains. Both are written by whoever makes the claim. Together they let a later read \
    tell what a claim is about and why it was written without opening it, and keep two \
    sessions from inventing two different names for the same idea.";

/// **The third domain — a rhythm that already has a history when it is
/// made.** `add_entity` takes no `fields`, so nothing about a schedule can be
/// sent through it: the mistake this teaches against happens on the `capture`
/// that follows, once the caller reaches for the only keys it can see and
/// types them by hand. `capture` on a subject that does not exist is refused
/// (`Blocked::MustExist`), so the entity always precedes that capture — this
/// domain is taught at `add_entity` instead, which is a teaching landing
/// before the mistake rather than after it.
///
/// **Shared with the check-in-date question, deliberately.** A rhythm
/// created with history behind it and a check-in whose `happened_at`
/// disagrees with `recorded_at` used to be two domains stating the same
/// underlying fact — that a check-in's own `recorded_at` is what a
/// schedule counts from — one implicitly, one by field name. A session
/// that creates a loop and then backdates its first check-in with the
/// wrong field meets both, and telling it twice is the defect this domain
/// now closes by construction: whichever site reaches the session first
/// spends the one row, and the other notes nothing.
pub(crate) const RHYTHM_HISTORY_DOMAIN: &str = "rhythm-history";

/// **Ships in the binary, exactly as the other teachings do.** The fuller
/// of the domain's two texts, for the site a caller usually meets first:
/// creating the rhythm, before any check-in has been sent. States the
/// opening mechanism AND which field dates it, so a caller who reads this
/// has nothing left to get wrong on the check-in that follows.
pub(crate) const RHYTHM_HISTORY_TEACHING: &str = "A rhythm's schedule is jojobot's arithmetic, never a caller's to type in — even for the \
    first cycle. A loop whose last run already happened, before this session opened it, is \
    opened the same way an ordinary cycle is closed: capture a check-in on it, and jojobot works \
    the rest of the schedule out from there. The day that check-in dates the schedule from is \
    recorded_at, never happened_at — send the day the turn actually happened as recorded_at, and \
    jojobot derives the rest.";

/// **The fourth domain — which entity a claim with an edge hangs on.** Named
/// on the call that actually raises the question: a `capture` drawing an
/// edge, never one that does not, because a claim naming no other entity has
/// no side to get wrong.
pub(crate) const CLAIM_DIRECTION_DOMAIN: &str = "claim-direction";

/// **Ships in the binary, exactly as the other teachings do.** The two
/// entities an edge touches are not interchangeable: one carries the claim's
/// content, the other is named by the edge, and nothing about the verb
/// stops a caller filing either side as the subject.
pub(crate) const CLAIM_DIRECTION_TEACHING: &str = "A claim hangs on the thing it is ABOUT; the \
    edge names the other party. Ask which entity the sentence itself describes, and put the \
    claim there — the edge then points at whichever other entity it also involves. Filing it on \
    the wrong side stores a claim that is true and finds nothing: a read scoped to the entity \
    the claim should have been about never sees it, because that read returns what it was \
    asked for and an inbound edge grants the other party nothing.";

/// **The fifth domain — archiving a rhythm's claim does not stop its
/// cadence.** Named on the call that actually raises the question: an
/// update_fact that archives a claim on a rhythm AND the rhythm still shows
/// a due moment afterward. Archiving most things has nothing to do with a
/// schedule, and archiving a rhythm claim that was never carrying an open
/// one leaves nothing standing to warn about — the gate checks the
/// aftermath rather than the act, so neither spends the one teaching.
pub(crate) const RHYTHM_ARCHIVE_DOMAIN: &str = "rhythm-archive";

/// **Ships in the binary, exactly as the other teachings do.** A thing's
/// fields are its writes, the newest write of each key winning, from
/// whichever active record made it — archiving one claim about a rhythm
/// takes only that record's own writes out of the running, and a schedule
/// key last written on a different, still-active record keeps counting.
pub(crate) const RHYTHM_ARCHIVE_TEACHING: &str = "Archiving a claim does not stop a rhythm's \
    cadence. The schedule is fields — cadence_days, counts_from — and a thing's fields are its \
    writes, the newest write of each key winning, from whichever active record made it. \
    Archiving one claim takes only that record's own writes out of the running; a schedule key \
    last written on a different, still-active record keeps counting. To silence it, clear \
    counts_from with update_fact: the rhythm reads NotYetOpened and nothing computes a next due \
    date, while the loop itself, its name and its history stay exactly as they were. Archiving \
    is still the right move when a claim about the rhythm was wrong or is no longer worth \
    keeping — it is simply not what stops the cadence.";

/// **The narrower half of [`RHYTHM_HISTORY_DOMAIN`], for the site a caller
/// reaches when the fuller text never landed first.** A check-in that also
/// sends `happened_at`, and the two disagree, raises the same question the
/// fuller text already answers for whoever met it at rhythm-creation — this
/// is what a session gets when a check-in is its FIRST contact with the
/// domain instead: a resumed session, one that never created the rhythm
/// itself, or a rhythm old enough to predate the fuller text. A check-in
/// with no `happened_at` at all has asked nothing; one where both name the
/// same day already got it right, whichever field the caller believed was
/// doing the work — neither spends this domain's one row.
pub(crate) const CHECK_IN_DATE_TEACHING: &str = "A check-in's schedule is dated by recorded_at, \
    never by happened_at — happened_at never reaches the scheduling engine at all. This check-in \
    named both, for different days: to record a turn that happened on an earlier day, send that \
    day as recorded_at.";

impl Jojobot {
    /// Whether this call is the first time `domain` has reached this
    /// session's handle.
    ///
    /// `false` for an anonymous caller — there is no handle to remember it
    /// against, so there is nobody to teach. A store failure is silent and
    /// answers `false` too, for the reason [`Jojobot::beat`] is silent on
    /// one: a teaching that could not be recorded must not fail the verb it
    /// rides on, and answering `true` on a write that did not land would
    /// teach nothing while believing it had.
    pub(crate) async fn first_contact(
        &self,
        domain: &'static str,
        caller: Option<&Caller>,
    ) -> bool {
        let Some(caller) = caller else {
            return false;
        };
        match self
            .teachings
            .first_contact(&caller.sid, domain, self.clock().now())
            .await
        {
            Ok(first) => first,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    domain,
                    "a teaching could not be recorded — the verb it rides on still succeeded"
                );
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::session::testing::resumed;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::testing::InMemorySessions;
    use jojobot_domain::teaching::testing::InMemoryTeachings;
    use rmcp::handler::server::wrapper::Parameters;

    /// **Both halves in one case.** The first claim a session ever captures
    /// carries the teaching; the same session capturing a second one does
    /// not. Asserted against the constant `capture` actually sent, not a
    /// second copy of the prose — a duplicate would drift the moment the
    /// wording improved.
    #[tokio::test]
    async fn the_first_capture_of_a_session_is_taught_and_the_second_is_not() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIMS_TEACHING)),
            "the first claim this session ever wrote carries the teaching: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "the same session touching claims again is not taught twice: {second}"
        );
    }

    /// **The trigger is an edge, not a capture.** A claim naming no other
    /// entity has no side to get wrong, so the session's first plain capture
    /// must not spend this domain's one teaching. The first capture that
    /// DOES draw an edge is the one that raises the question, whichever
    /// number capture it is — and the same session drawing a second edge is
    /// not taught twice.
    #[tokio::test]
    async fn only_a_capture_that_draws_an_edge_teaches_claim_direction_and_only_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let plain = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            !plain
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "a claim with no edge has no side to get wrong: {plain}"
        );

        let with_edge = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:globex".into()),
                ..capture_args("alpha", "rides with the club")
            },
        )
        .await;
        assert!(
            with_edge["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the first edge this session drew must carry the direction teaching: {with_edge}"
        );

        let second_edge = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:mr-plow".into()),
                ..capture_args("alpha", "also sponsors mr plow")
            },
        )
        .await;
        assert!(
            !second_edge
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the same session drawing a second edge is not taught twice: {second_edge}"
        );
    }

    /// **The same convention, reached through the verb that EDITS a claim.**
    /// `update_fact` can also draw or replace an edge (`shape` with
    /// `object`), and the question is identical: which side does the claim
    /// belong on. The first such edit this session makes teaches it; the
    /// second does not.
    #[tokio::test]
    async fn update_fact_drawing_an_edge_teaches_claim_direction_and_only_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&first);

        let drawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:globex".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the first edge this session drew, through update_fact, must carry the direction \
             teaching: {drawn}"
        );

        let redrawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:mr-plow".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !redrawn
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the same session replacing an edge a second time is not taught twice: {redrawn}"
        );
    }

    /// **The trigger is what THIS write sends, not what the fact ends up
    /// carrying.** Unlike a fresh capture, an edited fact's edge can already
    /// be set before this call runs, by an edit that named neither `shape`
    /// nor `object` and so left it exactly as it was. Gating on the
    /// resulting fact's edge would spend the teaching on a call that never
    /// raised the question; gating on the write itself does not.
    #[tokio::test]
    async fn an_edit_that_leaves_the_edge_alone_does_not_teach_claim_direction() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        // A different session draws the edge, so THIS session's own contact
        // with the domain has not happened yet.
        let edged = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:globex".into()),
                ..capture_args("beta", "rides with the club")
            },
        )
        .await;
        let address = address_of(&edged);

        let content_only = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    content: Some("rides with the club every week".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !content_only
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "the fact already carried an edge, but this edit named neither shape nor object: \
             {content_only}"
        );

        // The domain is still unspent for this session: a genuine
        // edge-drawing write still teaches.
        let drawn = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:mr-plow".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "the content-only edit must not have spent this session's one teaching: {drawn}"
        );
    }

    /// **The one-teaching-per-session property holds ACROSS the two verbs,
    /// not once per verb.** The domain is keyed by session and name alone;
    /// whichever verb a session meets it through first spends it.
    #[tokio::test]
    async fn claim_direction_is_spent_once_across_update_fact_and_capture() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "org:globex").await;
        ensure(&jojobot, "org:mr-plow").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&first);

        let drawn_by_update = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    shape: Some("membership".into()),
                    object: Some("org:globex".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            drawn_by_update["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING)),
            "update_fact must be able to spend the domain as the session's first edge: \
             {drawn_by_update}"
        );

        let drawn_by_capture = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:mr-plow".into()),
                ..capture_args("alpha", "also sponsors mr plow")
            },
        )
        .await;
        assert!(
            !drawn_by_capture
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CLAIM_DIRECTION_TEACHING))),
            "capture must not re-teach a domain update_fact already spent for this session: \
             {drawn_by_capture}"
        );
    }

    /// **The gate is the aftermath, not the act.** A rhythm's schedule is
    /// fields folded from whichever of its records last wrote them, so
    /// archiving one claim about a rhythm leaves a schedule opened on a
    /// DIFFERENT, still-active record exactly as it was — the paid-run
    /// failure this teaching answers. The first such archive this session
    /// makes teaches it; a second does not.
    #[tokio::test]
    async fn archiving_a_rhythm_claim_that_leaves_the_schedule_running_teaches_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "descale", "descale")
            }))
            .await
            .expect("add ok");

        // Opens the schedule, on its own record.
        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("rhythm:descale", "we should descale this regularly")
            },
        )
        .await;

        // A SEPARATE claim on the same rhythm — the one this session archives.
        let aside = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:descale", "the filter is due too"),
        )
        .await;
        let address = address_of(&aside);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    details: Some("no longer relevant".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            archived["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING)),
            "the schedule is still standing on another record, and archiving this one did not \
             touch it: {archived}"
        );

        let aside_two = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:descale", "the seal is worn"),
        )
        .await;
        let address_two = address_of(&aside_two);
        let archived_again = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address_two)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived_again
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "the same session archiving a second claim on the same rhythm is not taught twice: \
             {archived_again}"
        );
    }

    /// **A rhythm that never opened has no schedule to leave standing.**
    /// Archiving its only claim raises no question this teaching answers.
    #[tokio::test]
    async fn archiving_a_rhythm_claim_with_no_open_schedule_does_not_teach() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "water-the-fern", "water the fern")
            }))
            .await
            .expect("add ok");

        let plain = capture_as(
            &jojobot,
            &sid,
            capture_args("rhythm:water-the-fern", "the fern likes a weekly soak"),
        )
        .await;
        let address = address_of(&plain);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "this rhythm never had an open schedule, so nothing was left standing: {archived}"
        );
    }

    /// **Most archivals have nothing to do with a schedule.** Archiving an
    /// ordinary entity's claim never touches the rhythm-archive domain,
    /// whatever else is true of the session.
    #[tokio::test]
    async fn archiving_an_ordinary_entitys_claim_never_teaches_rhythm_archive() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let plain = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        let address = address_of(&plain);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "alpha is a person, not a rhythm: {archived}"
        );
    }

    /// **The kind check is load-bearing, not decorative.** A cadence is
    /// fields, and fields are not restricted to a rhythm — a thing answers
    /// whatever keys it carries whether or not anybody declared a type for
    /// them, and `carriers()` names no kind either. So a non-rhythm thing
    /// holding a due moment is not a hypothetical this product forbids; it
    /// is exactly what the model permits, and the teaching's own words —
    /// "a rhythm's cadence" — would be wrong about it.
    #[tokio::test]
    async fn archiving_a_non_rhythm_things_claim_with_a_due_moment_does_not_teach_rhythm_archive() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;

        // Opens a schedule directly on an ordinary thing — not a rhythm.
        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("thing:kettle", "descale it regularly")
            },
        )
        .await;

        // The premise the rest of the case depends on: the due moment is
        // actually there before anything gets archived.
        let opened = json_of(
            &jojobot
                .recall(Parameters(recall_args("thing:kettle")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            opened["objects"][0]["fields"]["due_on"], "2026-08-08",
            "the fixture has to actually carry an open schedule before archiving proves \
             anything: {opened}"
        );

        let aside = capture_as(
            &jojobot,
            &sid,
            capture_args("thing:kettle", "the handle is loose"),
        )
        .await;
        let address = address_of(&aside);

        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    sid: Some(sid.clone()),
                    status: Some("archived".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            !archived
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(RHYTHM_ARCHIVE_TEACHING))),
            "thing:kettle is not a rhythm, and the teaching's own words are about a rhythm's \
             cadence: {archived}"
        );
    }

    /// A rhythm ready to take a check-in, with no basis of its own yet —
    /// the same shape `capture`'s own version of this fixture builds,
    /// inlined here because that one is private to `capture`'s test module.
    async fn a_cadenced_rhythm_ready_for_a_check_in(jojobot: &Jojobot, handle: &str) {
        ensure(jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", handle, handle)
            }))
            .await
            .expect("add ok");
        capture_ok(
            jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("rhythm:{handle}"), "every week")
            },
        )
        .await;
    }

    /// **The gate is disagreement, not presence alone.** A check-in whose
    /// `happened_at` names a day OTHER than the one dating its schedule has
    /// raised the exact question this teaches: `recorded_at`, not
    /// `happened_at`, is what a rhythm's cadence counts from, and the two
    /// having different answers means whichever one the caller trusted is
    /// wrong about what actually happened to the schedule.
    #[tokio::test]
    async fn a_check_in_whose_happened_at_disagrees_with_recorded_at_teaches_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let mistaken = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-10".into()),
                ..capture_args("rhythm:descale", "did it a few days before logging it")
            },
        )
        .await;
        assert!(
            mistaken["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CHECK_IN_DATE_TEACHING)),
            "recorded_at and happened_at name different days on a check-in: {mistaken}"
        );

        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "water-the-fern").await;
        let mistaken_again = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-20".into()),
                happened_at: Some("2026-06-18".into()),
                ..capture_args("rhythm:water-the-fern", "same mix-up, a different loop")
            },
        )
        .await;
        assert!(
            !mistaken_again
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "the same session hitting the same mix-up again is not taught twice: {mistaken_again}"
        );
    }

    /// **A check-in that never mentions `happened_at` has asked nothing.**
    /// Most check-ins log the day they happened through `recorded_at` alone
    /// and never touch the inert field, so this is the common case and it
    /// must not spend the session's one teaching.
    #[tokio::test]
    async fn a_check_in_with_no_happened_at_does_not_teach_check_in_date() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let ordinary = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args("rhythm:descale", "logged it the same day")
            },
        )
        .await;
        assert!(
            !ordinary
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "this check-in never named happened_at, so nothing disagreed with anything: \
             {ordinary}"
        );
    }

    /// **The two fields naming the SAME day already got it right.** Whether
    /// the caller believes `recorded_at` or `happened_at` is doing the
    /// work, the schedule lands on the correct date either way, so there is
    /// nothing to warn about.
    #[tokio::test]
    async fn a_check_in_whose_happened_at_matches_recorded_at_does_not_teach_check_in_date() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        a_cadenced_rhythm_ready_for_a_check_in(&jojobot, "descale").await;

        let agreeing = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-14".into()),
                ..capture_args(
                    "rhythm:descale",
                    "logged it the same day, and said so twice",
                )
            },
        )
        .await;
        assert!(
            !agreeing
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "recorded_at and happened_at name the same day, so the schedule is already right: \
             {agreeing}"
        );
    }

    /// **The case nobody had.** `RHYTHM_HISTORY_TEACHING` (creating a
    /// rhythm) and `CHECK_IN_DATE_TEACHING` (a mismatched check-in) used to
    /// be two domains stating the same underlying fact — that a check-in's
    /// own `recorded_at` is what a rhythm's schedule is built from — one
    /// implicitly, one by field name. A session that creates a loop and
    /// then backdates its first check-in with the wrong field, the exact
    /// workflow this whole day's work was about, met both. They now share
    /// one domain, so this is no longer a property of the wording to keep
    /// in sync by hand: whichever site reaches the session first spends the
    /// row, and the other notes nothing.
    #[tokio::test]
    async fn a_session_that_creates_a_rhythm_then_mis_dates_its_check_in_is_told_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "thing:kettle").await;

        let created = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    parent: Some("thing:kettle".into()),
                    ..add_args("rhythm", "descale", "descale")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            created["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_HISTORY_TEACHING)),
            "creating the rhythm is this session's first contact with the shared domain: {created}"
        );

        capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("rhythm:descale", "every week")
            },
        )
        .await;
        let checked_in = capture_as(
            &jojobot,
            &sid,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                happened_at: Some("2026-06-10".into()),
                ..capture_args("rhythm:descale", "did it a few days before logging it")
            },
        )
        .await;
        assert!(
            !checked_in
                .get("teaching")
                .and_then(|t| t.as_array())
                .is_some_and(|t| t.contains(&serde_json::json!(CHECK_IN_DATE_TEACHING))),
            "rhythm-history already told this session the date basis — the mismatched check-in \
             must not tell it again: {checked_in}"
        );
    }

    /// **The trigger is a claim reaching the session, not the call.** A
    /// search that matches nothing has not touched the domain, so it must
    /// not spend the session's one teaching on an empty answer.
    #[tokio::test]
    async fn a_search_with_no_fact_hit_does_not_teach() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let empty = jojobot
            .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                query: Some("nothing here matches anything".into()),
                sid: Some(sid.clone()),
                ..search_args()
            }))
            .await
            .expect("search ok");
        let empty = json_of(&empty);
        assert_eq!(empty["count"], 0, "nothing matched: {empty}");
        assert!(
            empty.get("teaching").is_none(),
            "an empty search never touched claims: {empty}"
        );
    }

    /// **A search that surfaces a claim teaches, once.** The same session's
    /// next search — even one that surfaces the SAME claim again — does not.
    #[tokio::test]
    async fn a_search_that_surfaces_a_fact_teaches_once() {
        use jojobot_domain::memory::search::Hit;
        use jojobot_domain::memory::{Fact, FactId, FactStatus, Provenance, Standing};

        let fact = Fact {
            id: FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "plays go".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        };
        let hit = Hit::Fact {
            fact: Box::new(fact),
            subject: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            home: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            source: None,
        };
        let spy = Arc::new(SpySearch::answering(vec![hit]));
        let jojobot = handler_with(spy);
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                    query: Some("plays go".into()),
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            first["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first search surfacing a claim carries the teaching: {first}"
        );

        let second = json_of(
            &jojobot
                .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                    query: Some("plays go".into()),
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert!(
            second.get("teaching").is_none(),
            "the same session surfacing a claim again is not taught twice: {second}"
        );
    }

    /// **`recall` teaches the same way `search` does**: the trigger is
    /// facts coming back, not the call. Asking for a subject with no facts
    /// requested does not teach; asking with `facts: true` and getting one
    /// back does, once.
    #[tokio::test]
    async fn recall_teaches_when_facts_come_back_and_only_once() {
        // **A fact seeded straight through the store**, not through
        // `capture` — the MCP verb would consume the session's one teaching
        // itself, leaving nothing for this case to observe.
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                EntityId::person("person:alpha"),
                "plays go",
                jiff::civil::date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let without_facts = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                facts: None,
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let without_facts = json_of(&without_facts);
        assert!(
            without_facts.get("teaching").is_none(),
            "a recall that did not ask for facts never touched claims: {without_facts}"
        );

        let with_facts = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let with_facts = json_of(&with_facts);
        assert_eq!(
            with_facts["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first recall of a claim carries the teaching: {with_facts}"
        );

        let again = jojobot
            .recall(rmcp::handler::server::wrapper::Parameters(RecallArgs {
                sid: Some(sid.clone()),
                ..recall_args("person:alpha")
            }))
            .await
            .expect("recall ok");
        let again = json_of(&again);
        assert!(
            again.get("teaching").is_none(),
            "the same session recalling a claim again is not taught twice: {again}"
        );
    }

    /// **A resumed session inherits what its predecessor was taught** — free,
    /// because a resume answers back the SAME handle rather than minting a
    /// new one, and the ledger is keyed on the handle.
    #[tokio::test]
    async fn a_resumed_session_does_not_receive_the_teaching_twice() {
        let memory = Arc::new(InMemoryMemory::booted());
        let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let sessions = Arc::new(InMemorySessions::new());
        let teachings = Arc::new(InMemoryTeachings::new());
        let registry = Arc::new(sid::SessionRegistry::new());

        let first = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            mailboxes.clone(),
            sessions.clone(),
            teachings.clone(),
            registry.clone(),
        );
        make_bot(&first, "gamma").await;
        let opened = booted(&first, "gamma").await;
        let taught = capture_as(&first, &opened, capture_args("alpha", "plays go")).await;
        assert!(
            taught["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIMS_TEACHING)),
            "taught once: {taught}"
        );

        // A second connection over the same stores and the same registry —
        // what a reconnect is. `resumed` answers the choice this boot is
        // offered rather than starting a fresh session.
        let second = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            mailboxes,
            sessions,
            teachings,
            registry,
        );
        let picked_up = resumed(&second, "gamma").await;
        assert_eq!(picked_up, opened, "a reconnect resumes the same handle");

        let again = capture_as(
            &second,
            &picked_up,
            capture_args("alpha", "and checkers too"),
        )
        .await;
        assert!(
            again.get("teaching").is_none(),
            "a resumed session does not receive the teaching twice: {again}"
        );
    }

    /// **A second domain, with no change to the mechanism** — the same
    /// method, a different string. This is what proves it generalized;
    /// asserting that it is general would prove nothing.
    #[tokio::test]
    async fn a_second_domain_generalizes_with_no_change_to_the_mechanism() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        let caller = jojobot
            .caller(Some(sid.as_str()))
            .expect("a readable handle")
            .expect("a booted caller");

        const A_SECOND_DOMAIN: &str = "a-second-domain";
        assert!(
            jojobot.first_contact(A_SECOND_DOMAIN, Some(&caller)).await,
            "a domain nobody has declared before is a first contact"
        );
        assert!(
            !jojobot.first_contact(A_SECOND_DOMAIN, Some(&caller)).await,
            "the same session touching it again is not"
        );
        assert!(
            jojobot.first_contact(CLAIMS_DOMAIN, Some(&caller)).await,
            "a DIFFERENT domain, same session, is untouched by the first"
        );
    }

    /// **An anonymous caller is never taught** — there is no handle to
    /// remember it against, so there is nobody to teach.
    #[tokio::test]
    async fn an_anonymous_caller_is_never_taught() {
        let jojobot = handler();
        assert!(!jojobot.first_contact(CLAIMS_DOMAIN, None).await);
    }

    /// **The second key names itself, and names no value.** Pinned on the
    /// identifier a caller would need to spell correctly, never on the
    /// surrounding prose — and the operator's rejected fixed vocabulary
    /// (user/feedback/project/reference) must never appear here: naming an
    /// example is the first step toward a list somebody then has to
    /// maintain.
    #[test]
    fn the_purpose_key_is_named_and_no_value_for_it_is() {
        assert!(
            CLAIM_SUBJECT_TEACHING.contains("`purpose`"),
            "the key must be named: {CLAIM_SUBJECT_TEACHING}"
        );
        for rejected in ["user", "feedback", "project", "reference"] {
            assert!(
                !CLAIM_SUBJECT_TEACHING.to_lowercase().contains(rejected),
                "the shipped text must not anchor the open vocabulary on the rejected fixed \
                 set: found {rejected:?} in {CLAIM_SUBJECT_TEACHING}"
            );
        }
    }

    /// **The teaching says recording is correct, not only that it is safe.**
    /// A paid run lost the same phase four times: a sitting met two
    /// contradicting accounts, recalled that a further claim does not erase
    /// the first, and still asked instead of writing — because "does not
    /// destroy" says nothing was destroyed, never that jojobot settles
    /// nothing and both accounts are allowed to stand. Pinned on a
    /// distinctive word, not the sentence around it.
    #[test]
    fn the_claims_teaching_says_a_contradiction_may_stand() {
        assert!(
            CLAIMS_TEACHING.to_lowercase().contains("contradict"),
            "the teaching must say a contradiction is allowed to stand, not only that a write \
             is non-destructive: {CLAIMS_TEACHING}"
        );
    }

    /// **Both halves in one case, for the second domain.** The first capture
    /// carries the subject-convention teaching; the same session capturing
    /// again does not.
    #[tokio::test]
    async fn the_first_capture_teaches_the_subject_convention_and_the_second_does_not() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_SUBJECT_TEACHING)),
            "the first capture carries the subject-convention teaching: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "the same session capturing again is not taught the convention twice: {second}"
        );
    }

    /// ⭐ **The case that makes this a domain rather than a longer paragraph.**
    /// A session taught about claims through `search` has NOT been taught the
    /// subject convention — the two are independent rows on the same ledger,
    /// not one teaching that happens to render in two places.
    #[tokio::test]
    async fn claims_and_the_subject_convention_are_independently_tracked() {
        use jojobot_domain::memory::search::Hit;
        use jojobot_domain::memory::{Fact, FactId, FactStatus, Provenance, Standing};

        let fact = Fact {
            id: FactId("f1".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "plays go".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        };
        let hit = Hit::Fact {
            fact: Box::new(fact),
            subject: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            home: jojobot_domain::memory::search::EntityRef::unresolved(EntityId::person(
                "person:alpha",
            )),
            source: None,
        };
        let spy = Arc::new(SpySearch::answering(vec![hit]));
        let jojobot = handler_with(spy);
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        // `search` touches only the claims domain — it never mentions a
        // subject field — so this session is now taught claims and nothing
        // else.
        let searched = json_of(
            &jojobot
                .search(rmcp::handler::server::wrapper::Parameters(SearchArgs {
                    query: Some("plays go".into()),
                    sid: Some(sid.clone()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            searched["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "search taught claims, and only claims: {searched}"
        );

        // The first capture this session ever makes still owes it the
        // subject-convention teaching, because that domain is untouched —
        // and it must NOT re-teach claims, which search already covered.
        let captured = capture_as(&jojobot, &sid, capture_args("alpha", "plays go too")).await;
        assert_eq!(
            captured["teaching"],
            serde_json::json!([CLAIM_SUBJECT_TEACHING]),
            "capture owes only the domain search never touched: {captured}"
        );
    }

    /// ⭐ **The shape `first_contact`'s check-and-set could get wrong if
    /// delivery clobbered.** A session whose very first call is a capture —
    /// no prior search or recall — touches both domains at once, and the
    /// answer must carry both. A second capture must then carry neither: both
    /// rows are spent by the first answer, not just the one that rendered
    /// last.
    #[tokio::test]
    async fn a_first_ever_capture_teaches_both_domains_and_spends_both() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let first = capture_as(&jojobot, &sid, capture_args("alpha", "plays go")).await;
        assert_eq!(
            first["teaching"],
            serde_json::json!([CLAIMS_TEACHING, CLAIM_SUBJECT_TEACHING]),
            "the first call ever touches both domains at once and both survive: {first}"
        );

        let second = capture_as(&jojobot, &sid, capture_args("alpha", "also plays chess")).await;
        assert!(
            second.get("teaching").is_none(),
            "both domains were spent by the first answer, not just the one that rendered last: \
             {second}"
        );
    }

    /// A fact seeded straight through the store, bypassing `capture` — which
    /// would consume the session's one teaching itself, leaving nothing for
    /// the case under test to observe.
    async fn a_seeded_fact_handler() -> (Jojobot, String) {
        let memory = Arc::new(InMemoryMemory::booted());
        let jojobot = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(InMemoryTeachings::new()),
            seeded_registry(),
        );
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                EntityId::person("person:alpha"),
                "plays go",
                jiff::civil::date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        (jojobot, sid)
    }

    /// **`update_fact` teaches on the same trigger `capture` does: the write
    /// landing.**
    #[tokio::test]
    async fn update_fact_teaches_on_the_first_edit() {
        let (jojobot, sid) = a_seeded_fact_handler().await;

        let edited = jojobot
            .update_fact(rmcp::handler::server::wrapper::Parameters(
                crate::memory::UpdateFactArgs {
                    content: Some("plays go on weekends".into()),
                    sid: Some(sid),
                    ..update_args("person:alpha#f1")
                },
            ))
            .await
            .expect("update ok");
        let edited = json_of(&edited);
        assert_eq!(
            edited["teaching"],
            serde_json::json!([CLAIMS_TEACHING, CLAIM_SUBJECT_TEACHING]),
            "the first edit this session made touches both domains at once, \
             the same as a first capture does: {edited}"
        );
    }

    /// **`retract` teaches on the write landing.**
    #[tokio::test]
    async fn retract_teaches_on_the_first_retraction() {
        let (jojobot, sid) = a_seeded_fact_handler().await;

        let retracted = jojobot
            .retract(rmcp::handler::server::wrapper::Parameters(
                crate::memory::RetractArgs {
                    address: "person:alpha#f1".into(),
                    reason: Some("never happened".into()),
                    recorded_at: None,
                    sid: Some(sid),
                },
            ))
            .await
            .expect("retract ok");
        let retracted = json_of(&retracted);
        assert_eq!(
            retracted["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first retraction this session made carries the teaching: {retracted}"
        );
    }

    /// **`merge_entities` teaches on the write landing.**
    #[tokio::test]
    async fn merge_entities_teaches_on_the_first_merge() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        ensure(&jojobot, "person:alpha").await;
        ensure(&jojobot, "person:milhouse").await;

        let merged = jojobot
            .merge_entities(rmcp::handler::server::wrapper::Parameters(
                crate::memory::merge_entities::MergeArgs {
                    duplicate: "person:milhouse".into(),
                    survivor: "person:alpha".into(),
                    reason: Some("same person".into()),
                    recorded_at: None,
                    sid: Some(sid),
                },
            ))
            .await
            .expect("merge ok");
        let merged = json_of(&merged);
        assert_eq!(
            merged["teaching"],
            serde_json::json!([CLAIMS_TEACHING]),
            "the first merge this session made carries the teaching: {merged}"
        );
    }

    /// **`add_entity` teaches the rhythm-history domain, and only for a
    /// rhythm.** The absence has to be proved next to a build that DOES teach
    /// it — a person carrying no teaching passes identically whether the
    /// mechanism is wired or missing entirely, so the positive on a rhythm
    /// rides in the same test. The third call, a second rhythm in the same
    /// session, proves the ledger rather than a constant that always renders.
    #[tokio::test]
    async fn add_entity_teaches_rhythm_history_only_for_a_rhythm_and_once() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let person = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    ..add_args("person", "alpha", "Alpha")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            person.get("teaching").is_none(),
            "an ordinary entity never touches the rhythm-history domain: {person}"
        );

        let parent = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    sid: Some(sid.clone()),
                    ..add_args("thing", "kettle", "Kettle")
                }))
                .await
                .expect("add ok"),
        );
        assert_ne!(parent["status"], "blocked", "{parent}");

        let first = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    parent: Some("thing:kettle".into()),
                    sid: Some(sid.clone()),
                    ..add_args("rhythm", "descale", "Descale")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            first["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(RHYTHM_HISTORY_TEACHING)),
            "the first rhythm this session creates carries the teaching: {first}"
        );

        let second = json_of(
            &jojobot
                .add_entity(rmcp::handler::server::wrapper::Parameters(AddEntityArgs {
                    parent: Some("thing:kettle".into()),
                    sid: Some(sid),
                    ..add_args("rhythm", "water-the-fern", "Water The Fern")
                }))
                .await
                .expect("add ok"),
        );
        assert!(
            second.get("teaching").is_none(),
            "the same session creating a second rhythm is not taught again: {second}"
        );
    }
}
