//! **The input grammar** — the tokens a caller may send, and what they mean.
//!
//! Every one of these is a *client* mistake when it fails, so they return
//! `McpError` rather than a blocked answer: a token that is no kind and a
//! handle that is no address are malformed calls, not near misses. The one
//! exception is spelled out on [`parse_edge`], where half an edge parses fine
//! and is still wrong.
//!
//! **Input stays lowercase.** The response vocabulary speaks schema.org's words
//! (`Person`, `memberOf`); nothing here accepts them back.

use super::*;
use jojobot_domain::memory::Boot;

/// Parse the `shape`/`object` pair into an edge. **Half an edge is an error, not
/// a shrug:** a shape with no object has nothing to point at, and an object with
/// no shape has no meaning — either way the caller meant an edge and did not get
/// one, which is exactly the silence ask-across dies of.
/// **The two outcomes are different in kind, so the return type says so.** The
/// outer `Err` is a malformed call — a token that is no shape, a handle the
/// shape's kind rule forbids — and stays a protocol error, which is the line
/// the orientation essay draws. The inner `Err` is a MISUSE: both arguments
/// would have parsed, the mistake is that only one arrived, and the fix is the
/// other one. That is a blocked answer, the same as every other misuse here.
pub(crate) type ParsedEdge = Result<Result<Option<Edge>, CallToolResult>, McpError>;

/// Parse a kind token; the closed set is named in the error so a caller can fix
/// the call without guessing.
pub(crate) fn parse_kind(raw: &str) -> Result<EntityKind, McpError> {
    EntityKind::from_token(raw.trim()).ok_or_else(|| {
        let kinds: Vec<&str> = EntityKind::ALL.iter().map(|k| k.as_token()).collect();
        McpError::invalid_params(
            format!("kind must be one of {}, got '{raw}'", kinds.join(", ")),
            None,
        )
    })
}

/// Build an entity id from a `kind` argument and a handle that may be a bare
/// slug or a fully qualified id. A qualified handle that disagrees with `kind`
/// is a client error rather than a silent winner.
pub(crate) fn entity_id(kind: &str, handle: &str) -> Result<EntityId, McpError> {
    let kind = parse_kind(kind)?;
    match handle.trim().split_once(':') {
        None => Ok(EntityId::new(kind, handle)),
        Some((k, slug)) if EntityKind::from_token(k) == Some(kind) => Ok(EntityId::new(kind, slug)),
        Some((k, _)) => Err(McpError::invalid_params(
            format!("handle '{handle}' says kind '{k}' but kind is '{kind}'"),
            None,
        )),
    }
}

/// The identity a session verb was told to write as, if it was told one.
///
/// Blank is absent rather than an error: a client that sends `bot: ""` meant to
/// send nothing, and refusing the whole call over an empty string would be the
/// second-worst way to answer.
pub(crate) fn named_bot(name: Option<&str>) -> Result<Option<EntityId>, McpError> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some(name) => bot_id(name).map(Some),
    }
}

/// Read a bot handle off a name. A bare name is a bot here — this is the bot
/// door, so a bare slug is read with the bot kind on it — and a handle of
/// another kind is a client error rather than a silent winner: booting a person
/// as an identity would hand somebody's page back as a charter.
pub(crate) fn bot_id(name: &str) -> Result<EntityId, McpError> {
    let name = name.trim();
    match name.split_once(':') {
        None => Ok(EntityId::new(EntityKind::Bot, name)),
        Some(("bot", slug)) => Ok(EntityId::new(EntityKind::Bot, slug)),
        Some((kind, _)) => Err(McpError::invalid_params(
            format!(
                "'{name}' is a {kind}, and this verb takes a bot — pass a bare name, or a handle \
                 with the bot kind on it"
            ),
            None,
        )),
    }
}

/// Parse an edge-shape token; the closed set is named in the error. Strict about
/// case and spelling: the **response** names (`memberOf`, `attendee`) are not
/// input, and the input grammar stays lowercase.
pub(crate) fn parse_shape(raw: &str) -> Result<EdgeShape, McpError> {
    EdgeShape::from_token(raw).ok_or_else(|| {
        let shapes: Vec<&str> = EdgeShape::ALL.iter().map(|s| s.as_token()).collect();
        McpError::invalid_params(
            format!("shape must be one of {}, got '{raw}'", shapes.join(", ")),
            None,
        )
    })
}

