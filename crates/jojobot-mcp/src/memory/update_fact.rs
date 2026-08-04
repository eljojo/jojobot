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
    /// `testimony` or `inference`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// `settled` or `open`. **Settling an open claim requires
    /// `confirmed_by_user`** — the operator hedged the claim, and only the
    /// operator can withdraw the hedge. Reopening is free.
    #[serde(default)]
    pub standing: Option<String>,
    /// Required to promote a claim from inference to testimony, AND to settle
    /// one that is open: set it only when the user has actually confirmed the
    /// claim.
    #[serde(default)]
    pub confirmed_by_user: Option<bool>,
    /// The shape of an edge to attach: `location` · `membership` · `attendance` ·
    /// `about`. Requires `object`; neither works alone.
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist** —
    /// `add_entity` first if it is genuinely new.
    #[serde(default)]
    pub object: Option<String>,
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
                       TWO MOVES NEED confirmed_by_user, and they are different: promoting \
                       inference → testimony (who backs it), and settling a claim that is open \
                       (how sure anyone is). THIS IS HOW A HEDGE IS CONFIRMED — the \
                       operator hedged the claim and no longer does, so set standing settled \
                       and leave provenance alone; the claim was theirs from the start. \
                       Reopening is free. An address that \
                       names no fact comes back status: blocked with the addresses that do \
                       exist — it never creates.")]
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
                json_result(&fact_json(&fact))
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
        assert_eq!(updated["content"], "NOT a close contact — do not re-infer");
        assert_eq!(
            updated["address"], "person:alpha#f1",
            "the row keeps its address"
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

        let err = jojobot
            .update_fact(Parameters(promote(None)))
            .await
            .expect_err("an unconfirmed promotion must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let ok = json_of(
            &jojobot
                .update_fact(Parameters(promote(Some(true))))
                .await
                .expect("a confirmed promotion is allowed"),
        );
        assert_eq!(ok["provenance"], "testimony");
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            body["facts"].as_array().unwrap().len(),
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
