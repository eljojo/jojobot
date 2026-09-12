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
pub(crate) const RHYTHM_HISTORY_DOMAIN: &str = "rhythm-history";

/// **Ships in the binary, exactly as the other teachings do.** Says what the
/// case is — a loop with a history already behind it — rather than listing
/// the keys a schedule is made of; the rhythms procedure and the engine's own
/// refusals are where those are named.
pub(crate) const RHYTHM_HISTORY_TEACHING: &str = "A rhythm's schedule is jojobot's arithmetic, never a caller's to type in — even for the \
    first cycle. A loop whose last run already happened, before this session opened it, is \
    opened the same way an ordinary cycle is closed: capture a check-in on it, dated the day it \
    last ran, and jojobot works the rest of the schedule out from there.";

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
