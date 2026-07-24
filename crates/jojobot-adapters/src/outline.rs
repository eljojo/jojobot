//! The Outline store — the real [`Memory`] adapter.
//!
//! jojobot IS a schema layer over markdown docs: Outline is the typed document
//! store, and this adapter reads and writes the `### ⚙ facts` table at the
//! bottom of a target doc. Facts live next to the prose they're about — the
//! user reads the prose; jojobot reads the table.
//!
//! The adapter hits the Outline HTTP API directly (`documents.info` /
//! `documents.update`). That's allowed: the MCP-only rule governs the
//! assistant-in-session, not the server. The store is stateless — it holds a
//! client and its target, never any fact.
//!
//! The row codec (parse/render) is pure and lives at the top of this file so it
//! is unit-tested with no network. Everything below it is the thin HTTP shell.

use async_trait::async_trait;
use jiff::civil::Date;

use jojobot_domain::memory::{
    EntityId, Fact, FactId, FactStatus, Memory, MemoryError, NewFact, Provenance,
};

// --- fact-table format ------------------------------------------------------

/// The header that marks the machine-readable fact table at the bottom of a doc.
const FACTS_HEADER: &str = "### ⚙ facts";
/// The table's column header row.
const TABLE_HEADER: &str = "| id | subject | content | status | date | edges |";
/// The markdown table separator under the header.
const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- |";
/// The inference marker on a content cell. Clean content = testimony.
const INFERENCE_MARK: char = '❓';

/// Escape a value for a markdown table cell — the one character a cell can't
/// carry raw is the column delimiter.
fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Split a markdown table row into trimmed, unescaped cells, honouring `\|` as a
/// literal pipe inside a cell.
fn split_cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let inner = row.strip_prefix('|').unwrap_or(row);
    let inner = inner.strip_suffix('|').unwrap_or(inner);

    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
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

