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
use jojobot_domain::memory::kinds;

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

/// Parse a kind token. **The refusal carries the store's own answer**, so a
/// caller can fix the call without guessing — and so the two ways a token can
/// fail to be a kind stay apart.
///
/// The set of kinds is data, so this cannot recite a list: an instance may hold
/// a kind this build never heard of, and naming the shipped ten would tell a
/// caller their own kind does not exist. **And a process that seeded nothing
/// says so**, because a caller told "kind must be one of person, …, got
/// 'person'" has been handed a contradiction and no way forward (rule 68).
pub(crate) fn parse_kind(raw: &str) -> Result<EntityKind, McpError> {
    kinds::resolve(raw.trim())
        .map_err(|why| McpError::invalid_params(format!("kind '{raw}': {why}"), None))
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
        None => Ok(EntityId::new(EntityKind::BOT, name)),
        Some(("bot", slug)) => Ok(EntityId::new(EntityKind::BOT, slug)),
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

/// Parse a walk-direction token. **Omitted means `out`** — the edges an
/// object's own records draw, which is the end the record itself is written
/// on. A token that names neither direction is refused rather than defaulted,
/// because guessing which way a caller meant to walk answers a question they
/// did not ask.
pub(crate) fn parse_direction(raw: Option<&str>) -> Result<Direction, McpError> {
    match raw.map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(Direction::default()),
        Some(token) => Direction::of_token(token).ok_or_else(|| {
            McpError::invalid_params(format!("direction must be out or in, got '{token}'"), None)
        }),
    }
}

/// **What a key filter is asked of.** Absent is the thing, which is the
/// question a caller asking about an object is asking; a token naming neither
/// scope is refused rather than defaulted, because the two keep different
/// things and picking one would answer a question nobody asked.
pub(crate) fn parse_scope(
    raw: Option<&str>,
) -> Result<jojobot_domain::memory::graph::Scope, McpError> {
    use jojobot_domain::memory::graph::Scope;
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Scope::default()),
        Some(token) => Scope::of_token(token).ok_or_else(|| {
            McpError::invalid_params(
                format!("scope must be thing or record, got '{token}'"),
                None,
            )
        }),
    }
}

