//! The sessions page, encoded — pure functions, no I/O.
//!
//! **One page per bot, and two regions on it**, which is not a layout choice but
//! the shape of the domain: a session's state and focus are *current truth*, and
//! its chronology is *append-only with per-entry identity*. Those want opposite
//! things from a store, so they get different regions and different writes.
//!
//! * **The sessions table** — one row per session, the model's "a session is a
//!   row". State and focus are rewritten in place, which is a whole-document
//!   write, which is exactly what rewriting truth in place means.
//! * **The chronology** — one fenced block per entry, below the table, each
//!   appended with Outline's `append` rather than by rewriting the page.
//!
//! # Everything here was decided by probing the live API, not by reading docs
//!
//! Four findings, each of which killed a design that looked better on paper:
//!
//! 1. **`documents.update` with `append: true` is a genuine append.** So an
//!    append does not have to be a read-modify-write, and the trap the Vikunja
//!    adapter names — an append-only record quietly rewritten whole on every
//!    write — is avoidable here rather than merely regrettable.
//! 2. **An append cannot extend a markdown table.** Outline separates appended
//!    content with a blank line and starts a new block; a leading newline
//!    changes nothing. So the chronology cannot be table rows, however much it
//!    would like to be.
//! 3. **Comments are not an option**, though they are the obvious analogue of
//!    the Vikunja design: `comments.update` refuses markdown and accepts only
//!    ProseMirror JSON, and a comment record carries no markdown at all. An
//!    entry is ordinary multi-line prose, so that would mean hand-rolling a
//!    rich-text serializer on the write path and a parser on the read path.
//! 4. **A fence longer than three backticks is normalized back to three.** That
//!    is why [`escape_text`] exists instead of a wider fence: text quoting a
//!    code block would otherwise close its own entry early, and everything
//!    after it would be read as the next entry.
//!
//! What a fence *does* buy, and the reason the chronology uses one at all: text
//! inside it survives the editor model **verbatim** — asterisks, hashes,
//! dashes, blank lines and non-ASCII all came back byte-identical, where bare
//! prose is normalized. An entry is somebody's account of what they were doing
//! and it reads back as they wrote it.

use jiff::Timestamp;

use jojobot_domain::memory::{EntityId, MACHINERY_FIELD, validate_subject};
use jojobot_domain::session::{
    EntryId, JournalEntry, SessionId, SessionState, Sid, is_readable_sid,
};

/// The value of the machinery field on a sessions page. What keeps the page out
/// of the search index; see `MACHINERY_FIELD`.
pub(super) const SESSIONS: &str = "sessions";

/// The machine-block field naming the bot whose sessions these are.
const OF: &str = "of";

/// The header above the table of sessions.
pub(super) const SESSIONS_HEADER: &str = "### ⚙ sessions";
/// The header above the chronology.
pub(super) const CHRONOLOGY_HEADER: &str = "### ⚙ chronology";

/// The table's header row and separator, in the shape a write produces. Outline
/// re-serializes a table to padded form on every save, so neither is ever
/// compared byte-for-byte — the reader keys on the leading pipe, exactly as the
/// fact table's reader does.
const TABLE_HEADER: &str = "| id | sid | started | state | focus |";
const TABLE_SEP: &str = "| --- | --- | --- | --- | --- |";

/// The info string on a chronology entry's fence.
const ENTRY_FENCE: &str = "```jojobot-entry";

/// One row of the sessions table — a session without its chronology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub id: SessionId,
    pub sid: Option<Sid>,
    pub started_at: Timestamp,
    pub state: SessionState,
    pub focus: String,
}

/// The markdown a fresh sessions page is seeded with.
pub(super) fn seeded_page(bot: &EntityId) -> String {
    format!(
        "_Managed by jojobot — one row per session of this bot, and its chronology below. \
         Not searched._\n\n\
         ```yaml\n{MACHINERY_FIELD}: {SESSIONS}\n{OF}: {bot}\n```\n\n\
         {SESSIONS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n\n{CHRONOLOGY_HEADER}\n"
    )
}

