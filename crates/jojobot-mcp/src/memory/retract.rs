//! `retract` — Take back an event: one way, never reversed, and nothing is removed.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `retract`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RetractArgs {
    /// The event's global address, `kind:slug#local-id` — exactly as `recall`
    /// or a search hit returned it.
    pub address: String,
    /// Why it is being taken back, in one line. Optional — worth giving: a
    /// record marked taken-back with no account of why is hard for a later
    /// reader to tell from damage. Left out, the retraction says plainly that
    /// no reason was given rather than inventing one.
    #[serde(default)]
    pub reason: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Take back one addressed event, and record why.
#[tool_router(router = retract_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take back an EVENT — one way, never reversed, and a deliberate act rather \
                       than a flag on an edit. Nothing is removed: the record keeps its address, \
                       its words and its place, and is marked retracted; beside it lands a dated \
                       record of the retraction itself, naming what it takes back and the reason \
                       if you give one. The two then read as one story. A retracted record is \
                       out of every default read and out of \
                       every later edit, INCLUDING a status flip back — there is no un-retract, \
                       so if you are unsure, capture what is so now instead. THIS IS FOR \
                       CHRONOLOGY ONLY. A fact is current truth and gets FIXED: to correct one, \
                       or to say a claim turned out false, rewrite its content with update_fact \
                       — that stays active, because the negative truth is the truth. Retracting \
                       a fact, retracting something already retracted, or retracting a \
                       retraction all come back status: blocked, saying which it is and what to \
                       do instead. An address that names no record comes back blocked too, with \
                       the addresses that do exist."
    )]
    pub(crate) async fn retract(
        &self,
        Parameters(args): Parameters<RetractArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let date = parse_date(None)?;

        let taken_back = match self
            .memory
            .retract(&address, args.reason.as_deref(), date)
            .await
        {
            Ok(taken_back) => taken_back,
            Err(e) => return memory_declined("retract", e),
        };
        self.beat("retract", &address.to_string(), args.sid.as_deref())
            .await;
        json_result(&serde_json::json!({
            // **Both rows, because both were written.** The mark alone would
            // leave a caller holding a record it could not explain, and the
            // account alone would not prove the mark landed.
            "retracted": fact_json(&taken_back.retracted),
            "retraction": fact_json(&taken_back.record),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// Capture an event and hand back its address.
    async fn an_event(jojobot: &Jojobot, content: &str) -> String {
        let captured = capture_ok(
            jojobot,
            CaptureArgs {
                event_type: Some("an-appointment".into()),
                ..capture_args("person:alpha", content)
            },
        )
        .await;
        address_of(&captured)
    }

    fn retract_args(address: &str, reason: &str) -> RetractArgs {
        RetractArgs {
            address: address.to_string(),
            reason: Some(reason.to_string()),
            sid: None,
        }
    }

    /// **The whole verb in one pass**: the record stays and is marked, the
    /// reason lands beside it as a record of its own, and both come back.
    #[tokio::test]
    async fn retracting_an_event_marks_it_and_answers_with_both_rows() {
        let jojobot = handler();
        let address = an_event(&jojobot, "moved to the 14th").await;

        let body = json_of(
            &jojobot
                .retract(Parameters(retract_args(&address, "it was rebooked twice")))
                .await
                .expect("retract ok"),
        );
        assert_eq!(body["retracted"]["address"], address.as_str());
        assert_eq!(body["retracted"]["status"], "retracted");
        assert_eq!(
            body["retracted"]["content"], "moved to the 14th",
            "marked, not edited"
        );
        assert_eq!(body["retraction"]["content"], "it was rebooked twice");
        assert_eq!(body["retraction"]["event"]["type"], "retraction");
        assert_eq!(
            body["retraction"]["event"]["metadata"]["retracts"],
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

    /// **A fact is fixed, not retracted**, and the refusal has to name the way
    /// forward — a caller that only hears "no" tries the same call again.
    #[tokio::test]
    async fn retracting_a_fact_is_blocked_and_says_to_edit_it_instead() {
        let jojobot = handler();
        let captured =
            capture_ok(&jojobot, capture_args("person:alpha", "plays the theremin")).await;

        let body = blocked(
            &jojobot
                .retract(Parameters(retract_args(
                    &address_of(&captured),
                    "turns out not",
                )))
                .await
                .expect("a refusal is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false);
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("update_fact"),
            "the refusal must name what to do instead: {advice}"
        );

        // Untouched: still active, still the current truth.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: "person:alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(recalled["facts"][0]["status"], "active");
    }

    /// **One way, on the surface too.** A second retraction and an edit back to
    /// active are the same wish wearing two faces, and both are refused.
    #[tokio::test]
    async fn a_retracted_record_cannot_be_retracted_again_or_edited_back() {
        let jojobot = handler();
        let address = an_event(&jojobot, "it happened").await;
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
                    subject: "person:alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["facts"][0]["status"], "retracted",
            "neither one moved it: {recalled}"
        );
    }

    /// A malformed address is a protocol error; one that names nothing is a
    /// blocked answer carrying the addresses that do exist — the same split
    /// every addressed verb makes.
    #[tokio::test]
    async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
        let jojobot = handler();
        an_event(&jojobot, "the only record here").await;

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
}
