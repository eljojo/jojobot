//! The pure fact-table codec: markdown ⇄ [`Fact`]. No I/O, so it's unit-tested
//! with no network. jojobot's schema over a markdown doc lives here — the row
//! format, the `### ⚙ facts` table, the embedded `id:` identity marker, and the
//! cell escaping that keeps an adversarial value from corrupting the table.

use jiff::civil::Date;

use jojobot_domain::memory::{EntityId, Fact, FactId, FactStatus, Provenance};

/// The header that marks the machine-readable fact table at the bottom of a doc.
pub(super) const FACTS_HEADER: &str = "### ⚙ facts";
/// The table's column header row.
pub(super) const TABLE_HEADER: &str = "| id | subject | content | provenance | status | date |";
/// The markdown table separator under the header.
pub(super) const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- |";

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

/// Render one fact as a table row. **Both** the subject and the content are
/// escaped — each is a `|`-delimited cell, and a stray pipe in either would
/// otherwise split the row and corrupt or drop the fact. Provenance and status
/// are their own columns, never folded into content.
pub(super) fn render_fact_row(f: &Fact) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} |",
        f.id,
        escape_cell(&f.subject.to_string()),
        escape_cell(&f.content),
        f.provenance.as_token(),
        f.status.as_token(),
        f.date
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row.
pub(super) fn parse_fact_row(row: &str) -> Option<Fact> {
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
pub(super) fn parse_facts_table(doc: &str) -> Vec<Fact> {
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
pub(super) fn with_fact_appended(doc: &str, row: &str) -> String {
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
pub(super) fn next_fact_id(existing: &[Fact]) -> FactId {
    let max = existing
        .iter()
        .filter_map(|f| f.id.as_str().strip_prefix('f')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    FactId(format!("f{}", max + 1))
}

/// Read the doc's embedded `id:` identity marker — the durable, cosmetic-proof
/// handle on the entity. It lives in the machine block near the top, above the
/// fact table; the search stops at the table so a fact's own `id` never masks
/// the entity marker. This — not the (user-renamable) title — is what resolves a
/// doc to its entity.
pub(super) fn parse_id_marker(doc: &str) -> Option<String> {
    for line in doc.lines() {
        let t = line.trim();
        if t == FACTS_HEADER {
            break;
        }
        if let Some(rest) = t.strip_prefix("id:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// The markdown a freshly-created entity doc is seeded with: a note for the
/// human, the entity's `id:` marker (the durable identity), and an empty fact
/// table for jojobot to append to.
pub(super) fn seeded_doc(subject: &EntityId) -> String {
    format!(
        "_Managed by jojobot. Facts about this entity are in the table at the bottom._\n\n\
         ```yaml\nid: {subject}\nkind: person\n```\n\n\
         {FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n"
    )
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
        let testi = fact("f1", "person:jose", "born in Chile", Provenance::Testimony, date(2026, 1, 1));
        let infer = fact("f2", "person:jose", "prefers mornings ❓", Provenance::Inference, date(2026, 1, 2));
        assert_eq!(parse_fact_row(&render_fact_row(&testi)).unwrap(), testi);
        assert_eq!(parse_fact_row(&render_fact_row(&infer)).unwrap(), infer);
    }

    #[test]
    fn subject_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact("f1", "person:a|b", "x", Provenance::Testimony, date(2026, 7, 24));
        let row = render_fact_row(&f);
        assert!(row.contains("person:a\\|b"), "subject pipe must be escaped: {row}");
        assert_eq!(parse_fact_row(&row).unwrap(), f);
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
    fn seeded_doc_has_a_marker_and_an_empty_parseable_table() {
        let doc = seeded_doc(&EntityId::person("jose"));
        assert_eq!(parse_id_marker(&doc).as_deref(), Some("person:jose"));
        assert!(parse_facts_table(&doc).is_empty());
        let f = fact("f1", "person:jose", "first fact", Provenance::Testimony, date(2026, 7, 24));
        let updated = with_fact_appended(&doc, &render_fact_row(&f));
        assert_eq!(parse_facts_table(&updated), vec![f]);
        // A fact's own `id` (below the table) must not be mistaken for the marker.
        assert_eq!(parse_id_marker(&updated).as_deref(), Some("person:jose"));
    }

    #[test]
    fn marker_is_absent_when_there_is_no_machine_block() {
        assert_eq!(parse_id_marker("# just prose\n\nnothing structured here"), None);
    }
}