/// The comparison a filter uses. Absent is equality, which is what a key with
/// no declaration behind it has.
pub(crate) fn parse_compare(
    raw: Option<&str>,
) -> Result<jojobot_domain::memory::types::Compare, McpError> {
    use jojobot_domain::memory::types::Compare;
    match raw.map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(Compare::Equals),
        Some(token) => Compare::of_token(token).ok_or_else(|| {
            McpError::invalid_params(
                format!("compare must be equals, before, after, less or greater, got '{token}'"),
                None,
            )
        }),
    }
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
/// **`negated` and `superseded`/`retracted` are refused by name.** The reader
/// still maps all of them to `archived` (rows carrying any are on disk), but
/// the input grammar does not: a caller reaching for one is reaching for a
/// spelling this build retired, and silently aliasing it would teach the old
/// model back to whoever sent it. The error says what to do instead.
pub(crate) fn parse_status(raw: &str) -> Result<FactStatus, McpError> {
    match raw.trim() {
        "active" => Ok(FactStatus::Active),
        "archived" => Ok(FactStatus::Archived),
        "negated" => Err(McpError::invalid_params(
            "there is no 'negated' status: to record that something is NOT so, rewrite the \
             fact's content to state the negative truth — it stays 'active', because that is \
             the current truth. Use 'archived' only for a claim that stopped being current. \
             NOT FOR A PAST EVENT: turning a claim about one into its negation is never that \
             rewrite — archive it instead, with a note saying why, or retract it."
                .to_string(),
            None,
        )),
        "superseded" | "retracted" => Err(McpError::invalid_params(
            format!(
                "there is no '{}' status: it is 'archived' now, whether the claim changed or \
                 was never true. A note on the record says which, and a replacement, if there \
                 is one, is an ordinary new fact naming this one as derived_from.",
                raw.trim(),
            ),
            None,
        )),
        other => Err(McpError::invalid_params(
            format!("status must be 'active' or 'archived', got '{other}'"),
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
        "observation" => Ok(Provenance::Observation),
        "inference" => Ok(Provenance::Inference),
        other => Err(McpError::invalid_params(
            format!("provenance must be 'testimony', 'observation' or 'inference', got '{other}'"),
            None,
        )),
    }
}

/// Parse the provenance argument; unknown values are a client error.
pub(crate) fn parse_provenance(raw: Option<&str>) -> Result<Provenance, McpError> {
    match raw.map(str::trim) {
        None | Some("") | Some("inference") => Ok(Provenance::Inference),
        Some("testimony") => Ok(Provenance::Testimony),
        Some("observation") => Ok(Provenance::Observation),
        Some(other) => Err(McpError::invalid_params(
            format!("provenance must be 'testimony', 'observation' or 'inference', got '{other}'"),
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

/// **The zone a run resolves days against when it named none.**
///
/// Stated rather than silent: a session that supplies no zone is answered in
/// UTC, and every surface that takes one says so. It is a fallback and not a
/// setting — the frame belongs to the caller, and this is what jojobot uses
/// when the caller declined to supply one.
pub(crate) const FALLBACK_ZONE: &str = "UTC";

/// **The zone a name says, or why it is no zone.**
///
/// IANA names, which are what a session supplies. **Resolution happens here
/// rather than in the domain**, which stays clock-free and carries the name as
/// written: what a name means depends on a database on this machine, and that
/// is not a thing a pure model should have to read.
/// **The day a caller says its run is in**, or nothing when it said none.
///
/// A day that is no day is refused rather than read as nothing: a caller that
/// stated a frame and had it dropped would be answered on the server's clock
/// while believing otherwise, and the sweep is where that difference closes
/// somebody's runs or leaves them open.
pub(crate) fn parse_day(raw: Option<&str>) -> Result<Option<jiff::civil::Date>, McpError> {
    let Some(said) = raw.map(str::trim).filter(|day| !day.is_empty()) else {
        return Ok(None);
    };
    said.parse().map(Some).map_err(|_| {
        McpError::invalid_params(
            format!(
                "`today` is {said:?}, which is no day — send a calendar day as YYYY-MM-DD, or \
                 leave it off and the sweep answers on the clock"
            ),
            None,
        )
    })
}

pub(crate) fn parse_zone(raw: Option<&str>) -> Result<jiff::tz::TimeZone, McpError> {
    let name = raw.map(str::trim).filter(|n| !n.is_empty());
    let Some(name) = name else {
        return Ok(jiff::tz::TimeZone::UTC);
    };
    jiff::tz::TimeZone::get(name).map_err(|e| {
        McpError::invalid_params(
            format!(
                "'{name}' is no timezone this build can resolve: {e}. Send an IANA name, like                  'America/New_York' or 'Europe/Madrid', or send none and days are resolved in                  {FALLBACK_ZONE}."
            ),
            None,
        )
    })
}

/// **The day a caller named on a write**, or nothing when it named none.
///
/// 🚨 **An empty string is naming none, and this is the only place that is
/// decided.** An empty string is an ordinary way for a client to serialise an
/// optional field it has nothing for, so it is answered exactly as an absent
/// argument is — and every date argument on every write verb comes through
/// here to be told so. A filter at each call site is what produced the defect
/// this replaces: one write path stripped empties before the parser, its
/// neighbours did not, and the same empty string was answered three ways
/// depending on which argument it landed in.
///
/// ⛔️ **Nothing is filled in here.** What an unnamed day means is not the
/// parser's to say: on a write it is the run's frame ([`Jojobot::dated`]), and
/// on a field like `happened_at` it is silence, because a claim that says
/// nothing about when the thing happened says nothing. Reaching for the wall
/// clock inside the parser bypassed both — a server acting out a day stamped
/// records with the day the run really executed, which is a date nobody
/// uttered.
pub(crate) fn parse_date(raw: Option<&str>) -> Result<Option<jiff::civil::Date>, McpError> {
    let Some(named) = raw.map(str::trim).filter(|day| !day.is_empty()) else {
        return Ok(None);
    };
    named.parse().map(Some).map_err(|e| {
        McpError::invalid_params(format!("date must be YYYY-MM-DD, got '{named}': {e}"), None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **This build can resolve an IANA zone, and says so when it cannot.**
    ///
    /// Pinned rather than assumed: what a zone name means comes from a database
    /// on the machine, so a build that cannot read one would resolve every name
    /// to nothing and quietly answer every session in the fallback. That is the
    /// failure nobody would see from a green suite anywhere else.
    ///
    /// Two zones, because one proves only that SOMETHING resolved — and a
    /// negative offset and a positive one, so a case cannot pass on a build
    /// that hands back UTC under another name.
    #[test]
    fn a_zone_name_resolves_and_a_name_that_is_no_zone_says_so() {
        let stamp: jiff::Timestamp = "2026-08-19T02:30:00Z".parse().expect("a fixed instant");
        for (name, expected) in [
            // West of UTC: 02:30 UTC is still the previous evening.
            ("America/New_York", "2026-08-18"),
            // East of UTC: the same instant is already the same morning.
            ("Europe/Madrid", "2026-08-19"),
        ] {
            let zone = parse_zone(Some(name)).unwrap_or_else(|e| panic!("{name} is a zone: {e}"));
            assert_eq!(
                stamp.to_zoned(zone).date().to_string(),
                expected,
                "{name} puts that instant on {expected}",
            );
        }

        // The fallback, and it is what a caller that named none gets.
        assert_eq!(
            parse_zone(None)
                .expect("no name is the fallback")
                .iana_name(),
            Some(FALLBACK_ZONE),
        );

        let refused = parse_zone(Some("Nowhere/Atall")).expect_err("that is no zone");
        assert!(
            refused.to_string().contains("Nowhere/Atall"),
            "the refusal quotes what was sent: {refused}",
        );
    }

    use crate::harness::*;
    use crate::memory::testing::*;
    use jojobot_domain::clock::Clock;

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

    /// **A date argument sent as an empty string is the argument NOT sent** —
    /// on every verb that takes one.
    ///
    /// 🚨 An empty string is an ordinary way for a client to serialise an
    /// optional field it has nothing for. A parser that read it as "no date,
    /// so fill one in" reached for the WALL CLOCK rather than the frame the
    /// run is standing in, so a server acting out a day stamped the record
    /// with the day the run really executed — a date nobody uttered, on a
    /// record carrying whatever authority its writer had. That is the
    /// invented-date class arriving through the argument nobody was watching.
    ///
    /// **The coverage is what made it hard to see**: `capture`'s `recorded_at`
    /// went through [`Jojobot::dated`], which filters empties, and its two
    /// neighbours went straight to the parser; `update_fact` sent all three
    /// straight. The same empty string was answered three different ways
    /// depending on which argument it landed in. So this sends one to EVERY
    /// date argument on both verbs rather than to the one that was found.
    ///
    /// **Paired in the same read**, because a negative alone passes on a fix
    /// that ignored the argument entirely: a date the caller NAMES still wins,
    /// and an argument the caller OMITS still answers in the run's frame.
    #[tokio::test]
    async fn an_empty_date_argument_is_no_date_argument_on_every_verb_that_takes_one() {
        /// The day this server is acting out.
        const JUNE: &str = "2026-06-01";
        /// A day a caller names, well away from June and from any real today.
        const NAMED: &str = "2026-03-04";
        let jojobot = handler().on_clock(Clock::stating(JUNE.parse().expect("a day")));

        // ── capture: every date argument, empty ──────────────────────────
        let blank = capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some(String::new()),
                // Whitespace, because a client that trimmed nothing sends this
                // and it is as empty as the one above.
                happened_at: Some("   ".into()),
                stale_after: Some(String::new()),
                ..capture_args("alpha", "the kiln reached temperature")
            },
        )
        .await;
        assert_eq!(
            blank["recorded_at"], JUNE,
            "an empty recorded_at is the day the run states, never the day the run ran: {blank}"
        );
        assert!(
            blank["happened_at"].is_null(),
            "an empty happened_at says nothing about when it happened: {blank}"
        );
        assert!(
            blank["stale_after"].is_null(),
            "an empty stale_after puts no expiry on the claim: {blank}"
        );

        // ── capture: the same arguments, named ───────────────────────────
        let named = capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some(NAMED.into()),
                happened_at: Some(NAMED.into()),
                stale_after: Some(NAMED.into()),
                ..capture_args("alpha", "the glaze was mixed")
            },
        )
        .await;
        assert_eq!(named["recorded_at"], NAMED, "a named date wins: {named}");
        assert_eq!(named["happened_at"], NAMED, "…on each of them: {named}");
        assert_eq!(named["stale_after"], NAMED, "…and on the third: {named}");

        // ── capture: the same arguments, omitted ─────────────────────────
        let omitted = capture_ok(&jojobot, capture_args("alpha", "the shelf was loaded")).await;
        assert_eq!(
            omitted["recorded_at"], JUNE,
            "an omitted recorded_at is the run's day: {omitted}"
        );
        assert!(
            omitted["happened_at"].is_null(),
            "an omitted happened_at says nothing: {omitted}"
        );

        // ── update_fact: every date argument, empty ──────────────────────
        let address = address_of(&blank);
        let patched = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    recorded_at: Some(String::new()),
                    happened_at: Some(String::new()),
                    stale_after: Some("  ".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            patched["recorded_at"], JUNE,
            "an empty recorded_at changes nothing, so the run's day stands: {patched}"
        );
        assert!(
            patched["happened_at"].is_null(),
            "an empty happened_at does not invent one: {patched}"
        );
        assert!(
            patched["stale_after"].is_null(),
            "an empty stale_after does not invent an expiry: {patched}"
        );

        // ── update_fact: named, then omitted ─────────────────────────────
        let renamed = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    recorded_at: Some(NAMED.into()),
                    happened_at: Some(NAMED.into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            renamed["recorded_at"], NAMED,
            "a named date wins: {renamed}"
        );
        assert_eq!(renamed["happened_at"], NAMED, "…on both: {renamed}");

        let untouched = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("the kiln held temperature".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            untouched["recorded_at"], NAMED,
            "an omitted recorded_at leaves the one that is there: {untouched}"
        );
        assert_eq!(
            untouched["happened_at"], NAMED,
            "…and so does an omitted happened_at: {untouched}"
        );
    }

    /// **The control: a server nobody told a day to answers exactly as it
    /// always did.**
    ///
    /// The sabotage that matters here is the opposite one — a fictional clock
    /// leaking into an ordinary run. Every instance is this one, so an empty
    /// argument on it must still land on the wall clock, which is what an
    /// absent one has always done.
    #[tokio::test]
    async fn an_ordinary_server_dates_a_write_on_its_own_clock() {
        let jojobot = handler();
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            .to_string();

        let omitted = capture_ok(
            &jojobot,
            capture_args("alpha", "the kiln reached temperature"),
        )
        .await;
        assert_eq!(
            omitted["recorded_at"], today,
            "an omitted recorded_at is today on the clock: {omitted}"
        );

        let blank = capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some(String::new()),
                ..capture_args("alpha", "the glaze was mixed")
            },
        )
        .await;
        assert_eq!(
            blank["recorded_at"], today,
            "an empty recorded_at is answered the way an absent one is, and here that is the \
             clock: {blank}"
        );

        let named = capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some("2026-03-04".into()),
                ..capture_args("alpha", "the shelf was loaded")
            },
        )
        .await;
        assert_eq!(
            named["recorded_at"], "2026-03-04",
            "a named date wins on an ordinary server too: {named}"
        );
    }
}
