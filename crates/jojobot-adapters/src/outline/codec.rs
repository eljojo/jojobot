//! The pure fact-table codec: markdown ⇄ [`Fact`]. No I/O, so it's unit-tested
//! with no network. jojobot's schema over a markdown doc lives here — the row
//! format, the `### ⚙ facts` table, the embedded `id:` identity marker, and the
//! cell escaping that keeps an adversarial value from corrupting the table.

use jiff::civil::Date;

use jojobot_domain::memory::{
    Boot, Edge, EdgeShape, Entity, EntityId, Fact, FactId, FactStatus, Provenance, validate_subject,
};

/// The header that marks the machine-readable fact table at the bottom of a doc.
pub(super) const FACTS_HEADER: &str = "### ⚙ facts";
/// The table's column header row.
pub(super) const TABLE_HEADER: &str =
    "| id | subject | content | details | provenance | status | date | edges |";
/// The markdown table separator under the header.
pub(super) const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- | --- | --- |";
/// Where each field sits in a row. **Four layouts exist on disk** — the schema
/// grew twice and was reshuffled once — and rows written under every one of them
/// must keep reading. A column is added to a row on its next touch (lazy
/// migration); there is no sweep.
///
/// Width tells three of them apart. It cannot tell the last two apart: the
/// slice-1 row (`id | subject | content | status | date | edges`, provenance
/// riding a trailing `❓` on the content cell) is six cells wide, and so is the
/// pre-`details` row that replaced it (`id | subject | content | provenance |
/// status | date`) — with a different meaning in every column after `content`.
/// **Which cell holds a date** is what separates them.
struct Layout {
    /// Absent before the `details` column existed.
    details: Option<usize>,
    /// Absent in slice 1, where a trailing `❓` on the content cell carried it.
    provenance: Option<usize>,
    status: usize,
    date: usize,
    /// Absent in the two shapes written between slice 1 and the `edges` column.
    edges: Option<usize>,
}

/// The layout of a row, or `None` if it is not a fact row at all.
///
/// The six-cell ambiguity is resolved by looking for the date: whichever of the
/// two candidate cells parses as one names the layout. A row where neither does
/// is unreadable under both, so it is no row — the same verdict either way.
fn layout_of(cells: &[String]) -> Option<Layout> {
    let is_date = |i: usize| cells.get(i).is_some_and(|c| c.trim().parse::<Date>().is_ok());
    match cells.len() {
        8 => Some(Layout { details: Some(3), provenance: Some(4), status: 5, date: 6, edges: Some(7) }),
        7 => Some(Layout { details: Some(3), provenance: Some(4), status: 5, date: 6, edges: None }),
        // Pre-`details`: … | provenance | status | date
        6 if is_date(5) => {
            Some(Layout { details: None, provenance: Some(3), status: 4, date: 5, edges: None })
        }
        // Slice 1: … | status | date | edges
        6 if is_date(4) => {
            Some(Layout { details: None, provenance: None, status: 3, date: 4, edges: Some(5) })
        }
        _ => None,
    }
}

/// Render a fact's edge for its cell: `shape=object`, empty when there is none.
/// `=` rather than `:`, because an object id already carries a colon.
fn render_edge(edge: Option<&Edge>) -> String {
    edge.map(|e| format!("{}={}", e.shape.as_token(), e.object))
        .unwrap_or_default()
}

/// Parse an `edges` cell. Tolerant in one direction: anything that isn't a
/// well-formed `shape=object` costs the **edge**, never the fact — a hand-typo in
/// one cell must not take a claim off the page.
fn parse_edge(cell: &str) -> Option<Edge> {
    let (shape, object) = cell.trim().split_once('=')?;
    let shape = EdgeShape::from_token(shape)?;
    let object = EntityId(object.trim().to_string());
    validate_subject(&object).ok()?;
    Some(Edge::new(shape, object))
}

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
        "| {} | {} | {} | {} | {} | {} | {} | {} |",
        f.id,
        escape_cell(&f.subject.to_string()),
        escape_cell(&f.content),
        escape_cell(f.details.as_deref().unwrap_or_default()),
        f.provenance.as_token(),
        f.status.as_token(),
        f.date,
        escape_cell(&render_edge(f.edge.as_ref())),
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row. `home` is the entity whose doc the
/// row was read from — the other half of the fact's global address.
///
/// Every row layout that exists on disk is accepted — see [`Layout`]. Anything
/// else is not a fact row.
pub(super) fn parse_fact_row(row: &str, home: &EntityId) -> Option<Fact> {
    let cells = split_cells(row);
    let at = layout_of(&cells)?;
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

    let cell = |i: usize| cells[i].as_str();
    let details = at
        .details
        .map(|i| cells[i].trim())
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    // A row from before the provenance column reads as inference: absent is the
    // less-trusted side, and a read never promotes a claim it cannot vouch for.
    let provenance = at.provenance.map_or(Provenance::default(), |i| Provenance::from_token(cell(i)));
    let status = FactStatus::from_token(cell(at.status));
    let date: Date = cells[at.date].trim().parse().ok()?;
    let edge = at.edges.and_then(|i| parse_edge(cell(i)));

    Some(Fact {
        id: FactId(id.to_string()),
        home: home.clone(),
        subject: EntityId(subject.to_string()),
        content: content.to_string(),
        details,
        provenance,
        status,
        date,
        edge,
    })
}