pub(crate) fn parse_edge(shape: Option<&str>, object: Option<&str>) -> ParsedEdge {
    match (
        shape.map(str::trim).filter(|s| !s.is_empty()),
        object.map(str::trim).filter(|s| !s.is_empty()),
    ) {
        (None, None) => Ok(Ok(None)),
        (Some(shape), Some(object)) => {
            let shape = parse_shape(shape)?;
            let edge = Edge::new(shape, EntityId(object.to_string()));
            // Grammar and the shape's kind rule, checked here so the caller hears
            // it as a client error rather than a store failure.
            validate_edge(&edge).map_err(memory_error)?;
            Ok(Ok(Some(edge)))
        }
        (Some(_), None) => Ok(Err(misused(
            "Nothing was written, and the edge you meant was not drawn. `shape` needs an \
             `object`: an edge is a shape AND the entity it points at. Pass the object too, or \
             drop the shape if you meant no edge."
                .to_string(),
        ))),
        (None, Some(_)) => Ok(Err(misused(
            "Nothing was written, and the edge you meant was not drawn. `object` needs a \
             `shape` — one of location, membership, attendance, about, connection — saying how \
             this fact \
             points at it. Pass the shape too, or drop the object if you meant no edge."
                .to_string(),
        ))),
    }
}

/// Parse a lifecycle status; unknown values are a client error, never a silent
/// fallback to active — a mistyped status that quietly became `active` would
/// hide the state the caller was reaching for.
///
/// **`negated` is refused by name.** The reader still maps a legacy `negated`
/// cell to superseded (rows carrying it are on disk), but the input grammar
/// does not: a caller reaching for it is reaching for behaviour that is gone,
/// and silently aliasing it to superseded would file a refutation where nobody
/// would look for it. The error says what to do instead.
pub(crate) fn parse_status(raw: &str) -> Result<FactStatus, McpError> {
    match raw.trim() {
        "active" => Ok(FactStatus::Active),
        "superseded" => Ok(FactStatus::Superseded),
        "negated" => Err(McpError::invalid_params(
            "there is no 'negated' status: to record that something is NOT so, rewrite the \
             fact's content to state the negative truth — it stays 'active', because that is \
             the current truth. Use 'superseded' only for a claim a later fact replaced."
                .to_string(),
            None,
        )),
        other => Err(McpError::invalid_params(
            format!("status must be 'active' or 'superseded', got '{other}'"),
            None,
        )),
    }
}

/// Parse the `boot` argument; an unknown token is a client error, never a
/// silent fall back to the default.
///
/// **The silence was the bug.** `Boot::from_token` maps anything but the exact
/// `always` to `on-demand`, which is right for READING a stored field — a value
/// a person hand-edited should not make an entity unreadable — and wrong for an
/// argument, where it means a caller's token disappears with nothing said. It
/// went unnoticed because this field's description belonged to a parameter that
/// had been deleted, so callers were being invited to pass a mailbox name here.
///
/// A token that is no boot tier is a malformed call, exactly as a token that is
/// no kind and a token that is no status are.
pub(crate) fn parse_boot(raw: Option<&str>) -> Result<Boot, McpError> {
    let Some(token) = raw.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(Boot::default());
    };
    match token {
        "always" => Ok(Boot::Always),
        "on-demand" => Ok(Boot::OnDemand),
        other => Err(McpError::invalid_params(
            format!(
                "boot must be `always` or `on-demand`, got '{other}'. `always` marks this entity \
                 as part of the core an assistant loads every session; `on-demand` is the \
                 default. It does not name a mailbox — a bot's box is opened with the bot and is \
                 named for it."
            ),
            None,
        )),
    }
}

/// Parse an explicit provenance value (no default — the caller named one).
pub(crate) fn parse_one_provenance(raw: &str) -> Result<Provenance, McpError> {
    match raw.trim() {
        "testimony" => Ok(Provenance::Testimony),
        "inference" => Ok(Provenance::Inference),
        other => Err(McpError::invalid_params(
            format!("provenance must be 'testimony' or 'inference', got '{other}'"),
            None,
        )),
    }
}

