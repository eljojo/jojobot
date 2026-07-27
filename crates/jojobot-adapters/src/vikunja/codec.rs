//! A message card's description: the body a human reads, plus the machine block
//! that carries what the board cannot.
//!
//! The board already holds two of the four facts about a message — the **column**
//! is its state and the **label** is its mailbox — so neither is written here.
//! Duplicating them would create exactly the split brain the Memory context paid
//! for, where a doc's declared id and its rows' subject cells disagreed and the
//! entity became readable under one name and writable under another. What is
//! left is what no Vikunja field can hold: the declared **sender**, the
//! **sent-at** instant, the **subject** the poster gave it, and the **outcome
//! notes** a consumer records.
//!
//! The subject rides here rather than being read off the card's title, even
//! though the title is rendered from it. A title is one line of display text a
//! person may retype at any moment, and it also carries the sender; reading the
//! subject back out of it would mean guessing where one ends and the other
//! begins, on text nobody promised to leave alone.
//!
//! ```text
//! the shipment landed
//!
//! ```yaml
//! sender: alpha
//! sent-at: 2026-05-28T20:26:40Z
//! subject: the shipment
//! notes: filed under shipments
//! ```
//! ```

use jiff::Timestamp;

/// What a message card's description carries beyond its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Envelope {
    pub sender: String,
    pub sent_at: Timestamp,
    pub subject: Option<String>,
    pub notes: Option<String>,
    /// The id of the message this one answers, when it answers one. A card
    /// reference rather than a Vikunja task relation: the link belongs to the
    /// record jojobot keeps, and a relation a person could remove from the
    /// board would be a fact that silently stopped being true.
    pub in_reply_to: Option<String>,
}

/// Render a card's description: the body, then jojobot's machine block.
///
/// The body goes **first** because that is what a human opening the card in
/// Vikunja came to read; the block sits under it the way the Memory codec's fact
/// table sits under a doc's prose.
pub(super) fn render_description(body: &str, envelope: &Envelope) -> String {
    // `subject` and `notes` are absent rather than blank when there is nothing
    // to say — `render_block` drops an empty value — because a blank `notes:`
    // reads as "handled, nothing to say" and a blank `subject:` as a title
    // somebody wrote and left empty. Both are claims nobody made.
    render_block(
        body,
        &[
            (SENDER, envelope.sender.trim().to_string()),
            (SENT_AT, envelope.sent_at.to_string()),
            (SUBJECT, envelope.subject.clone().unwrap_or_default()),
            (NOTES, envelope.notes.clone().unwrap_or_default()),
            (IN_REPLY_TO, envelope.in_reply_to.clone().unwrap_or_default()),
        ],
    )
}

/// Read a card's description back: its body and its envelope, or `None` if this
/// card carries no jojobot machine block at all.
///
/// `None` is the honest answer for a card a human added to the board by hand: it
/// has no declared sender and no sent-at, so it is not a message, and inventing
/// either would put a card into a delivery with provenance nobody wrote.
pub(super) fn parse_description(description: &str) -> Option<(String, Envelope)> {
    let (body, fields) = split_description(description, |inner| {
        let has_sender = inner.iter().any(|l| field_of(l, SENDER).is_some());
        let has_instant = inner
            .iter()
            .any(|l| field_of(l, SENT_AT).is_some_and(|v| v.parse::<Timestamp>().is_ok()));
        has_sender && has_instant
    })?;
    let field = |key: &str| fields.iter().find_map(|l| field_of(l, key));
    Some((
        body,
        Envelope {
            sender: field(SENDER)?,
            sent_at: field(SENT_AT)?.parse().ok()?,
            // Absent on every card written before there was a field for it, and
            // that is not a defect — those messages have no subject.
            subject: field(SUBJECT),
            notes: field(NOTES),
            in_reply_to: field(IN_REPLY_TO),
        },
    ))
}

/// Split any jojobot card's description into **the prose a human reads** and
/// **the lines of its machine block** — the half of this codec that is not about
/// messages at all.
///
/// Shared, because the two things that make it safe are not obvious and must not
/// be reinvented per card type: the block is anchored at the END of the
/// description (see [`machine_block`]) so a body cannot forge one, and the
/// de-HTML pass is a FALLBACK rather than a first step. Vikunja's own editor
/// treats a description as rich text, so a card touched in the web UI can come
/// back tagged and entity-escaped — but a store that keeps plain text must never
/// be put through an entity decoder, or a body's literal `&amp;` silently
/// becomes an `&`. Plain text is tried first and wins whenever it parses; the
/// decoder only ever sees text that had no readable block.
///
/// `valid` decides whether what sits between the fences is jojobot's block for
/// this kind of card. It is what keeps a card a human fenced by hand inert
/// rather than turning it into a record with invented fields.
pub(super) fn split_description(
    description: &str,
    valid: impl Fn(&[&str]) -> bool + Copy,
) -> Option<(String, Vec<String>)> {
    read_block(description, valid).or_else(|| read_block(&de_html(description), valid))
}