/// Locate the fact table's line range within a doc: the half-open span of the
/// contiguous `|` lines under the `### ⚙ facts` header. `None` if no header.
///
/// The table is found **wherever it sits** under the header, not only flush
/// against it — a human will type a note in there, and requiring adjacency once
/// meant the reader saw no facts while the writer started a second table above
/// the note and orphaned every fact already on the page. An empty span (start ==
/// end) means the header is there but no table has been drawn yet.
fn facts_region(lines: &[&str]) -> Option<(usize, usize)> {
    let header = lines.iter().position(|l| l.trim() == FACTS_HEADER)?;
    let is_row = |l: &&str| l.trim_start().starts_with('|');

    match lines[header + 1..].iter().position(is_row) {
        Some(offset) => {
            let start = header + 1 + offset;
            let mut end = start;
            while end < lines.len() && is_row(&lines[end]) {
                end += 1;
            }
            Some((start, end))
        }
        // No table yet: the insertion point is the first line after the header.
        None => {
            let mut i = header + 1;
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            Some((i, i))
        }
    }
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
///
/// **Only a row the reader can parse is a target.** The writer used to match on
/// the id cell alone, a wider predicate than [`parse_fact_row`]'s: an edit could
/// then land on a row no read had ever returned — silently destroying a fact the
/// caller never saw, and passing read-back, because the verification matched
/// that same wrong row. A row the reader skips is inert: unreadable, and now
/// unwritable too.
pub(super) fn with_row_replaced(
    doc: &str,
    home: &EntityId,
    id: &FactId,
    row: &str,
) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = facts_region(&lines)?;
    let target = lines[start..end]
        .iter()
        .position(|l| parse_fact_row(l, home).is_some_and(|f| &f.id == id))?
        + start;

    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out[target] = row.to_string();
    Some(out.join("\n"))
}

