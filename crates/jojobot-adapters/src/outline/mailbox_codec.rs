//! A mailbox page, encoded — pure functions, no I/O.
//!
//! **The sessions precedent, applied to mail**, because mail wants the same two
//! things from a store and wants them for the same reasons:
//!
//! * **A message is a row.** Its state, sender, subject, reply link and the
//!   notes a consumer recorded are current truth — the state moves `new → read
//!   → processed` and the notes arrive at the end — so they live in a table
//!   that is rewritten in place.
//! * **A body is a fenced block, appended.** A body is written once at post and
//!   never rewritten by any verb, which makes it the one genuinely append-shaped
//!   thing here; and a body is arbitrary prose, which only survives Outline's
//!   editor model verbatim inside a fence. Both facts were established by
//!   probing the live store — see [`session_codec`](super::session_codec) for
//!   the findings, which are the same store's and hold here unchanged.
//!
//! One page per **box**, not per bot. The port is name-keyed from end to end —
//! nothing in `Mailboxes`, `NewMessage` or `Mailbox` names an owner, because
//! ownership is a claim on the owner's own record and this context is
//! deliberately ignorant of it. Where the page is *filed* is a separate
//! question from what it is, and the `name:` line is what answers the second.

use jiff::Timestamp;

use jojobot_domain::mailbox::{
    MailboxName, Message, MessageId, MessageState, normalize_notes, normalize_subject,
};
use jojobot_domain::memory::MACHINERY_FIELD;

/// The value of the machinery field on a mailbox page — what keeps the page
/// itself out of the prose index.
///
/// **This does not take the mail out of `search`.** Messages reach the index by
/// their own path, `scan_messages`, and being findable there is a designed
/// property with tests on it. What this excludes is the *page* — the raw
/// markdown of a box, which would otherwise become prose hits about the
/// operator's correspondence, quoting bodies out of their envelopes.
pub(super) const MAILBOX: &str = "mailbox";

/// The machine-block field naming the box a page holds.
const NAME: &str = "name";

/// The header above the table of messages.
pub(super) const MESSAGES_HEADER: &str = "### ⚙ messages";
/// The header above the bodies.
pub(super) const BODIES_HEADER: &str = "### ⚙ bodies";

const TABLE_HEADER: &str = "| id | state | sender | sent | subject | in-reply-to | notes |";
const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- | --- |";

/// The info string on a message body's fence.
const BODY_FENCE: &str = "```jojobot-message";

/// The cell written where a field is absent. A bare empty cell would be
/// indistinguishable from a field somebody blanked by hand.
const NONE: &str = "-";

/// One row of the messages table — a message without its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub id: MessageId,
    pub state: MessageState,
    pub sender: String,
    pub sent_at: Timestamp,
    pub subject: Option<String>,
    pub in_reply_to: Option<MessageId>,
    pub notes: Option<String>,
}

/// The markdown a fresh mailbox page is seeded with.
pub(super) fn seeded_page(name: &MailboxName) -> String {
    format!(
        "_Managed by jojobot — one row per message, bodies below. The page is not searched; \
         the messages on it are._\n\n\
         ```yaml\n{MACHINERY_FIELD}: {MAILBOX}\n{NAME}: {name}\n```\n\n\
         {MESSAGES_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n\n{BODIES_HEADER}\n"
    )
}

/// The box a page holds, off its `name:` line — `None` if this is not a mailbox
/// page or does not say.
pub(super) fn parse_name(doc: &str) -> Option<MailboxName> {
    let lines: Vec<&str> = doc.lines().collect();
    let (open, close) = machine_block(&lines)?;
    let inside = &lines[open + 1..close - 1];
    if inside
        .iter()
        .find_map(|l| field(l, MACHINERY_FIELD))
        .as_deref()
        != Some(MAILBOX)
    {
        return None;
    }
    let name = MailboxName(inside.iter().find_map(|l| field(l, NAME))?);
    jojobot_domain::mailbox::validate_mailbox_name(&name)
        .ok()
        .map(|()| name)
}

/// The fenced block carrying this page's machine fields.
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