/// Parse the provenance argument; unknown values are a client error.
pub(crate) fn parse_provenance(raw: Option<&str>) -> Result<Provenance, McpError> {
    match raw.map(str::trim) {
        None | Some("") | Some("inference") => Ok(Provenance::Inference),
        Some("testimony") => Ok(Provenance::Testimony),
        Some(other) => Err(McpError::invalid_params(
            format!("provenance must be 'testimony' or 'inference', got '{other}'"),
            None,
        )),
    }
}

/// Parse the standing argument. **`None` is not a default here**: it means the
/// caller said nothing, and what a silence means depends on the claim's
/// provenance — the domain resolves it ([`standing_of`]), so this hands the
/// silence through rather than deciding it. An unknown value is a client error,
/// because a caller who wrote `maybe` meant something and guessing which of two
/// values they meant is how a hedge becomes a settled fact.
pub(crate) fn parse_standing(raw: &str) -> Result<Standing, McpError> {
    match raw.trim() {
        "settled" => Ok(Standing::Settled),
        "open" => Ok(Standing::Open),
        other => Err(McpError::invalid_params(
            format!("standing must be 'settled' or 'open', got '{other}'"),
            None,
        )),
    }
}

/// Parse the date argument, or default to today in UTC. The UTC default keeps
/// the domain clock-free while giving `capture` a sensible freshness stamp.
pub(crate) fn parse_date(raw: Option<&str>) -> Result<jiff::civil::Date, McpError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()),
        Some(s) => s.parse().map_err(|e| {
            McpError::invalid_params(format!("date must be YYYY-MM-DD, got '{s}': {e}"), None)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// **Half an edge is a misuse, and misuses are answers here too.** Same
    /// class as `resume` without a `bot`: `shape` and `object` each parse, the
    /// mistake is the combination, and the fix is the other argument. It threw
    /// `invalid_params`, so a caller reaching across entities got a protocol
    /// failure where a next move belonged — and the edge it meant to draw was
    /// silently not drawn, which is the silence ask-across dies of.
    ///
    /// Both halves, through both verbs that parse the pair.
    #[tokio::test]
    async fn half_an_edge_is_a_blocked_answer_through_every_verb_that_takes_one() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        // capture, shape with nothing to point at
        let body = blocked(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    shape: Some("location".into()),
                    object: None,
                    ..capture_args("alpha", "was there")
                }))
                .await
                .expect("a misuse is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("object"),
            "the advice names the argument that completes it: {how}"
        );

        // update_fact, an object with no shape to draw it as
        let body = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    shape: None,
                    object: Some("place:shelbyville".into()),
                    ..update_args("person:alpha#1")
                }))
                .await
                .expect("a misuse is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("shape"),
            "the advice names the argument that completes it: {how}"
        );

        // …and a token that is no shape stays a plain ERROR, because that is a
        // malformed call rather than a combination — the line the orientation
        // essay draws, and this pins that the conversion did not blur it.
        let err = jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some("nonsense".into()),
                object: Some("place:shelbyville".into()),
                ..capture_args("alpha", "was there")
            }))
            .await
            .expect_err("a token outside a closed set is malformed");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// The two edges of reading an identity off a parameter: blank is absent,
    /// and a handle of another kind is a client error rather than a silent
    /// winner — booting a person as an identity would hand somebody's page back
    /// as a charter.
    #[test]
    fn a_named_bot_is_absent_when_blank_and_refused_when_it_is_another_kind() {
        assert_eq!(named_bot(None).expect("ok"), None);
        assert_eq!(named_bot(Some("   ")).expect("blank is absent"), None);
        assert_eq!(
            named_bot(Some(" gamma ")).expect("ok"),
            Some(EntityId("bot:gamma".into())),
            "a bare name is a bot at this door"
        );
        assert_eq!(
            named_bot(Some("bot:gamma")).expect("ok"),
            Some(EntityId("bot:gamma".into())),
            "…and so is the qualified handle"
        );
        let wrong = named_bot(Some("person:milhouse")).expect_err("another kind is refused");
        assert_eq!(wrong.code, ErrorCode::INVALID_PARAMS);
    }
}
