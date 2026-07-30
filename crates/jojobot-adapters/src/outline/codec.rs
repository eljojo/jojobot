//! The pure fact-table codec: markdown ⇄ [`Fact`]. No I/O, so it's unit-tested
//! with no network. jojobot's schema over a markdown doc lives here — the row
//! format, the `### ⚙ facts` table, the embedded `id:` identity marker, and the
//! cell escaping that keeps an adversarial value from corrupting the table.

use jiff::civil::Date;

use jojobot_domain::memory::{
    Boot, Edge, EdgeShape, Entity, EntityId, Fact, FactId, FactStatus, Provenance, event::Event,
    validate_prose, validate_subject,
};

/// The header that marks the machine-readable fact table at the bottom of a
/// doc. **Re-exported from the domain, never re-declared here**: it is part of
/// jojobot's document schema, and the one rule that turns on it
/// ([`validate_prose`]) has to bind the fake as hard as it binds this store.
/// A private copy would be a second literal for one idea, and the copy this
/// store enforced would be the only one anybody checked.
pub(super) use jojobot_domain::memory::{FACTS_HEADER, MACHINERY_FIELD};
/// The table's column header row.
pub(super) const TABLE_HEADER: &str =
    "| id | subject | content | details | provenance | status | date | edges | event |";
/// The markdown table separator under the header.
pub(super) const TABLE_SEP: &str = "| --- | --- | --- | --- | --- | --- | --- | --- | --- |";
/// The doc schema's CURRENT version, stamped into the machine block (`schema:`)
/// by every write. Schema evolution is a standing condition of this system —
/// long-lived docs, written by every era of this software — so the eras are
/// first-class, oldest first, and every one reads forever:
///
///   0 — slice 1:      `id | subject | content | status | date | edges`
///   1 — pre-details:  `id | subject | content | provenance | status | date`
///   2 — details (M1): `id | subject | content | details | provenance | status | date`
///   3 — edges (M2):   8 columns, through the surface release
///   4 — event:        the current 9-column [`TABLE_HEADER`]
///
/// A doc with no `schema:` line predates the field; its rows read by structural
/// inference ([`layout_of`]), which is kept forever — hand-written and ancient
/// docs never stop reading. **Declared beats inferred** ([`era_layout`]): a
/// stamp resolves the six-cell ambiguity by testimony instead of date-sniffing,
/// and makes a future format change that width alone can't betray recognizable
/// at all. Upgrades today are reparse + re-render ([`migrated_region`]); the
/// first version whose upgrade can't be that gets its explicit step registered
/// alongside this constant.
pub(super) const SCHEMA_CURRENT: u32 = 4;

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
/// **Which cell holds a date** is what separates them — and only when exactly
/// one of the two candidates parses as one. Both, or neither, is no evidence.
struct Layout {
    /// Absent before the `details` column existed.
    details: Option<usize>,
    /// Absent in slice 1, where a trailing `❓` on the content cell carried it.
    provenance: Option<usize>,
    status: usize,
    date: usize,
    /// Absent in the two shapes written between slice 1 and the `edges` column.
    edges: Option<usize>,
    /// The event payload. Absent in every layout before schema 4 — which is
    /// every row written before this slice, and most rows forever, because most
    /// facts are not events.
    event: Option<usize>,
}

/// The layout of a row, or `None` if it is not a fact row at all.
///
/// The six-cell ambiguity is resolved by looking for the date: whichever of the
/// two candidate cells parses as one names the layout — but only when **exactly
/// one** of them does. Both parsing is no more evidence than neither parsing, so
/// the verdict is the same in both directions: it is no row.
///
/// The alternative was first-match-wins, which answers regardless and hands back
/// a row carrying a plausible **wrong** date — the pre-`details` reading of a
/// row that may well have been slice-1. A dropped row is visible in a count; a
/// wrong date reads like a fact, and nobody goes back to check it. The id stays
/// reserved either way ([`row_id`] is deliberately wider than this), so an
/// unread row is inert rather than destroyed.
fn layout_of(cells: &[String]) -> Option<Layout> {
    let is_date = |i: usize| {
        cells
            .get(i)
            .is_some_and(|c| c.trim().parse::<Date>().is_ok())
    };
    match cells.len() {
        9 => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: Some(7),
            event: Some(8),
        }),
        8 => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: Some(7),
            event: None,
        }),
        7 => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: None,
            event: None,
        }),
        // Pre-`details`: … | provenance | status | date
        6 if is_date(5) && !is_date(4) => Some(Layout {
            details: None,
            provenance: Some(3),
            status: 4,
            date: 5,
            edges: None,
            event: None,
        }),
        // Slice 1: … | status | date | edges
        6 if is_date(4) && !is_date(5) => Some(Layout {
            details: None,
            provenance: None,
            status: 3,
            date: 4,
            edges: Some(5),
            event: None,
        }),
        _ => None,
    }
}

/// The row layout a declared era names, when the row's width matches that era.
/// A width that doesn't match the doc's own stamp is a hand edit from some
/// other era — structural inference handles it, so the stamp can sharpen a
/// read but never lose a row.
fn era_layout(version: u32, width: usize) -> Option<Layout> {
    match (version, width) {
        (4, 9) => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: Some(7),
            event: Some(8),
        }),
        (3, 8) => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: Some(7),
            event: None,
        }),
        (2, 7) => Some(Layout {
            details: Some(3),
            provenance: Some(4),
            status: 5,
            date: 6,
            edges: None,
            event: None,
        }),
        (1, 6) => Some(Layout {
            details: None,
            provenance: Some(3),
            status: 4,
            date: 5,
            edges: None,
            event: None,
        }),
        (0, 6) => Some(Layout {
            details: None,
            provenance: None,
            status: 3,
            date: 4,
            edges: Some(5),
            event: None,
        }),
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
pub(super) fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Split a markdown table row into trimmed, unescaped cells, honouring `\|` as a
/// literal pipe inside a cell.
///
pub(super) fn split_cells(row: &str) -> Vec<String> {
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
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        f.id,
        escape_cell(&f.subject.to_string()),
        escape_cell(&f.content),
        escape_cell(f.details.as_deref().unwrap_or_default()),
        f.provenance.as_token(),
        f.status.as_token(),
        f.date,
        escape_cell(&render_edge(f.edge.as_ref())),
        escape_cell(&f.event.as_ref().map(Event::render).unwrap_or_default()),
    )
}

/// Parse a single table row into a [`Fact`], or `None` if it's the header, the
/// separator, or not a well-formed fact row. `home` is the entity whose doc the
/// row was read from — the other half of the fact's global address.
///
/// Every row layout that exists on disk is accepted — see [`Layout`]. Anything
/// else is not a fact row.
pub(super) fn parse_fact_row(row: &str, home: &EntityId) -> Option<Fact> {
    parse_fact_row_in(row, home, None)
}

/// [`parse_fact_row`] with the doc's declared era in hand: declared beats
/// inferred, inference stays as the net for rows from another era.
fn parse_fact_row_in(row: &str, home: &EntityId, declared: Option<u32>) -> Option<Fact> {
    let cells = split_cells(row);
    let at = declared
        .and_then(|v| era_layout(v, cells.len()))
        .or_else(|| layout_of(&cells))?;
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
    let provenance = at
        .provenance
        .map_or(Provenance::default(), |i| Provenance::from_token(cell(i)));
    let status = FactStatus::from_token(cell(at.status));
    let date: Date = cells[at.date].trim().parse().ok()?;
    let edge = at.edges.and_then(|i| parse_edge(cell(i)));
    // **Read exactly as tolerantly as the edge cell is**, and for a stronger
    // reason: a payload this build cannot make sense of must still come back
    // whole. `Event::parse` is total on anything carrying a type, and `None`
    // means this row is an ordinary fact — which is what every row written
    // before schema 4 is, and what most rows will always be.
    let event = at.event.and_then(|i| Event::parse(cell(i)));

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
        event,
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
    let declared = parse_schema(doc);
    let lines: Vec<&str> = doc.lines().collect();
    let Some((start, end)) = facts_region(&lines) else {
        return Vec::new();
    };
    lines[start..end]
        .iter()
        .filter_map(|l| parse_fact_row_in(l, &home, declared))
        .collect()
}

