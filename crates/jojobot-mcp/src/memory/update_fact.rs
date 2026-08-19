//! `update_fact` — Correct an addressed fact in place — the source, never an addendum.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `update_fact`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateFactArgs {
    /// The fact's global address, `kind:slug#local-id` — exactly as `recall`
    /// returned it.
    pub address: String,
    /// Replacement claim.
    #[serde(default)]
    pub content: Option<String>,
    /// Replacement details; pass an empty string to clear them.
    #[serde(default)]
    pub details: Option<String>,
    /// `active` or `superseded`. **A refutation is not a status** — to record
    /// that something is not so, rewrite `content` to state the negative truth;
    /// it stays `active`, because that IS the current truth.
    #[serde(default)]
    pub status: Option<String>,
    /// `testimony`, `observation` or `inference`. Moving a claim TO `testimony`
    /// needs `confirmed_by_user` whichever value it held: a claim you read
    /// somewhere is not a step towards the user having said it. An
    /// `observation` must carry `read_from`, here as at capture.
    #[serde(default)]
    pub provenance: Option<String>,
    /// `settled` or `open`. **Moving a claim that is ALREADY open to settled
    /// requires `confirmed_by_user`** — the operator hedged that claim, and
    /// only the operator can withdraw the hedge. Reopening is free.
    ///
    /// That is the whole of the gate: it is on this promotion, not on
    /// declaring a standing. A fresh `capture` may state `settled` and is
    /// taken at its word, the way it is taken at its word about provenance.
    #[serde(default)]
    pub standing: Option<String>,
    /// Required for two promotions of an EXISTING claim: anything → testimony
    /// (inference or observation alike), and an open standing → settled. Set it only when the user has actually
    /// confirmed the claim. Nothing else is gated on it — a fresh `capture`
    /// declares its provenance and its standing on honour.
    #[serde(default)]
    pub confirmed_by_user: Option<bool>,
    /// The shape of an edge to attach: `location` · `membership` · `attendance` ·
    /// `about` · `connection` (a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone.
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist** —
    /// `add_entity` first if it is genuinely new.
    #[serde(default)]
    pub object: Option<String>,
    /// **Fields to set**, as key/value pairs. Each key named is written; a key
    /// the record already carries and this does not name is left alone, so an
    /// edit reaches one field without restating the rest.
    #[serde(default)]
    pub fields: Option<std::collections::BTreeMap<String, String>>,
    /// **Fields to remove**, by key. Its own argument rather than an empty
    /// value in `fields`: an empty value is a value somebody wrote, and
    /// setting a key to nothing and taking the key off the record are two
    /// different edits.
    #[serde(default)]
    pub clear_fields: Option<Vec<String>>,
    /// **The day after which this reading stops being good**, `YYYY-MM-DD`.
    ///
    /// It is a fact about jojobot's knowledge rather than about the world: a
    /// pass that runs out on a date is a claim about the world and belongs in
    /// the content. Past this day a read SAYS SO and **nothing else happens** —
    /// no sweep, no reminder, nobody is coming to check it.
    #[serde(default)]
    pub stale_after: Option<String>,
    /// **Take the day off**, leaving a claim that makes no promise about how
    /// long it stays good. Its own flag, because leaving it alone and removing
    /// it are two different edits.
    #[serde(default)]
    pub clear_stale_after: Option<bool>,
    /// **The claim this one was worked out from**, as its address
    /// `kind:slug#local-id`.
    ///
    /// **Lineage is learned late**, so it is set here as well as at capture: a
    /// claim is often written before anybody notices what it rests on. The
    /// named claim must exist.
    #[serde(default)]
    pub derived_from: Option<String>,
    /// **Take the lineage pointer off.** Its own flag, because leaving it alone
    /// and removing it are two different edits.
    #[serde(default)]
    pub clear_derived_from: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Edit one addressed fact in place — fix the source, never an addendum.