fn field(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

// --- the messages table ------------------------------------------------------

/// Split a row into cells, honouring the escape. Not a `split('|')`: a subject
/// or a note carrying a pipe is escaped on the way out, and a naive split would
/// cut the row there anyway.
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

fn escape_cell(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Render one message as its row. A newline in a note would break the row, so
/// notes are flattened — the domain already says a note is one plain line, and
/// this is the belt to that braces.
pub(super) fn render_row(row: &Row) -> String {
    let cell = |v: Option<&str>| v.map(escape_cell).unwrap_or_else(|| NONE.to_string());
    format!(
        "| {} | {} | {} | {} | {} | {} | {} |",
        row.id,
        row.state.as_token(),
        escape_cell(&row.sender),
        row.sent_at,
        cell(row.subject.as_deref()),
        row.in_reply_to
            .as_ref()
            .map(|r| r.as_str().to_string())
            .unwrap_or_else(|| NONE.to_string()),
        cell(
            row.notes
                .as_deref()
                .map(|n| n.replace('\n', " "))
                .as_deref()
        ),
    )
}

/// What the reader made of one line of the table.
pub(super) enum Read {
    /// A row it understood.
    Row(Box<Row>),
    /// A row carrying an id it could not otherwise read — **quarantined**, not
    /// discarded. It is invisible to every verb, so `list_mailboxes` is where
    /// its existence is surfaced: "N unreadable" rather than nothing.
    Quarantined(MessageId),
    /// Not a message row at all — a header, a separator, a blank.
    NotARow,
}

fn parse_row(line: &str) -> Read {
    let c = cells(line);
    let Some(id) = c.first().filter(|i| !i.is_empty() && *i != "id") else {
        return Read::NotARow;
    };
    if id.bytes().all(|b| b == b'-') {
        return Read::NotARow; // the separator
    }
    let id = MessageId(id.clone());
    let opt = |v: &String| Some(v.clone()).filter(|v| v != NONE && !v.is_empty());

    let ok = c.len() >= 7;
    let state = ok.then(|| MessageState::from_token(&c[1])).flatten();
    let sent = ok.then(|| c[3].parse::<Timestamp>().ok()).flatten();
    let (Some(state), Some(sent_at)) = (state, sent) else {
        return Read::Quarantined(id);
    };
    if c[2].is_empty() {
        return Read::Quarantined(id);
    }
    Read::Row(Box::new(Row {
        id,
        state,
        sender: c[2].clone(),
        sent_at,
        subject: opt(&c[4]),
        in_reply_to: opt(&c[5]).map(MessageId),
        notes: opt(&c[6]),
    }))
}

fn table_region(lines: &[&str]) -> Option<(usize, usize)> {
    let header = lines.iter().position(|l| l.trim() == MESSAGES_HEADER)?;
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

/// Every readable row on the page, and the ids of the ones that are not.
pub(super) fn parse_rows(doc: &str) -> (Vec<Row>, Vec<MessageId>) {
    let lines: Vec<&str> = doc.lines().collect();
    let Some((start, end)) = table_region(&lines) else {
        return (Vec::new(), Vec::new());
    };
    let mut rows = Vec::new();
    let mut quarantined = Vec::new();
    for line in &lines[start..end] {
        match parse_row(line) {
            Read::Row(row) => rows.push(*row),
            Read::Quarantined(id) => quarantined.push(id),
            Read::NotARow => {}
        }
    }
    (rows, quarantined)
}

/// Return `doc` with the whole messages table replaced. The bodies below are
/// untouched, which is what makes this safe on every state change.
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

/// The next free message id on this page — `pm-1`, `pm-2`, …
///
/// **Qualified by the box, because a message id is global**: `read_message` and
/// `mark_processed` take an id and nothing else. Minted over the raw first cell
/// of every row, so an id on a row nobody can read is still taken — reusing one
/// is a `mark_processed` landing on a different message.
pub(super) fn next_message_id(doc: &str, name: &MailboxName) -> MessageId {
    let lines: Vec<&str> = doc.lines().collect();
    let taken: Vec<String> = table_region(&lines)
        .map(|(s, e)| lines[s..e].to_vec())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| cells(l).into_iter().next())
        .collect();
    let prefix = format!("{name}-");
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
    MessageId(format!("{prefix}{}", highest + 1))
}

// --- the bodies --------------------------------------------------------------

/// Escape any line of a body that would close its own fence. A wider fence is
/// not available — Outline normalizes four backticks back to three — so a body
/// quoting a code block would otherwise end its own block early and take every
/// message below it with it.
fn escape_body(body: &str) -> String {
    body.lines()
        .map(|l| {
            if l.trim_start_matches('\\').trim_start().starts_with("```") {
                format!("\\{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape_body(body: &str) -> String {
    body.lines()
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

/// Render a message's body as the block appended to the page. The id rides
/// above a blank line and the body below it, so a body containing a
/// `key: value` line of its own is body rather than a field.
pub(super) fn render_body(id: &MessageId, body: &str) -> String {
    format!("{BODY_FENCE}\nid: {id}\n\n{}\n```", escape_body(body))
}

/// Every body on the page, by message id.
pub(super) fn parse_bodies(doc: &str) -> Vec<(MessageId, String)> {
    let lines: Vec<&str> = doc.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != BODY_FENCE {
            i += 1;
            continue;
        }
        let Some(close) = lines[i + 1..]
            .iter()
            .position(|l| l.trim() == "```")
            .map(|o| i + 1 + o)
        else {
            break;
        };
        let inside = &lines[i + 1..close];
        if let Some(blank) = inside.iter().position(|l| l.trim().is_empty())
            && let Some(id) = inside[..blank].iter().find_map(|l| field(l, "id"))
        {
            out.push((
                MessageId(id),
                unescape_body(&inside[blank + 1..].join("\n"))
                    .trim()
                    .to_string(),
            ));
        }
        i = close + 1;
    }
    out
}

/// Assemble one row and its body into a message.
pub(super) fn message(name: &MailboxName, row: &Row, body: String) -> Message {
    Message {
        id: row.id.clone(),
        mailbox: name.clone(),
        body,
        subject: normalize_subject(row.subject.as_deref()),
        sender: row.sender.clone(),
        sent_at: row.sent_at,
        state: row.state,
        notes: normalize_notes(row.notes.as_deref()),
        in_reply_to: row.in_reply_to.clone(),
    }
}