/// One table-region line under migration: a parseable fact row is re-rendered
/// in the current full-width layout; the old header and separator are dropped
/// (the caller emits the canonical pair); anything else is kept verbatim — a
/// migration must never cost content, and an unreadable row padded by the store
/// is still visible where a dropped one is gone.
fn migrated_row(line: &str, home: &EntityId) -> Option<String> {
    if let Some(f) = parse_fact_row(line, home) {
        return Some(render_fact_row(&f));
    }
    let cells = split_cells(line);
    let first = cells.first().map(|c| c.trim()).unwrap_or("");
    let is_header = first.eq_ignore_ascii_case("id");
    let is_separator = !first.is_empty() && first.chars().all(|c| c == '-');
    (!is_header && !is_separator).then(|| line.to_string())
}

/// The table region rewritten in the current layout: canonical header and
/// separator, every parseable row at full width, unreadable lines verbatim.
///
/// This is why the writers rewrite the WHOLE table and not just their row: the
/// store re-serializes every table **at its header's width** on save. An 8-cell
/// row appended under a 7-column header comes back without its last cell — the
/// production edge-loss bug. Lazy migration therefore covers the header, not
/// just the touched row: any write heals the doc it lands in.
fn migrated_region(lines: &[&str], start: usize, end: usize, home: &EntityId) -> Vec<String> {
    let mut out = vec![TABLE_HEADER.to_string(), TABLE_SEP.to_string()];
    out.extend(
        lines[start..end]
            .iter()
            .filter_map(|l| migrated_row(l, home)),
    );
    out
}

/// The doc's own declared identity, for re-parsing rows during migration; the
/// home never renders into a row, so a doc without a marker migrates the same.
fn migration_home(doc: &str) -> EntityId {
    EntityId(parse_id_marker(doc).unwrap_or_default())
}

/// Return `doc` with its machine block stamped `schema: SCHEMA_CURRENT` — the
/// declared era every write leaves behind, so the next reader dispatches by
/// testimony instead of sniffing. A doc with no machine block is not jojobot's
/// to stamp and comes back unchanged.
pub(super) fn with_schema_stamped(doc: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();
    let Some((open, close)) = machine_block(&lines) else {
        return doc.to_string();
    };
    let stamp = format!("schema: {SCHEMA_CURRENT}");
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    match (open + 1..close - 1).find(|&i| field_of(lines[i], "schema").is_some()) {
        Some(i) => out[i] = stamp,
        None => {
            let at = (open + 1..close - 1)
                .find(|&i| field_of(lines[i], "id").is_some())
                .map(|i| i + 1)
                .unwrap_or(open + 1);
            out.insert(at, stamp);
        }
    }
    out.join("\n")
}

/// Return `doc` with the row carrying `id` replaced by `row`, or `None` if no
/// such row exists. `None` is the signal that the address missed — the store
/// turns it into an error rather than appending a row nobody asked for. The
/// whole table migrates to the current layout on the way ([`migrated_region`]).
///
/// Only a row the reader can parse is a target — the writer's predicate must
/// never be wider than [`parse_fact_row`]'s, or an edit can land on a row no
/// read ever returned, silently destroying a fact the caller never saw while
/// passing read-back, because the verification matches that same wrong row.
pub(super) fn with_row_replaced(
    doc: &str,
    home: &EntityId,
    id: &FactId,
    row: &str,
) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (start, end) = facts_region(&lines)?;
    lines[start..end]
        .iter()
        .position(|l| parse_fact_row(l, home).is_some_and(|f| &f.id == id))?;

    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    out.push(TABLE_HEADER.to_string());
    out.push(TABLE_SEP.to_string());
    for line in &lines[start..end] {
        if parse_fact_row(line, home).is_some_and(|f| &f.id == id) {
            out.push(row.to_string());
        } else if let Some(kept) = migrated_row(line, home) {
            out.push(kept);
        }
    }
    out.extend(lines[end..].iter().map(|s| s.to_string()));
    Some(with_schema_stamped(&out.join("\n")))
}

/// The local id a table row carries, if it looks like a fact row at all — the
/// *widest* reading, deliberately: this is what id minting counts, so a row the
/// reader can't parse still holds its id and can never be handed out twice.
fn row_id(row: &str) -> Option<String> {
    let cells = split_cells(row);
    // Width alone, on purpose: a row the READER gives up on must still hold its
    // id, so this accepts widths `layout_of` may refuse to interpret.
    //
    // **The upper bound tracks the header rather than being a literal**, and it
    // is the header for a reason that cost a debugging session: adding the
    // event column widened `layout_of` and left this at `6..=8`, so every
    // freshly-written row was invisible to the id minter. `next_fact_id`
    // restarted at f1, handed out ids the page was already using, and the store
    // then refused reads with "its doc holds more than one row with that id" —
    // an address collision, from a range nobody remembered to widen.
    if !(6..=split_cells(TABLE_HEADER).len()).contains(&cells.len()) {
        return None;
    }
    let id = cells[0].trim();
    (!id.is_empty() && !id.eq_ignore_ascii_case("id") && !id.chars().all(|c| c == '-'))
        .then(|| id.to_string())
}