/// One `key: value` line out of a block's lines, if it is there.
pub(super) fn field(lines: &[String], key: &str) -> Option<String> {
    lines.iter().find_map(|l| field_of(l, key))
}

/// Render a card description: the prose a human reads, then a fenced block of
/// `key: value` lines. Blank values are dropped — an absent line and a blank one
/// say different things, and only one of them is true.
pub(super) fn render_block(prose: &str, fields: &[(&str, String)]) -> String {
    let mut block = String::from(FENCE);
    block.push('\n');
    for (key, value) in fields {
        let value = value.trim();
        if !value.is_empty() {
            block.push_str(&format!("{key}: {value}\n"));
        }
    }
    block.push_str(CLOSE);
    format!("{}\n\n{block}", prose.trim())
}

/// The fence a machine block opens and closes with.
const FENCE: &str = "```yaml";
/// The closing fence.
const CLOSE: &str = "```";
/// The field carrying the caller-declared sender.
const SENDER: &str = "sender";
/// The field carrying the instant the message was sent.
const SENT_AT: &str = "sent-at";
/// The field carrying a consumer's recorded outcome.
const NOTES: &str = "notes";
/// The field carrying the subject the poster gave the message.
const SUBJECT: &str = "subject";
/// The field carrying the id of the message this one answers.
const IN_REPLY_TO: &str = "in-reply-to";

/// The value of a `key: value` line, if this line is one.
fn field_of(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// The half-open line span of jojobot's machine block: **the last two fence
/// lines in the description**, when what sits between them is an envelope.
///
/// Anchored at the end, and that anchor is the whole defence. jojobot writes its
/// block last and writes no fence inside it, so the final pair of fence lines is
/// always its own, whatever the body above did.
///
/// Scanning *forward* for the first valid block fails two ways — and unlike a
/// wiki doc, a message body is text somebody else supplied:
///
/// * **forgery.** A body that quotes a machine block — trivially, by pasting a
///   message it received — supplies the first content-valid block, letting a
///   sender dictate who a message is from and when it was sent: precisely the
///   two fields the board cannot cross-check.
/// * **a lone fence.** One unbalanced ``` line in a body pairs with jojobot's
///   *opening* fence, leaving its closing fence unmatched and the whole
///   description unparseable. The card jojobot itself just wrote then fails its
///   own read-back, `post_message` rolls it back, and that message can never be
///   sent at all.
fn machine_block(lines: &[&str], valid: impl Fn(&[&str]) -> bool) -> Option<(usize, usize)> {
    let mut fences = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("```"))
        .map(|(i, _)| i);
    let close = fences.next_back()?;
    let open = fences.next_back()?;

    // Still content-checked, so a card a human added by hand — fenced or not —
    // stays inert rather than becoming a record with invented fields.
    valid(&lines[open + 1..close]).then_some((open, close + 1))
}

/// Read a description that is already plain text.
fn read_block(
    description: &str,
    valid: impl Fn(&[&str]) -> bool,
) -> Option<(String, Vec<String>)> {
    let lines: Vec<&str> = description.lines().collect();
    let (open, close) = machine_block(&lines, valid)?;
    let inner = lines[open + 1..close - 1]
        .iter()
        .map(|l| (*l).to_string())
        .collect();
    let prose = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (i < open || i >= close).then_some(*l))
        .collect::<Vec<&str>>()
        .join("\n")
        .trim()
        .to_string();
    Some((prose, inner))
}