/// The bot a sessions page belongs to, off its `of:` line — `None` if the page
/// is not a sessions page or does not say.
pub(super) fn parse_bot(doc: &str) -> Option<EntityId> {
    let lines: Vec<&str> = doc.lines().collect();
    let (open, close) = machine_block(&lines)?;
    let inside = &lines[open + 1..close - 1];
    if inside
        .iter()
        .find_map(|l| field(l, MACHINERY_FIELD))
        .as_deref()
        != Some(SESSIONS)
    {
        return None;
    }
    let bot = EntityId(inside.iter().find_map(|l| field(l, OF))?);
    validate_subject(&bot).ok().map(|()| bot)
}

/// The fenced block carrying this page's machine fields — the first fence that
/// declares itself jojobot's. Restated here rather than shared with the entity
/// codec because the two blocks identify themselves differently and a page that
/// satisfied both tests would be two things at once.
fn machine_block(lines: &[&str]) -> Option<(usize, usize)> {
    let is_fence = |l: &&str| l.trim_start().starts_with("```");
    let mut i = 0;
    while i < lines.len() {
        if !is_fence(&lines[i]) {
            i += 1;
            continue;
        }
        let close = i + 1 + lines[i + 1..].iter().position(is_fence)?;
        if lines[i + 1..close]
            .iter()
            .any(|l| field(l, MACHINERY_FIELD).is_some())
        {
            return Some((i, close + 1));
        }
        i = close + 1;
    }
    None
}

