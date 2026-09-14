//! `retract` — Take back a record: one way, never reversed, and nothing is removed.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;
use crate::teaching::{CLAIMS_DOMAIN, CLAIMS_TEACHING};

/// Arguments to `retract`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RetractArgs {
    /// The record's global address, `kind:slug#local-id` — exactly as `recall`
    /// or a search hit returned it.
    pub(crate) address: String,
    /// Why it is being taken back, in one line. Optional — worth giving: a
    /// record marked taken-back with no account of why is hard for a later
    /// reader to tell from damage. Left out, the retraction says plainly that
    /// no reason was given rather than inventing one.
    #[serde(default)]
    pub(crate) reason: Option<String>,
    /// **The day the record was taken back**, `YYYY-MM-DD`. Defaults to today
    /// in your session's zone.
    ///
    /// A retraction leaves a record of its own, and this is the day THAT
    /// record was made. **The day an operator changed their mind is the fact a
    /// later reader most wants about a retraction**, and it is not always the
    /// day the call is made — a session catching up on last week says so here.
    #[serde(default)]
    pub recorded_at: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Take back one addressed record, and record why.
#[tool_router(router = retract_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take back a record — one way, never reversed, and a deliberate act rather \
                       than a flag on an edit. Nothing is removed: the record keeps its address, \
                       its words and its place, and is marked archived; beside it lands a dated \
                       record of the retraction itself, naming what it takes back and the reason \
                       if you give one. The two then read as one story. An archived record is \
                       out of every default read and out of \
                       every later edit, INCLUDING a status flip back — there is no un-retract, \
                       so if you are unsure, capture what is so now instead. THIS IS THE MOVE FOR \
                       SOMETHING THAT HAPPENED and turned out not to have. For a claim about what \
                       is true NOW, rewrite its content with update_fact instead — that stays \
                       active, because the negative truth is the truth, and it leaves one current \
                       record where this leaves two. Retracting a retraction comes back status: \
                       blocked: it is the last word on what it takes back. Retracting something \
                       ALREADY archived comes back blocked as well, and reads differently on \
                       purpose: it says the record is archived, because it is — that answer \
                       tells you the state you asked for is the state jojobot holds, not that \
                       nothing happened. An address that names no record comes back blocked too, \
                       with the addresses that do exist."
    )]
    pub(crate) async fn retract(
        &self,
        Parameters(args): Parameters<RetractArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let date = self.dated(args.recorded_at.as_deref(), args.sid.as_deref())?;

        // **A write that landed is never reported as failed** (rule 130): see
        // `capture`'s own note on the same shape.
        let (taken_back, fold_behind) = match self
            .memory
            .retract(&address, args.reason.as_deref(), date)
            .await
        {
            Ok(taken_back) => (taken_back, None),
            Err(MemoryError::FoldBehind {
                landed: Landed::Retraction(taken_back),
                behind,
                ..
            }) => (*taken_back, Some(behind)),
            Err(e) => return memory_declined("retract", e),
        };
        // **What was built on it, said at the moment it is taken back.** That
        // is when the question is asked and it is the moment a caller can still
        // act on the answer — nothing here changes those claims, because what
        // to do about a claim resting on a withdrawn one is a judgement.
        let standing_on = match self.memory.built_on(&address).await {
            Ok(standing_on) => standing_on,
            Err(e) => return memory_declined("retract", e),
        };
        self.beat("retract", &address.to_string(), args.sid.as_deref())
            .await;
        let mut body = serde_json::json!({
            // **Both rows, because both were written.** The mark alone would
            // leave a caller holding a record it could not explain, and the
            // account alone would not prove the mark landed.
            "retracted": fact_json(&taken_back.retracted, date, None),
            "retraction": fact_json(&taken_back.record, date, None),
            // **Empty is the ordinary case and it is still here**, so a caller
            // reads "nothing rests on this" rather than inferring it from a
            // missing key.
            "built_on_this": standing_on
                .iter()
                .map(|fact| serde_json::json!({
                    "address": fact.address().to_string(),
                    "subject": fact.subject.as_str(),
                    "content": fact.content,
                    "status": fact.status.as_token(),
                }))
                .collect::<Vec<_>>(),
        });
        if let Some(behind) = fold_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        crate::answer::note_postcondition(
            &mut body,
            what_a_retraction_left_standing(&address, &standing_on),
        );
        if self.first_contact(CLAIMS_DOMAIN, Some(&caller)).await {
            crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
        }
        json_result(&body)
    }
}

