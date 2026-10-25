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
//! **A leading frontmatter block is one opaque stretch**: a `---` first line to the
//! next `---` or `...` line, in which no line is a heading or a fence. A block
//! that never closes is read as ordinary text.
//!
//! **Only ATX headings split** (`#` to `######`, up to three spaces of indent,
//! then a space or the end of the line). A `#` line inside a fenced block of
//! code is code, and so is any line in it: a heading is never found there.
//!
//! **A carriage return that is not followed by a line feed is refused**, because
//! a claim's words are one line and `\r` ends a line for the readers that
//! matter. `\r\n` is an ordinary line ending and survives inside a section.

use super::{EntityId, EntityKind, Fact, FactId, FactStatus, MemoryError};

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

/// **Where a leading frontmatter block ends**, as a byte offset after its closing
/// line, or 0 when the text has none. The block opens with a line that is `---`
/// as the very first line of the text and closes at the next line that is `---`
/// or `...`. **A block that never closes is not frontmatter**: nothing says
/// where it would stop, so the text is read as it stands.
fn frontmatter_end(text: &str) -> usize {
    let mut lines = text.split_inclusive('\n');
    let bare = |raw: &str| {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        line.strip_suffix('\r').unwrap_or(line).to_string()
    };
    let Some(first) = lines.next() else {
        return 0;
    };
    if bare(first) != "---" {
        return 0;
    }
    let mut at = first.len();
    for raw in lines {
        at += raw.len();
        let line = bare(raw);
        if line == "---" || line == "..." {
            return at;
        }
    }
    0
}

