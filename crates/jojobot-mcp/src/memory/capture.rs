//! `capture` — Remember one fact about an entity, with its provenance, at
//! most one edge, and — when it was derived from another claim rather than
//! from an entity — the claim it traces to.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `capture`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — any `kind:slug` id (a bare handle is read
    /// as a person). **It must already exist**: a subject jojobot doesn't know
    /// comes back with candidates and nothing is written. Create it with
    /// `add_entity` first if it is genuinely new.
    pub subject: String,
    /// The crisp claim to remember — single line, no line breaks.
    pub content: String,
    /// Nuance, the why, merge notes — the description under the claim.
    #[serde(default)]
    pub details: Option<String>,
    /// `testimony` (the user said it) or `inference` (derived). Defaults to
    /// `inference`: anything not tied to the user's words is a hypothesis.
    #[serde(default)]
    pub provenance: Option<String>,
    /// `settled` or `open` — **how sure anyone is**, which is a different
    /// question from `provenance`'s *who backs it*.
    ///
    /// Leave it off and it follows the provenance: the operator's word is
    /// `settled`, a claim you worked out is `open`. Set it only when the two
    /// come apart — and the case that matters is **the operator saying
    /// something first-hand and hedging it**: that is
    /// `provenance: testimony` with `standing: open`.
    ///
    /// **Declared on honour, exactly as `provenance` is.** Nothing here asks
    /// you to confirm a standing. `update_fact` is where the gate sits, and
    /// only on the one move that takes a claim the operator hedged and calls
    /// it settled.
    #[serde(default)]
    pub standing: Option<String>,
    /// The fact's freshness date, `YYYY-MM-DD`. Defaults to today (UTC).
    #[serde(default)]
    pub date: Option<String>,
    /// The shape of the edge this fact draws: `location` (object is a place) ·
    /// `membership` (an org) · `attendance` (an event) · `about` (any kind) ·
    /// `connection` (any kind — a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone.
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist**,
    /// exactly as `subject` must — an edge into a node nobody else references is
    /// how a cross-entity question quietly starts coming back empty.
    #[serde(default)]
    pub object: Option<String>,
    /// **The claim this one was derived from**, as its address
    /// (`kind:slug#local-id`), when it was derived from another claim rather
    /// than from an entity. An edge's object is an entity; this is not an
    /// edge, because a claim has no entity to point at when what it came from
    /// is itself a claim.
    #[serde(default)]
    pub derived_from: Option<String>,
    /// **The record's fields**, as a flat bag of key/value pairs — anything
    /// worth recording about this claim beyond the sentence.
    ///
    /// Flat and free-form on purpose: jojobot stores what you put here and
    /// interprets none of it, and a key it has never seen is kept exactly as
    /// you wrote it. **Nothing has to be declared first**, and no name for the
    /// class of thing is asked for: the fields ARE what the record says, and a
    /// type is something the keys answer rather than something you announce.
    #[serde(default)]
    pub fields: Option<std::collections::BTreeMap<String, String>>,
    /// The entities this record touches, as `kind:slug` — **each must already
    /// exist**, exactly as `subject` must.
    ///
    /// These are links whose MEANING is deliberately not recorded: the pointer
    /// is real and searchable, and what the connection was is left unsaid
    /// rather than guessed. That is why they are not `about` edges — `about`
    /// asserts the record is about that entity, and this only admits that it
    /// touches it.
    #[serde(default)]
    pub refs: Option<Vec<String>>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Remember a fact about an entity. Returns the stored fact including the
