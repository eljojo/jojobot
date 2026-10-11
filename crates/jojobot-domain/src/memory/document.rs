//! **A markdown document, held by section.**
//!
//! A long piece of markdown is read whole by a person and in parts by an agent,
//! and every edit is a version of one part. The unit is the **section**: a
//! heading and what sits under it. Each section becomes one record, so a claim
//! can cite a part of a document, an edit reaches one part, and a part has a
//! history of its own.
//!
//! **The round trip is byte-exact.** [`join`] of what [`split`] returns is the
//! text that went in, character for character. The store trims the edges of a
//! claim's words and of its details, and a claim's words are one line, so a
//! section cannot be kept as it was written. It is kept as parts instead: its
//! first line, what is under it, and the four runs of whitespace that were
//! trimmed off ([`Edges`]). The first line is a heading when the section starts
//! at one. A chunk that is not a heading (text before the first heading, a
//! frontmatter block, the rest of a section that was too long for one record)
//! is the same shape with level 0, so the join has no special case.
//!
//! **Only ATX headings split** (`#` to `######`, up to three spaces of indent,
//! then a space or the end of the line). A `#` line inside a fenced block of
//! code is code, and so is any line in it: a heading is never found there.
//!
//! **A carriage return that is not followed by a line feed is refused**, because
//! a claim's words are one line and `\r` ends a line for the readers that
//! matter. `\r\n` is an ordinary line ending and survives inside a section.

/// The whitespace trimmed off a section's parts, kept so the join restores it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edges {
    /// Before the first line, which is empty unless the text started with
    /// whitespace or the heading is indented.
    pub lead: String,
    /// Between the end of the first line and its line feed, e.g. a `\r`.
    pub mid: String,
    /// Between the first line and what is under it: the line feed and any blank
    /// lines. Empty when nothing is under the first line.
    pub sep: String,
    /// After the last text of the section: the line feeds before the next one.
    pub tail: String,
}

impl Edges {
    /// **The four runs as one line of text**, because a field value is trimmed at
    /// both edges and cannot hold a line feed. Each run is escaped (`\n`, `\r`,
    /// `\t`, `\s` for a space, `\u{..}` for another whitespace character) and the
    /// four are separated by `;`.
    pub fn encode(&self) -> String {
        [&self.lead, &self.mid, &self.sep, &self.tail]
            .iter()
            .map(|run| escape(run))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The runs an [`Edges::encode`] wrote, or nothing when the text is not one.
    pub fn decode(encoded: &str) -> Option<Edges> {
        let mut runs = encoded.split(';').map(unescape);
        let edges = Edges {
            lead: runs.next()??,
            mid: runs.next()??,
            sep: runs.next()??,
            tail: runs.next()??,
        };
        runs.next().is_none().then_some(edges)
    }
}

fn escape(run: &str) -> String {
    let mut out = String::new();
    for c in run.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' ' => out.push_str("\\s"),
            c => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
        }
    }
    out
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            return None;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            's' => out.push(' '),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut hex = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        h => hex.push(h),
                    }
                }
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// One section: the first line, what is under it, and what was trimmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The first line, trimmed. A heading when `level` is above 0. Never empty
    /// and never more than one line, so it can be a claim's words.
    pub first_line: String,
    /// The heading's depth, 1 to 6. 0 when the first line is not a heading.
    pub level: u8,
    /// Everything under the first line, trimmed. Empty when nothing is.
    pub body: String,
    /// The whitespace trimmed off the parts above.
    pub edges: Edges,
    /// True for a chunk that carries on the section before it. Set when a section
    /// is cut for size; [`split`] never sets it.
    pub continues: bool,
}

/// Why a text cannot be split.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    /// The text is not empty and holds nothing but whitespace, so there is no
    /// first line to be a section's words.
    #[error("the text is only whitespace, so it has no section to hold")]
    Blank,
    /// A `\r` that no `\n` follows, at this byte offset.
    #[error(
        "a carriage return at byte {at} is not followed by a line feed; write line endings as \\n or \\r\\n"
    )]
    LoneCarriageReturn {
        /// Where it is.
        at: usize,
    },
}

/// The level of an ATX heading line (no line terminator), or nothing.
fn heading_level(line: &str) -> Option<u8> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    match rest[hashes..].chars().next() {
        None | Some(' ') | Some('\t') => Some(hashes as u8),
        _ => None,
    }
}

/// An open or closing code fence: the fence character and how long the run is.
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let c = rest.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let run = rest.chars().take_while(|x| *x == c).count();
    (run >= 3).then(|| (c, run, &rest[run..]))
}