/// **What a caller's own retraction did, and what it did not do.**
///
/// ⚠️ **It does not remove the record.** The claim stays in the store, comes
/// back from a plain read marked as taken back, and keeps the edge it drew —
/// so a caller answering from a walk still arrives at it. Nothing else on this
/// surface says that, and an agent that assumes otherwise reads a withdrawn
/// claim as a standing one.
///
/// The claims resting on it are named because nothing here changes them: what
/// to do about a claim built on a withdrawn one is a judgement, and jojobot
/// makes none.
fn what_a_retraction_left_standing(address: &FactAddress, built_on: &[Fact]) -> String {
    let resting = if built_on.is_empty() {
        String::new()
    } else {
        format!(
            " {} claims were worked out from it and are unchanged; deciding what they are worth \
             now is yours.",
            built_on.len()
        )
    };
    format!(
        "{address} is marked as taken back. It is still stored, a read still returns it, and it \
         still carries the edge it drew, so a walk arriving along that edge still reaches it.\
         {resting}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::recall::FollowArgs;
    use crate::memory::testing::{ensure, recall_args};

    /// **Taking a claim back says what still stands, and the same case proves
    /// the sentence true.**
    ///
    /// ⚠️ **The model an agent arrives with is that a retraction removes the
    /// claim. It does not.** The record stays in the store, comes back from a
    /// plain `recall` marked `archived`, and an edge on it still reaches
    /// whatever it pointed at — now saying so, which
    /// [`a_walk_says_the_claim_behind_a_link_was_taken_back`] is about.
    ///
    /// **The line and the behaviour are asserted together on purpose.** A
    /// postcondition is prose the caller cannot check, so a case that pinned
    /// only the wording would keep passing on the day the behaviour changed
    /// underneath it — which is the one failure that makes this line worse
    /// than none.
    /// 🚨 **A walk says when the claim behind a link was taken back, through
    /// the surface a caller actually holds.**
    ///
    /// The resolver computes the marker and the shared contract proves it
    /// survives the store. **Neither says a caller can see it.** A walk is
    /// where an agent meets a retracted claim without ever reading the record,
    /// so the rendering is the half that decides whether the capability
    /// exists — and a build that computed the marker and dropped it on the
    /// wire would pass both of the others.
    ///
    /// **Marked, never filtered**, and both halves in one read: the standing
    /// claim's link carries nothing, the withdrawn one's says so. The negative
    /// alone would pass on a build where no link is ever marked.
    #[tokio::test]
    async fn a_walk_says_the_claim_behind_a_link_was_taken_back() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "event:birthday-party").await;
        ensure(&jojobot, "person:beta").await;
        let attending = |who: &str, said: &str| CaptureArgs {
            sid: Some(sid.clone()),
            shape: Some("attendance".into()),
            object: Some("event:birthday-party".into()),
            ..capture_args(who, said)
        };
        capture_ok(&jojobot, attending("person:alpha", "was at the party")).await;
        let withdrawn = capture_ok(&jojobot, attending("person:beta", "was at the party")).await;
        jojobot
            .retract(Parameters(RetractArgs {
                address: address_of(&withdrawn),
                reason: Some("was somewhere else that day".into()),
                sid: Some(sid.clone()),
                recorded_at: None,
            }))
            .await
            .expect("the retraction lands");

        let walked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    follow: Some(FollowArgs {
                        shape: Some("attendance".into()),
                        relation: None,
                        direction: Some("in".into()),
                        depth: None,
                        keeping: None,
                        fits_type: None,
                    }),
                    sid: Some(sid.clone()),
                    ..recall_args("event:birthday-party")
                }))
                .await
                .expect("a walk from the party"),
        );
        let via = |handle: &str| {
            walked["objects"][0]["connected"]
                .as_array()
                .unwrap_or_else(|| panic!("the walk reached its guests: {walked}"))
                .iter()
                .find(|o| o["id"] == handle)
                .unwrap_or_else(|| panic!("{handle} was not reached at all: {walked}"))["via"]
                .clone()
        };
        assert!(
            via("person:alpha").get("retracted").is_none(),
            "the claim that stands draws a link nothing marks: {walked}",
        );
        assert!(
            via("person:beta").get("retracted").is_some(),
            "the withdrawn claim is still reached, and its link says so on the wire: {walked}",
        );
    }

    #[tokio::test]
    async fn a_retraction_says_what_still_stands_and_the_store_agrees() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "place:shelbyville").await;
        let claim = capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                shape: Some("location".into()),
                object: Some("place:shelbyville".into()),
                ..capture_args("person:alpha", "was living in Shelbyville that spring")
            },
        )
        .await;
        let address = address_of(&claim);

        let body = json_of(
            &jojobot
                .retract(Parameters(RetractArgs {
                    address: address.clone(),
                    reason: Some("he was never there".into()),
                    sid: Some(sid.clone()),
                    recorded_at: None,
                }))
                .await
                .expect("the retraction lands"),
        );
        let line = body["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {body}"))
            .to_string();
        assert!(
            line.contains(&address),
            "the line has to name the record this took back: {body}",
        );

        // ⭐ **The half a constant cannot produce.** Nothing was worked out
        // from that claim, so the line says nothing about derived claims —
        // while a retraction that DOES have claims resting on it names how
        // many, because nothing here changes them and the caller has to decide
        // what they are worth. One sentence for both would tell the first
        // caller about work that does not exist.
        let source = capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("person:alpha", "the ferry moved to the north pier")
            },
        )
        .await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                derived_from: Some(address_of(&source)),
                ..capture_args("person:alpha", "so the crossing is longer")
            },
        )
        .await;
        let rested_on = json_of(
            &jojobot
                .retract(Parameters(RetractArgs {
                    address: address_of(&source),
                    reason: Some("the ferry moved back".into()),
                    sid: Some(sid.clone()),
                    recorded_at: None,
                }))
                .await
                .expect("the retraction lands"),
        );
        let with_dependants = rested_on["postcondition"]
            .as_str()
            .expect("a write states what now stands")
            .to_string();
        assert!(
            with_dependants.contains('1'),
            "a claim was worked out from this one and the line does not say so: {rested_on}",
        );
        assert_ne!(
            line, with_dependants,
            "one sentence for both, so the first retraction was told about derived claims that \
             do not exist: {rested_on}",
        );

        // ⭐ **What the line claims, checked against the store in the same
        // case.** The record is still there, still readable, and marked.
        let read_back = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        let kept = read_back["objects"][0]["facts"]
            .as_array()
            .expect("the records on the thing")
            .iter()
            .find(|f| f["address"] == address.as_str())
            .unwrap_or_else(|| panic!("a plain recall still returns it: {read_back}"));
        assert_eq!(
            kept["status"], "archived",
            "it comes back marked rather than gone: {read_back}",
        );
        assert!(
            kept["edge"].is_object(),
            "and it still carries the edge a walk follows, which is the half the line exists to \
             say: {read_back}",
        );
    }

    /// **Taking a claim back says what was built on it.**
    ///
    /// That question is asked at exactly this moment and had no answer at all:
    /// the pointer runs from a claim to its source and nothing could follow it
    /// the other way. **Nothing here changes those claims** — what to do about
    /// a claim resting on a withdrawn one is a judgement, and jojobot makes
    /// none — but a caller cannot make it without being told they exist.
    ///
    /// **Paired with a retraction that rests under nothing**, which must come
    /// back with an empty list rather than with the other claim: without that
    /// half this passes against a build that names every claim in the store.
    #[tokio::test]
    async fn taking_a_claim_back_names_what_was_built_on_it() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let source = capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("person:alpha", "the ferry moved to the north pier")
            },
        )
        .await;
        let unrelated = capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("person:alpha", "the bridge is closed on Sundays")
            },
        )
        .await;
        let built = capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                derived_from: Some(address_of(&source)),
                ..capture_args("person:alpha", "so the crossing is longer")
            },
        )
        .await;

        let taken_back = jojobot
            .retract(Parameters(RetractArgs {
                address: address_of(&source),
                reason: Some("the ferry moved back".into()),
                sid: Some(sid.clone()),
                recorded_at: None,
            }))
            .await
            .expect("the retraction lands");
        let body = json_of(&taken_back);
        assert_eq!(
            body["built_on_this"]
                .as_array()
                .expect("the claims standing on it")
                .iter()
                .map(|claim| claim["address"].as_str().expect("an address"))
                .collect::<Vec<_>>(),
            vec![address_of(&built)],
            "taking a claim back does not say what was built on it: {body}",
        );

        // The other retraction: nothing rests on it, and the answer says so
        // rather than leaving the caller to infer it from a missing key.
        let alone = jojobot
            .retract(Parameters(RetractArgs {
                address: address_of(&unrelated),
                reason: Some("it reopened".into()),
                sid: Some(sid),
                recorded_at: None,
            }))
            .await
            .expect("the retraction lands");
        let body = json_of(&alone);
        assert_eq!(
            body["built_on_this"].as_array().map(Vec::len),
            Some(0),
            "a claim nothing rests on names claims anyway: {body}",
        );
    }

    use crate::memory::testing::*;

    /// Capture a record and hand back its address.
    async fn a_record(jojobot: &Jojobot, content: &str) -> String {
        address_of(&capture_ok(jojobot, capture_args("person:alpha", content)).await)
    }

    fn retract_args(address: &str, reason: &str) -> RetractArgs {
        RetractArgs {
            address: address.to_string(),
            reason: Some(reason.to_string()),
            recorded_at: None,
            sid: Some(crate::harness::TEST_SID.into()),
        }
    }

    /// 🚨 **A retraction happens on a day, and the caller says which.**
    ///
    /// A retraction leaves a dated record of its own, and this verb used to
    /// stamp it with the day the CALL happened — the one write on this surface
    /// whose day was not the caller's to give. A date says when a thing is
    /// TRUE OF, not when somebody typed it, and the day an operator changed
    /// their mind is the fact a later reader most wants about a retraction.
    ///
    /// **Both halves.** A day given is the day carried, and no day given is
    /// still today — without the second, a build that ignored the argument
    /// entirely would satisfy the first.
    #[tokio::test]
    async fn a_retraction_carries_the_day_it_is_given_and_today_when_it_is_not() {
        let jojobot = handler();

        let address = a_record(&jojobot, "the club meets on Tuesdays").await;
        let said = json_of(
            &jojobot
                .retract(Parameters(RetractArgs {
                    recorded_at: Some("2026-07-05".into()),
                    ..retract_args(&address, "it never met on Tuesdays")
                }))
                .await
                .expect("the retraction lands"),
        );
        assert_eq!(
            said["retraction"]["recorded_at"], "2026-07-05",
            "the retraction carries the day the call happened rather than the day it is \
             about: {said}"
        );

        let other = a_record(&jojobot, "the club meets on Wednesdays").await;
        let undated = json_of(
            &jojobot
                .retract(Parameters(retract_args(&other, "it never met then either")))
                .await
                .expect("the retraction lands"),
        );
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            .to_string();
        assert_eq!(
            undated["retraction"]["recorded_at"], today,
            "a retraction given no day stopped being stamped with today: {undated}"
        );
    }

    /// **The whole verb in one pass**: the record stays and is marked, the
    /// reason lands beside it as a record of its own, and both come back.
    #[tokio::test]
    async fn retracting_a_record_marks_it_and_answers_with_both_rows() {
        let jojobot = handler();
        let address = a_record(&jojobot, "moved to the 14th").await;

        let body = json_of(
            &jojobot
                .retract(Parameters(retract_args(&address, "it was rebooked twice")))
                .await
                .expect("retract ok"),
        );
        assert_eq!(body["retracted"]["address"], address.as_str());
        assert_eq!(body["retracted"]["status"], "archived");
        assert_eq!(
            body["retracted"]["content"], "moved to the 14th",
            "marked, not edited"
        );
        assert_eq!(body["retraction"]["content"], "it was rebooked twice");
        assert_eq!(
            body["retraction"]["fields"]["retracts"],
            address.as_str(),
            "the account names what it takes back"
        );

        // **The "out of a default search" half is NOT asserted here**, and the
        // first draft of this test asserted it anyway. This handler has no
        // search index, so `search` returns nothing whatever the state of the
        // record — a `!contains` over it passes for the wrong reason and would
        // go on passing if retraction stopped hiding anything at all. It lives
        // in the search contract instead, where the index is real:
        // `search_excludes_a_retracted_record_by_default`.
    }

    /// **Any record can be taken back**, because there is one class of record.
    ///
    /// The refusal that stood here read the class off a label the writer
    /// chose, so whether a claim could be taken back depended on a word rather
    /// than on the claim. Editing in place is still the usual move for
    /// something that turned out false, and it is a choice the caller makes.
    #[tokio::test]
    async fn retracting_a_plain_record_is_accepted() {
        let jojobot = handler();
        let captured =
            capture_ok(&jojobot, capture_args("person:alpha", "plays the theremin")).await;
        let address = address_of(&captured);

        let body = json_of(
            &jojobot
                .retract(Parameters(retract_args(&address, "turns out not")))
                .await
                .expect("retract ok"),
        );
        assert_ne!(body["status"], "blocked", "{body}");
        assert_eq!(body["retracted"]["address"], address.as_str());
        assert_eq!(body["retracted"]["status"], "archived");

        // …and the mark is on the record a later reader takes.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(crate::harness::TEST_SID.into()),
                    ..recall_args("person:alpha")
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["status"], "archived",
            "{recalled}"
        );
    }

    /// **A retraction is the last word on what it takes back.** Retracting one
    /// would be the reversal the one-way rule exists to forbid, so it is
    /// refused — and the refusal survives the class going, because the marker
    /// is a reserved key on the record rather than a label somebody typed.
    #[tokio::test]
    async fn retracting_a_retraction_is_blocked() {
        let jojobot = handler();
        let address = a_record(&jojobot, "it happened").await;
        let taken_back = json_of(
            &jojobot
                .retract(Parameters(retract_args(&address, "it did not")))
                .await
                .expect("the first retraction lands"),
        );
        let account = taken_back["retraction"]["address"]
            .as_str()
            .expect("the account has an address of its own")
            .to_string();

        let refused = blocked(
            &jojobot
                .retract(Parameters(retract_args(&account, "and neither did that")))
                .await
                .expect("a refusal is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("retraction"),
            "the refusal must say which record it is refusing: {refused}"
        );
    }

    /// **One way, on the surface too.** A second retraction and an edit back to
    /// active are the same wish wearing two faces, and both are refused.
    #[tokio::test]
    async fn a_retracted_record_cannot_be_retracted_again_or_edited_back() {
        let jojobot = handler();
        let address = a_record(&jojobot, "it happened").await;
        jojobot
            .retract(Parameters(retract_args(&address, "it did not")))
            .await
            .expect("the first retraction lands");

        let again = blocked(
            &jojobot
                .retract(Parameters(retract_args(&address, "again")))
                .await
                .expect("an answer"),
        );
        assert!(
            again["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("one-way"),
            "{again}"
        );

        let edited = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    status: Some("active".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(edited["wrote"], false);

        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(crate::harness::TEST_SID.into()),
                    ..recall_args("person:alpha")
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["status"], "archived",
            "neither one moved it: {recalled}"
        );
    }

    /// A malformed address is a protocol error; one that names nothing is a
    /// blocked answer carrying the addresses that do exist — the same split
    /// every addressed verb makes.
    #[tokio::test]
    async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
        let jojobot = handler();
        a_record(&jojobot, "the only record here").await;

        let err = jojobot
            .retract(Parameters(retract_args("not-an-address", "nope")))
            .await
            .expect_err("a string that is no address is a malformed call");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let missed = blocked(
            &jojobot
                .retract(Parameters(retract_args("person:alpha#f99", "nope")))
                .await
                .expect("an address that names nothing is an answer"),
        );
        assert_eq!(missed["attempted"], "person:alpha#f99");
        assert!(
            missed["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("person:alpha#f1"),
            "the addresses that DO exist are what makes this repairable: {missed}"
        );
    }

    /// **The write says it landed, never that it failed, when only the fold
    /// behind it could not confirm it** (rule 130) — `retract`'s own catch of
    /// `MemoryError::FoldBehind`, the same shape `capture`'s own case proves.
    #[tokio::test]
    async fn a_retraction_whose_fold_could_not_refresh_answers_landed_not_failed() {
        let jojobot = Jojobot::new(
            Arc::new(FoldBehindMemory(Arc::new(InMemoryMemory::booted()))),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let address = a_record(&jojobot, "will be taken back").await;

        let body = json_of(
            &jojobot
                .retract(Parameters(retract_args(&address, "it did not happen")))
                .await
                .expect("retract ok"),
        );
        assert_eq!(body["fold"]["behind"], "stale", "{body}");
        // **The positive half.** The retraction still landed, exactly as an
        // ordinary retract does.
        assert_eq!(body["retracted"]["status"], "archived", "{body}");
    }
}
