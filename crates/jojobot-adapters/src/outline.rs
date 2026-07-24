//! The Outline store — the real [`Memory`] adapter.
//!
//! jojobot IS a schema layer over markdown docs: Outline is the typed document
//! store, and this adapter reads and writes the `### ⚙ facts` table at the
//! bottom of a per-entity doc. Facts live next to the prose they're about — the
//! user reads the prose; jojobot reads the table.
//!
//! **Convention over configuration.** The adapter is never handed an Outline id.
//! Its only config is credentials (base URL + token). It discovers its own
//! collection *by name* (a software constant, default `jojobot`), creating it if
//! absent, and within it resolves each entity's doc by a deterministic title,
//! creating a seeded doc on first capture. Everything — collection, docs,
//! mapping — lives in Outline and is discovered/created at runtime; nothing
//! authoritative lives in jojobot's process.
//!
//! The row codec (parse/render) is pure and lives at the top of this file so it
//! is unit-tested with no network. Everything below it is the HTTP shell.

use std::fmt;

use async_trait::async_trait;
use jiff::civil::Date;
use serde_json::json;

use jojobot_domain::memory::{
    EntityId, Fact, FactId, FactStatus, Memory, MemoryError, NewFact, Provenance, normalize_content,
};

// --- fact-table format ------------------------------------------------------

/// The header that marks the machine-readable fact table at the bottom of a doc.
const FACTS_HEADER: &str = "### ⚙ facts";
/// The table's column header row.
const TABLE_HEADER: &str = "| id | subject | content | provenance | status | date |";
/// The markdown table separator under the header.
const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- |";

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