/// The local id a table row carries, if it looks like a fact row at all — the
/// *widest* reading, deliberately: this is what id minting counts, so a row the
/// reader can't parse still holds its id and can never be handed out twice.
fn row_id(row: &str) -> Option<String> {
    let cells = split_cells(row);
    // Width alone, on purpose: this is deliberately wider than `layout_of`, so a
    // row the reader gives up on still holds its id and can never be re-minted.
    if !matches!(cells.len(), 6..=8) {
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

/// The next light/local id for a doc: `f{max+1}` over **every** `fN` id present
/// in the table, counted with the widest reading ([`row_id`]) rather than the
/// reader's.
///
/// Minting off the parsed facts alone was the bug: a row the reader dropped
/// freed its id, the next capture handed it out again, and two rows ended up
/// sharing one address — after which an edit to that address was a coin flip.
/// An id is taken the moment it appears on the page, readable or not.
pub(super) fn next_fact_id(doc: &str) -> FactId {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = facts_region(&lines).unwrap_or((0, 0));
    let max = lines[start..end]
        .iter()
        .filter_map(|l| row_id(l)?.strip_prefix('f')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    FactId(format!("f{}", max + 1))
}

/// The half-open line span of the **machine block**: the fenced block above the
/// fact table that carries the `id:` marker.
///
/// Keyed on the marker, not on being the first fence in the doc. A doc's prose
/// may hold fenced blocks of its own — pinning on position once meant an entity
/// edit overwrote the user's snippet and left the real frontmatter stale below
/// it, with read-back passing because the reader also took the first block.
/// Reader and writer now agree on which block is jojobot's.
fn machine_block(lines: &[&str]) -> Option<(usize, usize)> {
    let limit = lines
        .iter()
        .position(|l| l.trim() == FACTS_HEADER)
        .unwrap_or(lines.len());
    let is_fence = |l: &&str| l.trim_start().starts_with("```");

    let mut i = 0;
    while i < limit {
        if !is_fence(&lines[i]) {
            i += 1;
            continue;
        }
        let close = lines[i + 1..limit]
            .iter()
            .position(is_fence)
            .map(|o| i + 1 + o);
        let close = close?; // unterminated fence: no machine block
        // The `id:` must be a well-formed entity id, not merely present. Users
        // paste YAML and frontmatter into their own docs, and a snippet with an
        // `id:` line would otherwise take the doc's identity with it — leaving
        // the real entity unresolvable and its facts unreachable.
        if lines[i + 1..close]
            .iter()
            .any(|l| field_of(l, "id").is_some_and(|v| validate_subject(&EntityId(v)).is_ok()))
        {
            return Some((i, close + 1));
        }
        i = close + 1;
    }
    None
}

/// The value of a `key: value` line, if this line is one.
fn field_of(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.strip_prefix(':')?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// Read one `key: value` field out of the machine block. Fields are read from
/// **inside** the block when there is one, so a `name:`-shaped sentence in the
/// prose is prose — it can neither forge a field nor hijack the id marker. A doc
/// with no fenced block at all falls back to scanning above the fact table, so
/// an older or hand-written marker still identifies its entity.
fn parse_field(doc: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = match machine_block(&lines) {
        Some((open, close)) => (open + 1, close - 1),
        None => (
            0,
            lines
                .iter()
                .position(|l| l.trim() == FACTS_HEADER)
                .unwrap_or(lines.len()),
        ),
    };
    lines[start..end.min(lines.len())]
        .iter()
        .find_map(|l| field_of(l, key))
}

/// The **human** half of a doc: everything that is neither jojobot's machine
/// block nor its fact table. This is what the search index carries as prose, and
/// it is why a detail demoted into a paragraph is still findable — nobody has to
/// have remembered to file it as a fact.
///
/// Read generously: a doc with no marker and no table is prose from end to end,
/// because a page the user wrote by hand is exactly the page worth finding.
///
/// **The boundary is the table, not the header.** [`facts_region`] finds the
/// table wherever it sits under the header — deliberately, because humans type
/// notes in that gap — but prose used to stop at the header line, so anything in
/// the gap belonged to no hit class at all: a write preserved it forever and no
/// search could ever surface it. Text below the table is prose for the same
/// reason.
pub(super) fn parse_prose(doc: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    let header = lines.iter().position(|l| l.trim() == FACTS_HEADER);
    let machine = machine_block(&lines);
    let table = facts_region(&lines).filter(|(start, end)| start < end);

    let within = |span: Option<(usize, usize)>, i: usize| {
        span.is_some_and(|(start, end)| i >= start && i < end)
    };
    let kept = lines.iter().enumerate().filter_map(|(i, line)| {
        (Some(i) != header && !within(machine, i) && !within(table, i)).then_some(*line)
    });
    kept.collect::<Vec<&str>>().join("\n").trim().to_string()
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

/// Return `doc` with its machine block rewritten to `entity` — an in-place
/// metadata edit (fix the source). Prose above it and the fact table below are
/// untouched, including any fenced block the user wrote themselves. A doc with
/// no machine block yet gets one at the top rather than losing the edit.
pub(super) fn with_frontmatter_replaced(doc: &str, entity: &Entity) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    let block = frontmatter(entity);
    match machine_block(&lines) {
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
    use jojobot_domain::memory::{Boot, Edge, EdgeShape, Entity, EntityKind};

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
            edge: None,
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

    // --- a row the reader can't parse is inert, never a target ----------------
    //
    // These docs are human-editable wiki pages, so a hand-typed date or an
    // emptied cell is expected. The rule that keeps that harmless: **mint ids
    // over every row that carries one, address only rows the reader can parse.**
    // When those two sets disagreed, an unreadable row's id got re-minted and
    // the next edit landed on the unreadable row — destroying it, and passing
    // read-back because the verification matched the same wrong row.

    /// A hand-broken row still occupies its id: the next fact minted must step
    /// over it, or two rows end up sharing one address.
    ///
    /// **The silent drop this test observes is a known gap, not the contract.**
    /// The architecture calls for a malformed record to be *quarantined and
    /// surfaced* — "N records couldn't be parsed" — so a human or the AI can
    /// repair it. M1 drops it with no signal: the fact is simply gone from every
    /// read. What is asserted here is that the drop is at least *inert* (the id
    /// stays reserved, the row is never overwritten). Quarantine surfacing is
    /// deferred work; the `is_empty()` below records today's behaviour, it does
    /// not endorse it.
    #[test]
    fn an_unparseable_row_reserves_its_id_but_is_silently_dropped() {
        let doc = with_fact_appended(
            &seeded_doc(&alpha()),
            // A date a human retyped in the wiki — unparseable, so no reader sees it.
            "| f1 | person:alpha | allergic to penicillin |  | testimony | active | July 1, 2026 |",
        );
        assert!(
            parse_facts_table(&doc).is_empty(),
            "today the row vanishes from every read, with no quarantine signal"
        );
        assert_eq!(
            next_fact_id(&doc),
            FactId("f2".into()),
            "an id the reader can't see is still taken"
        );
    }

    /// …and it is not addressable: an edit aimed at that id must miss rather
    /// than overwrite a row nobody could read (and so nobody could have meant).
    #[test]
    fn an_unparseable_row_is_never_the_target_of_an_edit() {
        let doc = with_fact_appended(
            &seeded_doc(&alpha()),
            "| f1 | person:alpha | allergic to penicillin |  | testimony | active | July 1, 2026 |",
        );
        assert!(
            with_row_replaced(&doc, &EntityId::person("alpha"), &FactId("f1".into()), "| f1 | x |")
                .is_none(),
            "a row the reader skipped must not be rewritten"
        );
    }

    /// With both halves in place, a doc that already holds a broken row keeps it
    /// while the addressed row edits normally — the broken row is inert, not a
    /// landmine.
    #[test]
    fn an_edit_beside_a_broken_row_touches_only_the_addressed_row() {
        let mut doc = with_fact_appended(
            &seeded_doc(&alpha()),
            "| f1 | person:alpha | allergic to penicillin |  | testimony | active | July 1, 2026 |",
        );
        let good = fact("f2", "person:alpha", "takes the 8am train", Provenance::Testimony, date(2026, 7, 2));
        doc = with_fact_appended(&doc, &render_fact_row(&good));

        let edited = Fact { content: "takes the 7am train".into(), ..good };
        let updated = with_row_replaced(
            &doc,
            &EntityId::person("alpha"),
            &FactId("f2".into()),
            &render_fact_row(&edited),
        )
        .expect("the addressed row exists");

        assert!(
            updated.contains("allergic to penicillin"),
            "the unreadable row must survive untouched: {updated}"
        );
        assert!(updated.contains("takes the 7am train"));
        assert!(!updated.contains("takes the 8am train"));
    }

    // --- the table is found wherever it sits under its header -----------------

    /// A user typing a line under `### ⚙ facts` must not hide the table. It did:
    /// the reader saw no facts and the writer started a *second* table above the
    /// note, orphaning every fact already there.
    #[test]
    fn a_note_under_the_facts_header_does_not_hide_or_fork_the_table() {
        let doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\nnote: do not edit below\n\n\
             {TABLE_HEADER}\n{TABLE_SEP}\n\
             | f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |\n"
        );
        assert_eq!(parse_facts_table(&doc).len(), 1, "the table is still readable");

        let f2 = fact("f2", "person:alpha", "learning Rust", Provenance::Inference, date(2026, 7, 2));
        let updated = with_fact_appended(&doc, &render_fact_row(&f2));
        assert_eq!(
            updated.matches(TABLE_HEADER).count(),
            1,
            "one table, not a second one above the note: {updated}"
        );
        assert_eq!(parse_facts_table(&updated).len(), 2, "both facts readable");
        assert!(updated.contains("note: do not edit below"), "the note survives");
    }

    // --- the frontmatter block is the one carrying the marker -----------------

    /// A fenced block in the prose is not the machine block. Overwriting it
    /// destroyed the user's snippet AND left the real frontmatter stale below —
    /// with read-back passing, because the reader took the first match too.
    #[test]
    fn frontmatter_replacement_targets_the_block_carrying_the_marker() {
        let doc = format!(
            "Prose about this entity.\n\n```\nimportant snippet the user wrote\n```\n\n{}\n\n{FACTS_HEADER}\n",
            frontmatter(&alpha())
        );
        let renamed = Entity { name: "Alpha Renamed".into(), ..alpha() };
        let updated = with_frontmatter_replaced(&doc, &renamed);

        assert!(
            updated.contains("important snippet the user wrote"),
            "the user's own fenced block must survive: {updated}"
        );
        assert_eq!(
            updated.matches("id: person:alpha").count(),
            1,
            "exactly one machine block, not a stale duplicate: {updated}"
        );
        assert_eq!(parse_entity(&updated).unwrap(), renamed);
    }

    /// A user pasting a YAML/frontmatter snippet into their own doc must not
    /// hand that doc's identity to it. Any fence with an `id:` line used to be
    /// adopted as the machine block — the mirror image of the bug the
    /// marker-anchored lookup was written to fix, and worse: the doc stops
    /// resolving to its entity, so its facts become unreachable.
    #[test]
    fn a_decoy_fence_cannot_steal_the_docs_identity() {
        let doc = format!(
            "Prose about this entity.\n\n\
             ```yaml\nid: my-service\nversion: 2\n```\n\n\
             {}\n\n{FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n\
             | f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |\n",
            frontmatter(&alpha())
        );

        assert_eq!(
            parse_id_marker(&doc).as_deref(),
            Some("person:alpha"),
            "the decoy's `id:` is not an entity id, so it is not a marker"
        );
        assert_eq!(parse_entity(&doc).map(|e| e.id), Some(EntityId::person("alpha")));
        assert_eq!(parse_facts_table(&doc).len(), 1, "the doc's facts stay reachable");
    }

    /// The same predicate protects the write path: an entity edit rewrites
    /// jojobot's block, never the pasted snippet.
    #[test]
    fn a_decoy_fence_is_not_rewritten_by_an_entity_edit() {
        let doc = format!(
            "```yaml\nid: my-service\nversion: 2\n```\n\n{}\n\n{FACTS_HEADER}\n",
            frontmatter(&alpha())
        );
        let renamed = Entity { name: "Alpha Renamed".into(), ..alpha() };
        let updated = with_frontmatter_replaced(&doc, &renamed);

        assert!(updated.contains("id: my-service"), "the decoy survives: {updated}");
        assert!(updated.contains("version: 2"));
        assert_eq!(
            updated.matches("id: person:alpha").count(),
            1,
            "one machine block, rewritten in place: {updated}"
        );
        assert_eq!(parse_entity(&updated).unwrap(), renamed);
    }

    /// A `name:`-looking line in the prose is prose, not a field — the machine
    /// block is where fields live, so prose can't forge one.
    #[test]
    fn a_field_shaped_line_in_prose_is_not_read_as_frontmatter() {
        let doc = format!(
            "Notes: name: Someone Else\nid: person:hijacked\n\n{}\n\n{FACTS_HEADER}\n",
            frontmatter(&alpha())
        );
        let e = parse_entity(&doc).expect("the machine block identifies the entity");
        assert_eq!(e.id, EntityId::person("alpha"), "prose must not forge the marker");
        assert_eq!(e.name, "Alpha", "prose must not forge a field");
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

    /// **The slice-1 table.** Before `provenance` was its own column it rode a
    /// trailing `❓` on the content cell, and the row was
    /// `id | subject | content | status | date | edges` — six cells, exactly as
    /// wide as the pre-`details` shape that replaced it
    /// (`id | subject | content | provenance | status | date`) and meaning
    /// something different in every column after `content`.
    ///
    /// Reading one as the other put a status token in the provenance cell, a
    /// date in the status cell, and the empty `edges` cell where the date should
    /// be — which failed to parse, so the row was dropped. Silently, and for
    /// every row on the page: a slice-1 doc read back as having no facts at all.
    ///
    /// Width alone cannot tell the two apart. **Which cell holds a date** can.
    #[test]
    fn a_slice_one_row_parses_beside_the_six_column_shape_that_replaced_it() {
        let home = EntityId::person("alpha");

        let slice1 = parse_fact_row(
            "| f1 | person:alpha | plays go ❓ | active | 2026-07-01 |  |",
            &home,
        )
        .expect("a slice-1 row must parse");
        assert_eq!(slice1.content, "plays go ❓", "the ❓ is content now; nothing invents a column");
        assert_eq!(slice1.details, None);
        assert_eq!(slice1.status, FactStatus::Active);
        assert_eq!(slice1.date, date(2026, 7, 1));
        assert_eq!(slice1.edge, None);
        assert_eq!(
            slice1.provenance,
            Provenance::Inference,
            "no provenance column means the less-trusted side, never a promotion"
        );

        // Slice 1 wrote a blank status cell for active…
        let blank = parse_fact_row(
            "| f2 | person:alpha | keeps a paper notebook |  | 2026-07-02 |  |",
            &home,
        )
        .expect("a blank status cell must parse");
        assert_eq!(blank.status, FactStatus::Active);
        assert_eq!(blank.date, date(2026, 7, 2));

        // …and superseded survives the trip, where dropping the row would have
        // quietly promoted a retired claim back to current truth.
        let retired = parse_fact_row(
            "| f3 | person:alpha | the old address | superseded | 2026-07-03 |  |",
            &home,
        )
        .expect("a superseded slice-1 row must parse");
        assert_eq!(retired.status, FactStatus::Superseded);

        // The shape that replaced it is untouched: same width, other meaning.
        let no_details =
            parse_fact_row("| f1 | person:alpha | plays go | testimony | active | 2026-07-01 |", &home)
                .expect("the pre-details row must still parse");
        assert_eq!(no_details.provenance, Provenance::Testimony);
        assert_eq!(no_details.status, FactStatus::Active);
        assert_eq!(no_details.date, date(2026, 7, 1));
    }

    /// A whole slice-1 page reads, keeps its ids reserved, and gains the current
    /// columns on the rows a write touches — the same lazy migration the `edges`
    /// and `details` additions got, now for a column that was *missing*.
    #[test]
    fn a_slice_one_table_reads_and_gains_the_current_columns_on_touch() {
        let doc = "# Alpha\n\nSome prose.\n\n```yaml\nid: person:alpha\n```\n\n### ⚙ facts\n\n\
                   | id | subject | content | status | date | edges |\n\
                   | --- | --- | --- | --- | --- | --- |\n\
                   | f1 | person:alpha | plays go ❓ | active | 2026-07-01 |  |\n\
                   | f2 | person:alpha | speaks two languages |  | 2026-07-02 |  |\n";
        let facts = parse_facts_table(doc);
        assert_eq!(facts.len(), 2, "both slice-1 rows read: {facts:?}");
        assert_eq!(
            next_fact_id(doc),
            FactId("f3".into()),
            "the ids on the page are taken"
        );

        // A capture lands beside them, and everything still reads.
        let fresh = Fact {
            details: Some("mentioned twice".into()),
            ..fact("f3", "person:alpha", "learning Rust", Provenance::Testimony, date(2026, 7, 3))
        };
        let appended = with_fact_appended(doc, &render_fact_row(&fresh));
        let parsed = parse_facts_table(&appended);
        assert_eq!(parsed.len(), 3, "old rows and new one together: {parsed:?}");
        assert_eq!(parsed[2], fresh);
        assert!(appended.contains("Some prose."), "prose above the table is untouched");

        // …and touching a slice-1 row rewrites it in the current eight-cell form.
        let touched = with_row_replaced(
            &appended,
            &EntityId::person("alpha"),
            &FactId("f1".into()),
            &render_fact_row(&facts[0]),
        )
        .expect("a slice-1 row is addressable");
        assert!(
            touched.contains("| f1 | person:alpha | plays go ❓ |  | inference | active | 2026-07-01 |  |"),
            "the touched row carries every current column: {touched}"
        );
    }

    // --- the prose half of a doc ----------------------------------------------

    /// Prose is the doc minus jojobot's two machine sections. The point of
    /// reading it at all: a detail a human demoted into a paragraph stays
    /// findable, without anyone having filed it as a fact.
    #[test]
    fn prose_is_the_doc_without_the_machine_block_or_the_fact_table() {
        let doc = with_fact_appended(
            &format!(
                "Alpha keeps a paper notebook and hates phone calls.\n\n{}\n\n{FACTS_HEADER}\n\n\
                 {TABLE_HEADER}\n{TABLE_SEP}\n",
                frontmatter(&alpha())
            ),
            &render_fact_row(&fact("f1", "person:alpha", "plays go", Provenance::Testimony, date(2026, 7, 1))),
        );
        let prose = parse_prose(&doc);
        assert_eq!(prose, "Alpha keeps a paper notebook and hates phone calls.");
        assert!(!prose.contains("id: person:alpha"), "the machine block is not prose");
        assert!(!prose.contains("plays go"), "a fact row is not prose");
    }

    /// **The gap between the header and the table is prose.** The reader finds
    /// the table wherever it sits under `### ⚙ facts` — deliberately, because a
    /// human types notes in there — so a write preserves whatever is in that
    /// gap. But prose stopped at the header line, so that same text was in no
    /// hit class at all: kept forever, findable never. Prose is everything that
    /// is neither the machine block nor the actual table rows.
    #[test]
    fn a_note_between_the_facts_header_and_the_table_is_prose() {
        let doc = format!(
            "Prose above.\n\n{}\n\n{FACTS_HEADER}\n\nthe pass was closed on Tuesday\n\n\
             {TABLE_HEADER}\n{TABLE_SEP}\n\
             | f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |  |\n\
             \ntrailing note under the table\n",
            frontmatter(&alpha())
        );
        let prose = parse_prose(&doc);
        assert!(prose.contains("Prose above."), "got: {prose}");
        assert!(
            prose.contains("the pass was closed on Tuesday"),
            "a note in the header/table gap must be findable: {prose}"
        );
        assert!(
            prose.contains("trailing note under the table"),
            "…and so must one below the table: {prose}"
        );
        assert!(!prose.contains(FACTS_HEADER), "jojobot's own header is not prose: {prose}");
        assert!(!prose.contains("plays go"), "a fact row is not prose: {prose}");
        assert!(!prose.contains("id: person:alpha"), "the machine block is not prose: {prose}");
        // …and the note is still just a note: it neither hides nor forks the table.
        assert_eq!(parse_facts_table(&doc).len(), 1);
    }

    /// A doc the user wrote by hand — no marker, no table — is prose end to end.
    /// It is not an entity, and it is still exactly the page worth finding.
    #[test]
    fn a_hand_written_doc_is_prose_end_to_end() {
        let doc = "Notes from the trip.\n\nThe pass was closed on Tuesday.";
        assert_eq!(parse_prose(doc), doc);
        assert_eq!(parse_entity(doc), None);
    }

    /// A fenced block the user wrote is prose; only jojobot's own is stripped.
    #[test]
    fn a_users_own_fenced_block_stays_in_the_prose() {
        let doc = format!(
            "Prose above.\n\n```\nimportant snippet the user wrote\n```\n\n{}\n\n{FACTS_HEADER}\n",
            frontmatter(&alpha())
        );
        let prose = parse_prose(&doc);
        assert!(prose.contains("important snippet the user wrote"), "got: {prose}");
        assert!(!prose.contains("source: crm-card"), "jojobot's block is stripped: {prose}");
    }

    // --- the edges column -----------------------------------------------------

    /// Every shape and its object survive the row, in the `edges` cell.
    #[test]
    fn every_edge_shape_round_trips_in_its_own_cell() {
        let objects = [
            (EdgeShape::Location, "place:north-trail"),
            (EdgeShape::Membership, "org:north-trail-club"),
            (EdgeShape::Attendance, "event:winter-fest"),
            (EdgeShape::About, "topic:widgets"),
        ];
        for (shape, object) in objects {
            let f = Fact {
                edge: Some(Edge::new(shape, EntityId(object.into()))),
                ..fact("f1", "person:alpha", "a claim", Provenance::Inference, date(2026, 7, 1))
            };
            let parsed = parse_fact_row(&render_fact_row(&f), &EntityId::person("alpha")).unwrap();
            assert_eq!(parsed, f, "the {shape} edge must survive the row");
        }
    }

    /// The literal wire format, pinned — a symmetric round-trip alone can't catch
    /// a schema drift both sides share. The edges cell is `shape=object`, and the
    /// **storage** token is the lowercase one (`membership`, not `memberOf`):
    /// schema.org names are a response vocabulary, not a storage format.
    #[test]
    fn renders_the_exact_pinned_row_with_an_edge() {
        let f = Fact {
            edge: Some(Edge::new(
                EdgeShape::Membership,
                EntityId("org:north-trail-club".into()),
            )),
            ..fact("f2", "person:alpha", "rides with the club", Provenance::Testimony, date(2026, 7, 24))
        };
        assert_eq!(
            render_fact_row(&f),
            "| f2 | person:alpha | rides with the club |  | testimony | active | 2026-07-24 | \
             membership=org:north-trail-club |"
        );
    }

    /// A row written before the edges column existed still parses — it just draws
    /// no edge. Old docs are read fine and get the column on their next touch;
    /// a schema addition never orphans the rows already on disk.
    #[test]
    fn a_row_without_the_edges_column_still_parses() {
        let previous =
            "| f1 | person:alpha | plays go | twice a week | testimony | active | 2026-07-01 |";
        let parsed =
            parse_fact_row(previous, &EntityId::person("alpha")).expect("the 7-cell row must parse");
        assert_eq!(parsed.content, "plays go");
        assert_eq!(parsed.details.as_deref(), Some("twice a week"));
        assert_eq!(parsed.date, date(2026, 7, 1));
        assert_eq!(parsed.edge, None, "no column, no edge");
    }

    /// An `edges` cell the reader can't make sense of costs the **edge**, never
    /// the fact: the row still reads, so a hand-typo in one cell can't take a
    /// claim off the page.
    #[test]
    fn a_garbled_edges_cell_costs_the_edge_not_the_fact() {
        for cell in ["knows=person:beta", "location", "=place:x", "location=nope:x", "location="] {
            let row = format!(
                "| f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 | {cell} |"
            );
            let parsed = parse_fact_row(&row, &EntityId::person("alpha"))
                .unwrap_or_else(|| panic!("the fact must still read with cell {cell:?}"));
            assert_eq!(parsed.content, "plays go");
            assert_eq!(parsed.edge, None, "an unreadable edge is dropped, not guessed: {cell:?}");
        }
    }

    /// An appended row lands in a table that predates the column, and both read.
    #[test]
    fn append_into_a_pre_edges_table_then_parse_finds_both() {
        let doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n\
             | id | subject | content | details | provenance | status | date |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n\
             | f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |\n"
        );
        let f2 = Fact {
            edge: Some(Edge::new(EdgeShape::Location, EntityId("place:shelbyville".into()))),
            ..fact("f2", "person:alpha", "spending the winter away", Provenance::Testimony, date(2026, 7, 2))
        };
        let parsed = parse_facts_table(&with_fact_appended(&doc, &render_fact_row(&f2)));
        assert_eq!(parsed.len(), 2, "the pre-edges row and the new one both read");
        assert_eq!(parsed[0].edge, None);
        assert_eq!(parsed[1], f2);
    }

    // --- the status column ----------------------------------------------------

    /// A retired **`negated`** row still reads — as superseded, which is what it
    /// behaved like anyway: out of a default search, its id and content intact.
    /// A schema *removal* must not orphan the rows already on disk any more than
    /// an addition may; the row is rewritten in the current spelling on its next
    /// touch, with no sweep.
    #[test]
    fn a_legacy_negated_row_reads_as_superseded_and_rewrites_on_touch() {
        let legacy = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n\
             | f1 | person:alpha | does NOT play the theremin |  | testimony | negated | 2026-07-01 |  |\n"
        );
        let facts = parse_facts_table(&legacy);
        assert_eq!(facts.len(), 1, "the row must still read");
        assert_eq!(facts[0].status, FactStatus::Superseded);
        assert_eq!(facts[0].content, "does NOT play the theremin", "content is untouched");

        let touched = with_row_replaced(
            &legacy,
            &EntityId::person("alpha"),
            &FactId("f1".into()),
            &render_fact_row(&facts[0]),
        )
        .expect("the legacy row is addressable");
        assert!(!touched.contains("negated"), "the retired token is gone on touch: {touched}");
        assert!(touched.contains("| superseded |"), "…rewritten as superseded: {touched}");
    }

    /// Both lifecycle states survive the row, so a superseded fact reads back
    /// superseded rather than quietly returning as current truth.
    #[test]
    fn every_status_round_trips_through_the_row() {
        for status in [FactStatus::Active, FactStatus::Superseded] {
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
            status: FactStatus::Superseded,
            ..fact("f2", "person:alpha", "", Provenance::Inference, date(2026, 7, 2))
        };
        let updated = with_row_replaced(&doc, &EntityId::person("alpha"), &FactId("f2".into()), &render_fact_row(&edited))
            .expect("the row exists");

        let facts = parse_facts_table(&updated);
        assert_eq!(facts.len(), 3, "no row gained or lost");
        assert_eq!(facts[1].content, "second, corrected");
        assert_eq!(facts[1].status, FactStatus::Superseded);
        assert_eq!(facts[0].content, "first");
        assert_eq!(facts[2].content, "third");
        assert!(!updated.contains("| second |"), "the old row is gone, not left beside");
    }

    /// Replacing a row that isn't there changes nothing and says so — the store
    /// turns that into an error rather than appending a surprise row.
    #[test]
    fn replacing_a_missing_row_reports_rather_than_appends() {
        let doc = seeded_doc(&alpha());
        assert!(with_row_replaced(&doc, &EntityId::person("alpha"), &FactId("f9".into()), "| f9 | x |").is_none());
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
            "| f1 | person:alpha | keeps a paper notebook |  | inference | active | 2026-07-24 |  |"
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
        let mut doc = seeded_doc(&alpha());
        assert_eq!(next_fact_id(&doc), FactId("f1".into()));
        for id in ["f1", "f3"] {
            let row = render_fact_row(&fact(id, "person:alpha", "a", Provenance::Testimony, date(2026, 1, 1)));
            doc = with_fact_appended(&doc, &row);
        }
        assert_eq!(next_fact_id(&doc), FactId("f4".into()));
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

/// What a bare carriage return in a fact's content does — **the reason
/// [`validate_content`](jojobot_domain::memory::validate_content) refuses one.**
///
/// While the store keeps the byte, nothing breaks. But a store that normalizes
/// line endings (`\r` → `\n`, which markdown pipelines routinely do) splits the
/// row in two, and the split ends the table's contiguous run of `|` lines — so
/// **every fact below it is lost as well**, not just the one carrying the CR.
///
/// The domain now rejects a bare `\r` on the write path, so a CR can only reach
/// a cell by a hand edit. These tests stay as the evidence for that rule: they
/// build the rows directly, below validation, and measure the blast radius.
#[cfg(test)]
mod bare_cr {
    use super::*;
    use jiff::civil::date;

    fn doc_with_cr_in_the_first_row() -> String {
        let fact = |id: &str, content: &str| Fact {
            id: FactId(id.into()),
            home: EntityId("person:alpha".into()),
            subject: EntityId("person:alpha".into()),
            content: content.into(),
            details: None,
            provenance: Provenance::Inference,
            status: FactStatus::Active,
            date: date(2026, 7, 25),
            edge: None,
        };
        let mut doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n"
        );
        for (id, content) in [("f1", "hello\rworld"), ("f2", "b"), ("f3", "c"), ("f4", "d")] {
            doc = with_fact_appended(&doc, &render_fact_row(&fact(id, content)));
        }
        doc
    }

    /// A store that preserves the byte: the CR rides inside its cell and every
    /// fact still reads back.
    #[test]
    fn a_preserved_bare_cr_round_trips_inside_its_cell() {
        let doc = doc_with_cr_in_the_first_row();
        let parsed = parse_facts_table(&doc);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].content, "hello\rworld");
    }

    /// A store that normalizes `\r` to `\n` takes the whole table down with it —
    /// the split row ends the table's contiguous span, so the three innocent
    /// rows below are dropped too. Known gap; the guard is a separate decision.
    #[test]
    fn a_normalized_bare_cr_silently_drops_the_rest_of_the_table() {
        let normalized = doc_with_cr_in_the_first_row().replace('\r', "\n");
        assert!(
            parse_facts_table(&normalized).is_empty(),
            "today every fact below the split is lost, silently"
        );
    }
}