/// Render one fact as a table row. Provenance rides the content cell (a trailing
/// `❓` marks inference); status blank means active; edges are empty this slice.
fn render_fact_row(f: &Fact) -> String {
    let mut content = escape_cell(&f.content);
    if f.provenance == Provenance::Inference {
        content.push(' ');
        content.push(INFERENCE_MARK);
    }
    let status = match f.status {
        FactStatus::Active => "",
        FactStatus::Superseded => "superseded",
        FactStatus::Negated => "negated",
    };
    format!(
        "| {} | {} | {} | {} | {} |  |",
        f.id, f.subject, content, status, f.date
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row.
fn parse_fact_row(row: &str) -> Option<Fact> {
    let cells = split_cells(row);
    // id, subject, content, status, date required; edges optional.
    if cells.len() < 5 {
        return None;
    }
    let id = cells[0].trim();
    if id.is_empty() || id.eq_ignore_ascii_case("id") {
        return None; // empty or the header row
    }
    if id.chars().all(|c| c == '-') {
        return None; // the `--- | ---` separator row
    }

    let subject = cells[1].trim();
    if subject.is_empty() {
        return None;
    }

    let raw = cells[2].trim();
    let (content, provenance) = match raw.strip_suffix(INFERENCE_MARK) {
        Some(rest) => (rest.trim().to_string(), Provenance::Inference),
        None => (raw.to_string(), Provenance::Testimony),
    };
    if content.is_empty() {
        return None;
    }

    let status = match cells[3].trim() {
        "" | "active" => FactStatus::Active,
        "superseded" => FactStatus::Superseded,
        "negated" => FactStatus::Negated,
        _ => return None,
    };

    let date: Date = cells[4].trim().parse().ok()?;

    Some(Fact {
        id: FactId(id.to_string()),
        subject: EntityId(subject.to_string()),
        content,
        provenance,
        status,
        date,
    })
}

/// Locate the fact table's line range within a doc: the half-open span of lines
/// that start with `|` under the `### ⚙ facts` header. `None` if no header.
fn facts_region(lines: &[&str]) -> Option<(usize, usize)> {
    let header = lines.iter().position(|l| l.trim() == FACTS_HEADER)?;
    let mut i = header + 1;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let start = i;
    while i < lines.len() && lines[i].trim_start().starts_with('|') {
        i += 1;
    }
    Some((start, i))
}

/// Every fact in a doc, in document order.
fn parse_facts_table(doc: &str) -> Vec<Fact> {
    let lines: Vec<&str> = doc.lines().collect();
    let Some((start, end)) = facts_region(&lines) else {
        return Vec::new();
    };
    lines[start..end]
        .iter()
        .filter_map(|l| parse_fact_row(l))
        .collect()
}

/// Return `doc` with `row` appended to the fact table. Creates the section (and
/// its header/separator) if the doc doesn't have one yet.
fn with_fact_appended(doc: &str, row: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 6);

    match facts_region(&lines) {
        Some((start, end)) => {
            out.extend(lines[..end].iter().map(|s| s.to_string()));
            if start == end {
                // Header present but no table drawn yet.
                out.push(TABLE_HEADER.to_string());
                out.push(TABLE_SEP.to_string());
            }
            out.push(row.to_string());
            out.extend(lines[end..].iter().map(|s| s.to_string()));
        }
        None => {
            out.extend(lines.iter().map(|s| s.to_string()));
            if !out.is_empty() {
                out.push(String::new());
            }
            out.push(FACTS_HEADER.to_string());
            out.push(String::new());
            out.push(TABLE_HEADER.to_string());
            out.push(TABLE_SEP.to_string());
            out.push(row.to_string());
        }
    }
    out.join("\n")
}

/// The next light/local id for a doc: `f{max+1}` over existing `fN` ids.
fn next_fact_id(existing: &[Fact]) -> FactId {
    let max = existing
        .iter()
        .filter_map(|f| f.id.as_str().strip_prefix('f')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    FactId(format!("f{}", max + 1))
}

// --- the store --------------------------------------------------------------

/// Where the store writes: an Outline base URL, an API token, and the target
/// doc that holds the fact table. The operator supplies these at runtime; the
/// adapter never scans the environment itself.
#[derive(Debug, Clone)]
pub struct OutlineConfig {
    /// Outline base URL, e.g. `https://wiki.example.org` (no trailing slash).
    pub base_url: String,
    /// API token (bearer). Never logged.
    pub token: String,
    /// The id of the doc whose `### ⚙ facts` table this store reads/writes.
    pub doc_id: String,
}

/// The real Memory adapter, fronting an Outline doc. Stateless: it holds a
/// client and a target, never a fact. Constructed unconfigured when the operator
/// hasn't wired Outline yet — then the verbs refuse with
/// [`MemoryError::NotConfigured`] rather than lie.
#[derive(Clone)]
pub struct OutlineStore {
    http: reqwest::Client,
    config: Option<OutlineConfig>,
}

impl OutlineStore {
    /// A store pointed at a configured Outline doc.
    pub fn new(http: reqwest::Client, config: OutlineConfig) -> Self {
        Self {
            http,
            config: Some(config),
        }
    }

    /// A store with no target yet — every verb returns
    /// [`MemoryError::NotConfigured`]. Lets the server boot (and keep serving
    /// `ping`) before Outline is wired, without shipping a toy store.
    pub fn unconfigured(http: reqwest::Client) -> Self {
        Self { http, config: None }
    }

    fn cfg(&self) -> Result<&OutlineConfig, MemoryError> {
        self.config.as_ref().ok_or_else(|| {
            MemoryError::NotConfigured("set the Outline base URL, token and target doc".into())
        })
    }

    /// Fetch the target doc's markdown text via `documents.info`.
    async fn fetch_doc_text(&self, cfg: &OutlineConfig) -> Result<String, MemoryError> {
        let resp = self
            .http
            .post(format!("{}/api/documents.info", cfg.base_url))
            .bearer_auth(&cfg.token)
            .json(&serde_json::json!({ "id": cfg.doc_id }))
            .send()
            .await
            .map_err(|e| MemoryError::Store(format!("documents.info request: {e}")))?;
        if !resp.status().is_success() {
            return Err(MemoryError::Store(format!(
                "documents.info returned {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| MemoryError::Store(format!("documents.info body: {e}")))?;
        body["data"]["text"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| MemoryError::Store("documents.info: no data.text field".into()))
    }

    /// Overwrite the target doc's markdown text via `documents.update`.
    async fn put_doc_text(&self, cfg: &OutlineConfig, text: &str) -> Result<(), MemoryError> {
        let resp = self
            .http
            .post(format!("{}/api/documents.update", cfg.base_url))
            .bearer_auth(&cfg.token)
            .json(&serde_json::json!({ "id": cfg.doc_id, "text": text }))
            .send()
            .await
            .map_err(|e| MemoryError::Store(format!("documents.update request: {e}")))?;
        if !resp.status().is_success() {
            return Err(MemoryError::Store(format!(
                "documents.update returned {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl Memory for OutlineStore {
    async fn capture(&self, fact: NewFact) -> Result<Fact, MemoryError> {
        let cfg = self.cfg()?;
        if fact.content.trim().is_empty() {
            return Err(MemoryError::InvalidFact("content is empty".into()));
        }
        if fact.content.contains('\n') {
            return Err(MemoryError::InvalidFact(
                "content spans multiple lines; a table cell is one line".into(),
            ));
        }

        // Read-modify-write. Outline has no atomic append, so two captures
        // racing on the same doc could collide an id or lose a row — acceptable
        // for a single-session assistant; noted for a later CAS/revision guard.
        let text = self.fetch_doc_text(cfg).await?;
        let existing = parse_facts_table(&text);
        let stored = Fact {
            id: next_fact_id(&existing),
            subject: fact.subject,
            content: fact.content,
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        let updated = with_fact_appended(&text, &render_fact_row(&stored));
        self.put_doc_text(cfg, &updated).await?;
        Ok(stored)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let cfg = self.cfg()?;
        let text = self.fetch_doc_text(cfg).await?;
        Ok(parse_facts_table(&text)
            .into_iter()
            .filter(|f| &f.subject == subject)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn fact(id: &str, content: &str, prov: Provenance, status: FactStatus, d: Date) -> Fact {
        Fact {
            id: FactId(id.into()),
            subject: EntityId::self_(),
            content: content.into(),
            provenance: prov,
            status,
            date: d,
        }
    }

    #[test]
    fn row_round_trips_every_provenance_and_status() {
        let cases = [
            fact("f1", "drinks oat milk", Provenance::Inference, FactStatus::Active, date(2026, 7, 24)),
            fact("f2", "lived in Montréal", Provenance::Testimony, FactStatus::Active, date(2026, 3, 9)),
            fact("f3", "old job", Provenance::Testimony, FactStatus::Superseded, date(2025, 1, 1)),
            fact("f4", "not vegetarian", Provenance::Testimony, FactStatus::Negated, date(2026, 6, 6)),
        ];
        for f in cases {
            let parsed = parse_fact_row(&render_fact_row(&f))
                .unwrap_or_else(|| panic!("row must round-trip: {f:?}"));
            assert_eq!(parsed, f);
        }
    }

    #[test]
    fn content_with_a_pipe_round_trips() {
        let f = fact("f1", "reads a|b|c notation", Provenance::Testimony, FactStatus::Active, date(2026, 7, 24));
        let row = render_fact_row(&f);
        assert!(!row.trim_start_matches("| f1 | self | ").starts_with("reads a |"), "pipe must be escaped, not split: {row}");
        assert_eq!(parse_fact_row(&row).unwrap(), f);
    }

    #[test]
    fn header_and_separator_are_not_facts() {
        assert!(parse_fact_row(TABLE_HEADER).is_none());
        assert!(parse_fact_row(TABLE_SEP).is_none());
    }

    #[test]
    fn next_id_increments_over_existing() {
        let existing = vec![
            fact("f1", "a", Provenance::Testimony, FactStatus::Active, date(2026, 1, 1)),
            fact("f3", "b", Provenance::Testimony, FactStatus::Active, date(2026, 1, 1)),
        ];
        assert_eq!(next_fact_id(&existing), FactId("f4".into()));
        assert_eq!(next_fact_id(&[]), FactId("f1".into()));
    }

    #[test]
    fn append_into_existing_table_then_parse_finds_it() {
        let doc = "# About the user\n\nSome prose the user reads.\n\n### ⚙ facts\n\n| id | subject | content | status | date | edges |\n| --- | --- | --- | --- | --- | --- |\n| f1 | self | plays go | | 2026-07-01 |  |\n";
        let f = fact("f2", "learning Rust", Provenance::Inference, FactStatus::Active, date(2026, 7, 2));
        let updated = with_fact_appended(doc, &render_fact_row(&f));
        let parsed = parse_facts_table(&updated);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], f);
        // The prose above the table is untouched.
        assert!(updated.contains("Some prose the user reads."));
    }

    #[test]
    fn creates_a_table_when_the_doc_has_none() {
        let doc = "# About the user\n\nJust prose, no fact table yet.\n";
        let f = fact("f1", "first fact", Provenance::Testimony, FactStatus::Active, date(2026, 7, 24));
        let updated = with_fact_appended(doc, &render_fact_row(&f));
        assert!(updated.contains(FACTS_HEADER));
        let parsed = parse_facts_table(&updated);
        assert_eq!(parsed, vec![f]);
    }
}