/// Flatten rich text back to the plain text it was written as: block-level tags
/// become line breaks, every other tag is dropped, and the entities a serializer
/// escapes are decoded.
///
/// `&amp;` is decoded **last**, so a body that legitimately contained `&amp;lt;`
/// does not come back as `<`.
///
/// **What this does not cover.** It recovers a description whose line structure
/// was re-expressed as block tags. It cannot recover one whose newlines were
/// simply dropped — in HTML they are insignificant whitespace, so an editor
/// round trip that collapses the machine block onto one line leaves nothing to
/// reconstruct from. That case is not silently papered over: the read-back on
/// every write fails loudly instead. Which of the two Vikunja actually does is
/// unverified until the gated integration test runs.
fn de_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut tag = String::new();
    while let Some(c) = chars.next() {
        if c != '<' {
            out.push(c);
            continue;
        }
        tag.clear();
        for c in chars.by_ref() {
            if c == '>' {
                break;
            }
            tag.push(c);
        }
        // Anything that ends a block in HTML ends a line in plain text.
        let name = tag.trim_start_matches('/').trim_end_matches('/').trim();
        let name = name.split_whitespace().next().unwrap_or(name).to_lowercase();
        if matches!(name.as_str(), "br" | "p" | "div" | "li" | "tr" | "pre" | "code") {
            out.push('\n');
        }
    }
    for (entity, decoded) in [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
        ("&amp;", "&"),
    ] {
        out = out.replace(entity, decoded);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(secs).expect("a valid instant")
    }

    fn envelope() -> Envelope {
        Envelope {
            sender: "alpha".into(),
            sent_at: at(1_780_000_000),
            subject: None,
            notes: None,
            in_reply_to: None,
        }
    }

    /// The round trip is the whole contract: what a post writes is what a read
    /// gets back, body byte-for-byte.
    #[test]
    fn a_description_round_trips_its_body_and_envelope() {
        let body = "the shipment landed";
        let rendered = render_description(body, &envelope());
        assert!(
            rendered.starts_with(body),
            "the body a human reads comes first: {rendered:?}"
        );

        let (read_body, read_envelope) = parse_description(&rendered).expect("a message card");
        assert_eq!(read_body, body);
        assert_eq!(read_envelope, envelope());
    }

    /// A body is prose: paragraphs, markdown, pipes and colons all survive —
    /// including a line that looks exactly like a machine-block field.
    #[test]
    fn a_body_is_prose_and_nothing_in_it_forges_a_field() {
        let body = "first line\n\nsecond paragraph | with a pipe\nsender: someone-else";
        let rendered = render_description(body, &envelope());
        let (read_body, read_envelope) = parse_description(&rendered).expect("a message card");
        assert_eq!(read_body, body, "the body survives verbatim");
        assert_eq!(
            read_envelope.sender, "alpha",
            "a `sender:` line in the prose is prose, not a field: {read_envelope:?}"
        );
    }

    /// Notes are written only once a consumer records an outcome — an unhandled
    /// message's block has no `notes` line at all, rather than an empty one.
    #[test]
    fn notes_appear_only_once_there_is_an_outcome() {
        let without = render_description("the shipment landed", &envelope());
        assert!(!without.contains("notes:"), "got {without:?}");

        let with = render_description(
            "the shipment landed",
            &Envelope {
                notes: Some("filed under shipments".into()),
                ..envelope()
            },
        );
        let (_, read) = parse_description(&with).expect("a message card");
        assert_eq!(read.notes.as_deref(), Some("filed under shipments"));
    }

    /// A subject is written only when the poster gave one, and **a card written
    /// before the field existed still reads** — its absence is a message with no
    /// subject, not a card that fails to parse. That back-compatibility is the
    /// whole reason this field is optional rather than required: the operator's
    /// board is full of cards jojobot wrote last milestone.
    #[test]
    fn a_subject_is_optional_and_an_older_card_still_reads() {
        let without = render_description("the shipment landed", &envelope());
        assert!(!without.contains("subject:"), "got {without:?}");
        let (_, read) = parse_description(&without).expect("a message card");
        assert_eq!(read.subject, None);

        let with = render_description(
            "it landed at dawn",
            &Envelope { subject: Some("the shipment".into()), ..envelope() },
        );
        let (body, read) = parse_description(&with).expect("a message card");
        assert_eq!(read.subject.as_deref(), Some("the shipment"));
        assert_eq!(body, "it landed at dawn", "the subject is not carved out of the body");

        // Verbatim, as a card jojobot wrote before this field existed: no
        // subject line at all, and every other field where it always was.
        let legacy = "the shipment landed\n\n```yaml\nsender: alpha\nsent-at: \
                      2026-05-28T20:26:40Z\nnotes: filed\n```";
        let (legacy_body, legacy_read) = parse_description(legacy).expect("an older message card");
        assert_eq!(legacy_body, "the shipment landed");
        assert_eq!(legacy_read.subject, None);
        assert_eq!(legacy_read.notes.as_deref(), Some("filed"));
    }

    /// A card a human added to the board by hand is not a message. It has no
    /// declared sender and no sent-at, and jojobot invents neither.
    #[test]
    fn a_card_without_a_machine_block_is_not_a_message() {
        assert_eq!(parse_description("just a note someone typed"), None);
        assert_eq!(parse_description(""), None);
        // A fenced block that is not jojobot's — no sender, no parseable instant.
        assert_eq!(
            parse_description("notes\n\n```yaml\nfoo: bar\n```"),
            None,
            "somebody else's yaml is not an envelope"
        );
        // Half a block is not a block: an instant with nobody behind it is not
        // provenance, and a sender with no instant cannot be ordered.
        assert_eq!(
            parse_description("```yaml\nsent-at: 2026-05-28T20:26:40Z\n```"),
            None
        );
        assert_eq!(parse_description("```yaml\nsender: alpha\n```"), None);
        // …and an unparseable instant is not an instant.
        assert_eq!(
            parse_description("```yaml\nsender: alpha\nsent-at: last Tuesday\n```"),
            None
        );
    }

    /// **A store that keeps descriptions as rich text still reads.** Vikunja's
    /// own editor treats a description as HTML, so a card touched in the web UI
    /// can come back wrapped in block tags and entity-escaped — the fence
    /// characters survive as literal text, because they are text.
    ///
    /// The de-HTML pass is a **fallback**, reached only when the raw text holds
    /// no machine block — so a plain-text store is never put through an entity
    /// decoder that would turn a body's literal `&amp;` into an `&`.
    ///
    /// *Unverified against a live Vikunja at the time of writing (the gated
    /// integration test is what settles it). Being tolerant here is the safe
    /// direction: if the store is a pass-through, this path is never taken.*
    #[test]
    fn a_description_wrapped_in_html_still_reads() {
        let html = "<p>the shipment landed &amp; the crates are stacked</p>\
                    <p>```yaml<br>sender: alpha<br>sent-at: 2026-05-28T20:26:40Z<br>```</p>";
        let (body, envelope) = parse_description(html).expect("an HTML-wrapped message card");
        assert_eq!(body, "the shipment landed & the crates are stacked");
        assert_eq!(envelope.sender, "alpha");
        assert_eq!(envelope.sent_at, at(1_780_000_000));
    }

    /// …and the fallback stays a fallback: a plain-text body carrying HTML-ish
    /// characters is not decoded, because its block parsed without help.
    #[test]
    fn a_plain_body_is_never_put_through_the_html_decoder() {
        let body = "compare a &amp; b, and note that 1 < 2";
        let rendered = render_description(body, &envelope());
        let (read_body, _) = parse_description(&rendered).expect("a message card");
        assert_eq!(read_body, body, "no entity decoding on a plain-text store");
    }

    /// **A body that quotes a machine block must not hijack the envelope.**
    /// jojobot's block is always written last, so the reader takes the last one
    /// — otherwise a sender could forge who a message is from and when it was
    /// sent, in the one field the board cannot cross-check.
    #[test]
    fn a_body_quoting_a_machine_block_cannot_forge_the_envelope() {
        let body = "look what the last one said:\n\n\
                    ```yaml\nsender: someone-else\nsent-at: 2020-01-01T00:00:00Z\nnotes: forged\n```";
        let rendered = render_description(body, &envelope());
        let (read_body, read) = parse_description(&rendered).expect("a message card");
        assert_eq!(read.sender, "alpha", "the quoted block is prose, not the envelope");
        assert_eq!(read.sent_at, at(1_780_000_000));
        assert_eq!(read.notes, None, "…including its notes");
        assert_eq!(read_body, body, "…and the quote survives in the body verbatim");
    }

    /// **An unbalanced fence in a body must not cost the whole card.** A lone
    /// ``` line pairs with jojobot's opening fence if the reader scans forward,
    /// and then the card jojobot itself just wrote fails to parse — so
    /// post_message rolls back and the message can never be sent at all.
    #[test]
    fn an_unbalanced_fence_in_a_body_still_leaves_a_readable_card() {
        for body in [
            "hello\n```\nworld",
            "```",
            "trailing fence\n```",
            "```\nleading fence",
            "```yaml\nsender: half a block\n```\nand one more ```",
        ] {
            let rendered = render_description(body, &envelope());
            let (read_body, read) = parse_description(&rendered)
                .unwrap_or_else(|| panic!("jojobot's own card must read back: {body:?}"));
            assert_eq!(read.sender, "alpha", "for body {body:?}");
            assert_eq!(read.sent_at, at(1_780_000_000), "for body {body:?}");
            assert_eq!(read_body, body, "the body survives verbatim: {body:?}");
        }
    }

    /// A message that has been processed re-renders from its parsed body, so the
    /// round trip has to be stable under repetition — otherwise every outcome
    /// recorded on a card grows or loses a line.
    #[test]
    fn re_rendering_a_parsed_card_is_stable() {
        let body = "the shipment landed\n\n```\nsome quoted thing\n```";
        let once = render_description(body, &envelope());
        let (parsed, read) = parse_description(&once).expect("a message card");

        let twice = render_description(
            &parsed,
            &Envelope { notes: Some("filed".into()), ..read },
        );
        let (again, read_again) = parse_description(&twice).expect("a message card");
        assert_eq!(again, body, "the body neither grows nor loses a line");
        assert_eq!(read_again.notes.as_deref(), Some("filed"));
        assert_eq!(read_again.sender, "alpha");
    }
}
