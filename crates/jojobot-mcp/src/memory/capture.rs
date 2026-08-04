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
    #[serde(default)]
    pub standing: Option<String>,
    /// The fact's freshness date, `YYYY-MM-DD`. Defaults to today (UTC).
    #[serde(default)]
    pub date: Option<String>,
    /// The shape of the edge this fact draws: `location` (object is a place) ·
    /// `membership` (an org) · `attendance` (an event) · `about` (any kind).
    /// Requires `object`; neither works alone.
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
    /// **What makes this an EVENT rather than a fact** — the type name, free
    /// text, required to record one and never interpreted.
    ///
    /// A fact is current truth and gets rewritten in place when it changes. An
    /// event is chronology: it happened, it stays put, and if it turns out to
    /// be wrong it is retracted rather than edited. If you find yourself
    /// wanting to edit one, it was never an event.
    ///
    /// There is no list of types to pick from and nothing checks this against
    /// one. Write what the thing is, in your own words.
    #[serde(default)]
    pub event_type: Option<String>,
    /// Anything else worth recording about the event, as a flat bag of
    /// key/value pairs. Requires `event_type` — there is no event to describe
    /// without one.
    ///
    /// Flat and free-form on purpose: jojobot stores what you put here and
    /// interprets none of it, and a key it has never seen is kept exactly as
    /// you wrote it.
    #[serde(default)]
    pub metadata: Option<std::collections::BTreeMap<String, String>>,
    /// The entities this event touches, as `kind:slug` — **each must already
    /// exist**, exactly as `subject` must. Requires `event_type`.
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