/// address a later `update_fact` can edit it through.
#[tool_router(router = capture_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Remember one fact about an entity: the claim, when it became true, and \
                       whether it is testimony or inference (default inference — a hypothesis, \
                       not a finding). PROVENANCE AND STANDING ARE TWO QUESTIONS: provenance says \
                       WHO BACKS IT, standing says HOW SURE anyone is (settled or open). Leave \
                       standing off and it follows the provenance. Set it when the two come \
                       apart, and the case that matters is the operator stating something \
                       first-hand and hedging it — that is provenance testimony with \
                       standing open, and there is no other way to record it. It may also draw \
                       one typed edge at another entity. \
                       Returns the stored fact with the address you later edit it through. \
                       Every entity it names — the subject, and an edge's object — must \
                       ALREADY EXIST: one jojobot doesn't know comes back status: blocked with \
                       candidates and nothing is written. A genuinely new entity is two \
                       deliberate steps — add_entity, then capture. GIVE IT FIELDS: fields is \
                       a flat bag of key/value pairs jojobot stores and never interprets, and \
                       refs names the entities the record touches — those are links whose \
                       meaning is deliberately unrecorded, so they are searchable but assert \
                       nothing, which is what makes them not `about` edges. NOTHING HAS TO BE \
                       DECLARED FIRST and no name for the class of thing is asked for: a key you \
                       invent is kept as you wrote it, and a type is something the keys answer \
                       rather than something you announce. derived_from names the claim this one \
                       was worked out from, as its address — use it when the source is another \
                       claim, not an entity. IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the \
                       address that edits it, the subject as it was qualified, the date, the \
                       provenance and the standing it was given — which is how a caller that \
                       named none of those learns what was recorded — and how many keys landed, \
                       with your claim elided and said to be. The write is still verified \
                       against the store before it is called a success; what stops is shipping \
                       you the words you just sent. recall the subject to read it back."
    )]
    pub(crate) async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let subject = EntityId::person(&args.subject);
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let date = parse_date(args.date.as_deref())?;
        let edge = match parse_edge(args.shape.as_deref(), args.object.as_deref())? {
            Ok(edge) => edge,
            Err(refused) => return Ok(refused),
        };

        let derived_from = args
            .derived_from
            .as_deref()
            .map(FactAddress::parse)
            .transpose()
            .map_err(memory_error)?;

        let new = NewFact {
            subject,
            content: args.content,
            details: args.details,
            provenance,
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            status: Default::default(),
            date,
            edge,
            fields: args.fields.unwrap_or_default(),
            refs: args
                .refs
                .iter()
                .flatten()
                .map(|r| EntityId::person(r.trim()))
                .collect(),
            derived_from,
        };
        // Routed through the declined path rather than straight to the mapper:
        // a fact the validators refuse is a caller mistake, and it comes back
        // as an answer with a way forward (rule 68).
        let captured = match self.memory.capture(new).await {
            Ok(captured) => captured,
            Err(e) => return memory_declined("capture", e),
        };
        match captured {
            Guarded::Written(fact) => {
                self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
                    .await;
                json_result(&fact_receipt_json(&fact))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("capture"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// **Fields ride on a fact, and no label is asked for.**
    ///
    /// A record's fields ARE the thing it describes, so the write that puts a
    /// key on a record cannot be gated on the writer also naming a class for
    /// it. While it was gated, "the fields of a thing" meant "the fields
    /// somebody opted in", which is a biased sample — and every read that
    /// groups a thing's records computes over that sample.
    /// **A capture is answered with a receipt, not with the record.**
    ///
    /// The content, the details and the fields are what the caller sent in
    /// this call. **What the read-back proved is untouched**: the store is
    /// still read before the write is called a success, so a claim that did
    /// not survive storage is still an error rather than a success with
    /// mangled bytes. The proof does not require shipping the proof.
    ///
    /// **What must survive is everything the caller could not know**, and the
    /// defaulted values are the sharp part: a caller that omitted `provenance`,
    /// `standing` or `date` learns here what was recorded, and there is a case
    /// in this file because `standing` once vanished silently. The address is
    /// the other half — without it the record cannot be edited, and a receipt
    /// that costs a caller the address has broken the verb.
    #[tokio::test]
    async fn a_capture_is_receipted_without_reading_the_record_back() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        let content = "said the kiln was finally lit, after three weeks of not being lit";

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    details: Some("and that the flue was the problem".into()),
                    fields: Some(
                        [("mood".to_string(), "delighted".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("person:alpha", content)
                }))
                .await
                .expect("capture ok"),
        );

        assert_eq!(
            body["content"],
            serde_json::Value::Null,
            "the claim comes back to nobody who just wrote it: {body}"
        );
        assert_eq!(body["content_elided"], true, "{body}");
        assert_eq!(body["content_bytes"], content.len(), "{body}");
        assert_eq!(
            body["details"],
            serde_json::Value::Null,
            "…and the nuance beside it: {body}"
        );
        assert_eq!(
            body["fields_count"], 1,
            "how many keys landed, not which: {body}"
        );

        // The half a receipt may never cost: the address, and everything
        // jojobot decided for a caller that named none of it.
        assert_eq!(body["address"], "person:alpha#f1", "{body}");
        assert_eq!(body["subject"], "person:alpha", "{body}");
        assert_eq!(
            body["provenance"], "inference",
            "a caller that named no provenance learns what was recorded: {body}"
        );
        assert!(
            body["standing"].is_string() && body["status"].is_string(),
            "…and the standing this claim was given: {body}"
        );
        assert!(
            body["date"].is_string(),
            "…and the date it was stamped: {body}"
        );
        assert!(
            body["how_to_read"]
                .as_str()
                .is_some_and(|how| how.contains("recall")),
            "eliding is never silent — the answer names the call that returns it: {body}"
        );
    }

    #[tokio::test]
    async fn a_capture_carries_fields_with_no_label_and_reads_its_keys_back() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    fields: Some(
                        [
                            ("mood".to_string(), "delighted".to_string()),
                            ("weather".to_string(), "clear".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    refs: Some(vec!["person:milhouse".into()]),
                    ..capture_args("person:alpha", "the kiln was finally lit")
                }))
                .await
                .expect("capture ok"),
        );
        assert_ne!(body["status"], "blocked", "no label is asked for: {body}");
        assert_eq!(body["fields_count"], 2, "both keys landed: {body}");
        assert_eq!(body["refs"], serde_json::json!(["person:milhouse"]));

        // …and they are on the record a later reader takes, not only in the
        // answer to the write.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["fields"]["mood"], "delighted",
            "{recalled}"
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["refs"],
            serde_json::json!(["person:milhouse"]),
            "{recalled}"
        );
    }

    /// **A record with no fields carries an empty bag, not a missing key.**
    ///
    /// Carrying none is the ordinary case, so a reader must learn it from the
    /// answer rather than by branching on whether the key is there at all.
    #[tokio::test]
    async fn a_capture_with_no_fields_answers_with_an_empty_bag() {
        let jojobot = handler();
        let body = capture_ok(&jojobot, capture_args("person:alpha", "plays go")).await;
        assert_eq!(
            body["fields_count"], 0,
            "a reader learns there were none from the count, not by branching on a missing \
             key: {body}"
        );
        assert_eq!(body["refs"], serde_json::json!([]), "{body}");
    }

    /// **A ref names an entity, so it must already exist.** The rule is not
    /// about edges, it is about naming: nothing a write mentions is brought
    /// into being as a side effect of mentioning it. A ref that provisioned its
    /// own entity would make the open bag the one place on this surface where
    /// that stopped being true — and the bag takes any key precisely so that
    /// everything else about it stays strict.
    #[tokio::test]
    async fn a_ref_to_an_entity_nobody_created_is_blocked_and_writes_nothing() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    refs: Some(vec!["person:ghost".into()]),
                    ..capture_args("person:alpha", "it happened")
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["attempted"], "person:ghost");
        assert_eq!(body["wrote"], false);

        // …and the fact did not land either: a record is one write, so a ref it
        // could not resolve takes the whole thing with it.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["objects"][0]["facts"]
                .as_array()
                .expect("a list")
                .is_empty(),
            "a blocked record wrote nothing: {recalled}"
        );
    }

    #[tokio::test]
    async fn a_fact_can_be_about_any_kind() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            capture_args("place:north-trail", "swimmable in August"),
        )
        .await;
        assert_eq!(captured["subject"], "place:north-trail");
    }

    /// Capture's subject must exist, near miss or complete stranger, and the
    /// way through is `add_entity` — never an override. The advice must say
    /// `add_entity`: `override_token` is not a parameter on this verb, and
    /// telling the caller to send one would offer a way out that does not
    /// exist.
    #[tokio::test]
    async fn a_blocked_capture_says_to_add_the_entity_first() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let near = jojobot
            .capture(Parameters(capture_args("zenit", "should not land")))
            .await
            .expect("call ok");
        let body = blocked(&near);
        assert_eq!(body["candidates"][0]["handle"], "person:zenith");
        // The near-miss branch has its own copy, and it has to earn its keep: the
        // candidate list is the whole reason this case differs from a stranger,
        // so the advice must point at it rather than repeat the stranger's text.
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("above"),
            "with candidates in hand, the advice must point at them: {advice}"
        );
        assert!(
            advice.contains("add_entity"),
            "…and still name the way through: {advice}"
        );
        assert!(
            !advice.contains("nothing resembles it"),
            "something does resemble it — that is what the candidates are: {advice}"
        );
        assert!(
            !advice.contains("override_token"),
            "capture has no override_token, near miss or not: {advice}"
        );

        // A handle nothing resembles blocks too, with nothing to suggest.
        let stranger = jojobot
            .capture(Parameters(capture_args("work:first-mix", "32 tracks")))
            .await
            .expect("call ok");
        let body = blocked(&stranger);
        assert_eq!(body["attempted"], "work:first-mix");
        assert!(
            body["candidates"].as_array().unwrap().is_empty(),
            "got {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("add_entity"),
            "must name the way through: {advice}"
        );
        assert!(
            !advice.contains("override_token"),
            "capture has no override_token; advising one offers a way out that does \
             not exist: {advice}"
        );
        assert!(
            !advice.contains("above"),
            "there are no candidates above to point at: {advice}"
        );

        // Two deliberate steps, and it lands.
        jojobot
            .add_entity(Parameters(add_args("work", "first-mix", "First Mix")))
            .await
            .expect("add ok");
        let landed = capture_ok(&jojobot, capture_args("work:first-mix", "32 tracks")).await;
        assert_eq!(landed["subject"], "work:first-mix");
    }

    /// `capture` draws a typed edge, and the edge comes back on every read of the
    /// fact — rendered with schema.org's word for the shape (`memberOf`), while
    /// the input token stays the lowercase `membership`.
    #[tokio::test]
    async fn capture_draws_an_edge_and_renders_its_schema_org_name() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:north-trail-club".into()),
                ..capture_args("alpha", "rides with the club")
            },
        )
        .await;
        assert_eq!(captured["edge"]["type"], "memberOf");
        assert_eq!(captured["edge"]["object"], "org:north-trail-club");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["edge"]["type"],
            "memberOf"
        );
    }

    /// The shape set is closed, and the response spellings are not input tokens —
    /// the input grammar stays lowercase.
    #[tokio::test]
    async fn an_unknown_shape_is_a_client_error() {
        let jojobot = handler();
        for shape in ["knows", "memberOf", "Location", "attendee"] {
            let err = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: Some(shape.into()),
                    object: Some("place:north-trail".into()),
                    ..capture_args("alpha", "an unknown shape")
                }))
                .await
                .expect_err("must reject shape {shape}");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {shape}");
            assert!(
                err.message.contains("location"),
                "the error must name the closed set: {}",
                err.message
            );
        }
    }

    /// A shape's object must be the kind it requires — a `location` pointing at a
    /// person is a mis-drawn edge, and the caller hears about it.
    #[tokio::test]
    async fn a_wrong_kind_edge_object_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("person:beta".into()),
                ..capture_args("alpha", "in the wrong kind of place")
            }))
            .await
            .expect_err("a wrong-kind object must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("place"),
            "must say what it wanted: {}",
            err.message
        );
    }

    /// A typo'd edge object comes back as the guard's candidates — the same
    /// error-flagged response a blocked subject gets, and nothing is written.
    #[tokio::test]
    async fn a_blocked_edge_object_returns_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("place", "riverbend", "Riverbend")))
            .await
            .expect("add ok");
        // The subject faces the gate too, and the guard reports the first handle
        // it stops — this spec is about the object.
        ensure(&jojobot, "alpha").await;

        let result = jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("place:riverbnd".into()),
                ..capture_args("alpha", "should not land")
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "place:riverbnd");
        assert_eq!(body["candidates"][0]["handle"], "place:riverbend");
        assert_eq!(body["candidates"][0]["type"], "Place");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["objects"][0]["facts"]
                .as_array()
                .unwrap()
                .is_empty(),
            "a blocked edge object must write no fact: {recalled}"
        );
    }

    /// The end-to-end MCP path: capture through the handler, then recall through
    /// the handler, and the fact comes back.
    #[tokio::test]
    async fn capture_then_recall_through_the_handler() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "drinks oat milk")).await;
        assert_eq!(captured["subject"], "person:alpha");

        let body = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(body["objects"][0]["id"], "person:alpha");
        let facts = body["objects"][0]["facts"]
            .as_array()
            .expect("recall returns a list");
        assert!(
            facts.iter().any(|f| {
                f["address"] == captured["address"] && f["content"] == "drinks oat milk"
            }),
            "recall must return the captured fact: {body}"
        );
    }

    /// Omitting `provenance` defaults to inference (a hypothesis until confirmed).
    #[tokio::test]
    async fn provenance_defaults_to_inference() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "maybe a morning person")).await;
        assert_eq!(captured["provenance"], "inference");
    }

    /// **A `derived_from` naming no claim is blocked, with the addresses that
    /// do exist.**
    ///
    /// The store refuses it; this is the half that says a caller sees a
    /// refusal it can act on rather than an error. The pair is here too,
    /// because a blocked answer proves nothing on a build where the accepted
    /// case never writes the link either.
    #[tokio::test]
    async fn a_derived_from_must_name_a_claim_that_exists() {
        let jojobot = handler();
        let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
        let address = source["address"].as_str().expect("an address").to_string();

        let linked = capture_ok(
            &jojobot,
            CaptureArgs {
                derived_from: Some(address.clone()),
                ..capture_args("alpha", "so the crossing is longer")
            },
        )
        .await;
        assert_eq!(
            linked["derived_from"], address,
            "a link to a claim that exists is written: {linked}"
        );

        let refused = blocked(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    derived_from: Some("person:alpha#f99".into()),
                    ..capture_args("alpha", "and the fare went up")
                }))
                .await
                .expect("a miss is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains(&address)),
            "the addresses that DO exist are what makes it repairable: {refused}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall answers"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"]
                .as_array()
                .expect("facts")
                .len(),
            2,
            "a refused capture wrote nothing: {recalled}"
        );
    }

    /// **The `standing` argument reaches the store, and comes back.**
    ///
    /// Nothing tested this. The domain contract builds `NewFact { standing }`
    /// directly and never touches `parse_standing`; the argument builders only
    /// ever sent `None`; and the story that exercises the field asserts with a
    /// substring over a response holding two facts, so it cannot see the
    /// argument dropped. Setting this verb's `standing` to `None` — the MCP
    /// layer silently discarding what the caller asked for — left the entire
    /// suite green.
    ///
    /// The hedge is the case that matters: `testimony` with `open` is the one
    /// pairing a default cannot produce, so it is the only one that proves the
    /// argument travelled rather than being re-derived at the far end.
    #[tokio::test]
    async fn the_standing_argument_travels_and_reads_back() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                standing: Some("open".into()),
                ..capture_args("alpha", "thinks it shuts early")
            },
        )
        .await;
        // Paired: both halves, because `open` alone is what a default would
        // give an inference and `testimony` alone is what the provenance
        // argument already proves.
        assert_eq!(captured["provenance"], "testimony");
        assert_eq!(captured["standing"], "open");

        // …and it is on the page, not just in the answer.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall answers"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["standing"], "open",
            "{recalled}"
        );
    }

    /// An unknown `standing` is a client error, not a silent default — a
    /// caller who wrote something else meant something, and guessing which of
    /// two values they meant is how a hedge becomes a settled fact.
    #[tokio::test]
    async fn an_unknown_standing_is_a_client_error() {
        let jojobot = handler();
        let refused = jojobot
            .capture(Parameters(CaptureArgs {
                standing: Some("maybe".into()),
                ..capture_args("alpha", "something")
            }))
            .await;
        assert!(refused.is_err(), "an unknown standing must be refused");
    }

    /// Omitting `date` defaults to today in UTC.
    #[tokio::test]
    async fn date_defaults_to_today_utc() {
        let jojobot = handler();
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
        let captured = capture_ok(&jojobot, capture_args("alpha", "dated today")).await;
        assert_eq!(captured["date"], today.to_string());
    }

    /// An explicit testimony provenance is honoured.
    #[tokio::test]
    async fn explicit_testimony_is_honoured() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                date: Some("2026-01-01".into()),
                ..capture_args("alpha", "speaks two languages")
            },
        )
        .await;
        assert_eq!(captured["provenance"], "testimony");
        assert_eq!(captured["date"], "2026-01-01");
    }

    #[tokio::test]
    async fn unknown_provenance_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                provenance: Some("maybe".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject unknown provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn malformed_date_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                date: Some("not-a-date".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject a malformed date");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// Empty content is a caller mistake, so it comes back as a blocked
    /// answer with a way forward rather than as a protocol error (rule 68).
    #[tokio::test]
    async fn empty_content_is_a_blocked_answer() {
        let body = blocked(
            &handler()
                .capture(Parameters(capture_args("alpha", "   ")))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let said = jojobot_domain::memory::validate_content("   ")
            .expect_err("empty content is refused")
            .to_string();
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains(&said)),
            "the refusal names the fault: {body}"
        );
    }
}