/// The byte offsets where a heading line starts, outside any fenced block and
/// outside a leading frontmatter block.
fn heading_starts(text: &str) -> Vec<(usize, u8)> {
    let mut starts = Vec::new();
    let mut open: Option<(char, usize)> = None;
    let mut at = 0;
    let frontmatter = frontmatter_end(text);
    for raw in text.split_inclusive('\n') {
        if at < frontmatter {
            at += raw.len();
            continue;
        }
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

/// **The line feeds a seam needs added so exactly one blank line separates two
/// sections.** The seam is the tail of the section before and the lead of the
/// section after, and it is judged by its shape and not by a count alone: two
/// line feeds make a blank line, and a tail that ends on spaces or tabs after its
/// last line feed (or holds only them) would put the next heading at that indent,
/// where markdown reads it as code or as text. One more line feed puts those
/// characters on a line of their own and the heading at column 0. Line feeds the
/// seam already holds are kept: more than two stay, and nothing is taken away
/// (what is stored reads as stored).
fn line_feeds_needed(tail: &str, lead: &str, blank_line: bool) -> usize {
    let held = tail.matches('\n').count() + lead.matches('\n').count();
    let indented = tail
        .rsplit_once('\n')
        .map_or(tail, |(_, last)| last)
        .chars()
        .any(|c| c != '\r')
        && lead.is_empty();
    // A seam a move closes is only mended when it is glued: one that already holds
    // a line feed was stored by its writer and reads as stored (rule 342).
    let wanted = if blank_line || held == 0 { 2 } else { held };
    wanted.saturating_sub(held).max(usize::from(indented))
}

/// **Whether `sent` differs from the `held` edges only in a lead, and both leads
/// are line feeds and nothing else.** The seam between two sections is the one
/// change to a section's edges the engine makes to a record another session
/// wrote, and it moves no word: a lead of line feeds reads as nothing. A lead that
/// holds a space or a tab is an indent, which changes how the heading reads, and
/// any change to the other runs is a rewrite of the section.
pub fn differs_only_by_a_blank_lead(held: &str, sent: &str) -> bool {
    let (Some(held), Some(sent)) = (Edges::decode(held), Edges::decode(sent)) else {
        return false;
    };
    let line_feeds_only = |lead: &str| lead.chars().all(|c| c == '\n');
    held.mid == sent.mid
        && held.sep == sent.sep
        && held.tail == sent.tail
        && line_feeds_only(&held.lead)
        && line_feeds_only(&sent.lead)
}

/// **The lead a section must carry to start on a line of its own** behind a
/// section whose tail is `before_tail`: `lead` with the line feeds the seam lacks
/// put in front of it. The document's first section has nothing before it and
/// never keeps a leading blank, so `None` gives an empty lead.
pub fn lead_behind(before_tail: Option<&str>, lead: &str) -> String {
    match before_tail {
        None => String::new(),
        Some(tail) => format!("{}{lead}", "\n".repeat(line_feeds_needed(tail, lead, true))),
    }
}

/// **The lead a section must carry once a move has made it the neighbour of a
/// section it did not follow before.** A seam that holds no line feed is glued and
/// gets a blank line; one that holds any is as its writer left it. The document's
/// first section never keeps a leading blank.
pub fn lead_closing_a_gap(before_tail: Option<&str>, lead: &str) -> String {
    match before_tail {
        None => String::new(),
        Some(tail) => format!(
            "{}{lead}",
            "\n".repeat(line_feeds_needed(tail, lead, false))
        ),
    }
}

/// **The tail a section must carry so the section after it, whose lead is
/// `next_lead`, starts on a line of its own**: `tail` with the line feeds the seam
/// lacks added. A put writes this onto its own last section, so the section after
/// it, written by another session, is not touched.
pub fn tail_before(tail: &str, next_lead: Option<&str>) -> String {
    match next_lead {
        None => tail.to_string(),
        Some(lead) => format!("{tail}{}", "\n".repeat(line_feeds_needed(tail, lead, true))),
    }
}

/// The field that holds a section's heading depth.
pub const LEVEL: &str = "level";
/// The field that holds a section's place among its siblings: an order key.
pub const ORDER: &str = "order";
/// The field that holds the whitespace trimmed off a section's parts.
pub const EDGES: &str = "edges";
/// The field on a chunk that carries on a section: the local id of the record that
/// holds the section's heading. A section is found again by this link rather than
/// by what sits next to it, so a move that stopped partway can be finished.
pub const CONTINUES: &str = "continues";
/// The field on a record a restore writes: the address of the version it
/// reproduces. A restore reads this to know a section is already restored, and
/// reads nothing else on a record that has it. It is stored on records, so the
/// spelling is pinned by a test of its own.
pub const RESTORED_FROM: &str = "restored_from";

/// **Whether a fact is a section of the document it sits on**: a claim on a
/// document that carries the order key of a section. An ordinary claim about a
/// document is not part of its text.
pub fn is_section(fact: &Fact) -> bool {
    super::entries::is_placed(fact) && fact.home.kind() == Some(EntityKind::DOCUMENT)
}

/// **The own fields of the record that holds a section.** `edges` is always
/// written, so a reader never has to guess what was trimmed.
pub fn record_fields(section: &Section, order: &str) -> std::collections::BTreeMap<String, String> {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert(LEVEL.to_string(), section.level.to_string());
    fields.insert(ORDER.to_string(), order.to_string());
    fields.insert(EDGES.to_string(), section.edges.encode());
    if section.continues {
        // The record that holds the heading does not exist yet when the fields are
        // made, so the writer replaces this with that record's id.
        fields.insert(CONTINUES.to_string(), "true".to_string());
    }
    fields
}

/// **The section a record holds, and its order key**, or nothing when the record
/// carries no order and so is not a section.
///
/// A record a caller edited keeps the fields it was written with, and the edit
/// may have given a bare heading a body or taken the body away. The whitespace
/// between the first line and the body is then made to fit: a body needs a line
/// feed in front of it, and no body needs nothing there.
pub fn from_record(
    content: &str,
    details: Option<&str>,
    fields: &std::collections::BTreeMap<String, String>,
) -> Option<(String, Section)> {
    let order = fields.get(ORDER)?.clone();
    let body = details.unwrap_or("").to_string();
    let mut edges = fields
        .get(EDGES)
        .and_then(|encoded| Edges::decode(encoded))
        .unwrap_or_else(|| Edges {
            lead: String::new(),
            mid: String::new(),
            sep: "\n\n".to_string(),
            tail: "\n\n".to_string(),
        });
    if body.is_empty() {
        edges.sep.clear();
    } else if !edges.sep.contains('\n') {
        edges.sep = "\n\n".to_string();
    }
    Some((
        order,
        Section {
            first_line: content.to_string(),
            level: fields
                .get(LEVEL)
                .and_then(|level| level.parse().ok())
                .unwrap_or(0),
            body,
            edges,
            continues: fields
                .get(CONTINUES)
                .is_some_and(|value| !value.is_empty() && value != "false"),
        },
    ))
}

/// A section that cannot be cut small enough for a record. It carries the piece
/// no cut could get under the record's room, so a caller can say which part is in
/// the way and how big it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a section cannot be cut small enough for a record")]
pub struct TooBig {
    /// The piece that does not fit, as a record of its own: a fenced block, which
    /// no cut falls inside, or the first character of the text.
    pub unit: Box<Section>,
    /// Whether that piece is a fenced block.
    pub fenced: bool,
}

/// **The piece a cut could not get under the room**, from the text that was left
/// when no cut passed: the fenced block the text begins with, or its first
/// character when it begins with anything else.
fn too_big(rest: &str) -> TooBig {
    let start = rest.len() - rest.trim_start().len();
    let block = fenced_blocks(rest)
        .into_iter()
        .find(|block| block.start == start);
    let (text, fenced) = match block {
        Some(block) => (rest[block].to_string(), true),
        None => (
            rest[start..].chars().next().unwrap_or(' ').to_string(),
            false,
        ),
    };
    let mut unit = section_of(&text, 0);
    unit.continues = true;
    TooBig {
        unit: Box::new(unit),
        fenced,
    }
}

/// **Cut a section into chunks that each pass `fits`**, and no more chunks than
/// that takes. The text is cut at the last blank line that leaves a chunk
/// passing; when the first paragraph alone does not pass, at the last line end;
/// when one line alone does not pass, at the last character. The first chunk
/// keeps the section's heading and level. Every later chunk has level 0 and
/// `continues` set, and the whitespace at each cut is the first chunk's `tail`,
/// so [`join`] of the chunks is [`join`] of the section.
///
/// `fits` must answer yes to a smaller piece whenever it answers yes to a larger
/// one containing it; a record's size does.
pub fn fit(
    section: Section,
    fits: &mut dyn FnMut(&Section) -> bool,
) -> Result<Vec<Section>, TooBig> {
    if fits(&section) {
        return Ok(vec![section]);
    }
    let level = section.level;
    let continues = section.continues;
    let mut rest = join(std::slice::from_ref(&section));
    let mut chunks = Vec::new();
    loop {
        let as_chunk = |raw: &str| {
            let mut chunk = section_of(raw, if chunks.is_empty() { level } else { 0 });
            chunk.continues = if chunks.is_empty() { continues } else { true };
            chunk
        };
        let whole = as_chunk(&rest);
        if fits(&whole) {
            chunks.push(whole);
            return Ok(chunks);
        }
        let Some(cut) = largest_cut(&rest, &mut |prefix| fits(&as_chunk(prefix))) else {
            return Err(too_big(&rest));
        };
        let chunk = as_chunk(&rest[..cut]);
        chunks.push(chunk);
        rest = rest[cut..].to_string();
    }
}

/// **The byte ranges of the fenced blocks in `text`**, each from the start of its
/// opening line to the end of its closing line. A fence opens and closes by the
/// rule [`heading_starts`] reads fences with. A fence nothing closes is not a
/// block: it holds no code a cut could break.
fn fenced_blocks(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut blocks = Vec::new();
    let mut open: Option<(char, usize, usize)> = None;
    let mut at = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (open, fence_of(line)) {
            (None, Some((c, run, _))) => open = Some((c, run, at)),
            (Some((c, run, start)), Some((fc, frun, after)))
                if fc == c && frun >= run && after.trim().is_empty() =>
            {
                blocks.push(start..at + raw.len());
                open = None;
            }
            _ => {}
        }
        at += raw.len();
    }
    blocks
}

/// The longest prefix of `text` that passes, cut at the coarsest kind of place
/// that has one: after a blank line, after a line, after a character. **No cut
/// falls inside a fenced block**, so code is never split from its fences; a
/// block too big to fit whole leaves no cut and the section is refused.
fn largest_cut(text: &str, passes: &mut dyn FnMut(&str) -> bool) -> Option<usize> {
    let blocks = fenced_blocks(text);
    let first_text = text.len() - text.trim_start().len();
    let blank_line_ends = text
        .match_indices('\n')
        .filter(|(at, _)| {
            let after = &text[at + 1..];
            let blank = after.len() - after.trim_start_matches([' ', '\t', '\r']).len();
            after[blank..].starts_with('\n')
        })
        .map(|(at, _)| {
            let after = &text[at + 1..];
            let skipped = after.len() - after.trim_start().len();
            at + 1 + skipped
        });
    let line_ends = text.match_indices('\n').map(|(at, _)| at + 1);
    let characters = text.char_indices().map(|(at, c)| at + c.len_utf8());
    let tiers: [Vec<usize>; 3] = [
        blank_line_ends.collect(),
        line_ends.collect(),
        characters.collect(),
    ];
    for tier in tiers {
        let candidates: Vec<usize> = tier
            .into_iter()
            .filter(|at| *at < text.len() && *at > first_text && !text[..*at].trim().is_empty())
            .filter(|at| {
                !blocks
                    .iter()
                    .any(|block| *at > block.start && *at < block.end)
            })
            .collect();
        // Binary search for the last candidate that passes.
        let (mut low, mut high) = (0, candidates.len());
        while low < high {
            let mid = (low + high) / 2;
            if passes(&text[..candidates[mid]]) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        if low > 0 {
            return Some(candidates[low - 1]);
        }
    }
    None
}

/// **A merge of a document and something that is not one is refused.** Two
/// documents merge into one that holds both; a document and a thing do not
/// merge, in either direction. `None` when the two sides are alike.
pub fn refuses_a_merge_of_a_document_and_a_thing(
    folded: &EntityId,
    survivor: &EntityId,
) -> Option<MemoryError> {
    let is_document = |side: &EntityId| side.kind() == Some(EntityKind::DOCUMENT);
    match (is_document(folded), is_document(survivor)) {
        (true, false) => Some(MemoryError::DocumentMergedWithAThing {
            document: folded.to_string(),
            other: survivor.to_string(),
        }),
        (false, true) => Some(MemoryError::DocumentMergedWithAThing {
            document: survivor.to_string(),
            other: folded.to_string(),
        }),
        _ => None,
    }
}

/// **The records an order key places, in order**: every record that carries the
/// key, whatever its status or the kind its home names, by key and then by id. A
/// claim with no key is not placed. This is the one reading of that order, for a
/// document's sections and a list's entries alike (rule 498), so the live
/// sections, a merge's last key and the records a merge moves cannot disagree
/// about what the order is. Two records that carry one key, which only a
/// hand-written key can make, read in the order of their claim numbers.
///
/// The kind of the home is not consulted: a store holds a record's home as the
/// thing's permanent id, which names no kind, and has already judged what the
/// thing is.
pub fn placed_in_order(facts: &[Fact]) -> Vec<&Fact> {
    let mut placed: Vec<&Fact> = facts
        .iter()
        .filter(|fact| fact.fields.contains_key(ORDER))
        .collect();
    placed.sort_by(|a, b| {
        (&a.fields[ORDER], claim_sequence(a), &a.id.0).cmp(&(
            &b.fields[ORDER],
            claim_sequence(b),
            &b.id.0,
        ))
    });
    placed
}

/// **The number in a claim's id**, `f10` being 10: the place a record has in the
/// order its thing's claims were written. Ids compare as text in the wrong order
/// (`f10` before `f9`), so every tie between records reads by this number first.
/// An id that is no number reads after every numbered one.
pub fn claim_sequence(fact: &Fact) -> u64 {
    fact.id
        .0
        .trim_start_matches('f')
        .parse()
        .unwrap_or(u64::MAX)
}

/// **What a merge of one document into another writes onto a folded section's
/// record**: where the section goes, and which heading a chunk now continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocated {
    /// The record, by the id it was stored under on the folded document.
    pub fact: FactId,
    /// The order key it takes: after every section the survivor has.
    pub order: String,
    /// For a chunk that carries a section on: the id its heading has under the
    /// survivor. `None` for a heading, and for a chunk whose heading the merge
    /// gave no new id.
    pub continues: Option<String>,
}

/// **Place the folded document's sections after the survivor's last section.**
///
/// `survivor` and `folded` are the two documents' records, `claims` the merge's
/// map from each folded record's old id to the id it has under the survivor.
/// The folded sections that stand keep their own order among themselves and take
/// keys minted after the greatest order key the survivor holds, archived
/// sections included, so a section restored later cannot land among them. Each
/// chunk's `continues` is rewritten to its heading's new id, so a reader that
/// finds a section by that link finds it on the survivor.
///
/// The order keys are the server's to mint (rule 449); this is the one place a
/// merge mints them, called by every store.
pub fn append_after(
    survivor: &[Fact],
    folded: &[Fact],
    claims: &[(FactId, FactId)],
) -> Result<Vec<Relocated>, super::order::OrderError> {
    let last = placed_in_order(survivor)
        .last()
        .map(|fact| fact.fields[ORDER].as_str());
    let standing: Vec<&Fact> = placed_in_order(folded)
        .into_iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .collect();
    let orders = super::order::spread(last, None, standing.len())?;
    Ok(standing
        .into_iter()
        .zip(orders)
        .map(|(fact, order)| Relocated {
            fact: fact.id.clone(),
            order,
            continues: fact
                .fields
                .get(CONTINUES)
                .filter(|head| !head.is_empty() && head.as_str() != "false")
                .and_then(|head| {
                    claims
                        .iter()
                        .find(|(was, _)| was.0 == *head)
                        .map(|(_, now)| now.0.clone())
                }),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejoined(text: &str) -> String {
        join(&split(text).expect("a text that splits"))
    }

    /// **One blank line separates two sections whatever their edges held, judged
    /// by the shape of the seam.** Each pair of a tail and a lead is joined with
    /// the lead `lead_behind` returns, and the text splits back into the same two
    /// sections, the second one starting at column 0 after a blank line. Seams that
    /// already hold two line feeds, or more, are kept as they are.
    #[test]
    fn a_seam_is_one_blank_line_whatever_its_edges_held() {
        for (tail, lead, gap) in [
            ("", "", "\n\n"),
            ("\n", "", "\n\n"),
            ("\n\n", "", "\n\n"),
            ("\n\n\n", "", "\n\n\n"),
            ("", "\n\n", "\n\n"),
            ("", "\n", "\n\n"),
            ("\n\n    ", "", "\n\n    \n"),
            ("\n    ", "", "\n    \n"),
            ("    ", "", "    \n\n"),
            ("\r\n", "", "\r\n\n"),
        ] {
            let lead = lead_behind(Some(tail), lead);
            let text = format!("# A\ntext{tail}{lead}## B\nx");
            assert_eq!(
                &text["# A\ntext".len()..text.len() - "## B\nx".len()],
                gap,
                "tail {tail:?}"
            );
            let sections = split(&text).expect("the text splits");
            assert_eq!(sections.len(), 2, "{text:?}");
            assert_eq!(sections[1].first_line, "## B", "{text:?}");
            assert_eq!(join(&sections), text);
        }
    }

    /// **Only a blank lead may change on a record someone else wrote.** The pair:
    /// a lead that holds a character, a change to any other run, and edges that
    /// do not decode are each a rewrite.
    #[test]
    fn only_a_blank_lead_may_differ() {
        let edges = |lead: &str, tail: &str| {
            Edges {
                lead: lead.into(),
                mid: String::new(),
                sep: "\n".into(),
                tail: tail.into(),
            }
            .encode()
        };
        assert!(differs_only_by_a_blank_lead(
            &edges("", ""),
            &edges("\n\n", "")
        ));
        assert!(differs_only_by_a_blank_lead(
            &edges("\n\n", ""),
            &edges("", "")
        ));
        assert!(!differs_only_by_a_blank_lead(
            &edges("", ""),
            &edges("X", "")
        ));
        // A space or a tab in either lead is an indent, and a change to the middle
        // or the separator alone is a rewrite.
        for (held, sent) in [
            (edges("", ""), edges(" ", "")),
            (edges("", ""), edges("\n\t", "")),
            (edges("  ", ""), edges("\n", "")),
            (edges("\t", ""), edges("\n", "")),
        ] {
            assert!(
                !differs_only_by_a_blank_lead(&held, &sent),
                "{held} -> {sent}"
            );
        }
        let with = |mid: &str, sep: &str| {
            Edges {
                lead: String::new(),
                mid: mid.into(),
                sep: sep.into(),
                tail: String::new(),
            }
            .encode()
        };
        assert!(!differs_only_by_a_blank_lead(
            &with("", "\n"),
            &with("\r", "\n")
        ));
        assert!(!differs_only_by_a_blank_lead(
            &with("", "\n"),
            &with("", "\n\n")
        ));
        assert!(!differs_only_by_a_blank_lead(
            &edges("", ""),
            &edges("\n", "\n")
        ));
        assert!(!differs_only_by_a_blank_lead("", "not edges"));
    }

    /// **A seam a move closes is mended only when it is glued.** A gap that holds a
    /// line feed stays as stored; one that holds none gets the blank line; the
    /// first section keeps no lead.
    #[test]
    fn a_closed_gap_is_mended_only_when_glued() {
        assert_eq!(lead_closing_a_gap(Some(""), ""), "\n\n");
        assert_eq!(lead_closing_a_gap(Some("\n"), ""), "");
        assert_eq!(lead_closing_a_gap(Some(""), "\n"), "\n");
        assert_eq!(lead_closing_a_gap(Some("\n\n    "), ""), "\n");
        assert_eq!(lead_closing_a_gap(None, "\n\n"), "");
    }

    /// **The first section never keeps a leading blank, and a lead of its own
    /// behind another section is kept.** The pair for the seam above: an indent
    /// the writer put on the heading is part of what the section is and stays.
    #[test]
    fn a_first_lead_is_empty_and_a_writers_own_indent_stays() {
        assert_eq!(lead_behind(None, "\n\n"), "");
        assert_eq!(lead_behind(Some("\n\n"), "  "), "  ");
    }

    /// **A put gives its own last section the tail the section after it needs.**
    /// With no section after it the tail is as it was.
    #[test]
    fn a_tail_gets_the_line_feeds_the_next_section_lacks() {
        assert_eq!(tail_before("", Some("")), "\n\n");
        assert_eq!(tail_before("\n", Some("")), "\n\n");
        assert_eq!(tail_before("", Some("\n\n")), "");
        assert_eq!(tail_before("\n\n", None), "\n\n");
        assert_eq!(tail_before("", None), "");
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

    fn size_of(section: &Section) -> usize {
        section.first_line.len() + section.body.len()
    }

    /// **A section cut for size joins back to the section.** The cut falls on
    /// every kind of place, and the join is the text, character for character,
    /// including the blank lines the cut sits in.
    #[test]
    fn a_section_cut_for_size_joins_back_to_itself() {
        let text = "## Title  \n\nfirst paragraph here\nwith two lines\n\n\nsecond paragraph\n\nthird \u{e9}\u{e9} paragraph, a little longer\n\n";
        for cap in [20, 25, 40, 70] {
            let section = split(text).expect("splits").remove(0);
            let chunks = fit(section.clone(), &mut |s| size_of(s) <= cap).expect("fits");
            assert_eq!(join(&chunks), join(&[section]), "cap {cap}");
            assert!(chunks.iter().all(|c| size_of(c) <= cap), "cap {cap}");
            assert_eq!(chunks[0].level, 2);
            assert!(!chunks[0].continues);
            assert!(chunks[1..].iter().all(|c| c.level == 0 && c.continues));
        }
    }

    /// **A section that already fits is returned as it is**, and one that does not
    /// is cut at a blank line rather than inside a paragraph when it can be.
    #[test]
    fn a_cut_falls_on_a_blank_line_when_one_leaves_a_chunk_that_fits() {
        let section = split("# T\n\naaaa bbbb\n\ncccc dddd\n\neeee ffff\n")
            .expect("splits")
            .remove(0);
        let whole = fit(section.clone(), &mut |_| true).expect("fits");
        assert_eq!(whole, vec![section.clone()]);
        let chunks = fit(section, &mut |s| size_of(s) <= 14).expect("fits");
        let firsts: Vec<&str> = chunks.iter().map(|c| c.first_line.as_str()).collect();
        assert_eq!(firsts, ["# T", "cccc dddd", "eeee ffff"]);
        assert_eq!(chunks[0].body, "aaaa bbbb");
    }

    /// **A paragraph that is over the cap alone is cut inside**, at line ends
    /// first and at characters when one line is over too.
    #[test]
    fn a_paragraph_over_the_cap_is_cut_at_lines_and_then_at_characters() {
        let lines = split("# T\none one\ntwo two\nthree three\n")
            .expect("splits")
            .remove(0);
        let chunks = fit(lines.clone(), &mut |s| size_of(s) <= 14).expect("fits");
        assert!(chunks.len() > 1);
        assert_eq!(join(&chunks), join(&[lines]));
        let long = split("# T\nabcdefghijklmnopqrstuvwxyz\n")
            .expect("splits")
            .remove(0);
        let chunks = fit(long.clone(), &mut |s| size_of(s) <= 8).expect("fits");
        assert!(chunks.iter().all(|c| size_of(c) <= 8));
        assert_eq!(join(&chunks), join(&[long]));
    }

    /// **A section nothing of which fits is refused**, not cut into nothing.
    #[test]
    fn a_section_no_piece_of_which_fits_is_refused() {
        let section = split("# Title\nbody\n").expect("splits").remove(0);
        let refused = fit(section, &mut |_| false).expect_err("nothing fits");
        assert!(!refused.fenced, "a plain text is not a fenced block");
        assert_eq!(refused.unit.first_line, "#");
    }

    /// **The coarsest cut that leaves a chunk passing wins**, even when a finer
    /// one would leave a longer chunk: a paragraph is cut inside only when it
    /// alone is over.
    #[test]
    fn a_paragraph_is_kept_whole_when_a_blank_line_cut_will_do() {
        let section = split("# T\n\naaaa\n\nbbbb\ncccc\n")
            .expect("splits")
            .remove(0);
        let chunks = fit(section, &mut |s| size_of(s) <= 13).expect("fits");
        let firsts: Vec<&str> = chunks.iter().map(|c| c.first_line.as_str()).collect();
        assert_eq!(firsts, ["# T", "bbbb"]);
        assert_eq!(chunks[1].body, "cccc");
    }

    /// **A cut never falls inside a fenced block.** The natural cut here, the
    /// last blank line that leaves a chunk passing, is inside the fence, so it is
    /// skipped and the cut falls before the block, which then rides whole in the
    /// next chunk. **Paired**: the same text with no fence is cut at that blank
    /// line, so the skip is the fence's doing and not a cut that never moved.
    #[test]
    fn a_cut_never_falls_inside_a_fenced_block() {
        let text = "# H\n\naaaa aaaa\n\n```\nxxxx\n\nyyyy\n```\n\nbbbb\n";
        let section = split(text).expect("splits").remove(0);
        let chunks = fit(section, &mut |s| size_of(s) <= 28).expect("fits");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].first_line, "# H");
        assert_eq!(chunks[0].body, "aaaa aaaa");
        assert_eq!(chunks[1].first_line, "```");
        assert_eq!(chunks[1].body, "xxxx\n\nyyyy\n```\n\nbbbb");
        assert_eq!(join(&chunks), text);

        let open = "# H\n\naaaa aaaa\n\nzzzz\nxxxx\n\nyyyy\nwwww\n\nbbbb\n";
        let section = split(open).expect("splits").remove(0);
        let chunks = fit(section, &mut |s| size_of(s) <= 28).expect("fits");
        assert_eq!(chunks[1].first_line, "yyyy");
        assert_eq!(join(&chunks), open);
    }

    /// **A fenced block bigger than a record cannot be cut, so the section is
    /// refused**, the way a section nothing of which fits is. **Paired**: the same
    /// text with no fence is cut and fits.
    #[test]
    fn a_fenced_block_bigger_than_the_cap_is_refused_not_cut() {
        let fenced = split("# H\n\n```\nxxxx\n\nyyyy\n```\n")
            .expect("splits")
            .remove(0);
        let refused = fit(fenced, &mut |s| size_of(s) <= 12).expect_err("the block is bigger");
        assert!(refused.fenced, "the piece in the way is a fenced block");
        assert_eq!(
            join(std::slice::from_ref(&*refused.unit)),
            "```\nxxxx\n\nyyyy\n```\n",
            "the piece is the block alone, fences and all"
        );
        let plain = split("# H\n\nxxxx\n\nyyyy\n").expect("splits").remove(0);
        let chunks = fit(plain, &mut |s| size_of(s) <= 12).expect("fits");
        assert!(chunks.len() > 1);
    }

    /// **No more chunks than the cap takes.** Thirty short paragraphs of two
    /// lengths are cut the way a greedy fill of the same paragraphs cuts them.
    #[test]
    fn the_chunks_are_as_few_as_the_cap_allows() {
        let paragraphs: Vec<String> = (0..30).map(|i| "w".repeat(1 + i % 2)).collect();
        let text = format!("# T\n\n{}\n", paragraphs.join("\n\n"));
        let cap = 20;
        let section = split(&text).expect("splits").remove(0);
        let chunks = fit(section, &mut |s| size_of(s) <= cap).expect("fits");
        let mut expected = 0;
        let mut taken = vec!["# T".to_string()];
        for paragraph in &paragraphs {
            taken.push(paragraph.clone());
            let candidate = split(&taken.join("\n\n")).expect("splits").remove(0);
            if size_of(&candidate) > cap {
                expected += 1;
                taken = vec![paragraph.clone()];
            }
        }
        expected += 1;
        assert_eq!(chunks.len(), expected);
        // Each chunk is as long as it can be: the next chunk's first paragraph
        // would not have fitted on the end of it.
        for pair in chunks.windows(2) {
            let longer = format!("{}{}", join(&pair[..1]), pair[1].first_line);
            let longer = split(&longer).expect("splits").remove(0);
            assert!(size_of(&longer) > cap, "{longer:?} fitted");
        }
    }

    /// **A section written as a record and read back is the section.** The
    /// fields are the record's own, and a record with no order is no section.
    #[test]
    fn a_section_survives_the_record_that_holds_it() {
        for text in [
            "## Two  \r\n\r\nbody\r\n\r\n\r\n",
            "# Only a heading\n",
            "  # Indented\ntext",
        ] {
            let section = split(text).expect("splits").remove(0);
            let fields = record_fields(&section, "V");
            let details = (!section.body.is_empty()).then_some(section.body.as_str());
            let (order, back) =
                from_record(&section.first_line, details, &fields).expect("a section");
            assert_eq!((order.as_str(), &back), ("V", &section), "{text:?}");
        }
        let mut cut = split("# T\nx\n").expect("splits").remove(0);
        cut.continues = true;
        let fields = record_fields(&cut, "a");
        assert_eq!(fields.get(CONTINUES).map(String::as_str), Some("true"));
        assert!(!record_fields(&split("# T\n").expect("splits")[0], "a").contains_key(CONTINUES));
        assert!(from_record("# T", None, &Default::default()).is_none());
    }

    /// **An edit that changes whether a section has a body keeps the join
    /// well-formed**: a body gets a line feed in front of it and a missing body
    /// leaves none behind.
    #[test]
    fn an_edit_that_adds_or_takes_a_body_keeps_the_lines_apart() {
        let bare = split("# T\n\n# U\nx\n").expect("splits").remove(0);
        let fields = record_fields(&bare, "a");
        let (_, with_body) = from_record("# T", Some("now text"), &fields).expect("a section");
        assert_eq!(join(&[with_body]), "# T\n\nnow text\n\n");
        let full = split("# T\n\nold\n").expect("splits").remove(0);
        let fields = record_fields(&full, "a");
        let (_, without) = from_record("# T", None, &fields).expect("a section");
        assert_eq!(join(&[without]), "# T\n");
    }

    /// **A `#` line inside a leading frontmatter block is a line of that block,
    /// not a heading.** The block stays the first section, verbatim, and the
    /// paired positive: a heading after the block still splits.
    #[test]
    fn a_hash_line_inside_frontmatter_is_not_a_heading() {
        let text = "---\ntitle: Notes\n# internal note\n---\n\n# Start\nbody\n";
        let sections = split(text).expect("splits");
        let firsts: Vec<&str> = sections.iter().map(|s| s.first_line.as_str()).collect();
        assert_eq!(firsts, ["---", "# Start"]);
        assert_eq!(sections[0].level, 0);
        assert_eq!(sections[0].body, "title: Notes\n# internal note\n---");
        assert_eq!(sections[1].level, 1);
        assert_eq!(join(&sections), text);
    }

    /// **The block ends at `---` or `...`, may use CRLF, and a fence marker inside
    /// it opens nothing.**
    #[test]
    fn frontmatter_ends_at_a_dash_or_dot_line_and_hides_fence_marks() {
        for text in [
            "---\r\ntitle: a\r\n# note\r\n---\r\n# Start\r\nx\r\n",
            "---\ntitle: a\n# note\n...\n# Start\nx\n",
            "---\nnote: ```\n# note\n---\n# Start\nx\n",
        ] {
            let sections = split(text).expect("splits");
            let firsts: Vec<&str> = sections.iter().map(|s| s.first_line.as_str()).collect();
            assert_eq!(firsts, ["---", "# Start"], "{text:?}");
            assert_eq!(join(&sections), text);
        }
    }

    /// **A block that never closes is not frontmatter**, and neither is a `---`
    /// that is not the first line: headings split as they always do.
    #[test]
    fn a_dash_line_that_does_not_open_and_close_a_block_at_the_top_is_text() {
        for (text, firsts) in [
            ("---\ntitle: a\n# real\nbody\n", vec!["---", "# real"]),
            ("intro\n---\n# one\n---\n", vec!["intro", "# one"]),
            ("\n---\n# one\n---\n", vec!["---", "# one"]),
        ] {
            let sections = split(text).expect("splits");
            let got: Vec<&str> = sections.iter().map(|s| s.first_line.as_str()).collect();
            assert_eq!(got, firsts, "{text:?}");
            assert_eq!(join(&sections), text);
        }
    }

    /// The key a restore marks its copies with is stored on records, and nothing
    /// outside this process declares it, so its spelling is pinned here.
    #[test]
    fn the_key_a_restore_marks_its_copies_with_is_restored_from() {
        assert_eq!(RESTORED_FROM, "restored_from");
    }

    /// A section record of a document, carrying only an order key.
    fn section_record(local: &str, order: &str, status: FactStatus) -> Fact {
        use crate::memory::EntityId;
        let home = EntityId("document:homer".into());
        Fact {
            app_stamp: None,
            written_by_another: false,
            id: FactId(local.into()),
            home: home.clone(),
            subject: home,
            content: format!("# Section {local}"),
            details: None,
            provenance: crate::memory::Provenance::Inference,
            standing: crate::memory::Standing::Open,
            status,
            recorded_at: jiff::civil::date(2026, 10, 16),
            happened_at: None,
            happened_through: None,
            inserted_at: None,
            stale_after: None,
            receipt: None,
            edge: None,
            fields: [(ORDER.to_string(), order.to_string())]
                .into_iter()
                .collect(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            sets: Default::default(),
            reaching: Default::default(),
            field_links: Default::default(),
        }
    }

    /// **Two records that carry one order key read in the order of their claim numbers**,
    /// and a record with a smaller key reads before both. Only a hand-written key
    /// makes a tie, and the read still has to be the same read every time.
    #[test]
    fn records_that_share_an_order_key_read_in_the_order_of_their_ids() {
        let facts = [
            section_record("f7", "m", FactStatus::Active),
            section_record("f2", "m", FactStatus::Active),
            section_record("f9", "a", FactStatus::Active),
        ];
        let ids: Vec<&str> = placed_in_order(&facts)
            .iter()
            .map(|fact| fact.id.0.as_str())
            .collect();
        assert_eq!(ids, ["f9", "f2", "f7"]);
    }

    /// **Records that share an order key read by claim number, and claim 9 reads
    /// before claim 10**, which text order puts the other way round.
    #[test]
    fn records_that_share_an_order_key_read_by_claim_number_not_by_text() {
        let facts = [
            section_record("f10", "m", FactStatus::Active),
            section_record("f9", "m", FactStatus::Active),
        ];
        let ids: Vec<&str> = placed_in_order(&facts)
            .iter()
            .map(|fact| fact.id.0.as_str())
            .collect();
        assert_eq!(ids, ["f9", "f10"]);
    }

    /// **A merge moves tied sections in the order of their ids.** The folded
    /// document holds two standing sections under one key; they take keys after
    /// the survivor's last, the lower id first.
    #[test]
    fn a_merge_places_folded_sections_that_share_a_key_in_the_order_of_their_ids() {
        let survivor = [section_record("f1", "m", FactStatus::Active)];
        let folded = [
            section_record("f5", "g", FactStatus::Active),
            section_record("f3", "g", FactStatus::Active),
        ];

        let placed = append_after(&survivor, &folded, &[]).expect("room");

        let ids: Vec<&str> = placed.iter().map(|moved| moved.fact.0.as_str()).collect();
        assert_eq!(ids, ["f3", "f5"]);
        assert!(placed[0].order < placed[1].order, "{placed:?}");
        assert!(placed[0].order.as_str() > "m", "{placed:?}");
    }
}