/// The value of a `key: value` line, if this line is one.
fn field(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

// --- the sessions table ------------------------------------------------------

/// Split a table row into its cells, pipes at both ends dropped.
///
/// **Splitting has to honour the escape**, which is the whole reason this is
/// not a `split('|')`: a focus carrying a pipe is escaped on the way out, and a
/// naive split would cut the row there anyway — taking the state column with it
/// and reading the tail of somebody's focus as their session's state.
fn cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let inner = row.strip_prefix('|').unwrap_or(row);
    let inner = inner.strip_suffix('|').unwrap_or(inner);

    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('|' | '\\')) => {
                cur.push(chars.next().expect("peeked"));
            }
            '|' => {
                cells.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// A pipe inside a focus would forge a cell, and a focus is free text the
/// operator reads. Escaped rather than refused: `validate_focus` already says
/// what a focus may be, and a pipe is not on its list of sins.
fn escape_cell(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Render one session as its row.
pub(super) fn render_row(row: &Row) -> String {
    format!(
        "| {} | {} | {} | {} | {} |",
        row.id,
        row.sid.as_ref().map(Sid::as_str).unwrap_or("-"),
        row.started_at,
        row.state.as_token(),
        escape_cell(&row.focus),
    )
}

/// Read one row back, or `None` if the reader cannot make sense of it.
///
/// **Tolerant, and inert on a miss.** These are wiki pages; a row somebody
/// hand-broke is skipped rather than hard-failing the read, exactly as an
/// unparseable fact row is — the sessions around it still answer.
fn parse_row(line: &str) -> Option<Row> {
    let c = cells(line);
    if c.len() < 5 {
        return None;
    }
    let id = SessionId(c[0].clone());
    if id.as_str().is_empty() || id.as_str() == "id" {
        return None;
    }
    Some(Row {
        id,
        sid: Some(c[1].clone()).filter(|s| is_readable_sid(s)).map(Sid),
        started_at: c[2].parse().ok()?,
        state: SessionState::from_token(&c[3])?,
        focus: c[4].clone(),
    })
}

/// The half-open line span of the table's contiguous run of rows, and the rows
/// themselves. The run starts at the first line beginning with a pipe after the
/// sessions header.
fn table_region(lines: &[&str]) -> Option<(usize, usize)> {
    let header = lines.iter().position(|l| l.trim() == SESSIONS_HEADER)?;
    let start = lines[header..]
        .iter()
        .position(|l| l.trim_start().starts_with('|'))?
        + header;
    let end = lines[start..]
        .iter()
        .position(|l| !l.trim_start().starts_with('|'))
        .map(|o| start + o)
        .unwrap_or(lines.len());
    Some((start, end))
}

/// Every session on the page, in the order the table holds them.
pub(super) fn parse_rows(doc: &str) -> Vec<Row> {
    let lines: Vec<&str> = doc.lines().collect();
    let Some((start, end)) = table_region(&lines) else {
        return Vec::new();
    };
    lines[start..end]
        .iter()
        .filter_map(|l| parse_row(l))
        .collect()
}

/// Return `doc` with its whole sessions table replaced by `rows` — the
/// current-truth write. The chronology below is untouched, which is the
/// property that makes this safe to do on every focus change.
pub(super) fn with_rows_replaced(doc: &str, rows: &[Row]) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = table_region(&lines)?;
    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    out.push(TABLE_HEADER.to_string());
    out.push(TABLE_SEP.to_string());
    out.extend(rows.iter().map(render_row));
    out.extend(lines[end..].iter().map(|s| s.to_string()));
    Some(out.join("\n"))
}

/// The next free session id on this page — `gamma-1`, `gamma-2`, …
///
/// **Qualified by the bot's slug, because a session id is global.**
/// `read_session` takes an id and nothing else, so a bare per-page counter
/// would have every bot minting `s1` and an address would name one session per
/// bot. The slug is fixed per page and the counter is digits, so two pages
/// cannot collide however their slugs nest.
///
/// Minted over the ids in every row the page carries, read off the raw first
/// cell rather than off the rows that parsed — so an id is never reused even
/// where a row has been hand-broken into unreadability. A reused id is an
/// amend landing on somebody else's session.
pub(super) fn next_session_id(doc: &str, bot: &EntityId) -> SessionId {
    let lines: Vec<&str> = doc.lines().collect();
    let taken: Vec<String> = table_region(&lines)
        .map(|(s, e)| lines[s..e].to_vec())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| cells(l).into_iter().next())
        .collect();
    let prefix = format!("{}-", bot.slug());
    let highest = taken
        .iter()
        .filter_map(|v| {
            let rest = v.strip_prefix(&prefix)?;
            (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
                .then(|| rest.parse::<u64>().ok())
                .flatten()
        })
        .max()
        .unwrap_or(0);
    SessionId(format!("{prefix}{}", highest + 1))
}

// --- the chronology ----------------------------------------------------------

/// Escape any line of an entry that would close its own fence.
///
/// Only a line whose content begins a fence can end a block, so only those are
/// touched — a backtick mid-sentence is left alone. The escape is a leading
/// backslash, and a line that already begins with backslashes gets one more, so
/// the transform is reversible for every input rather than for the ones nobody
/// writes.
///
/// A longer fence would be the obvious fix and is not available: Outline
/// normalizes a four-backtick fence back to three, which turns the quoted code
/// block inside somebody's entry into the end of that entry.
fn escape_text(text: &str) -> String {
    text.lines()
        .map(|l| {
            let bare = l.trim_start_matches('\\');
            if bare.trim_start().starts_with("```") {
                format!("\\{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape_text(text: &str) -> String {
    text.lines()
        .map(|l| match l.strip_prefix('\\') {
            Some(rest)
                if rest
                    .trim_start_matches('\\')
                    .trim_start()
                    .starts_with("```") =>
            {
                rest.to_string()
            }
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render one chronology entry as the block that is appended to the page.
///
/// The fields ride above a blank line and the text below it, so the text can
/// contain a `key: value` line of its own without being read as a field.
pub(super) fn render_entry(session: &SessionId, entry: &JournalEntry) -> String {
    let mut out = format!(
        "{ENTRY_FENCE}\nid: {}\nsession: {session}\nat: {}\n",
        entry.id, entry.at
    );
    if let Some(beat) = &entry.beat {
        out.push_str(&format!("beat: {beat}\n"));
    }
    if let Some(touched) = entry.touched {
        out.push_str(&format!("touched: {touched}\n"));
    }
    out.push_str(&format!("\n{}\n```", escape_text(&entry.text)));
    out
}

/// Every chronology entry on the page, paired with the session it belongs to,
/// in the order the page holds them.
pub(super) fn parse_entries(doc: &str) -> Vec<(SessionId, JournalEntry)> {
    let lines: Vec<&str> = doc.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != ENTRY_FENCE {
            i += 1;
            continue;
        }
        let Some(close) = lines[i + 1..]
            .iter()
            .position(|l| l.trim() == "```")
            .map(|o| i + 1 + o)
        else {
            break; // an unterminated block: everything after it is unreadable
        };
        if let Some(parsed) = parse_entry(&lines[i + 1..close]) {
            out.push(parsed);
        }
        i = close + 1;
    }
    out
}

/// One entry out of the lines inside its fence.
fn parse_entry(inside: &[&str]) -> Option<(SessionId, JournalEntry)> {
    let blank = inside.iter().position(|l| l.trim().is_empty())?;
    let (head, body) = (&inside[..blank], &inside[blank + 1..]);
    let get = |key: &str| head.iter().find_map(|l| field(l, key));

    let id = EntryId(get("id")?);
    let session = SessionId(get("session")?);
    if id.as_str().is_empty() || session.as_str().is_empty() {
        return None;
    }
    Some((
        session,
        JournalEntry {
            id,
            at: get("at")?.parse().ok()?,
            text: unescape_text(&body.join("\n")).trim().to_string(),
            touched: get("touched").and_then(|t| t.parse().ok()),
            beat: get("beat"),
        },
    ))
}

/// Return `doc` with the block for `entry` rewritten in place, or `None` if no
/// block on the page carries that id. Everything around it — the table, the
/// other entries, their order — is untouched.
pub(super) fn with_entry_replaced(
    doc: &str,
    session: &SessionId,
    entry: &JournalEntry,
) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut replaced = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != ENTRY_FENCE {
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        let Some(close) = lines[i + 1..]
            .iter()
            .position(|l| l.trim() == "```")
            .map(|o| i + 1 + o)
        else {
            out.extend(lines[i..].iter().map(|s| s.to_string()));
            break;
        };
        let here = parse_entry(&lines[i + 1..close]);
        if here.is_some_and(|(_, e)| e.id == entry.id) {
            out.push(render_entry(session, entry));
            replaced = true;
        } else {
            out.extend(lines[i..=close].iter().map(|s| s.to_string()));
        }
        i = close + 1;
    }
    replaced.then(|| out.join("\n"))
}

/// The next free entry id on this page — `e1`, `e2`, … Minted across the whole
/// page rather than per session, because the ids share one namespace: the page
/// is where they live, and two sessions minting `e3` on the same page would
/// make an amend ambiguous.
///
/// Unqualified, unlike a session id, because an entry is only ever addressed
/// after its session has been found — which means after its page has been —
/// so the page is the namespace. Scanned over every `id:` line the page
/// carries, so an id is never reused.
pub(super) fn next_entry_id(doc: &str) -> EntryId {
    let highest = doc
        .lines()
        .filter_map(|l| field(l, "id"))
        .filter_map(|v| {
            let rest = v.strip_prefix('e')?;
            (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
                .then(|| rest.parse::<u64>().ok())
                .flatten()
        })
        .max()
        .unwrap_or(0);
    EntryId(format!("e{}", highest + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot() -> EntityId {
        EntityId("bot:gamma".into())
    }

    fn at(s: &str) -> Timestamp {
        s.parse().expect("a test timestamp")
    }

    fn entry(id: &str, text: &str) -> JournalEntry {
        JournalEntry {
            id: EntryId(id.into()),
            at: at("2026-07-28T00:00:00Z"),
            text: text.into(),
            touched: None,
            beat: None,
        }
    }

    /// **An entry quoting a code block must not close its own block.** This is
    /// the one place the encoding can lose data outright: read back, everything
    /// after the quoted fence would be a different entry, or no entry at all.
    ///
    /// A wider fence is the obvious fix and Outline does not allow it — four
    /// backticks come back as three, which is what makes the escape necessary
    /// rather than merely tidy.
    #[test]
    fn an_entry_quoting_a_code_block_survives_its_own_fence() {
        let quoted = "found it:\n```rust\nfn main() {}\n```\nand that was the cause";
        let session = SessionId("gamma-1".into());
        let page = format!(
            "{}\n\n{}",
            seeded_page(&bot()),
            render_entry(&session, &entry("e1", quoted))
        );

        let read = parse_entries(&page);
        assert_eq!(read.len(), 1, "one entry, not two: {page}");
        assert_eq!(read[0].1.text, quoted, "verbatim, backticks and all");
    }

    /// The escape is reversible for text that already looks escaped, not only
    /// for the text somebody is likely to write.
    #[test]
    fn text_that_already_begins_with_a_backslash_fence_round_trips() {
        for text in [
            "\\```not really a fence",
            "\\\\```two backslashes",
            "a ``` mid-sentence is untouched",
            "```",
        ] {
            let session = SessionId("gamma-1".into());
            let page = format!(
                "{}\n\n{}",
                seeded_page(&bot()),
                render_entry(&session, &entry("e1", text))
            );
            let read = parse_entries(&page);
            assert_eq!(read.len(), 1, "{text:?} produced {} entries", read.len());
            assert_eq!(read[0].1.text, text, "{text:?} did not round-trip");
        }
    }

    /// A focus is free text the operator reads, and a pipe in it would forge a
    /// cell — taking the state column with it.
    #[test]
    fn a_focus_carrying_a_pipe_does_not_forge_a_cell() {
        let row = Row {
            id: SessionId("gamma-1".into()),
            sid: Some(Sid("ab12".into())),
            started_at: at("2026-07-28T00:00:00Z"),
            state: SessionState::Active,
            focus: "weighing a | b, and \\ too".into(),
        };
        let page = with_rows_replaced(&seeded_page(&bot()), std::slice::from_ref(&row))
            .expect("the seeded page has a table");
        let read = parse_rows(&page);
        assert_eq!(read, vec![row], "the row survives its own punctuation");
    }

    /// **A hand-broken row is skipped, and the rows around it still answer.**
    /// These are wiki pages; the alternative is one bad line taking a bot's
    /// whole session history out of reach.
    #[test]
    fn an_unreadable_row_is_inert_and_never_reuses_its_id() {
        let good = Row {
            id: SessionId("gamma-2".into()),
            sid: None,
            started_at: at("2026-07-28T00:00:00Z"),
            state: SessionState::Wrapped,
            focus: "done".into(),
        };
        let page = with_rows_replaced(&seeded_page(&bot()), std::slice::from_ref(&good))
            .expect("table")
            .replace(
                "| gamma-2 |",
                "| gamma-7 | ab12 | not-a-timestamp | active | broken |\n| gamma-2 |",
            );

        assert_eq!(
            parse_rows(&page),
            vec![good],
            "the readable row still reads"
        );
        assert_eq!(
            next_session_id(&page, &bot()),
            SessionId("gamma-8".into()),
            "the broken row's id is still taken — reusing it would land a later \
             write on somebody else's session"
        );
    }

    /// An entry's own `key: value` line is body text, not a field: the blank
    /// line is the boundary, so prose cannot forge an id or a timestamp.
    #[test]
    fn a_field_shaped_line_in_an_entry_is_body_not_a_field() {
        let session = SessionId("gamma-1".into());
        let text = "id: not-a-real-id\nat: nonsense\nbeat: no";
        let page = render_entry(&session, &entry("e1", text));
        let read = parse_entries(&page);
        assert_eq!(read[0].1.id, EntryId("e1".into()));
        assert_eq!(read[0].1.text, text);
        assert_eq!(
            read[0].1.beat, None,
            "prose cannot promote itself to a beat"
        );
    }
}
