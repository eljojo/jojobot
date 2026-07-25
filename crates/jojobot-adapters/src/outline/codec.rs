//! The pure fact-table codec: markdown ⇄ [`Fact`]. No I/O, so it's unit-tested
//! with no network. jojobot's schema over a markdown doc lives here — the row
//! format, the `### ⚙ facts` table, the embedded `id:` identity marker, and the
//! cell escaping that keeps an adversarial value from corrupting the table.

use jiff::civil::Date;

use jojobot_domain::memory::{Boot, Entity, EntityId, Fact, FactId, FactStatus, Provenance};

/// The header that marks the machine-readable fact table at the bottom of a doc.
pub(super) const FACTS_HEADER: &str = "### ⚙ facts";
/// The table's column header row.
pub(super) const TABLE_HEADER: &str =
    "| id | subject | content | details | provenance | status | date |";
/// The markdown table separator under the header.
pub(super) const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- | --- |";
/// Cell count of the current row format, and of the pre-`details` format that
/// still exists on disk. A row is parsed by its width — the schema grew by a
/// column, and rows written before that must keep reading (never hard-fail).
const CELLS: usize = 7;
const CELLS_LEGACY: usize = 6;

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

/// Render one fact as a table row. Every free-text cell — subject, content,
/// details — is escaped, because a stray pipe in any of them would split the row
/// and corrupt or drop the fact. Provenance and status are their own columns,
/// never folded into content.
pub(super) fn render_fact_row(f: &Fact) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} |",
        f.id,
        escape_cell(&f.subject.to_string()),
        escape_cell(&f.content),
        escape_cell(f.details.as_deref().unwrap_or_default()),
        f.provenance.as_token(),
        f.status.as_token(),
        f.date
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row. `home` is the entity whose doc the
/// row was read from — the other half of the fact's global address.
///
/// Both row widths are accepted: the current seven-cell format and the six-cell
/// one written before `details` existed. Anything else is not a fact row.
pub(super) fn parse_fact_row(row: &str, home: &EntityId) -> Option<Fact> {
    let cells = split_cells(row);
    let legacy = match cells.len() {
        CELLS => false,
        CELLS_LEGACY => true,
        _ => return None,
    };
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

    // The legacy row has no details cell, so every column after content shifts.
    let details = (!legacy)
        .then(|| cells[3].trim())
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    let shift = usize::from(legacy);
    let provenance = Provenance::from_token(&cells[4 - shift]);
    let status = FactStatus::from_token(&cells[5 - shift]);
    let date: Date = cells[6 - shift].trim().parse().ok()?;

    Some(Fact {
        id: FactId(id.to_string()),
        home: home.clone(),
        subject: EntityId(subject.to_string()),
        content: content.to_string(),
        details,
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

/// Every fact in a doc, in document order. A doc with no `id:` marker yields
/// nothing: without a home its rows have no address, and an unaddressable fact
/// is one nobody could ever correct.
pub(super) fn parse_facts_table(doc: &str) -> Vec<Fact> {
    let Some(home) = parse_id_marker(doc).map(EntityId) else {
        return Vec::new();
    };
    let lines: Vec<&str> = doc.lines().collect();
    let Some((start, end)) = facts_region(&lines) else {
        return Vec::new();
    };
    lines[start..end]
        .iter()
        .filter_map(|l| parse_fact_row(l, &home))
        .collect()
}

/// Return `doc` with the row carrying `id` replaced by `row`, or `None` if no
/// such row exists. `None` is the signal that the address missed — the store
/// turns it into an error rather than appending a row nobody asked for.
pub(super) fn with_row_replaced(doc: &str, id: &FactId, row: &str) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = facts_region(&lines)?;
    let target = lines[start..end]
        .iter()
        .position(|l| row_id(l).as_deref() == Some(id.as_str()))?
        + start;

    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out[target] = row.to_string();
    Some(out.join("\n"))
}

/// The local id a table row carries, if it looks like a fact row at all.
fn row_id(row: &str) -> Option<String> {
    let cells = split_cells(row);
    if !matches!(cells.len(), CELLS | CELLS_LEGACY) {
        return None;
    }
    let id = cells[0].trim();
    (!id.is_empty() && !id.eq_ignore_ascii_case("id") && !id.chars().all(|c| c == '-'))
        .then(|| id.to_string())
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

/// Read one `key: value` field out of the machine block. The scan stops at the
/// fact table so a fact's own cells can never masquerade as a frontmatter field.
fn parse_field(doc: &str, key: &str) -> Option<String> {
    for line in doc.lines() {
        let t = line.trim();
        if t == FACTS_HEADER {
            break;
        }
        if let Some(rest) = t.strip_prefix(key).and_then(|r| r.strip_prefix(':')) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Read the doc's embedded `id:` identity marker — the durable, cosmetic-proof
/// handle on the entity. It lives in the machine block near the top, above the
/// fact table. This — not the (user-renamable) title — is what resolves a doc to
/// its entity.
pub(super) fn parse_id_marker(doc: &str) -> Option<String> {
    parse_field(doc, "id")
}

/// Read the doc's entity out of its frontmatter, or `None` if the doc carries no
/// id marker — a doc the user wrote by hand is not an entity, and jojobot never
/// adopts one. Tolerant on every other field: a doc written before the
/// frontmatter grew still identifies its entity, it just has less to say.
///
/// **The handle decides the kind.** A `kind:` line that disagrees with the id is
/// stale text, not a second opinion.
pub(super) fn parse_entity(doc: &str) -> Option<Entity> {
    let id = EntityId(parse_id_marker(doc)?);
    let kind = id.kind()?;
    Some(Entity {
        id,
        kind,
        name: parse_field(doc, "name").unwrap_or_default(),
        source: parse_field(doc, "source").unwrap_or_default(),
        crm: parse_field(doc, "crm"),
        boot: parse_field(doc, "boot").map_or(Boot::default(), |b| Boot::from_token(&b)),
    })
}

/// The frontmatter block for an entity — lean and identical for all eight kinds.
/// An absent `crm` writes no line at all, so the block says only what is true.
fn frontmatter(e: &Entity) -> String {
    let mut out = format!(
        "```yaml\nid: {}\nkind: {}\nname: {}\nsource: {}\n",
        e.id,
        e.kind,
        e.name,
        e.source
    );
    if let Some(crm) = &e.crm {
        out.push_str(&format!("crm: {crm}\n"));
    }
    out.push_str(&format!("boot: {}\n```", e.boot.as_token()));
    out
}

/// The half-open line span of the fenced machine block above the fact table.
fn frontmatter_region(lines: &[&str]) -> Option<(usize, usize)> {
    let limit = lines
        .iter()
        .position(|l| l.trim() == FACTS_HEADER)
        .unwrap_or(lines.len());
    let open = lines[..limit]
        .iter()
        .position(|l| l.trim_start().starts_with("```"))?;
    let close = lines[open + 1..limit]
        .iter()
        .position(|l| l.trim_start().starts_with("```"))?
        + open
        + 1;
    Some((open, close + 1))
}

/// Return `doc` with its frontmatter block rewritten to `entity` — an in-place
/// metadata edit (fix the source). Prose above it and the fact table below are
/// untouched. A doc with no block yet gets one at the top rather than losing the
/// edit.
pub(super) fn with_frontmatter_replaced(doc: &str, entity: &Entity) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    let block = frontmatter(entity);
    match frontmatter_region(&lines) {
        Some((start, end)) => {
            let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
            out.push(block);
            out.extend(lines[end..].iter().map(|s| s.to_string()));
            out.join("\n")
        }
        None => format!("{block}\n\n{doc}"),
    }
}

/// The markdown a freshly-created entity doc is seeded with: a note for the
/// human, the entity's frontmatter (durable identity + metadata), and an empty
/// fact table for jojobot to append to.
pub(super) fn seeded_doc(entity: &Entity) -> String {
    format!(
        "_Managed by jojobot. Facts about this entity are in the table at the bottom._\n\n\
         {}\n\n{FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n",
        frontmatter(entity)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jojobot_domain::memory::{Boot, Entity, EntityKind};

    fn fact(id: &str, subject: &str, content: &str, prov: Provenance, d: Date) -> Fact {
        Fact {
            id: FactId(id.into()),
            home: EntityId(subject.into()),
            subject: EntityId(subject.into()),
            content: content.into(),
            details: None,
            provenance: prov,
            status: FactStatus::Active,
            date: d,
        }
    }

    fn alpha() -> Entity {
        Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::Person,
            name: "Alpha".into(),
            source: "crm-card".into(),
            crm: Some("card:554".into()),
            boot: Boot::OnDemand,
        }
    }

    // --- the details column ---------------------------------------------------

    /// A fact's details ride in their own escaped cell and round-trip with it.
    #[test]
    fn details_round_trip_in_their_own_cell() {
        let f = Fact {
            details: Some("changed jobs in July; a|b in the margin".into()),
            ..fact("f1", "person:alpha", "works somewhere new", Provenance::Testimony, date(2026, 7, 24))
        };
        let row = render_fact_row(&f);
        assert!(row.contains("a\\|b"), "details must be escaped too: {row}");
        assert_eq!(parse_fact_row(&row, &EntityId::person("alpha")).unwrap(), f);
    }

    /// A row written before the details column existed still parses — it just
    /// has no details. A schema addition must never orphan the rows already on
    /// disk (never hard-fail a read).
    #[test]
    fn a_legacy_six_column_row_still_parses() {
        let legacy = "| f1 | person:alpha | plays go | testimony | active | 2026-07-01 |";
        let parsed = parse_fact_row(legacy, &EntityId::person("alpha")).expect("legacy row must parse");
        assert_eq!(parsed.content, "plays go");
        assert_eq!(parsed.details, None);
        assert_eq!(parsed.provenance, Provenance::Testimony);
        assert_eq!(parsed.status, FactStatus::Active);
        assert_eq!(parsed.date, date(2026, 7, 1));
    }

    // --- the status column ----------------------------------------------------

    /// All three lifecycle states survive the row, so a negated fact reads back
    /// negated rather than quietly returning as current truth.
    #[test]
    fn every_status_round_trips_through_the_row() {
        for status in [FactStatus::Active, FactStatus::Superseded, FactStatus::Negated] {
            let f = Fact {
                status,
                ..fact("f1", "person:alpha", "a claim", Provenance::Inference, date(2026, 7, 1))
            };
            let parsed = parse_fact_row(&render_fact_row(&f), &EntityId::person("alpha")).unwrap();
            assert_eq!(parsed.status, status, "status must survive the row");
        }
    }

    // --- addresses ------------------------------------------------------------

    /// Every parsed fact knows the doc it came out of, so it can hand back a
    /// global address the caller can edit through.
    #[test]
    fn parsed_facts_carry_their_home_from_the_docs_marker() {
        let doc = with_fact_appended(
            &seeded_doc(&alpha()),
            &render_fact_row(&fact("f1", "person:alpha", "plays go", Provenance::Testimony, date(2026, 7, 1))),
        );
        let facts = parse_facts_table(&doc);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].home, EntityId::person("alpha"));
        assert_eq!(facts[0].address().to_string(), "person:alpha#f1");
    }

    // --- in-place row replacement (fix the source) ----------------------------

    /// Replacing a row rewrites it where it stands: the rows around it, and the
    /// prose above, are untouched — and no second copy appears.
    #[test]
    fn replacing_a_row_edits_in_place_and_leaves_the_rest_alone() {
        let mut doc = seeded_doc(&alpha());
        for (id, content) in [("f1", "first"), ("f2", "second"), ("f3", "third")] {
            let row = render_fact_row(&fact(id, "person:alpha", content, Provenance::Inference, date(2026, 7, 1)));
            doc = with_fact_appended(&doc, &row);
        }
        let edited = Fact {
            content: "second, corrected".into(),
            status: FactStatus::Negated,
            ..fact("f2", "person:alpha", "", Provenance::Inference, date(2026, 7, 2))
        };
        let updated = with_row_replaced(&doc, &FactId("f2".into()), &render_fact_row(&edited))
            .expect("the row exists");

        let facts = parse_facts_table(&updated);
        assert_eq!(facts.len(), 3, "no row gained or lost");
        assert_eq!(facts[1].content, "second, corrected");
        assert_eq!(facts[1].status, FactStatus::Negated);
        assert_eq!(facts[0].content, "first");
        assert_eq!(facts[2].content, "third");
        assert!(!updated.contains("| second |"), "the old row is gone, not left beside");
    }

    /// Replacing a row that isn't there changes nothing and says so — the store
    /// turns that into an error rather than appending a surprise row.
    #[test]
    fn replacing_a_missing_row_reports_rather_than_appends() {
        let doc = seeded_doc(&alpha());
        assert!(with_row_replaced(&doc, &FactId("f9".into()), "| f9 | x |").is_none());
    }

    // --- entity frontmatter ---------------------------------------------------

    /// The frontmatter carries every entity field, and reads back as the same
    /// entity — the entity read path, mirroring the fact table's.
    #[test]
    fn entity_frontmatter_round_trips() {
        let doc = seeded_doc(&alpha());
        assert_eq!(parse_entity(&doc).expect("a seeded doc is an entity"), alpha());
        assert_eq!(parse_id_marker(&doc).as_deref(), Some("person:alpha"));
    }

    /// An entity with no `crm` link writes no `crm` line — absent, not blank.
    #[test]
    fn an_absent_crm_link_is_not_written() {
        let doc = seeded_doc(&Entity { crm: None, ..alpha() });
        assert!(!doc.contains("crm:"), "no empty crm line: {doc}");
        assert_eq!(parse_entity(&doc).unwrap().crm, None);
    }

    /// A doc from before the frontmatter grew its fields still identifies its
    /// entity: the id marker is the load-bearing part, the rest defaults.
    #[test]
    fn a_legacy_doc_with_only_a_marker_still_reads_as_an_entity() {
        let legacy = "_Managed by jojobot._\n\n```yaml\nid: person:alpha\nkind: person\n```\n\n### ⚙ facts\n";
        let e = parse_entity(legacy).expect("a marker is enough to be an entity");
        assert_eq!(e.id, EntityId::person("alpha"));
        assert_eq!(e.kind, EntityKind::Person);
        assert_eq!(e.name, "");
        assert_eq!(e.boot, Boot::OnDemand);
    }

    /// A doc with no marker is not an entity — jojobot never adopts a doc the
    /// user wrote by hand.
    #[test]
    fn a_doc_without_a_marker_is_not_an_entity() {
        assert_eq!(parse_entity("# just prose\n\nnothing structured"), None);
    }

    /// Editing an entity rewrites the frontmatter in place; the prose and the
    /// fact table below it are untouched.
    #[test]
    fn replacing_the_frontmatter_leaves_prose_and_facts_alone() {
        let doc = with_fact_appended(
            &format!("Some prose about the entity.\n\n{}", seeded_doc(&alpha())),
            &render_fact_row(&fact("f1", "person:alpha", "plays go", Provenance::Testimony, date(2026, 7, 1))),
        );
        let renamed = Entity { name: "Alpha Renamed".into(), ..alpha() };
        let updated = with_frontmatter_replaced(&doc, &renamed);

        assert_eq!(parse_entity(&updated).unwrap(), renamed);
        assert!(updated.contains("Some prose about the entity."), "prose survives");
        assert_eq!(parse_facts_table(&updated).len(), 1, "the fact table survives");
        assert!(!updated.contains("name: Alpha\n"), "the old name is gone");
    }

    /// The kind always comes from the handle, never from a `kind:` line that
    /// disagrees with it — the id is the identity.
    #[test]
    fn the_handle_decides_the_kind_when_a_stale_line_disagrees() {
        let doc = "```yaml\nid: project:atlas\nkind: person\n```\n\n### ⚙ facts\n";
        assert_eq!(parse_entity(doc).unwrap().kind, EntityKind::Project);
    }

    #[test]
    fn renders_the_exact_pinned_row() {
        // Pin the literal wire format — a symmetric round-trip alone can't catch
        // a schema drift both sides share.
        let f = fact("f1", "person:alpha", "keeps a paper notebook", Provenance::Inference, date(2026, 7, 24));
        assert_eq!(
            render_fact_row(&f),
            "| f1 | person:alpha | keeps a paper notebook |  | inference | active | 2026-07-24 |"
        );
    }

    #[test]
    fn both_provenances_round_trip_via_their_own_column() {
        let home = EntityId::person("alpha");
        let testi = fact("f1", "person:alpha", "speaks two languages", Provenance::Testimony, date(2026, 1, 1));
        let infer = fact("f2", "person:alpha", "prefers mornings ❓", Provenance::Inference, date(2026, 1, 2));
        assert_eq!(parse_fact_row(&render_fact_row(&testi), &home).unwrap(), testi);
        assert_eq!(parse_fact_row(&render_fact_row(&infer), &home).unwrap(), infer);
    }

    #[test]
    fn subject_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact("f1", "person:a|b", "x", Provenance::Testimony, date(2026, 7, 24));
        let row = render_fact_row(&f);
        assert!(row.contains("person:a\\|b"), "subject pipe must be escaped: {row}");
        assert_eq!(parse_fact_row(&row, &f.home).unwrap(), f);
    }

    #[test]
    fn content_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact("f1", "person:alpha", "reads a|b|c notation", Provenance::Testimony, date(2026, 7, 24));
        let row = render_fact_row(&f);
        assert!(row.contains("a\\|b\\|c"), "pipes must be escaped in the row: {row}");
        assert_eq!(parse_fact_row(&row, &f.home).unwrap(), f);
    }

    #[test]
    fn header_and_separator_are_not_facts() {
        let home = EntityId::person("alpha");
        assert!(parse_fact_row(TABLE_HEADER, &home).is_none());
        assert!(parse_fact_row(TABLE_SEP, &home).is_none());
    }

    #[test]
    fn next_id_increments_over_existing() {
        let existing = vec![
            fact("f1", "person:alpha", "a", Provenance::Testimony, date(2026, 1, 1)),
            fact("f3", "person:alpha", "b", Provenance::Testimony, date(2026, 1, 1)),
        ];
        assert_eq!(next_fact_id(&existing), FactId("f4".into()));
        assert_eq!(next_fact_id(&[]), FactId("f1".into()));
    }

    /// A doc written entirely in the pre-`details` format — old header, old
    /// separator, old rows — keeps reading, and a new row appends beside the old
    /// ones without a migration step.
    #[test]
    fn append_into_a_legacy_table_then_parse_finds_both() {
        let doc = "# About\n\nSome prose.\n\n```yaml\nid: person:alpha\n```\n\n### ⚙ facts\n\n| id | subject | content | provenance | status | date |\n| --- | --- | --- | --- | --- | --- |\n| f1 | person:alpha | plays go | testimony | active | 2026-07-01 |\n";
        let f = fact("f2", "person:alpha", "learning Rust", Provenance::Inference, date(2026, 7, 2));
        let updated = with_fact_appended(doc, &render_fact_row(&f));
        let parsed = parse_facts_table(&updated);
        assert_eq!(parsed.len(), 2, "the legacy row and the new one both read");
        assert_eq!(parsed[0].content, "plays go");
        assert_eq!(parsed[1], f);
        assert!(updated.contains("Some prose."), "prose above the table is untouched");
    }

    #[test]
    fn seeded_doc_has_a_marker_and_an_empty_parseable_table() {
        let home = Entity { id: EntityId::person("alpha"), name: "Alpha".into(), ..alpha() };
        let doc = seeded_doc(&home);
        assert_eq!(parse_id_marker(&doc).as_deref(), Some("person:alpha"));
        assert!(parse_facts_table(&doc).is_empty());
        let f = fact("f1", "person:alpha", "first fact", Provenance::Testimony, date(2026, 7, 24));
        let updated = with_fact_appended(&doc, &render_fact_row(&f));
        assert_eq!(parse_facts_table(&updated), vec![f]);
        // A fact's own `id` (below the table) must not be mistaken for the marker.
        assert_eq!(parse_id_marker(&updated).as_deref(), Some("person:alpha"));
    }

    #[test]
    fn marker_is_absent_when_there_is_no_machine_block() {
        assert_eq!(parse_id_marker("# just prose\n\nnothing structured here"), None);
    }
}
