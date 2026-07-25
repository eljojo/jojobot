//! A message card's description: the body a human reads, plus the machine block
//! that carries what the board cannot.
//!
//! The board already holds two of the four facts about a message — the **column**
//! is its state and the **label** is its mailbox — so neither is written here.
//! Duplicating them would create exactly the split brain the Memory context paid
//! for, where a doc's declared id and its rows' subject cells disagreed and the
//! entity became readable under one name and writable under another. What is
//! left is what no Vikunja field can hold: the declared **sender**, the
//! **sent-at** instant, and the **outcome notes** a consumer records.
//!
//! ```text
//! the shipment landed
//!
//! ```yaml
//! sender: alpha
//! sent-at: 2026-05-28T20:26:40Z
//! notes: filed under shipments
//! ```
//! ```

use jiff::Timestamp;

/// What a message card's description carries beyond its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Envelope {
    pub sender: String,
    pub sent_at: Timestamp,
    pub notes: Option<String>,
}

/// Render a card's description: the body, then jojobot's machine block.
///
/// The body goes **first** because that is what a human opening the card in
/// Vikunja came to read; the block sits under it the way the Memory codec's fact
/// table sits under a doc's prose.
pub(super) fn render_description(body: &str, envelope: &Envelope) -> String {
    let mut block = format!(
        "{FENCE}\n{SENDER}: {}\n{SENT_AT}: {}\n",
        envelope.sender.trim(),
        envelope.sent_at
    );
    // The line is absent, not blank, when there is no outcome yet: a blank
    // `notes:` reads as "handled, nothing to say", which is a different claim.
    if let Some(notes) = envelope.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        block.push_str(&format!("{NOTES}: {notes}\n"));
    }
    block.push_str(CLOSE);
    format!("{}\n\n{block}", body.trim())
}

/// Read a card's description back: its body and its envelope, or `None` if this
/// card carries no jojobot machine block at all.
///
/// `None` is the honest answer for a card a human added to the board by hand: it
/// has no declared sender and no sent-at, so it is not a message, and inventing
/// either would put a card into a delivery with provenance nobody wrote.
pub(super) fn parse_description(description: &str) -> Option<(String, Envelope)> {
    // The de-HTML pass is a **fallback**, not a first step. Vikunja's own editor
    // treats a description as rich text, so a card touched in the web UI can
    // come back tagged and entity-escaped — but a store that keeps plain text
    // must never be put through an entity decoder, or a body's literal `&amp;`
    // silently becomes an `&`. Plain text is tried first and wins whenever it
    // parses; the decoder only ever sees text that had no readable block.
    read(description).or_else(|| read(&de_html(description)))
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

/// The value of a `key: value` line, if this line is one.
fn field_of(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// The half-open line span of jojobot's machine block.
///
/// Keyed on the block's **contents**, not on being the first fence in the text:
/// a body may hold fenced blocks of its own, and pinning on position is how the
/// Memory codec once let a pasted snippet take a doc's identity. A block counts
/// only if it carries both a sender and an instant that actually parses —
/// half an envelope is not an envelope.
fn machine_block(lines: &[&str]) -> Option<(usize, usize)> {
    let is_fence = |l: &&str| l.trim_start().starts_with("```");
    let mut i = 0;
    while i < lines.len() {
        if !is_fence(&lines[i]) {
            i += 1;
            continue;
        }
        let close = i + 1 + lines[i + 1..].iter().position(is_fence)?;
        let inner = &lines[i + 1..close];
        let has_sender = inner.iter().any(|l| field_of(l, SENDER).is_some());
        let has_instant = inner
            .iter()
            .any(|l| field_of(l, SENT_AT).is_some_and(|v| v.parse::<Timestamp>().is_ok()));
        if has_sender && has_instant {
            return Some((i, close + 1));
        }
        i = close + 1;
    }
    None
}

/// Read a description that is already plain text.
fn read(description: &str) -> Option<(String, Envelope)> {
    let lines: Vec<&str> = description.lines().collect();
    let (open, close) = machine_block(&lines)?;
    let inner = &lines[open + 1..close - 1];
    let field = |key: &str| inner.iter().find_map(|l| field_of(l, key));

    let envelope = Envelope {
        sender: field(SENDER)?,
        sent_at: field(SENT_AT)?.parse().ok()?,
        notes: field(NOTES),
    };
    let body = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (i < open || i >= close).then_some(*l))
        .collect::<Vec<&str>>()
        .join("\n")
        .trim()
        .to_string();
    Some((body, envelope))
}

/// Flatten rich text back to the plain text it was written as: block-level tags
/// become line breaks, every other tag is dropped, and the five entities a
/// serializer escapes are decoded.
///
/// `&amp;` is decoded **last**, so a body that legitimately contained `&amp;lt;`
/// does not come back as `<`.
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
            notes: None,
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
}