/// Return `doc` with `row` appended to the fact table. Creates the section (and
/// its header/separator) if the doc doesn't have one yet. The whole table
/// migrates to the current layout on the way ([`migrated_region`]).
pub(super) fn with_fact_appended(doc: &str, row: &str) -> String {
    let home = migration_home(doc);
    let lines: Vec<&str> = doc.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 6);

    match facts_region(&lines) {
        Some((start, end)) => {
            out.extend(lines[..start].iter().map(|s| s.to_string()));
            out.extend(migrated_region(&lines, start, end, &home));
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
    with_schema_stamped(&out.join("\n"))
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
        //
        // A `machinery:` line identifies the block just as well, and is the only
        // other thing that does. A page jojobot keeps for its own bookkeeping —
        // a bot's sessions page — is not an entity and carries no handle, so
        // without this its block would not be jojobot's by any test here and
        // every field on it would be read out of the prose fallback, where the
        // prose could forge one.
        if lines[i + 1..close].iter().any(|l| {
            field_of(l, "id").is_some_and(|v| validate_subject(&EntityId(v)).is_ok())
                || field_of(l, MACHINERY_FIELD).is_some()
        }) {
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
/// The boundary is the table, not the header: [`facts_region`] must find the
/// table wherever it sits under the header, because humans type notes in
/// that gap — treating prose as stopping at the header line would mean a
/// write preserves that gap forever but no search can surface it. Text below
/// the table is prose for the same reason.
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

/// The doc's declared schema era, if a write ever stamped one. `None` is a doc
/// from before the field — read by inference, upgraded on its next touch.
fn parse_schema(doc: &str) -> Option<u32> {
    parse_field(doc, "schema")?.trim().parse().ok()
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
    let parent = parse_parent(doc, &id);
    Some(Entity {
        id,
        kind,
        name: parse_field(doc, "name").unwrap_or_default(),
        aliases: parse_aliases(doc),
        source: parse_field(doc, "source").unwrap_or_default(),
        crm: parse_field(doc, "crm"),
        parent,
        boot: parse_field(doc, "boot").map_or(Boot::default(), |b| Boot::from_token(&b)),
    })
}

/// Which kind of **jojobot machinery** this page is, if it is any — a bot's
/// sessions page says `sessions`.
///
/// Read only from **inside** the machine block, never from the prose fallback
/// [`parse_field`] allows. The fallback exists so a hand-written page with a
/// bare `id:` line above its fact table still identifies its entity, which is a
/// generosity worth having; here it would be a hole, because a page that claims
/// to be machinery is a page search never returns, and prose that could claim it
/// could hide itself.
pub(super) fn parse_machinery(doc: &str) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let (open, close) = machine_block(&lines)?;
    lines[open + 1..close - 1]
        .iter()
        .find_map(|l| field_of(l, MACHINERY_FIELD))
}

/// The entity this doc's entity sits under, off its `parent:` line. `own` is
/// the doc's own handle, which the line may not name.
///
/// **Read tolerantly, like an edges cell:** a value that isn't a well-formed
/// handle costs the parentage, never the entity. A doc whose `parent:` line
/// somebody hand-mangled still identifies whose page it is and still holds its
/// facts; reading it as a root is the cheap, safe side, and the line is
/// rewritten in the current spelling on the doc's next touch.
///
/// **Nothing is below itself, and the reader enforces that too.** The write
/// path already refuses it, but these are wiki pages a human edits, and the
/// reader is the other door into the store — a `parent:` line naming the page's
/// own handle is one keystroke from a legitimate one. Read as written it would
/// make the entity its own child, and a level that descends into itself is not
/// something a caller should have to defend against.
///
/// **This line is the truth, not the page's position in Outline.** The store
/// also nests the page under its parent's, because that is what makes the wiki
/// navigable to a human — but the two can drift the moment somebody drags a
/// page in the browser, and jojobot's own schema is what jojobot reads. A moved
/// page is a page in a different spot; a rewritten `parent:` line is a
/// different parent.
fn parse_parent(doc: &str, own: &EntityId) -> Option<EntityId> {
    let parent = EntityId(parse_field(doc, "parent")?);
    (&parent != own && validate_subject(&parent).is_ok()).then_some(parent)
}

/// The other names an entity answers to, off its `aliases:` line — one
/// comma-separated list, blanks dropped.
///
/// **A doc written before the field existed has no line, and reads as none** —
/// the same lazy migration `details` and `edges` got: nothing is swept, and the
/// line appears the next time a write rewrites the block.
fn parse_aliases(doc: &str) -> Vec<String> {
    parse_field(doc, "aliases")
        .into_iter()
        .flat_map(|line| {
            line.split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The frontmatter block for an entity — lean and identical for all nine kinds.
/// An absent `crm` (or an empty alias set) writes no line at all, so the block
/// says only what is true.
fn frontmatter(e: &Entity) -> String {
    let mut out = format!(
        "```yaml\nid: {}\nschema: {SCHEMA_CURRENT}\nkind: {}\nname: {}\n",
        e.id, e.kind, e.name,
    );
    if !e.aliases.is_empty() {
        out.push_str(&format!("aliases: {}\n", e.aliases.join(", ")));
    }
    out.push_str(&format!("source: {}\n", e.source));
    if let Some(crm) = &e.crm {
        out.push_str(&format!("crm: {crm}\n"));
    }
    if let Some(parent) = &e.parent {
        out.push_str(&format!("parent: {parent}\n"));
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

/// Return `doc` with its prose replaced by `prose`, or `None` if `prose`
/// carries a line this doc schema reserves.
///
/// **Whole, and in one place.** [`parse_prose`] reads prose as every line that
/// is neither the machine block nor the fact table, wherever it sits — so a
/// write that added text in one spot while old paragraphs sat in another would
/// read back as the concatenation of both, and could never be verified. Every
/// other prose line is therefore dropped and the new text lands in a single
/// canonical spot: **below the machine block, above the fact table.**
///
/// Below rather than above, because prose is free text that may well contain a
/// fenced example: [`machine_block`] takes the first fence carrying a
/// well-formed `id:`, so prose above the real block is a standing offer to
/// hijack the doc's identity.
///
/// Two refusals, both returning `None`.
///
/// **Prose the domain will not have written anywhere** — empty, or carrying a
/// line the document schema reserves. Checked through [`validate_prose`] rather
/// than restated here, so this pure function and the port's contract cannot
/// come to disagree about what prose is.
///
/// **A doc with no machine block.** The reader accepts a bare `id:` line above
/// the fact table, so a hand-written page can be an entity without one — and a
/// bare marker line is prose by every structural rule here, so replacing prose
/// whole would delete the line that says whose page this is and orphan every
/// fact on it. Read-back would catch that and restore the page, but a write
/// that has to be undone is a write that should never have been attempted. An
/// ordinary metadata edit gives such a page a block; then it is writable.
pub(super) fn with_prose_replaced(doc: &str, prose: &str) -> Option<String> {
    validate_prose(prose).ok()?;
    let lines: Vec<&str> = doc.lines().collect();
    let (open, close) = machine_block(&lines)?;
    let header = lines.iter().position(|l| l.trim() == FACTS_HEADER);
    let table = facts_region(&lines).filter(|(start, end)| start < end);
    let within = |span: Option<(usize, usize)>, i: usize| {
        span.is_some_and(|(start, end)| i >= start && i < end)
    };

    // Everything structural, in order, with the prose spliced in directly under
    // the machine block; every other line was prose and is gone.
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 4);
    for (i, line) in lines.iter().enumerate() {
        if !(within(Some((open, close)), i) || within(table, i) || Some(i) == header) {
            continue;
        }
        out.push(line.to_string());
        if i + 1 == close {
            out.push(String::new());
            out.push(prose.to_string());
            out.push(String::new());
        }
    }
    Some(out.join("\n"))
}

/// The markdown a freshly-created entity doc is seeded with: a note for the
/// human, the entity's frontmatter (durable identity + metadata), and an empty
/// fact table for jojobot to append to.
///
/// **The note is written for the one reader who is standing here.** Whatever it
/// says is prose, and prose is indexed and searched — so it is read by an agent
/// too, one that asked about something else and got a sentence about this page's
/// insides. The note it replaced described the layout in a complete sentence
/// ("facts about this entity are in the table at the bottom") and two separate
/// sessions quoted it back as if it were content. So: the warning stays, because
/// a human opening this needs it; the description of where things sit goes,
/// because the software decides that and nobody else is owed it.
pub(super) fn seeded_doc(entity: &Entity) -> String {
    format!(
        "_Managed by jojobot — it rewrites this automatically. Anything you type here may be \
         overwritten._\n\n\
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
            event: None,
        }
    }

    fn alpha() -> Entity {
        Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::Person,
            name: "Alpha".into(),
            aliases: Vec::new(),
            source: "crm-card".into(),
            crm: Some("card:554".into()),
            parent: None,
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
            with_row_replaced(
                &doc,
                &EntityId::person("alpha"),
                &FactId("f1".into()),
                "| f1 | x |"
            )
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
        let good = fact(
            "f2",
            "person:alpha",
            "takes the 8am train",
            Provenance::Testimony,
            date(2026, 7, 2),
        );
        doc = with_fact_appended(&doc, &render_fact_row(&good));

        let edited = Fact {
            content: "takes the 7am train".into(),
            ..good
        };
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
        assert_eq!(
            parse_facts_table(&doc).len(),
            1,
            "the table is still readable"
        );

        let f2 = fact(
            "f2",
            "person:alpha",
            "learning Rust",
            Provenance::Inference,
            date(2026, 7, 2),
        );
        let updated = with_fact_appended(&doc, &render_fact_row(&f2));
        assert_eq!(
            updated.matches(TABLE_HEADER).count(),
            1,
            "one table, not a second one above the note: {updated}"
        );
        assert_eq!(parse_facts_table(&updated).len(), 2, "both facts readable");
        assert!(
            updated.contains("note: do not edit below"),
            "the note survives"
        );
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
        let renamed = Entity {
            name: "Alpha Renamed".into(),
            ..alpha()
        };
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
    /// hand that doc's identity to it. Adopting any fence with an `id:` line
    /// as the machine block would do exactly that: the doc stops resolving
    /// to its entity, so its facts become unreachable.
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
        assert_eq!(
            parse_entity(&doc).map(|e| e.id),
            Some(EntityId::person("alpha"))
        );
        assert_eq!(
            parse_facts_table(&doc).len(),
            1,
            "the doc's facts stay reachable"
        );
    }

    /// The same predicate protects the write path: an entity edit rewrites
    /// jojobot's block, never the pasted snippet.
    #[test]
    fn a_decoy_fence_is_not_rewritten_by_an_entity_edit() {
        let doc = format!(
            "```yaml\nid: my-service\nversion: 2\n```\n\n{}\n\n{FACTS_HEADER}\n",
            frontmatter(&alpha())
        );
        let renamed = Entity {
            name: "Alpha Renamed".into(),
            ..alpha()
        };
        let updated = with_frontmatter_replaced(&doc, &renamed);

        assert!(
            updated.contains("id: my-service"),
            "the decoy survives: {updated}"
        );
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
        assert_eq!(
            e.id,
            EntityId::person("alpha"),
            "prose must not forge the marker"
        );
        assert_eq!(e.name, "Alpha", "prose must not forge a field");
    }

    // --- the details column ---------------------------------------------------

    /// A fact's details ride in their own escaped cell and round-trip with it.
    #[test]
    fn details_round_trip_in_their_own_cell() {
        let f = Fact {
            details: Some("changed jobs in July; a|b in the margin".into()),
            ..fact(
                "f1",
                "person:alpha",
                "works somewhere new",
                Provenance::Testimony,
                date(2026, 7, 24),
            )
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
        let parsed =
            parse_fact_row(legacy, &EntityId::person("alpha")).expect("legacy row must parse");
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
        assert_eq!(
            slice1.content, "plays go ❓",
            "the ❓ is content now; nothing invents a column"
        );
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
        let no_details = parse_fact_row(
            "| f1 | person:alpha | plays go | testimony | active | 2026-07-01 |",
            &home,
        )
        .expect("the pre-details row must still parse");
        assert_eq!(no_details.provenance, Provenance::Testimony);
        assert_eq!(no_details.status, FactStatus::Active);
        assert_eq!(no_details.date, date(2026, 7, 1));
    }

    /// **A six-cell row whose date is ambiguous is no row at all.**
    ///
    /// Which cell holds the date is the only thing separating the slice-1 layout
    /// from the pre-`details` one. When *both* candidates parse as dates that
    /// evidence says nothing — and first-match-wins would answer anyway, picking
    /// the pre-`details` reading and handing back a row with a plausible, wrong
    /// date. A silently wrong date is worse than a missing row: nobody goes
    /// looking for it. So both-parse is refused exactly as neither-parse is, and
    /// the id stays reserved either way (`row_id` is deliberately wider than the
    /// reader), which is what keeps the row inert rather than destroyed.
    #[test]
    fn a_six_cell_row_whose_date_is_ambiguous_is_refused_like_one_with_no_date() {
        let home = EntityId::person("alpha");

        // Both cell 4 and cell 5 parse: slice 1 would read 2026-01-01 and call
        // the rest an edges cell; the pre-details shape would read 2026-02-02.
        let two_dates = "| f1 | person:alpha | moved house | active | 2026-01-01 | 2026-02-02 |";
        assert_eq!(
            parse_fact_row(two_dates, &home),
            None,
            "two candidate dates is no evidence, and a guess here is a wrong date nobody checks"
        );

        // Neither parses: unreadable under both layouts, the same verdict.
        let no_date = "| f1 | person:alpha | moved house | active | someday | later |";
        assert_eq!(parse_fact_row(no_date, &home), None);

        // Both are still rows for the purpose of minting: an id on the page is
        // taken, readable or not, or the next capture hands it out twice.
        for row in [two_dates, no_date] {
            let doc = with_fact_appended(&seeded_doc(&alpha()), row);
            assert!(
                parse_facts_table(&doc).is_empty(),
                "no reader sees it: {row}"
            );
            assert_eq!(
                next_fact_id(&doc),
                FactId("f2".into()),
                "…and its id is spent: {row}"
            );
        }
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
            ..fact(
                "f3",
                "person:alpha",
                "learning Rust",
                Provenance::Testimony,
                date(2026, 7, 3),
            )
        };
        let appended = with_fact_appended(doc, &render_fact_row(&fresh));
        let parsed = parse_facts_table(&appended);
        assert_eq!(parsed.len(), 3, "old rows and new one together: {parsed:?}");
        assert_eq!(parsed[2], fresh);
        assert!(
            appended.contains("Some prose."),
            "prose above the table is untouched"
        );

        // …and touching a slice-1 row rewrites it in the current eight-cell form.
        let touched = with_row_replaced(
            &appended,
            &EntityId::person("alpha"),
            &FactId("f1".into()),
            &render_fact_row(&facts[0]),
        )
        .expect("a slice-1 row is addressable");
        assert!(
            touched.contains(
                "| f1 | person:alpha | plays go ❓ |  | inference | active | 2026-07-01 |  |"
            ),
            "the touched row carries every current column: {touched}"
        );
    }

    /// **Alternate names round-trip, and a doc from before the field reads
    /// fine.** The set rides one comma-separated line, so the entity's other
    /// names are as legible in the wiki as its display name is.
    ///
    /// The lazy migration is the same one `details` and `edges` got: a doc with
    /// no `aliases:` line reads as having none — never a hard-failed read, never
    /// a sweep — and gains the line the next time a write rewrites its block. An
    /// entity with no aliases writes no line at all, so the block still says only
    /// what is true.
    #[test]
    fn alternate_names_round_trip_and_a_doc_without_them_still_reads() {
        let named = Entity {
            aliases: vec!["Al".into(), "A. One".into()],
            ..alpha()
        };
        let doc = seeded_doc(&named);
        assert!(
            doc.contains("aliases: Al, A. One"),
            "one legible line: {doc}"
        );
        assert_eq!(parse_entity(&doc).expect("the doc is an entity"), named);

        // A doc written before the field existed: no line, no aliases, no drama.
        let legacy = "```yaml\nid: person:alpha\nkind: person\nname: Alpha\n\
                      source: crm-card\ncrm: card:554\nboot: on-demand\n```\n";
        let read = parse_entity(legacy).expect("a legacy doc still identifies its entity");
        assert!(
            read.aliases.is_empty(),
            "an absent field is none, not a failure"
        );
        assert_eq!(
            read.name, "Alpha",
            "…and everything else reads as it always did"
        );

        // …and it gains the line on the next write that touches the block.
        let touched = with_frontmatter_replaced(
            legacy,
            &Entity {
                aliases: vec!["Al".into()],
                ..read
            },
        );
        assert!(
            touched.contains("aliases: Al"),
            "gained on touch: {touched}"
        );
        assert_eq!(parse_aliases(&touched), vec!["Al".to_string()]);

        // An entity with none writes no line — the block says only what is true.
        assert!(
            !seeded_doc(&alpha()).contains("aliases"),
            "no aliases, no line: {}",
            seeded_doc(&alpha())
        );

        // Blanks in the list are dropped rather than becoming empty names.
        assert_eq!(
            parse_aliases("```yaml\nid: person:alpha\naliases: Al, , Alph ,\n```"),
            vec!["Al".to_string(), "Alph".to_string()]
        );
    }

    /// **Prose is replaced whole, and it lands in one canonical place** —
    /// between the machine block and the fact table. Whole, because
    /// [`parse_prose`] reads every line that is neither block nor table: prose
    /// written in one spot while an old paragraph sat in another would read
    /// back as both, and no write could ever be verified.
    ///
    /// Below the block, not above it, on purpose. A charter is free text and may
    /// well contain a fenced example; [`machine_block`] takes the FIRST fence
    /// carrying a well-formed `id:`, so prose placed above the real block is a
    /// standing offer to hijack the doc's identity.
    #[test]
    fn prose_is_replaced_whole_and_sits_between_the_block_and_the_table() {
        let doc = seeded_doc(&alpha());
        assert!(
            !parse_prose(&doc).is_empty(),
            "the seeded doc opens with a note, so this really is a replacement"
        );

        let charter = "Keeps the schedule.\n\nHard line: never writes to the ledger.";
        let written = with_prose_replaced(&doc, charter).expect("plain prose is writable");
        assert_eq!(parse_prose(&written), charter, "read back whole: {written}");

        let block = written.find("```yaml").expect("the machine block survives");
        let table = written.find(FACTS_HEADER).expect("the table survives");
        let at = written
            .find("Keeps the schedule")
            .expect("the prose is there");
        assert!(
            block < at && at < table,
            "prose sits between the two: {written}"
        );
        assert_eq!(
            parse_entity(&written).expect("the doc is still an entity"),
            alpha(),
            "identity and metadata are untouched"
        );

        // A second write replaces rather than accumulates.
        let rewritten = with_prose_replaced(&written, "Nothing else.").expect("writable");
        assert_eq!(parse_prose(&rewritten), "Nothing else.");
        assert!(
            !rewritten.contains("ledger"),
            "the old prose is gone: {rewritten}"
        );

        // …and a fenced example in the charter does NOT become the doc's
        // identity: the real block is still the first one.
        let fenced = with_prose_replaced(&doc, "For example:\n\n```yaml\nid: person:ghost\n```")
            .expect("a fenced example is ordinary prose");
        assert_eq!(
            parse_id_marker(&fenced).as_deref(),
            Some("person:alpha"),
            "the doc keeps its own identity: {fenced}"
        );
    }

    /// **Prose that would forge the table's header is refused.** The reader
    /// finds the fact table by the FIRST header line, so a charter carrying one
    /// moves the boundary: every real fact below it stops being a fact and the
    /// prose above it stops being prose. Refused outright rather than escaped —
    /// silently mangling somebody's charter is worse than declining it.
    ///
    /// The rule itself lives in the domain, where it binds every adapter; what
    /// this pins is that **this rewriter refuses rather than mangles**, which a
    /// caller reaching the pure function directly still depends on.
    #[test]
    fn prose_carrying_the_tables_own_header_is_refused() {
        let doc = seeded_doc(&alpha());
        for forged in [
            format!("a charter\n\n{FACTS_HEADER}\n\n| id | subject |"),
            FACTS_HEADER.to_string(),
            format!("   {FACTS_HEADER}   "),
        ] {
            assert!(
                with_prose_replaced(&doc, &forged).is_none(),
                "must refuse prose carrying the table header: {forged:?}"
            );
        }
        // The words on their own, not on a line of their own, are just words.
        assert!(
            with_prose_replaced(&doc, "the facts table is at the bottom").is_some(),
            "an ordinary sentence mentioning facts is prose"
        );
    }

    /// **A page whose marker is not inside a fenced block is not rewritten.**
    ///
    /// The reader accepts a bare `id:` line above the fact table, so a
    /// hand-written page really can be an entity without a machine block — and
    /// a bare marker line is *prose* by every structural rule here. Replacing
    /// prose whole would therefore delete the one line that says whose page
    /// this is, orphaning every fact on it. Read-back would catch it and the
    /// page would be restored, but a write that has to be undone is a write
    /// that should never have been attempted.
    #[test]
    fn a_page_with_no_machine_block_is_not_rewritten_out_of_its_identity() {
        let hand_written = "id: bot:gamma\nkind: bot\nname: Gamma\n\nSome notes.\n";
        assert_eq!(
            parse_id_marker(hand_written).as_deref(),
            Some("bot:gamma"),
            "the reader does accept this page, which is exactly the hazard"
        );
        assert!(
            with_prose_replaced(hand_written, "a charter").is_none(),
            "refused up front rather than written and rolled back"
        );

        // …and once an ordinary metadata write has given it a block, it takes
        // a charter like any other page.
        let repaired = with_frontmatter_replaced(
            hand_written,
            &parse_entity(hand_written).expect("it is an entity"),
        );
        let written = with_prose_replaced(&repaired, "a charter").expect("now writable");
        assert_eq!(parse_prose(&written), "a charter");
        assert_eq!(parse_id_marker(&written).as_deref(), Some("bot:gamma"));
    }

    /// **A page cannot hand-edit itself into being its own parent.** The write
    /// path refuses it, and the reader is the other door into the store: these
    /// are wiki pages a human edits, and a `parent:` line pointing at the
    /// page's own handle is one keystroke away from a legitimate one. Read as
    /// written it would make the entity its own child, so `children` would hand
    /// a caller a level that descends into itself.
    #[test]
    fn a_page_naming_itself_as_its_parent_reads_as_a_root() {
        let doc = "```yaml\nid: place:leftorium\nkind: place\nname: Leftorium\n\
                   source: user-named\nparent: place:leftorium\nboot: on-demand\n```\n";
        let read = parse_entity(doc).expect("the doc is still an entity");
        assert_eq!(
            read.parent, None,
            "nothing is below itself, whoever typed the line"
        );
        assert_eq!(read.name, "Leftorium", "…with everything else intact");
    }

    /// **A mangled `parent:` costs the parentage, never the entity** — the same
    /// tolerance a garbled edges cell gets. These are hand-editable wiki pages,
    /// and a page whose one bad line made it stop being an entity would take
    /// every fact on it out of reach. Reading it as a root is the cheap, safe
    /// side, and the next write that touches the block spells it correctly.
    #[test]
    fn a_parent_line_that_is_not_a_handle_reads_as_a_root() {
        for bad in [
            "Some Project", // a human typed a display name
            "person:",      // a kind with no slug
            "notakind:atlas",
            "person:Alpha", // uppercase is not the handle grammar
            "person:a b",
        ] {
            let doc = format!(
                "```yaml\nid: place:leftorium\nkind: place\nname: Leftorium\n\
                 source: user-named\nparent: {bad}\nboot: on-demand\n```\n"
            );
            let read = parse_entity(&doc).expect("the doc is still an entity");
            assert_eq!(
                read.parent, None,
                "{bad:?} is not a handle, so it is no parentage — but the entity survives"
            );
            assert_eq!(read.name, "Leftorium", "…with everything else intact");
        }
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
            &render_fact_row(&fact(
                "f1",
                "person:alpha",
                "plays go",
                Provenance::Testimony,
                date(2026, 7, 1),
            )),
        );
        let prose = parse_prose(&doc);
        assert_eq!(prose, "Alpha keeps a paper notebook and hates phone calls.");
        assert!(
            !prose.contains("id: person:alpha"),
            "the machine block is not prose"
        );
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
        assert!(
            !prose.contains(FACTS_HEADER),
            "jojobot's own header is not prose: {prose}"
        );
        assert!(
            !prose.contains("plays go"),
            "a fact row is not prose: {prose}"
        );
        assert!(
            !prose.contains("id: person:alpha"),
            "the machine block is not prose: {prose}"
        );
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
        assert!(
            prose.contains("important snippet the user wrote"),
            "got: {prose}"
        );
        assert!(
            !prose.contains("source: crm-card"),
            "jojobot's block is stripped: {prose}"
        );
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
                event: None,
                ..fact(
                    "f1",
                    "person:alpha",
                    "a claim",
                    Provenance::Inference,
                    date(2026, 7, 1),
                )
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
            ..fact(
                "f2",
                "person:alpha",
                "rides with the club",
                Provenance::Testimony,
                date(2026, 7, 24),
            )
        };
        assert_eq!(
            render_fact_row(&f),
            "| f2 | person:alpha | rides with the club |  | testimony | active | 2026-07-24 | \
             membership=org:north-trail-club |  |"
        );
    }

    /// A row written before the edges column existed still parses — it just draws
    /// no edge. Old docs are read fine and get the column on their next touch;
    /// a schema addition never orphans the rows already on disk.
    #[test]
    fn a_row_without_the_edges_column_still_parses() {
        let previous =
            "| f1 | person:alpha | plays go | twice a week | testimony | active | 2026-07-01 |";
        let parsed = parse_fact_row(previous, &EntityId::person("alpha"))
            .expect("the 7-cell row must parse");
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
        for cell in [
            "knows=person:beta",
            "location",
            "=place:x",
            "location=nope:x",
            "location=",
        ] {
            let row = format!(
                "| f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 | {cell} |"
            );
            let parsed = parse_fact_row(&row, &EntityId::person("alpha"))
                .unwrap_or_else(|| panic!("the fact must still read with cell {cell:?}"));
            assert_eq!(parsed.content, "plays go");
            assert_eq!(
                parsed.edge, None,
                "an unreadable edge is dropped, not guessed: {cell:?}"
            );
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
            edge: Some(Edge::new(
                EdgeShape::Location,
                EntityId("place:shelbyville".into()),
            )),
            ..fact(
                "f2",
                "person:alpha",
                "spending the winter away",
                Provenance::Testimony,
                date(2026, 7, 2),
            )
        };
        let parsed = parse_facts_table(&with_fact_appended(&doc, &render_fact_row(&f2)));
        assert_eq!(
            parsed.len(),
            2,
            "the pre-edges row and the new one both read"
        );
        assert_eq!(parsed[0].edge, None);
        assert_eq!(parsed[1], f2);
    }

    /// A write migrates the WHOLE table to the current layout — header,
    /// separator, every parseable row re-rendered at full width. The store this
    /// doc lives in re-serializes tables at the header's width, so an 8-cell row
    /// under a 7-column header loses its last cell on save: the production
    /// edge-loss bug. Rows the reader cannot parse are kept verbatim — a
    /// migration must never cost content.
    #[test]
    fn an_append_migrates_a_legacy_table_to_the_current_layout() {
        let doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n\
             | id | subject | content | details | provenance | status | date |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n\
             | f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |\n\
             | f2 | person:alpha | joined the guild | active | 2026-07-02 | membership=org:guild |\n\
             | not-a-fact-row |\n"
        );
        let f3 = Fact {
            edge: Some(Edge::new(
                EdgeShape::Location,
                EntityId("place:shelbyville".into()),
            )),
            ..fact(
                "f3",
                "person:alpha",
                "spending the winter away",
                Provenance::Testimony,
                date(2026, 7, 3),
            )
        };
        let out = with_fact_appended(&doc, &render_fact_row(&f3));

        assert!(
            out.contains(TABLE_HEADER),
            "the header is rewritten to the current layout"
        );
        assert!(out.contains(TABLE_SEP), "so is the separator");
        assert!(
            !out.contains("| id | subject | content | details | provenance | status | date |\n"),
            "the narrow header is gone"
        );
        assert!(
            out.contains("| not-a-fact-row |"),
            "an unreadable line is kept verbatim, never dropped"
        );
        for line in out.lines().filter(|l| l.trim_start().starts_with('|')) {
            if line.contains("not-a-fact-row") {
                continue;
            }
            assert_eq!(
                split_cells(line).len(),
                split_cells(TABLE_HEADER).len(),
                "every rewritten table line is full-width so the store's \
                 rectangularizer has nothing to truncate: {line:?}"
            );
        }

        let parsed = parse_facts_table(&out);
        assert_eq!(
            parsed.len(),
            3,
            "both legacy rows and the appended one read"
        );
        assert_eq!(parsed[0].edge, None);
        assert_eq!(
            parsed[1].edge,
            Some(Edge::new(
                EdgeShape::Membership,
                EntityId("org:guild".into())
            )),
            "a slice-1 row's edge survives the migration into the full-width shape"
        );
        assert_eq!(parsed[2], f3);
    }

    /// **Declared beats inferred.** A six-cell row where BOTH candidate cells
    /// parse as dates is unreadable to inference (the ambiguity is genuine) —
    /// but a doc stamped `schema: 0` has already testified which era wrote it,
    /// and the row reads under that era's layout. The stamp turns a dropped row
    /// into a kept one; it can never do the reverse, because inference stays as
    /// the net when widths don't match the stamp.
    #[test]
    fn a_stamped_doc_reads_by_its_declared_era_not_by_sniffing() {
        let ambiguous = "| f1 | person:alpha | joined up | active | 2026-07-01 | 2026-07-02 |";
        let unstamped = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n\
             | id | subject | content | status | date | edges |\n\
             | --- | --- | --- | --- | --- | --- |\n\
             {ambiguous}\n"
        );
        assert_eq!(
            parse_facts_table(&unstamped).len(),
            0,
            "without a stamp the ambiguous row is no evidence, and stays unread"
        );

        let stamped = unstamped.replace("id: person:alpha\n", "id: person:alpha\nschema: 0\n");
        let parsed = parse_facts_table(&stamped);
        assert_eq!(
            parsed.len(),
            1,
            "the declared era resolves what sniffing cannot"
        );
        assert_eq!(
            parsed[0].date,
            date(2026, 7, 1),
            "slice-1's date column, by testimony"
        );
        assert_eq!(parsed[0].status, FactStatus::Active);
    }

    /// Every write leaves the current era stamped on the doc, so the next
    /// reader dispatches by testimony — and a legacy doc gains the field on its
    /// first touch, never by sweep.
    #[test]
    fn a_write_stamps_the_current_schema() {
        let legacy = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n\
             | id | subject | content | details | provenance | status | date |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n"
        );
        let out = with_fact_appended(
            &legacy,
            "| f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |  |",
        );
        assert!(
            out.contains(&format!("schema: {SCHEMA_CURRENT}")),
            "the append stamped the era: {out}"
        );
        let restamped = with_fact_appended(
            &out.replace(&format!("schema: {SCHEMA_CURRENT}"), "schema: 2"),
            "| f2 | person:alpha | still plays |  | testimony | active | 2026-07-02 |  |",
        );
        assert!(
            restamped.contains(&format!("schema: {SCHEMA_CURRENT}")),
            "a stale stamp is rewritten in place, not duplicated"
        );
        assert_eq!(restamped.matches("schema:").count(), 1);
    }

    /// The edit path migrates too — a doc healed on append would be re-broken by
    /// the next edit if `with_row_replaced` preserved a narrow header.
    #[test]
    fn a_row_edit_migrates_the_table_too() {
        let doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n\
             | id | subject | content | details | provenance | status | date |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n\
             | f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |\n"
        );
        let edited = Fact {
            edge: Some(Edge::new(
                EdgeShape::Location,
                EntityId("place:shelbyville".into()),
            )),
            ..fact(
                "f1",
                "person:alpha",
                "plays go",
                Provenance::Testimony,
                date(2026, 7, 1),
            )
        };
        let out = with_row_replaced(
            &doc,
            &EntityId::person("alpha"),
            &FactId("f1".into()),
            &render_fact_row(&edited),
        )
        .expect("f1 is a readable target");

        assert!(
            out.contains(TABLE_HEADER),
            "the header is rewritten to the current layout"
        );
        let parsed = parse_facts_table(&out);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], edited);
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
        assert_eq!(
            facts[0].content, "does NOT play the theremin",
            "content is untouched"
        );

        let touched = with_row_replaced(
            &legacy,
            &EntityId::person("alpha"),
            &FactId("f1".into()),
            &render_fact_row(&facts[0]),
        )
        .expect("the legacy row is addressable");
        assert!(
            !touched.contains("negated"),
            "the retired token is gone on touch: {touched}"
        );
        assert!(
            touched.contains("| superseded |"),
            "…rewritten as superseded: {touched}"
        );
    }

    /// Both lifecycle states survive the row, so a superseded fact reads back
    /// superseded rather than quietly returning as current truth.
    #[test]
    fn every_status_round_trips_through_the_row() {
        for status in [FactStatus::Active, FactStatus::Superseded] {
            let f = Fact {
                status,
                ..fact(
                    "f1",
                    "person:alpha",
                    "a claim",
                    Provenance::Inference,
                    date(2026, 7, 1),
                )
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
            &render_fact_row(&fact(
                "f1",
                "person:alpha",
                "plays go",
                Provenance::Testimony,
                date(2026, 7, 1),
            )),
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
            let row = render_fact_row(&fact(
                id,
                "person:alpha",
                content,
                Provenance::Inference,
                date(2026, 7, 1),
            ));
            doc = with_fact_appended(&doc, &row);
        }
        let edited = Fact {
            content: "second, corrected".into(),
            status: FactStatus::Superseded,
            ..fact(
                "f2",
                "person:alpha",
                "",
                Provenance::Inference,
                date(2026, 7, 2),
            )
        };
        let updated = with_row_replaced(
            &doc,
            &EntityId::person("alpha"),
            &FactId("f2".into()),
            &render_fact_row(&edited),
        )
        .expect("the row exists");

        let facts = parse_facts_table(&updated);
        assert_eq!(facts.len(), 3, "no row gained or lost");
        assert_eq!(facts[1].content, "second, corrected");
        assert_eq!(facts[1].status, FactStatus::Superseded);
        assert_eq!(facts[0].content, "first");
        assert_eq!(facts[2].content, "third");
        assert!(
            !updated.contains("| second |"),
            "the old row is gone, not left beside"
        );
    }

    /// Replacing a row that isn't there changes nothing and says so — the store
    /// turns that into an error rather than appending a surprise row.
    #[test]
    fn replacing_a_missing_row_reports_rather_than_appends() {
        let doc = seeded_doc(&alpha());
        assert!(
            with_row_replaced(
                &doc,
                &EntityId::person("alpha"),
                &FactId("f9".into()),
                "| f9 | x |"
            )
            .is_none()
        );
    }

    // --- entity frontmatter ---------------------------------------------------

    /// The frontmatter carries every entity field, and reads back as the same
    /// entity — the entity read path, mirroring the fact table's.
    #[test]
    fn entity_frontmatter_round_trips() {
        let doc = seeded_doc(&alpha());
        assert_eq!(
            parse_entity(&doc).expect("a seeded doc is an entity"),
            alpha()
        );
        assert_eq!(parse_id_marker(&doc).as_deref(), Some("person:alpha"));
    }

    /// An entity with no `crm` link writes no `crm` line — absent, not blank.
    #[test]
    fn an_absent_crm_link_is_not_written() {
        let doc = seeded_doc(&Entity {
            crm: None,
            ..alpha()
        });
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
            &render_fact_row(&fact(
                "f1",
                "person:alpha",
                "plays go",
                Provenance::Testimony,
                date(2026, 7, 1),
            )),
        );
        let renamed = Entity {
            name: "Alpha Renamed".into(),
            ..alpha()
        };
        let updated = with_frontmatter_replaced(&doc, &renamed);

        assert_eq!(parse_entity(&updated).unwrap(), renamed);
        assert!(
            updated.contains("Some prose about the entity."),
            "prose survives"
        );
        assert_eq!(
            parse_facts_table(&updated).len(),
            1,
            "the fact table survives"
        );
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
        let f = fact(
            "f1",
            "person:alpha",
            "keeps a paper notebook",
            Provenance::Inference,
            date(2026, 7, 24),
        );
        assert_eq!(
            render_fact_row(&f),
            "| f1 | person:alpha | keeps a paper notebook |  | inference | active | 2026-07-24 |  |  |"
        );
    }

    #[test]
    fn both_provenances_round_trip_via_their_own_column() {
        let home = EntityId::person("alpha");
        let testi = fact(
            "f1",
            "person:alpha",
            "speaks two languages",
            Provenance::Testimony,
            date(2026, 1, 1),
        );
        let infer = fact(
            "f2",
            "person:alpha",
            "prefers mornings ❓",
            Provenance::Inference,
            date(2026, 1, 2),
        );
        assert_eq!(
            parse_fact_row(&render_fact_row(&testi), &home).unwrap(),
            testi
        );
        assert_eq!(
            parse_fact_row(&render_fact_row(&infer), &home).unwrap(),
            infer
        );
    }

    #[test]
    fn subject_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact(
            "f1",
            "person:a|b",
            "x",
            Provenance::Testimony,
            date(2026, 7, 24),
        );
        let row = render_fact_row(&f);
        assert!(
            row.contains("person:a\\|b"),
            "subject pipe must be escaped: {row}"
        );
        assert_eq!(parse_fact_row(&row, &f.home).unwrap(), f);
    }

    #[test]
    fn content_with_a_pipe_is_escaped_and_round_trips() {
        let f = fact(
            "f1",
            "person:alpha",
            "reads a|b|c notation",
            Provenance::Testimony,
            date(2026, 7, 24),
        );
        let row = render_fact_row(&f);
        assert!(
            row.contains("a\\|b\\|c"),
            "pipes must be escaped in the row: {row}"
        );
        assert_eq!(parse_fact_row(&row, &f.home).unwrap(), f);
    }

    #[test]
    fn header_and_separator_are_not_facts() {
        let home = EntityId::person("alpha");
        assert!(parse_fact_row(TABLE_HEADER, &home).is_none());
        assert!(parse_fact_row(TABLE_SEP, &home).is_none());
    }

    /// **An event survives the row it is stored in, and the links survive with
    /// it.**
    ///
    /// The invariant test over in `search` builds its `Fact` in memory, so it
    /// proves the PROJECTION and nothing about the storage underneath: if the
    /// codec ever stopped reading this column the links would stop walking and
    /// that test would keep passing. This is the other end — a row that has
    /// been rendered and parsed back, with the payload intact and `linked()`
    /// still finding what it should.
    ///
    /// Byte-identity is asserted on the RE-RENDER rather than only on equality,
    /// because that is what the next write puts back on the page.
    #[test]
    fn an_event_survives_a_round_trip_through_the_row() {
        let mut recorded = Event::of("a-type-nobody-defined");
        recorded
            .metadata
            .insert("mood".into(), "delighted, and = punctuated".into());
        recorded
            .metadata
            .insert("mechanic".into(), "person:milhouse".into());
        recorded.refs.push(EntityId("place:north-trail".into()));

        let stored = Fact {
            event: Some(recorded.clone()),
            ..fact(
                "f1",
                "person:alpha",
                "the kiln was lit",
                Provenance::Testimony,
                date(2026, 7, 1),
            )
        };

        let row = render_fact_row(&stored);
        let back = parse_fact_row(&row, &EntityId::person("alpha")).expect("the row reads");
        assert_eq!(back.event.as_ref(), Some(&recorded), "the payload survived");
        assert_eq!(
            render_fact_row(&back),
            row,
            "…and re-renders to the same bytes, which is what the next write stores"
        );

        // The links are still findable off the row that came back — named and
        // unnamed alike, which is what the projection consumes.
        assert_eq!(
            back.event.expect("an event").linked(),
            vec![
                EntityId("person:milhouse".into()),
                EntityId("place:north-trail".into()),
            ],
            "a link that does not survive storage is not a link"
        );
    }

    /// **The id minter must see a row of the CURRENT width, and this is the
    /// test that says so out loud.**
    ///
    /// `row_id` is what reserves an id so it can never be handed out twice, and
    /// it screens on width. Adding the event column widened `layout_of` and
    /// left that screen at `6..=8`: every freshly-written row became invisible
    /// to the minter, `next_fact_id` restarted at f1, and the page ended up with
    /// two rows sharing one address — which the store reports as
    /// "its doc holds more than one row with that id", a long way from the
    /// range that caused it.
    ///
    /// Asserted against the header rather than a number, so the next column to
    /// arrive is covered by this test on the day it lands rather than by
    /// somebody remembering.
    #[test]
    fn a_row_of_the_current_width_reserves_its_id() {
        let widest = split_cells(TABLE_HEADER).len();
        let row = render_fact_row(&fact(
            "f7",
            "person:alpha",
            "a claim",
            Provenance::Testimony,
            date(2026, 1, 1),
        ));
        assert_eq!(
            split_cells(&row).len(),
            widest,
            "a rendered row is exactly as wide as the header it sits under"
        );
        assert_eq!(
            row_id(&row).as_deref(),
            Some("f7"),
            "…and its id is reserved, or the minter will hand it out again"
        );
    }

    #[test]
    fn next_id_increments_over_existing() {
        let mut doc = seeded_doc(&alpha());
        assert_eq!(next_fact_id(&doc), FactId("f1".into()));
        for id in ["f1", "f3"] {
            let row = render_fact_row(&fact(
                id,
                "person:alpha",
                "a",
                Provenance::Testimony,
                date(2026, 1, 1),
            ));
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
        let f = fact(
            "f2",
            "person:alpha",
            "learning Rust",
            Provenance::Inference,
            date(2026, 7, 2),
        );
        let updated = with_fact_appended(doc, &render_fact_row(&f));
        let parsed = parse_facts_table(&updated);
        assert_eq!(parsed.len(), 2, "the legacy row and the new one both read");
        assert_eq!(parsed[0].content, "plays go");
        assert_eq!(parsed[1], f);
        assert!(
            updated.contains("Some prose."),
            "prose above the table is untouched"
        );
    }

    #[test]
    fn seeded_doc_has_a_marker_and_an_empty_parseable_table() {
        let home = Entity {
            id: EntityId::person("alpha"),
            name: "Alpha".into(),
            ..alpha()
        };
        let doc = seeded_doc(&home);
        assert_eq!(parse_id_marker(&doc).as_deref(), Some("person:alpha"));
        assert!(parse_facts_table(&doc).is_empty());
        let f = fact(
            "f1",
            "person:alpha",
            "first fact",
            Provenance::Testimony,
            date(2026, 7, 24),
        );
        let updated = with_fact_appended(&doc, &render_fact_row(&f));
        assert_eq!(parse_facts_table(&updated), vec![f]);
        // A fact's own `id` (below the table) must not be mistaken for the marker.
        assert_eq!(parse_id_marker(&updated).as_deref(), Some("person:alpha"));
    }

    #[test]
    fn marker_is_absent_when_there_is_no_machine_block() {
        assert_eq!(
            parse_id_marker("# just prose\n\nnothing structured here"),
            None
        );
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
            event: None,
        };
        let mut doc = format!(
            "```yaml\nid: person:alpha\n```\n\n{FACTS_HEADER}\n\n{TABLE_HEADER}\n{TABLE_SEP}\n"
        );
        for (id, content) in [
            ("f1", "hello\rworld"),
            ("f2", "b"),
            ("f3", "c"),
            ("f4", "d"),
        ] {
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

/// **The golden fixtures: pages recorded from real Outline, asserted to parse
/// forever.**
///
/// Every other test in this file builds its own markdown, which means every
/// other test agrees with this reader by construction — they prove the codec
/// is self-consistent and cannot prove it reads what the store actually
/// writes. This project has been bitten by that gap twice in one day: an event
/// payload the adapter never stored (both halves of the read-back comparison
/// missing it in the same way) and a grammar the store rewrote under it (every
/// hand-built round trip green throughout).
///
/// So these bytes are not written here. They were produced by writing records
/// through the live store and reading the page back verbatim
/// (`record_the_golden_fixtures` in the integration suite), and the `.json`
/// beside each one is what the store then returned through the read path.
/// **The recorder is not run by the suite**, deliberately: a golden that
/// re-records itself moves silently the day the store starts mangling
/// something, and every test stays green while it does.
#[cfg(test)]
mod golden {
    use super::*;

    /// The recorded cases, by name. Named individually rather than walked, so
    /// a fixture that goes missing fails here — a directory walk over an empty
    /// directory asserts nothing and reports success.
    const RECORDED: [&str; 3] = ["event-punctuation", "retraction", "plain-fact"];

    fn fixture(name: &str, extension: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/facts")
            .join(format!("{name}.{extension}"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "the recorded fixture {} must be readable: {e}",
                path.display()
            )
        })
    }

    /// **A page the real store wrote parses into exactly the records the real
    /// store read back from it.**
    #[test]
    fn the_golden_pages_still_parse() {
        for name in RECORDED {
            let expected: Vec<Fact> = serde_json::from_str(&fixture(name, "json"))
                .unwrap_or_else(|e| panic!("{name}'s expectation must deserialize: {e}"));
            assert!(
                !expected.is_empty(),
                "{name} recorded no rows, so it is asserting nothing"
            );
            assert_eq!(
                parse_facts_table(&fixture(name, "md")),
                expected,
                "{name}: the page real Outline returned no longer parses into what it held"
            );
        }
    }

    /// **The punctuation case earns its keep by carrying its values back
    /// whole.** Equality above would pass if BOTH the page and the expectation
    /// had been recorded from a broken build, so the values a human can check
    /// are checked here against what was written, not against what was read.
    #[test]
    fn the_punctuation_a_markdown_store_rewrites_survives_a_real_round_trip() {
        let facts = parse_facts_table(&fixture("event-punctuation", "md"));
        let event = facts
            .first()
            .and_then(|f| f.event.as_ref())
            .expect("the recorded page holds one event");

        assert_eq!(event.kind, "a type with spaces");
        for (key, written) in [
            ("spaced", "a value with spaces"),
            ("equals", "a = b"),
            ("backslash", "c:\\dir\\file"),
            ("tilde", "a~b~c"),
            ("markup", "<b>bold</b> & *starred* _under_"),
            ("quoted", "\"double\" and 'single'"),
            ("unicode", "café — ünïcode ✓"),
            // A value that was ALREADY percent-encoded when the caller wrote
            // it: the one case where an escaping scheme eats its own tail.
            ("percent", "already %20 encoded"),
            ("empty", ""),
        ] {
            assert_eq!(
                event.metadata.get(key).map(String::as_str),
                Some(written),
                "{key} did not survive the real store: {event:?}"
            );
        }
    }

    /// **A retraction is two rows on one page, and the page says so.** The mark
    /// and the account of it went up in a single write; this is the proof that
    /// what came back is a page a reader can still tell that story from.
    #[test]
    fn the_golden_retraction_reads_as_a_marked_row_and_its_account() {
        let facts = parse_facts_table(&fixture("retraction", "md"));
        assert_eq!(facts.len(), 2, "two rows: {facts:?}");

        let taken_back = &facts[0];
        assert_eq!(taken_back.status, FactStatus::Retracted);
        assert_eq!(
            taken_back.content, "moved to the 14th",
            "marked, never edited"
        );

        let account = &facts[1];
        assert_eq!(account.status, FactStatus::Active);
        assert_eq!(
            account.event.as_ref().and_then(Event::retracts),
            Some(taken_back.address().to_string().as_str()),
            "the account still names what it takes back"
        );
        // The reason carries a pipe, which is the cell separator — recorded on
        // purpose, because a value that broke out of its cell would take the
        // whole row's shape with it.
        assert!(
            account.content.contains(" | "),
            "the pipe survived its cell: {account:?}"
        );
    }
}