/// The byte offsets where a heading line starts, outside any fenced block.
fn heading_starts(text: &str) -> Vec<(usize, u8)> {
    let mut starts = Vec::new();
    let mut open: Option<(char, usize)> = None;
    let mut at = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (open, fence_of(line)) {
            (None, Some((c, run, _))) => open = Some((c, run)),
            (Some((c, run)), Some((fc, frun, after)))
                if fc == c && frun >= run && after.trim().is_empty() =>
            {
                open = None;
            }
            (None, None) => {
                if let Some(level) = heading_level(line) {
                    starts.push((at, level));
                }
            }
            _ => {}
        }
        at += raw.len();
    }
    starts
}

/// Split markdown into sections at its headings.
///
/// An empty text has no sections. A text of only whitespace is [`SplitError::Blank`].
pub fn split(text: &str) -> Result<Vec<Section>, SplitError> {
    if let Some(at) = text
        .char_indices()
        .find(|(i, c)| *c == '\r' && text.as_bytes().get(i + 1) != Some(&b'\n'))
        .map(|(i, _)| i)
    {
        return Err(SplitError::LoneCarriageReturn { at });
    }
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if text.trim().is_empty() {
        return Err(SplitError::Blank);
    }
    let headings = heading_starts(text);
    let mut cuts: Vec<(usize, u8)> = Vec::new();
    if headings.first().is_none_or(|(at, _)| *at != 0) {
        cuts.push((0, 0));
    }
    cuts.extend(headings);
    let mut sections = Vec::new();
    // Whitespace before the first heading, when it is all that precedes it, rides
    // on that heading's lead rather than being a section with no words.
    let mut carried = String::new();
    for (n, (start, level)) in cuts.iter().enumerate() {
        let end = cuts.get(n + 1).map_or(text.len(), |(next, _)| *next);
        let raw = &text[*start..end];
        if raw.trim().is_empty() {
            carried.push_str(raw);
            continue;
        }
        let mut section = section_of(raw, *level);
        if !carried.is_empty() {
            section.edges.lead.insert_str(0, &carried);
            carried.clear();
        }
        sections.push(section);
    }
    Ok(sections)
}

/// One raw stretch of text as a section. The stretch holds a non-whitespace
/// character, so it has a first line.
fn section_of(raw: &str, level: u8) -> Section {
    let core = raw.trim();
    let lead = raw[..raw.len() - raw.trim_start().len()].to_string();
    let tail = raw[raw.trim_end().len()..].to_string();
    let (first_raw, rest_raw) = match core.find('\n') {
        Some(at) => (&core[..at], &core[at..]),
        None => (core, ""),
    };
    let first_line = first_raw.trim_end();
    let mid = first_raw[first_line.len()..].to_string();
    let body = rest_raw.trim_start();
    let sep = rest_raw[..rest_raw.len() - body.len()].to_string();
    Section {
        first_line: first_line.to_string(),
        level,
        body: body.to_string(),
        edges: Edges {
            lead,
            mid,
            sep,
            tail,
        },
        continues: false,
    }
}