#[tool_router(router = update_fact_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(description = "Edit an addressed fact in place \
                       (content/details/status/provenance/standing). To record that something \
                       is NOT so, rewrite content to state the negative truth — that is an \
                       ordinary edit and the fact stays active; there is no negated status. \
                       TWO MOVES NEED confirmed_by_user, and they are different: moving a \
                       claim TO testimony (who backs it), from inference or from observation \
                       alike — a claim you read in a system of record is not a step towards the \
                       operator having said it — and settling a claim that is already open (how \
                       sure anyone is). THIS IS HOW A HEDGE IS CONFIRMED — the \
                       operator hedged the claim and no longer does, so set standing settled \
                       and leave provenance alone; the claim was theirs from the start. \
                       Reopening is free. THE GATE IS ON PROMOTION, NOT ON ASSERTION: a fresh \
                       capture may declare standing settled and nobody is asked to confirm it, \
                       exactly as it declares its provenance — what needs the operator's word \
                       is moving a claim they hedged. IT ALSO REACHES THE RECORD'S FIELDS: \
                       fields sets the keys you name and leaves every other key alone, and \
                       clear_fields takes keys off. Those are two arguments rather than one, \
                       because setting a key to an empty value and removing the key are \
                       different edits and a caller means one of them. An address that \
                       names no fact comes back status: blocked with the addresses that do \
                       exist — it never creates. IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the \
                       address, the date, the provenance, the standing, the status and how many \
                       keys the record now carries, with the claim itself elided and said to be. \
                       The write is still verified against the store before it is called a \
                       success; what stops is shipping you the words you just sent. recall the \
                       subject to read the record back.")]
    pub(crate) async fn update_fact(
        &self,
        Parameters(args): Parameters<UpdateFactArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let patch = FactPatch {
            content: args.content,
            details: args.details,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            confirmed_by_user: args.confirmed_by_user.unwrap_or(false),
            fields: args.fields.unwrap_or_default(),
            clear_fields: args.clear_fields.unwrap_or_default(),
            stale_after: args
                .stale_after
                .as_deref()
                .map(|day| parse_date(Some(day), &self.zone_for(args.sid.as_deref())))
                .transpose()?,
            clear_stale_after: args.clear_stale_after.unwrap_or(false),
            derived_from: args
                .derived_from
                .as_deref()
                .map(|address| FactAddress::parse(address).map_err(memory_error))
                .transpose()?,
            clear_derived_from: args.clear_derived_from.unwrap_or(false),
            edge: match parse_edge(args.shape.as_deref(), args.object.as_deref())? {
                Ok(edge) => edge,
                Err(refused) => return Ok(refused),
            },
        };
        let written = match self.memory.update_fact(&address, patch).await {
            Ok(written) => written,
            Err(e) => return memory_declined("update_fact", e),
        };
        match written {
            Guarded::Written(fact) => {
                self.beat(
                    "update_fact",
                    &fact.address().to_string(),
                    args.sid.as_deref(),
                )
                .await;
                json_result(&fact_receipt_json(
                    &fact,
                    parse_date(None, &self.zone_for(args.sid.as_deref()))?,
                ))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("update_fact"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use jojobot_domain::memory::types::{Field, ValueType};

    /// **A field is set and cleared in place, and a plain recall shows it.**
    ///
    /// Edit-in-place is the surface the model puts in front of an agent: it
    /// edits and it sees the record change. This is that surface reaching the
    /// one part of a record it could not reach — a record's fields could only
    /// be written by the call that created it, so a key that turned out wrong
    /// meant a second record beside the first.
    ///
    /// **Set and clear are separate arguments** rather than one bag where an
    /// empty value means "remove". An empty value is a value somebody wrote,
    /// and the two moves must not be spelled the same.
    /// **A clear that would drop the thing below a type it answers is blocked,
    /// and the same clear on a thing that answers nothing goes through.**
    ///
    /// Strict is a floor: what a type asks for has to survive. The pairing is
    /// the point — a build that refused both would pass the first half and
    /// make a half-described thing unrepairable, which is the failure the rule
    /// is shaped to avoid.
    #[tokio::test]
    async fn a_clear_that_would_break_a_fit_is_blocked_and_says_what_it_would_cost() {
        let jojobot = handler();
        writing_as(&jojobot);
        // **A KIND, not a declared type.** What a write may take off a thing is
        // its own kind's question; a declared type is the vocabulary a caller
        // asks with and gates nothing.
        jojobot
            .memory
            .declare_kind(
                "thing",
                jojobot_domain::memory::types::Origin::Shipped,
                vec![
                    Field::required("cost", ValueType::Number),
                    Field::required("done_on", ValueType::Date),
                ],
            )
            .await
            .expect("a kind may name the keys its things keep");
        // A shipped kind rather than a new one: declaring a new kind fills the
        // set this process parses against, and every case beside this one that
        // stands a store up empties it again.

        let whole = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cost".to_string(), "40".to_string()),
                        ("done_on".to_string(), "2026-04-18".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("thing:gravel-bike", "the annual service")
            },
        )
        .await;
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["cost".into()]),
                    ..update_args(&address_of(&whole))
                }))
                .await
                .expect("a refusal is an answer, not a protocol failure"),
        );
        let advice = refused["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal carries its way forward: {refused}"));
        assert!(
            advice.contains("thing") && advice.contains("cost"),
            "the refusal names the kind and the key it would cost: {advice}"
        );
        assert_eq!(refused["wrote"], false, "{refused}");

        // The same clear, on a thing that answers no type: served. Nothing
        // here is protecting anything, and a record nobody can repair is worse
        // than a record with a key missing.
        let loose = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("cost".to_string(), "40".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("thing:road-bike", "somebody wrote down a price")
            },
        )
        .await;
        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["cost".into()]),
                    ..update_args(&address_of(&loose))
                }))
                .await
                .expect("update ok"),
        );
        assert_ne!(
            edited["status"], "blocked",
            "a thing that fits nothing has nothing to protect: {edited}"
        );
        assert_eq!(
            edited["fields_count"], 0,
            "…so the key comes off, and the count is what says so: {edited}"
        );
    }

    #[tokio::test]
    async fn update_fact_sets_and_clears_a_field() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cost".to_string(), "40".to_string()),
                        ("done_on".to_string(), "2026-04-18".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("alpha", "the annual service")
            },
        )
        .await;
        let address = address_of(&captured);

        let set = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some(
                        [("cost".to_string(), "45".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        // **The receipt counts the keys rather than reading them back.** Two
        // is the answer to both halves at once: the key named was rewritten
        // rather than added, and the key the patch did not name is still
        // there. What each one HOLDS is read below, off the record.
        assert_eq!(set["fields_count"], 2, "{set}");

        let cleared = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["done_on".into()]),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            cleared["fields_count"], 1,
            "the key named is gone and the other is not: {cleared}"
        );

        // …and both moves are on the record a later read takes, which is what
        // makes editing in place true rather than an answer's shape.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["fields"],
            serde_json::json!({"cost": "45"}),
            "{recalled}"
        );
    }

    /// **The key jojobot writes itself is refused here too.** A verb that
    /// could set `retracts` would be a way to mark somebody else's record
    /// taken back without going through the verb that decides whether it may
    /// be — so the gate is on both write paths, not on the one somebody
    /// thought of first.
    #[tokio::test]
    async fn update_fact_refuses_the_reserved_key() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;

        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some(
                        [("retracts".to_string(), "person:alpha#f1".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("retracts"),
            "the refusal names the key: {refused}"
        );
    }

    /// `update_fact` attaches an edge to a fact that didn't have one.
    #[tokio::test]
    async fn update_fact_attaches_an_edge() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "was at the festival")).await;
        assert!(captured["edge"].is_null());
        ensure(&jojobot, "event:winter-fest").await;

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    shape: Some("attendance".into()),
                    object: Some("event:winter-fest".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["edge"]["type"], "attendee");
        assert_eq!(updated["edge"]["object"], "event:winter-fest");
    }

    /// **A refutation is a content edit, and `negated` is refused by name.** The
    /// rewritten row stays `active` and keeps its address — the negative truth is
    /// the current truth, so it has to be what a plain read returns. Asking for
    /// the retired status is a client error that says what to do instead, rather
    /// than an alias that would file the correction where nobody looks.
    #[tokio::test]
    async fn a_refutation_is_a_content_edit_and_negated_is_refused() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            capture_args("alpha", "a close contact of the user"),
        )
        .await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("negated".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect_err("the retired status must be refused, not aliased");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("rewrite"),
            "the error must say what to do instead: {}",
            err.message
        );

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("NOT a close contact — do not re-infer".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("the refutation is an ordinary edit"),
        );
        assert_eq!(
            updated["status"], "active",
            "the negative truth is the truth"
        );
        assert_eq!(
            updated["content_elided"], true,
            "the refutation is not read back to whoever just wrote it: {updated}"
        );
        assert_eq!(
            updated["address"], "person:alpha#f1",
            "the row keeps its address"
        );
        // **The edit is proven where it landed, and the receipt cannot prove
        // it.** The answer says the write happened; only a read says what the
        // record now holds — and a case named for a content edit that asserts
        // nothing about the content passes on a build where the edit is
        // dropped.
        let read = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            read["objects"][0]["facts"][0]["content"], "NOT a close contact — do not re-infer",
            "the refutation is what the record says now: {read}"
        );
    }

    /// Promotion to testimony needs the explicit confirmation flag.
    #[tokio::test]
    async fn promoting_to_testimony_requires_the_confirmation_flag() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "prefers mornings")).await;
        let promote = |confirmed: Option<bool>| UpdateFactArgs {
            provenance: Some("testimony".into()),
            confirmed_by_user: confirmed,
            ..update_args(&address_of(&captured))
        };

        // Refused as a blocked ANSWER: the call is well formed and jojobot is
        // declining to bless a claim the operator has not blessed, which is a
        // next move rather than a failure (rule 68).
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(promote(None)))
                .await
                .expect("an unconfirmed promotion is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("confirmed_by_user"),
            "the way forward names the flag the operator's word unlocks: {advice}"
        );

        let ok = json_of(
            &jojobot
                .update_fact(Parameters(promote(Some(true))))
                .await
                .expect("a confirmed promotion is allowed"),
        );
        assert_eq!(ok["provenance"], "testimony");
    }

    /// **The confirmation gate is on PROMOTION, not on assertion**, and this
    /// is the scope the surface has to state.
    ///
    /// `check_standing` fires on one move only: an existing OPEN claim being
    /// made SETTLED. A fresh capture may declare `settled` on an inference and
    /// is accepted — standing is declared on honour exactly as provenance is,
    /// and the gate is on promotion rather than on assertion, by design.
    ///
    /// Both halves in one read. The refusal alone reads as "settling is
    /// guarded" and the acceptance alone reads as a hole in the gate; it is
    /// the pair that says where the line actually is, and a reader of the
    /// surface who has only one of them believes the wrong thing.
    #[tokio::test]
    async fn the_confirmation_gate_is_on_promotion_and_not_on_assertion() {
        let jojobot = handler();

        // Asserted, and ungated: nobody is asked to confirm this.
        let asserted = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("inference".into()),
                standing: Some("settled".into()),
                ..capture_args("alpha", "shuts early on sundays")
            },
        )
        .await;
        assert_eq!(asserted["provenance"], "inference", "{asserted}");
        assert_eq!(
            asserted["standing"], "settled",
            "a capture declares its standing on honour: {asserted}"
        );

        // Promoted, and gated: the operator hedged this one, so only the
        // operator withdraws the hedge.
        let hedged = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                standing: Some("open".into()),
                ..capture_args("alpha", "thinks the ferry moved")
            },
        )
        .await;
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    standing: Some("settled".into()),
                    ..update_args(&address_of(&hedged))
                }))
                .await
                .expect("an unconfirmed settling is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// **A malformed address and a missed one are different answers**, and
    /// never a new fact. Malformed is the caller writing something that is not
    /// an address at all — a protocol error. Missed is a well-formed address
    /// naming nothing, which is the same "you named what does not exist" every
    /// gate answers, so it wears the blocked shape and carries the addresses
    /// that do exist.
    #[tokio::test]
    async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "the only fact here")).await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                content: Some("nope".into()),
                ..update_args("not-an-address")
            }))
            .await
            .expect_err("a string that is no address is a malformed call");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let missed = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("nope".into()),
                    ..update_args("person:alpha#f99")
                }))
                .await
                .expect("an address that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:alpha#f99");
        let advice = missed["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("person:alpha#f1"),
            "the addresses that DO exist are what makes this repairable: {advice}"
        );
        let body = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            body["objects"][0]["facts"].as_array().unwrap().len(),
            1,
            "nothing was created"
        );
    }

    /// An unknown status token is a client error, not a silently-active fact.
    #[tokio::test]
    async fn an_unknown_status_is_a_client_error() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;
        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("retired".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect_err("must reject an unknown status");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }
}