/// Render one fact as a table row. Provenance and status are their **own**
/// columns — never folded into content — so a claim can end in any glyph without
/// being misread.
fn render_fact_row(f: &Fact) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} |",
        f.id,
        f.subject,
        escape_cell(&f.content),
        f.provenance.as_token(),
        f.status.as_token(),
        f.date
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row.
fn parse_fact_row(row: &str) -> Option<Fact> {
    let cells = split_cells(row);
    // id, subject, content, provenance, status, date.
    if cells.len() < 6 {
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

    let content = cells[2].trim();
    if content.is_empty() {
        return None;
    }

    let provenance = Provenance::from_token(&cells[3]);

    // Status is Active-only this slice; the cell is not load-bearing yet.
    let status = FactStatus::Active;

    let date: Date = cells[5].trim().parse().ok()?;

    Some(Fact {
        id: FactId(id.to_string()),
        subject: EntityId(subject.to_string()),
        content: content.to_string(),
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

/// The markdown a freshly-created entity doc is seeded with: a note for the
/// human, the entity's id marker, and an empty fact table for jojobot to append
/// to.
fn seeded_doc(subject: &EntityId) -> String {
    format!(
        "_Managed by jojobot. Facts about this entity are in the table at the bottom._\n\n\
         ```yaml\nid: {subject}\nkind: person\n```\n\n\
         {FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n"
    )
}

// --- secret -----------------------------------------------------------------

/// An API token that never prints itself. `Debug` redacts, so the token can't
/// leak through a `#[derive(Debug)]` on a config that holds it, a `dbg!`, or a
/// `tracing` field.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Borrow the raw value — only at the point it's actually used (the bearer
    /// header). Deliberately named so call sites are greppable.
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(\"***\")")
    }
}

// --- the store --------------------------------------------------------------

/// The store's only configuration: **credentials**. No collection id, no doc id
/// — those are discovered by convention. `Debug` is safe: the token redacts.
#[derive(Debug, Clone)]
pub struct OutlineConfig {
    /// Outline base URL, e.g. `https://wiki.example.org` (no trailing slash).
    pub base_url: String,
    /// API token (bearer). Redacted in `Debug`.
    pub token: Secret,
}

/// The real Memory adapter, fronting an Outline collection it manages by name.
/// Stateless: it holds a client, credentials, and the collection *name*, never
/// an id or a fact. Constructed unconfigured when the operator hasn't wired
/// credentials yet — then the verbs refuse with [`MemoryError::NotConfigured`]
/// rather than lie.
#[derive(Clone)]
pub struct OutlineStore {
    http: reqwest::Client,
    config: Option<OutlineConfig>,
    collection: String,
}

impl OutlineStore {
    /// The collection jojobot manages by default. A software constant — jojobot
    /// creates and owns this collection; it never touches the user's own.
    pub const DEFAULT_COLLECTION: &'static str = "jojobot";

    /// A store pointed at Outline via credentials, managing the default
    /// `jojobot` collection.
    pub fn new(http: reqwest::Client, config: OutlineConfig) -> Self {
        Self::with_collection(http, config, Self::DEFAULT_COLLECTION)
    }

    /// A store managing a named collection (e.g. `jojobot-test` for the gated
    /// integration test). jojobot only ever creates/manages its own collections.
    pub fn with_collection(
        http: reqwest::Client,
        config: OutlineConfig,
        collection: impl Into<String>,
    ) -> Self {
        Self {
            http,
            config: Some(config),
            collection: collection.into(),
        }
    }

    /// A store with no credentials yet — every verb returns
    /// [`MemoryError::NotConfigured`]. Lets the server boot (and keep serving
    /// `ping`) before Outline is wired, without shipping a toy store.
    pub fn unconfigured(http: reqwest::Client) -> Self {
        Self {
            http,
            config: None,
            collection: Self::DEFAULT_COLLECTION.to_string(),
        }
    }

    fn cfg(&self) -> Result<&OutlineConfig, MemoryError> {
        self.config.as_ref().ok_or_else(|| {
            MemoryError::NotConfigured("set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN".into())
        })
    }

    /// POST a JSON body to an Outline API endpoint and return the parsed JSON.
    /// Central place for auth + error mapping — no `unwrap` on any path.
    async fn api(
        &self,
        cfg: &OutlineConfig,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, MemoryError> {
        let resp = self
            .http
            .post(format!("{}/api/{endpoint}", cfg.base_url))
            .bearer_auth(cfg.token.expose())
            .json(&body)
            .send()
            .await
            .map_err(|e| MemoryError::Store(format!("{endpoint} request: {e}")))?;
        if !resp.status().is_success() {
            return Err(MemoryError::Store(format!(
                "{endpoint} returned {}",
                resp.status()
            )));
        }
        resp.json()
            .await
            .map_err(|e| MemoryError::Store(format!("{endpoint} body: {e}")))
    }

    /// Discover jojobot's collection by name, creating it if absent. Returns its
    /// id (used only in-flight; never persisted in jojobot).
    async fn resolve_collection_id(&self, cfg: &OutlineConfig) -> Result<String, MemoryError> {
        let mut offset = 0u64;
        loop {
            let page = self
                .api(cfg, "collections.list", json!({ "offset": offset, "limit": 100 }))
                .await?;
            let items = page["data"]
                .as_array()
                .ok_or_else(|| MemoryError::Store("collections.list: no data array".into()))?;
            for c in items {
                if c["name"].as_str() == Some(self.collection.as_str()) {
                    return c["id"]
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| MemoryError::Store("collection has no id".into()));
                }
            }
            if items.len() < 100 {
                break;
            }
            offset += 100;
        }

        let created = self
            .api(cfg, "collections.create", json!({ "name": self.collection }))
            .await?;
        created["data"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| MemoryError::Store("collections.create: no id".into()))
    }

    /// Resolve an entity's doc within jojobot's collection by its deterministic
    /// title (the entity id). Creates a seeded doc when `create` is set and none
    /// exists. `Ok(None)` means "no doc and not asked to create one".
    async fn resolve_entity_doc(
        &self,
        cfg: &OutlineConfig,
        subject: &EntityId,
        create: bool,
    ) -> Result<Option<String>, MemoryError> {
        let collection_id = self.resolve_collection_id(cfg).await?;
        let title = subject.as_str();

        let mut offset = 0u64;
        loop {
            let page = self
                .api(
                    cfg,
                    "documents.list",
                    json!({ "collectionId": collection_id, "offset": offset, "limit": 100 }),
                )
                .await?;
            let items = page["data"]
                .as_array()
                .ok_or_else(|| MemoryError::Store("documents.list: no data array".into()))?;
            for d in items {
                if d["title"].as_str() == Some(title) {
                    return Ok(Some(
                        d["id"]
                            .as_str()
                            .map(str::to_string)
                            .ok_or_else(|| MemoryError::Store("document has no id".into()))?,
                    ));
                }
            }
            if items.len() < 100 {
                break;
            }
            offset += 100;
        }

        if !create {
            return Ok(None);
        }

        let created = self
            .api(
                cfg,
                "documents.create",
                json!({
                    "collectionId": collection_id,
                    "title": title,
                    "text": seeded_doc(subject),
                    "publish": true,
                }),
            )
            .await?;
        created["data"]["id"]
            .as_str()
            .map(str::to_string)
            .map(Some)
            .ok_or_else(|| MemoryError::Store("documents.create: no id".into()))
    }

    /// Fetch a doc's markdown text via `documents.info`.
    async fn fetch_doc_text(
        &self,
        cfg: &OutlineConfig,
        doc_id: &str,
    ) -> Result<String, MemoryError> {
        let body = self.api(cfg, "documents.info", json!({ "id": doc_id })).await?;
        body["data"]["text"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| MemoryError::Store("documents.info: no data.text".into()))
    }

    /// Overwrite a doc's markdown text via `documents.update`.
    async fn put_doc_text(
        &self,
        cfg: &OutlineConfig,
        doc_id: &str,
        text: &str,
    ) -> Result<(), MemoryError> {
        self.api(cfg, "documents.update", json!({ "id": doc_id, "text": text }))
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl Memory for OutlineStore {
    async fn capture(&self, fact: NewFact) -> Result<Fact, MemoryError> {
        let cfg = self.cfg()?;
        let content = normalize_content(&fact.content);
        if content.is_empty() {
            return Err(MemoryError::InvalidFact("content is empty".into()));
        }
        if content.contains('\n') {
            return Err(MemoryError::InvalidFact(
                "content spans multiple lines; a table cell is one line".into(),
            ));
        }

        let doc_id = self
            .resolve_entity_doc(cfg, &fact.subject, true)
            .await?
            .ok_or_else(|| MemoryError::Store("entity doc was not created".into()))?;

        // Read-modify-write. Outline has no atomic append, so two captures
        // racing on the same doc could collide an id or lose a row — acceptable
        // for a single-session assistant; noted for a later revision guard.
        let text = self.fetch_doc_text(cfg, &doc_id).await?;
        let existing = parse_facts_table(&text);
        let stored = Fact {
            id: next_fact_id(&existing),
            subject: fact.subject,
            content,
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        let updated = with_fact_appended(&text, &render_fact_row(&stored));
        self.put_doc_text(cfg, &doc_id, &updated).await?;
        Ok(stored)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let cfg = self.cfg()?;
        match self.resolve_entity_doc(cfg, subject, false).await? {
            None => Ok(Vec::new()),
            Some(doc_id) => {
                let text = self.fetch_doc_text(cfg, &doc_id).await?;
                Ok(parse_facts_table(&text)
                    .into_iter()
                    .filter(|f| &f.subject == subject)
                    .collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn fact(id: &str, subject: &str, content: &str, prov: Provenance, d: Date) -> Fact {
        Fact {
            id: FactId(id.into()),
            subject: EntityId(subject.into()),
            content: content.into(),
            provenance: prov,
            status: FactStatus::Active,
            date: d,
        }
    }

    #[test]
    fn renders_the_exact_pinned_row() {
        // Pin the literal wire format — a symmetric round-trip alone can't catch
        // a schema drift both sides share.
        let f = fact("f1", "person:jose", "drinks oat milk", Provenance::Inference, date(2026, 7, 24));
        assert_eq!(
            render_fact_row(&f),
            "| f1 | person:jose | drinks oat milk | inference | active | 2026-07-24 |"
        );
    }

    #[test]
    fn both_provenances_round_trip_via_their_own_column() {
        // The regression guard for the collision: content ending in ❓ must NOT
        // be read as inference — provenance rides its own column.
        let testi = fact("f1", "person:jose", "born in Chile", Provenance::Testimony, date(2026, 1, 1));
        let infer = fact("f2", "person:jose", "prefers mornings ❓", Provenance::Inference, date(2026, 1, 2));
        assert_eq!(parse_fact_row(&render_fact_row(&testi)).unwrap(), testi);
        assert_eq!(parse_fact_row(&render_fact_row(&infer)).unwrap(), infer);
    }

    #[test]
    fn content_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact("f1", "person:jose", "reads a|b|c notation", Provenance::Testimony, date(2026, 7, 24));
        let row = render_fact_row(&f);
        assert!(row.contains("a\\|b\\|c"), "pipes must be escaped in the row: {row}");
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
            fact("f1", "person:jose", "a", Provenance::Testimony, date(2026, 1, 1)),
            fact("f3", "person:jose", "b", Provenance::Testimony, date(2026, 1, 1)),
        ];
        assert_eq!(next_fact_id(&existing), FactId("f4".into()));
        assert_eq!(next_fact_id(&[]), FactId("f1".into()));
    }

    #[test]
    fn append_into_existing_table_then_parse_finds_it() {
        let doc = "# About\n\nSome prose.\n\n### ⚙ facts\n\n| id | subject | content | provenance | status | date |\n| --- | --- | --- | --- | --- | --- |\n| f1 | person:jose | plays go | testimony | active | 2026-07-01 |\n";
        let f = fact("f2", "person:jose", "learning Rust", Provenance::Inference, date(2026, 7, 2));
        let updated = with_fact_appended(doc, &render_fact_row(&f));
        let parsed = parse_facts_table(&updated);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], f);
        assert!(updated.contains("Some prose."), "prose above the table is untouched");
    }

    #[test]
    fn seeded_doc_has_an_empty_but_parseable_table() {
        let doc = seeded_doc(&EntityId::person("jose"));
        assert!(doc.contains("id: person:jose"));
        assert!(parse_facts_table(&doc).is_empty());
        // And a capture-shaped append lands correctly in the seed.
        let f = fact("f1", "person:jose", "first fact", Provenance::Testimony, date(2026, 7, 24));
        let updated = with_fact_appended(&doc, &render_fact_row(&f));
        assert_eq!(parse_facts_table(&updated), vec![f]);
    }

    #[test]
    fn secret_debug_redacts_the_token() {
        let cfg = OutlineConfig {
            base_url: "https://wiki.example".into(),
            token: Secret::new("super-secret-token"),
        };
        let shown = format!("{cfg:?}");
        assert!(!shown.contains("super-secret-token"), "token leaked: {shown}");
        assert!(shown.contains("***"));
    }
}