/// The text the sections were split from: each section's parts and the
/// whitespace between them, in order.
pub fn join(sections: &[Section]) -> String {
    let mut out = String::new();
    for s in sections {
        out.push_str(&s.edges.lead);
        out.push_str(&s.first_line);
        out.push_str(&s.edges.mid);
        out.push_str(&s.edges.sep);
        out.push_str(&s.body);
        out.push_str(&s.edges.tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejoined(text: &str) -> String {
        join(&split(text).expect("a text that splits"))
    }

    /// **A text split and joined is the text.** The cases are the shapes whose
    /// edges a trimming store would lose: no final newline, many, a heading
    /// with trailing spaces, CRLF, blank lines under a heading, a preamble.
    #[test]
    fn a_text_split_and_joined_is_the_text() {
        for text in [
            "# One\n\nbody\n",
            "# One\n\nbody",
            "# One\n\nbody\n\n\n\n",
            "# One   \nbody\n## Two\t\nmore\n",
            "preamble\n\n# One\nbody\n",
            "\n\n  # One\nbody\n",
            "   \n# One\n",
            "# One\r\n\r\nbody\r\n## Two\r\nmore\r\n",
            "# A\n# B\n# C\n",
            "# Only a heading",
            "no heading at all\njust text\n",
            "# One\n\n```\n# not a heading\n```\n\n## Two\nx\n",
            "---\ntitle: a\n---\n\n# One\nx\n",
            "",
        ] {
            assert_eq!(rejoined(text), text, "{text:?}");
        }
    }

    /// **Headings split sections, and what is not a heading does not.**
    #[test]
    fn only_atx_headings_outside_code_split_a_section() {
        let text = "# One\ntext\n#hashtag\n    # indented four\n## Two\n```\n# in code\n```\n~~~\n## in tilde\n~~~\n";
        let sections = split(text).expect("splits");
        let firsts: Vec<&str> = sections.iter().map(|s| s.first_line.as_str()).collect();
        assert_eq!(firsts, ["# One", "## Two"]);
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[1].level, 2);
        assert!(sections[1].body.contains("# in code"));
        assert!(sections[1].body.contains("## in tilde"));
    }

    /// **A fence closes only on its own character and a run at least as long.**
    #[test]
    fn a_fence_closes_only_on_a_matching_run() {
        let text = "# One\n````\n```\n# still code\n````\n# Two\n";
        let firsts: Vec<String> = split(text)
            .expect("splits")
            .into_iter()
            .map(|s| s.first_line)
            .collect();
        assert_eq!(firsts, ["# One", "# Two"]);
    }

    /// **Text before the first heading is a level-0 section**, and a frontmatter
    /// block is one, verbatim.
    #[test]
    fn text_before_the_first_heading_is_a_section_of_level_zero() {
        let sections = split("---\ntitle: a\n---\n\n# One\nx\n").expect("splits");
        assert_eq!(sections[0].first_line, "---");
        assert_eq!(sections[0].level, 0);
        assert_eq!(sections[0].body, "title: a\n---");
        assert_eq!(sections[1].first_line, "# One");
    }

    /// **The parts are the words and the body, trimmed, and the trimmed whitespace
    /// is in the edges.**
    #[test]
    fn the_parts_are_trimmed_and_the_edges_hold_what_was_trimmed() {
        let sections = split("# One  \r\n\r\nbody text\n\n").expect("splits");
        assert_eq!(sections.len(), 1);
        let s = &sections[0];
        assert_eq!(s.first_line, "# One");
        assert_eq!(s.body, "body text");
        assert_eq!(s.edges.mid, "  \r");
        assert_eq!(s.edges.sep, "\n\r\n");
        assert_eq!(s.edges.tail, "\n\n");
    }

    /// **Only whitespace has no section, and a bare carriage return is refused.**
    #[test]
    fn a_blank_text_and_a_lone_carriage_return_are_refused() {
        assert_eq!(split("  \n \n"), Err(SplitError::Blank));
        assert_eq!(
            split("# a\rb\n"),
            Err(SplitError::LoneCarriageReturn { at: 3 })
        );
        assert_eq!(split(""), Ok(Vec::new()));
    }

    /// **The edges survive being written as one line of text.**
    #[test]
    fn edges_round_trip_through_their_one_line_form() {
        let edges = Edges {
            lead: "  \n".into(),
            mid: "\r".into(),
            sep: "\n\n\t".into(),
            tail: "\u{a0}\n".into(),
        };
        let line = edges.encode();
        assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
        assert_eq!(line, line.trim());
        assert_eq!(Edges::decode(&line), Some(edges));
        assert_eq!(Edges::decode("no"), None);
        assert_eq!(Edges::decode(";;"), None);
    }

    /// **Generated texts round-trip.** A seeded generator joins fragments that
    /// include every shape the splitter treats specially. A text that does not
    /// split must be one of the two refused kinds, never a lost byte.
    #[test]
    fn generated_texts_round_trip_or_are_refused_by_name() {
        const PIECES: &[&str] = &[
            "# H", "## H2", "### ", "#", "#x", "    # ", " # ", "text", " ", "\t", "\n", "\n\n",
            "\r\n", "```", "~~~", "````", "---", "|", "é", "\u{a0}", "- item", "> q",
        ];
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut split_count = 0;
        for _ in 0..3000 {
            let mut text = String::new();
            for _ in 0..(next() % 24) {
                text.push_str(PIECES[(next() % PIECES.len() as u64) as usize]);
            }
            match split(&text) {
                Ok(sections) => {
                    split_count += 1;
                    assert_eq!(join(&sections), text, "{text:?}");
                    for s in &sections {
                        assert!(!s.first_line.is_empty(), "{text:?}");
                        assert!(
                            !s.first_line.contains('\n') && !s.first_line.contains('\r'),
                            "{text:?}"
                        );
                        assert_eq!(s.first_line, s.first_line.trim(), "{text:?}");
                    }
                }
                Err(SplitError::Blank) => assert!(text.trim().is_empty(), "{text:?}"),
                Err(SplitError::LoneCarriageReturn { .. }) => {
                    panic!("the generator emits no bare carriage return: {text:?}")
                }
            }
        }
        assert!(
            split_count > 500,
            "only {split_count} generated texts split"
        );
    }
}