/// Read the event half of a capture: `None` for an ordinary fact, a record for
/// an event, and a refusal when the caller described one without saying what it
/// is.
///
/// **The type name is what makes an event an event**, so metadata or refs
/// without one is a caller mistake rather than a defaulting opportunity —
/// jojobot guesses no type, and inventing one here would be the one place on
/// this surface where it did.
fn parse_event(args: &CaptureArgs) -> Result<Option<Event>, CallToolResult> {
    let named = args
        .event_type
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let described = args.metadata.as_ref().is_some_and(|m| !m.is_empty())
        || args.refs.as_ref().is_some_and(|r| !r.is_empty());
    let Some(kind) = named else {
        if described {
            return Err(misused(
                "Nothing was written. You gave metadata or refs, which describe an EVENT, \
                 without an event_type saying what the event is — and jojobot will not invent \
                 one, because a type it guessed would be indistinguishable later from one you \
                 chose. Add event_type in your own words, or drop metadata and refs and capture \
                 this as an ordinary fact."
                    .to_string(),
            ));
        }
        return Ok(None);
    };
    Ok(Some(Event {
        kind: kind.to_string(),
        metadata: args.metadata.clone().unwrap_or_default(),
        refs: args
            .refs
            .iter()
            .flatten()
            .map(|r| EntityId::person(r.trim()))
            .collect(),
    }))
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
                       deliberate steps — add_entity, then capture. THIS IS ALSO HOW AN EVENT IS \
                       RECORDED: pass event_type and the same call writes chronology instead of \
                       current truth. A fact is what is true NOW and gets rewritten in place when \
                       it changes; an event happened, stays put, and is retracted rather than \
                       edited if it turns out wrong — if you want to edit one, it was never an \
                       event. The type is FREE TEXT in your own words: there is no list to pick \
                       from, nothing checks it, and nothing is refused for being unrecognized. \
                       Add metadata as a flat bag of key/value pairs jojobot stores and never \
                       interprets, and refs to name the entities it touches — those are links \
                       whose meaning is deliberately unrecorded, so they are searchable but \
                       assert nothing, which is what makes them not `about` edges. Metadata or \
                       refs WITHOUT an event_type comes back blocked: jojobot will not invent a \
                       type, because one it guessed would be indistinguishable later from one you \
                       chose. derived_from names the claim this one was worked out from, as its \
                       address — use it when the source is another claim, not an entity."
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

        let event = match parse_event(&args) {
            Ok(event) => event,
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
            event,
            derived_from,
        };
        match self.memory.capture(new).await.map_err(memory_error)? {
            Guarded::Written(fact) => {
                self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
                    .await;
                json_result(&fact_json(&fact))
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

    /// **The open hatch: an event is captured, ungated, in the caller's own
    /// words.**
    ///
    /// No list of types to pick from and nothing checking against one. Ungated
    /// is the load-bearing half rather than a stage that has not shipped yet:
    /// refusing a type only means something once there are real types to refuse
    /// against, and a gate every caller is trained to walk through is worse
    /// than no gate at all. What the real types eventually are gets derived
    /// from what accumulates here, so what accumulates has to be what people
    /// actually meant.
    #[tokio::test]
    async fn an_event_is_captured_in_the_callers_own_words_and_comes_back_whole() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    event_type: Some("a-type-nobody-defined".into()),
                    metadata: Some(
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
        assert_ne!(body["status"], "blocked", "nothing gates a type: {body}");
        assert_eq!(body["event"]["type"], "a-type-nobody-defined");
        assert_eq!(body["event"]["metadata"]["mood"], "delighted");
        assert_eq!(body["event"]["metadata"]["weather"], "clear");
        assert_eq!(
            body["event"]["refs"],
            serde_json::json!(["person:milhouse"])
        );
        // It is still a fact, with everything a fact has — the address included,
        // because an event that could not be addressed could not be retracted.
        assert_eq!(body["content"], "the kiln was finally lit");
        assert!(body["address"].is_string(), "{body}");
    }

    /// **An ordinary fact is the default, and says so by omission rather than
    /// by absence.** Most captures are not events; the field is null on them
    /// so a reader learns that from the answer instead of from a missing key.
    #[tokio::test]
    async fn a_capture_with_no_type_is_an_ordinary_fact() {
        let jojobot = handler();
        let body = capture_ok(&jojobot, capture_args("person:alpha", "plays go")).await;
        assert!(
            body.as_object().expect("an object").contains_key("event"),
            "the key is always there: {body}"
        );
        assert!(body["event"].is_null(), "…and null for a fact: {body}");
    }

    /// **Describing an event without saying what it is comes back blocked.**
    ///
    /// jojobot invents no type. It could plausibly default one here — "event",
    /// or the content's first words — and that is exactly the failure: a type
    /// jojobot guessed would be indistinguishable later from one the caller
    /// chose, in the very records the real types are going to be derived from.
    /// So it refuses, and says which of the two things the caller meant.
    #[tokio::test]
    async fn metadata_or_refs_without_a_type_is_blocked_rather_than_guessed() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        for (what, args) in [
            (
                "metadata",
                CaptureArgs {
                    metadata: Some([("mood".to_string(), "curious".to_string())].into()),
                    ..capture_args("person:alpha", "something happened")
                },
            ),
            (
                "refs",
                CaptureArgs {
                    refs: Some(vec!["person:alpha".into()]),
                    ..capture_args("person:alpha", "something happened")
                },
            ),
        ] {
            let body = json_of(&jojobot.capture(Parameters(args)).await.expect("an answer"));
            assert_eq!(body["status"], "blocked", "{what}: {body}");
            assert_eq!(body["wrote"], false, "{what}: {body}");
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("event_type"),
                "{what}: the refusal names the missing half: {how}"
            );
            assert!(
                how.contains("ordinary fact"),
                "{what}: …and the other thing they might have meant: {how}"
            );
        }
    }

    /// **A ref names an entity, so it must already exist.** The rule is not
    /// about edges, it is about naming: nothing a write mentions is brought
    /// into being as a side effect of mentioning it. A ref that provisioned its
    /// own entity would make the open hatch the one place on this surface where
    /// that stopped being true — and the hatch is ungated precisely so that
    /// everything else about it stays strict.
    #[tokio::test]
    async fn a_ref_to_an_entity_nobody_created_is_blocked_and_writes_nothing() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    event_type: Some("a-thing-that-happened".into()),
                    refs: Some(vec!["person:ghost".into()]),
                    ..capture_args("person:alpha", "it happened")
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["attempted"], "person:ghost");
        assert_eq!(body["wrote"], false);

        // …and the fact did not land either: an event is one write, so a ref it
        // could not resolve takes the whole thing with it.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: "person:alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["facts"].as_array().expect("a list").is_empty(),
            "a blocked event wrote nothing: {recalled}"
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
    /// way through is `add_entity` — never a flag. The advice must say
    /// `add_entity`: `create_new` is not a parameter on this verb, and telling
    /// the caller to pass it would send it round a loop it cannot leave.
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
            !advice.contains("create_new"),
            "capture has no create_new, near miss or not: {advice}"
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
            !advice.contains("create_new: true"),
            "capture has no create_new; advising it sends the caller round a loop \
             with no exit: {advice}"
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(recalled["facts"][0]["edge"]["type"], "memberOf");
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["facts"].as_array().unwrap().is_empty(),
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(body["subject"], "person:alpha");
        let facts = body["facts"].as_array().expect("recall returns a list");
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
                .recall(Parameters(RecallArgs {
                    subject: "person:alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall answers"),
        );
        assert_eq!(recalled["facts"][0]["standing"], "open", "{recalled}");
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

    #[tokio::test]
    async fn empty_content_is_a_client_error() {
        let err = handler()
            .capture(Parameters(capture_args("alpha", "   ")))
            .await
            .expect_err("must reject empty content");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }
}
