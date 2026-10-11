//! **A handle written into a sentence** — stored as the name that does not
//! move, served as the name the thing wears today (rule 205).
//!
//! An agent writes `@person:milhouse rode @thing:handcart` and means two
//! things, not two words. A handle is a PATH: it carries the kind and the slug,
//! and a thing that is renamed, reparented or retyped answers to a different
//! one afterwards. Text holding the old spelling is text pointing at nothing,
//! and nothing indexes it, marks it, or knows to go and fix it.
//!
//! So a mention is resolved on the way in. What the store keeps is the badge —
//! the name a row wears for as long as it exists and cannot be made to change —
//! and what a reader gets back is the handle that badge answers to NOW. **A
//! rename changes what every past mention renders as, with nothing rewritten
//! anywhere.**
//!
//! # One mechanism, three questions
//!
//! [`named`] reads the mentions an author wrote, which is what a write guard
//! screens. [`resolved`] turns them into what the store keeps. [`rendered`]
//! turns what the store keeps back into what a reader sees. **The spelling of
//! the stored form is written down once, here** (rule 51): a second reader of
//! it is a second answer to what a stored mention is.
//!
//! # Rendering is not optional for a door
//!
//! [`Mentioning`] reads the store beneath it as [`super::stored::StoredMemory`],
//! so every read that returns text comes back as [`super::stored::Stored`] and
//! cannot be handed on as it is: a door that forgets to render does not compile.
//! The one gap the type leaves is named in [`super::stored`].
//!
//! # What this does NOT do
//!
//! ⛔️ **It migrates nothing.** A claim's content or a thing's prose written
//! before this mechanism existed holds the spelling its author typed, and
//! reading it back does not rewrite it — only a fresh write goes through
//! [`resolved`]. **The store's own commit messages carry no caller text at
//! all** (rule 260): jojobot marks a boundary in them with a fixed string,
//! never a caller's words, so no handle ever needs migrating there.
//!
//! ⛔️ **It renames nothing.** This stores something permanent; it does not add
//! the ability to change a handle.
//!
//! # What this is not the only user of
//!
//! **A journal beat and a mailbox message hold a handle in text on the same
//! terms a claim's words do (rule 260)** — see
//! [`crate::session::mention`] and [`crate::mailbox::mention`], which call
//! [`resolved`] and [`rendered`] directly rather than duplicating the
//! spelling of the stored form.

use super::stored::{StoredMemory, Unrendered};
use super::{Entity, EntityId, kinds};

/// **What marks a resolved mention in stored text.**
///
/// Two characters that no handle can start with, so a stored mention can never
/// be read as one somebody typed. **Pinned by a test rather than derived**: the
/// spelling is STORED and nothing outside this process declares it, so a change
/// to it is a change every row written before it disagrees with — and a row
/// cannot fail a test.
pub const MARK: &str = "@#";

/// **A mention this process cannot tell from ordinary text**, because it has
/// loaded no kinds at all.
///
/// [`kinds::resolve`] already tells this apart from *no kind by that name* —
/// [`NotAKind::SetNeverLoaded`](kinds::NotAKind::SetNeverLoaded) versus
/// [`NotAKind::NotDeclared`](kinds::NotAKind::NotDeclared) — and this is that
/// same distinction, carried to the one place a lost mention is a lost link
/// rather than a lost lookup: a caller who wrote `@person:milhouse` gets no
/// answer telling them it did not take, only text that reads as though it
/// meant nothing else, forever, once stored that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindsUnloaded;

/// Every entity an author named in this text, in the order they wrote them.
///
/// **Shape and kind only.** Whether the handle names anything is the caller's
/// question, asked against the store: this says what was written, and a write
/// guard says whether it is there.
///
/// **Silent on an unloaded set, on purpose.** This answers "what did the
/// author mean," and on an unloaded set that question has no answer at all —
/// callers that need to know whether the set was even readable ask
/// [`resolved`], which refuses rather than guesses.
pub fn named(text: &str) -> Vec<EntityId> {
    let mut found = Vec::new();
    for at in outside_code(text) {
        // **A stored mark needs no skip of its own.** What follows it is a
        // badge rather than a kind, so the read below turns it down for the
        // reason it turns down any other word.
        if let Attempt::Mention(handle, _) = handle_at(text, at) {
            found.push(handle);
        }
    }
    found
}

/// **What the store keeps**: every mention of a badged row becomes its badge.
///
/// A mention of something wearing no badge is left exactly as written. That is
/// the record the build supplies, which is in no table — there is no row to
/// rename, so its handle is as permanent as anything here.
///
/// **Refuses rather than guesses when this process has loaded no kinds.** An
/// unloaded set cannot tell `@person:milhouse` from an ordinary word, and
/// guessing "not a mention" would store the author's link as the words they
/// typed — silently, and for good, since nothing rewrites stored text later.
pub fn resolved(text: &str, known: &[Entity]) -> Result<String, KindsUnloaded> {
    if any_unknowable(&[text]) {
        return Err(KindsUnloaded);
    }
    Ok(rewrite(text, |at, in_code| {
        if in_code {
            return None;
        }
        let Attempt::Mention(handle, end) = handle_at(text, at) else {
            return None;
        };
        let badge = known
            .iter()
            .find(|e| e.id == handle)?
            .badge
            .as_deref()
            .filter(|badge| !badge.is_empty())?;
        Some((format!("{MARK}{badge}"), end))
    }))
}

/// **What a reader sees**: every stored mention becomes the handle its badge
/// answers to today.
///
/// Two things cannot be rendered as a link, and they are told apart because
/// they are stored differently — never by how they are dressed.
///
/// * **A badge nothing wears.** jojobot held this link and the row it named is
///   not in the store. The badge stays, so a person has something to go and
///   look for.
/// * **A handle nothing answers.** It was never made a link: it is text that
///   looks like a handle, out of prose written before mentions existed.
///
/// ⛔️ **Neither is served bare.** A broken link that renders as ordinary text
/// reads as a sentence that is complete, and the reader has no way to know the
/// pointer is gone.
pub fn rendered(text: &str, known: &[Entity]) -> String {
    rewrite(text, |at, in_code| {
        if text[at..].starts_with(MARK) {
            let badge = run_of(text, at + MARK.len(), is_badge_byte);
            if badge.is_empty() {
                return None;
            }
            let end = at + MARK.len() + badge.len();
            return Some(match wearing(known, badge) {
                Some(entity) => (format!("@{}", entity.id.as_str()), end),
                None => (format!("{MARK}{badge} {GONE}"), end),
            });
        }
        if in_code {
            return None;
        }
        let Attempt::Mention(handle, end) = handle_at(text, at) else {
            return None;
        };
        match known.iter().any(|e| e.id == handle) {
            // It reads as a link and it names something that is here. Nothing
            // to say: marking it would be reporting damage that is not there.
            true => None,
            false => Some((format!("@{} {UNKNOWN}", handle.as_str()), end)),
        }
    })
}

/// **Where a mention sits in text a reader is about to see, and what it
/// points at** — for a caller that wants to draw a mention as something
/// rather than serve it as a string.
///
/// **Reads served text, not stored text.** Only [`rendered`]'s output is a
/// safe input: the two shapes it cannot make followable — a badge nothing
/// wears, a handle nothing answers — are told apart by their trailing marker,
/// which is what this reads to leave them out. **Neither becomes a link, and
/// this is the one place that rule is enforced**: a caller that walks the
/// result never has to re-derive it.
pub fn followable(text: &str) -> Vec<(std::ops::Range<usize>, EntityId)> {
    let mut found = Vec::new();
    for at in outside_code(text) {
        // **A stored mark parses as no handle at all**, so it is already
        // excluded — `handle_at` needs `kind:` right after the `@`, and `#`
        // is never that.
        let Attempt::Mention(handle, end) = handle_at(text, at) else {
            continue;
        };
        if text[end..].starts_with(&format!(" {UNKNOWN}")) {
            continue;
        }
        found.push((at..end, handle));
    }
    found
}

/// **A link whose thing is no longer in the store.**
const GONE: &str = "(gone)";

/// **Text shaped like a handle that jojobot never made a link.**
const UNKNOWN: &str = "(no such handle)";

/// The entity wearing this badge, if any.
fn wearing<'a>(known: &'a [Entity], badge: &str) -> Option<&'a Entity> {
    known
        .iter()
        .find(|e| e.badge.as_deref().is_some_and(|worn| worn == badge))
}

/// Walk every `@` in the text and let `at` decide what replaces it, told
/// whether the `@` sits inside code.
///
/// **One walk for both directions**, so the two can never disagree about where
/// a mention starts or ends. **It offers the `@` inside code too**, because the
/// stored mark is jojobot's own and is read wherever it sits: a handle in a code
/// span was resolved before code was literal, and the row holds the badge there.
fn rewrite(text: &str, at: impl Fn(usize, bool) -> Option<(String, usize)>) -> String {
    let code = code_ranges(text);
    let mut out = String::with_capacity(text.len());
    let mut cut = 0;
    for (start, _) in text.match_indices('@') {
        if start < cut {
            continue;
        }
        let Some((replacement, end)) = at(start, within(&code, start)) else {
            continue;
        };
        out.push_str(&text[cut..start]);
        out.push_str(&replacement);
        cut = end;
    }
    out.push_str(&text[cut..]);
    out
}

/// **Every `@` that is not inside code**, in order.
///
/// Text inside an inline code span or a fenced block is literal (rule 450): a
/// document that shows sample code holds handles that name nothing, and reading
/// them as mentions would refuse the whole write. **Every walk over a mention
/// starts here**, so what is stored, what is served and what a write guard
/// screens cannot disagree about where code is. Replacements this module makes
/// hold no backtick or tilde, so the code in stored text is where the code in
/// written text was.
fn outside_code(text: &str) -> Vec<usize> {
    let code = code_ranges(text);
    text.match_indices('@')
        .map(|(at, _)| at)
        .filter(|at| !within(&code, *at))
        .collect()
}

/// Whether a position sits inside one of these ranges.
fn within(code: &[std::ops::Range<usize>], at: usize) -> bool {
    code.iter().any(|range| range.contains(&at))
}

/// **Where the code is**: fenced blocks first, then inline spans in what is
/// left. Only two markdown shapes are read, and nothing else about markdown is.
///
/// * **A fenced block** opens on a line of three or more backticks or tildes
///   (any indent; a backtick fence may not have a backtick after it) and closes
///   on a line of the same character, at least as long, with nothing after it.
///   **A fence that never closes is not a fence.** Treating the rest of the text
///   as code would drop every link after a stray fence without a word, so it is
///   read as the text it is and the lines after it are read again.
/// * **An inline span** opens on a run of backticks and closes on the next run
///   of the same length, within one paragraph. A run with no partner is literal.
/// * **An indented block is not code.** It cannot be told from the continuation
///   of a list item without reading the list, and skipping a real mention is the
///   worse mistake.
fn code_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<(usize, &str)> = {
        let mut at = 0;
        text.split_inclusive('\n')
            .map(|line| {
                let start = at;
                at += line.len();
                (start, line)
            })
            .collect()
    };
    let mut fenced: Vec<std::ops::Range<usize>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((mark, length)) = fence_opening(lines[i].1) else {
            i += 1;
            continue;
        };
        match (i + 1..lines.len()).find(|&j| fence_closes(lines[j].1, mark, length)) {
            Some(j) => {
                fenced.push(lines[i].0..lines[j].0 + lines[j].1.len());
                i = j + 1;
            }
            None => i += 1,
        }
    }
    let mut code = fenced.clone();
    let mut from = 0;
    for range in fenced
        .iter()
        .chain(std::iter::once(&(text.len()..text.len())))
    {
        code.extend(
            inline_spans(&text[from..range.start])
                .into_iter()
                .map(|span| span.start + from..span.end + from),
        );
        from = range.end;
    }
    code
}

/// The fence character and length a line opens a fence with, if it does.
fn fence_opening(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let mark = *trimmed.as_bytes().first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let length = trimmed.bytes().take_while(|b| *b == mark).count();
    let after = &trimmed[length..];
    (length >= 3 && !(mark == b'`' && after.contains('`'))).then_some((mark, length))
}

/// Whether a line closes the fence of this character and length.
fn fence_closes(line: &str, mark: u8, length: usize) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let run = trimmed.bytes().take_while(|b| *b == mark).count();
    run >= length && trimmed[run..].trim().is_empty()
}

/// The inline code spans in a piece of text that holds no fenced block.
fn inline_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let run_at = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let length = run_at(i);
        let limit = paragraph_end(text, i);
        let mut j = i + length;
        let mut closed = None;
        while j < limit {
            if bytes[j] == b'`' {
                let found = run_at(j);
                if found == length {
                    closed = Some(j + found);
                    break;
                }
                j += found;
            } else {
                j += 1;
            }
        }
        match closed {
            Some(end) => {
                spans.push(i..end);
                i = end;
            }
            None => i += length,
        }
    }
    spans
}

/// Where the paragraph that holds `from` ends: the start of the next blank line,
/// or the end of the text.
fn paragraph_end(text: &str, from: usize) -> usize {
    let mut at = from;
    while let Some(newline) = text[at..].find('\n') {
        let line_end = at + newline;
        let rest = &text[line_end + 1..];
        let next = rest.find('\n').map_or(rest.len(), |n| n);
        if rest[..next].trim().is_empty() && line_end + 1 < text.len() {
            return line_end;
        }
        at = line_end + 1;
    }
    text.len()
}

/// What trying to read a mention at `at` turned up.
enum Attempt {
    /// A real mention: the handle and where it ends.
    Mention(EntityId, usize),
    /// This shape is not a mention, whatever the kind set holds — no colon,
    /// no slug, or a kind this process has loaded and does not declare.
    NotAMention,
    /// This shapes exactly like a mention, and this process cannot say
    /// whether it is one: it has loaded no kinds at all, so every token
    /// fails to resolve the same way a typo would.
    Unknowable,
}

/// The handle written at `at`, and where it ends.
///
/// **A kind this instance does not hold is not a mention at all**, so `@` in
/// front of an ordinary word is ordinary text. That is the discriminator: a
/// mention names a kind, and the set of kinds is data this process loaded.
///
/// **A process that has loaded no kinds cannot draw that line.**
/// [`kinds::resolve`] fails every token the same way then — the same
/// ambiguity `resolve` itself refuses rather than guesses at — so this says
/// so explicitly instead of quietly picking the answer that looks safest.
fn handle_at(text: &str, at: usize) -> Attempt {
    let rest = &text[at + 1..];
    let Some((kind, _)) = rest.split_once(':') else {
        return Attempt::NotAMention;
    };
    // **The run of slug bytes, whole.** A trailing hyphen is not trimmed: a
    // slug may end in one, so trimming would read a mention as naming a
    // different thing than the author wrote. Checked before the kind
    // resolves: a shape with no slug at all is not a mention regardless of
    // what the kind set holds, so it is never ambiguous either.
    let slug = run_of(text, at + 1 + kind.len() + 1, is_slug_byte);
    if slug.is_empty() {
        return Attempt::NotAMention;
    }
    match kinds::resolve(kind) {
        Ok(_) => {
            let end = at + 1 + kind.len() + 1 + slug.len();
            Attempt::Mention(EntityId(format!("{kind}:{slug}")), end)
        }
        Err(kinds::NotAKind::SetNeverLoaded) => Attempt::Unknowable,
        Err(kinds::NotAKind::NotDeclared { .. }) => Attempt::NotAMention,
    }
}

/// **Whether a caller's text carries the stored mark.** [`rendered`] reads an
/// `@` and `#` followed by a badge as a link to whatever wears that badge, so
/// text a caller wrote in that shape would be a link nothing checked. The mark
/// is jojobot's to write, and this is the question every write of a caller's
/// text asks first. A mark with no badge after it reads as ordinary text and is
/// not one.
pub fn forged_mark(text: &str) -> bool {
    text.match_indices('@').any(|(at, _)| {
        text[at..].starts_with(MARK) && !run_of(text, at + MARK.len(), is_badge_byte).is_empty()
    })
}

/// The error a write reaches for when [`resolved`] refuses. A single spelling
/// so every call site names the same failure the same way.
fn unloaded(_: KindsUnloaded) -> super::MemoryError {
    super::MemoryError::KindsNeverLoaded { attempted: None }
}

/// **The refusal a caller's text gets when it carries the stored mark** (see
/// [`forged_mark`]). One spelling for every write that takes text.
fn refuse_forged(text: &[&str]) -> Result<(), super::MemoryError> {
    match text.iter().any(|part| forged_mark(part)) {
        false => Ok(()),
        true => Err(super::MemoryError::InvalidFact(format!(
            "the text carries '{MARK}' followed by a badge, which is how jojobot stores a link \
             to a thing, and only jojobot writes it. Write @kind:slug to link a \
             thing, or leave the '{MARK}' off."
        ))),
    }
}

/// Whether any of this text holds a mention-shaped substring this process
/// cannot resolve because it has loaded no kinds.
fn any_unknowable(text: &[&str]) -> bool {
    text.iter().any(|part| {
        outside_code(part)
            .into_iter()
            .any(|at| matches!(handle_at(part, at), Attempt::Unknowable))
    })
}

/// The run of bytes from `from` that `keeps` accepts.
fn run_of(text: &str, from: usize, keeps: fn(u8) -> bool) -> &str {
    let bytes = text.as_bytes();
    let mut end = from;
    while end < bytes.len() && keeps(bytes[end]) {
        end += 1;
    }
    &text[from..end]
}

fn is_slug_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}

/// A badge is drawn from the handle alphabet, which is a subset of the slug
/// bytes — so the same character ends both, and a mention followed by
/// punctuation reads the same in either direction.
fn is_badge_byte(b: u8) -> bool {
    crate::handle::ALPHABET.contains(&b)
}

// --- the decorator ---------------------------------------------------------

/// **A store whose text carries mentions.**
///
/// Resolve on the way in, render on the way out — one implementation over every
/// store (rule 51). A rule each store implemented for itself is a rule they
/// eventually disagree about, which is the argument the supplied-record layer
/// makes for itself and it holds here for the same reason.
///
/// ⚠️ **It sits in the domain rather than beside the other decorators**, and
/// the reason is the bar rather than taste: the shared contract has to ask this
/// of the fake and of the real store alike, and the fake's contract run lives
/// here.
pub struct Mentioning {
    /// **The store beneath, read as stored.** It is a [`StoredMemory`] and not a
    /// [`super::Memory`], so a door that returns what the store returned does not
    /// compile: the text comes back as [`super::stored::Stored`], and only its `open`
    /// (visible here and nowhere else) turns it into something a reader sees.
    inner: std::sync::Arc<dyn StoredMemory>,
}

impl Mentioning {
    pub fn new(inner: std::sync::Arc<dyn super::Memory>) -> Self {
        Mentioning {
            inner: std::sync::Arc::new(Unrendered(inner)),
        }
    }

    /// **What exists, as both directions of a mention need to see it.**
    ///
    /// One read of the index answers handle-to-badge on the way in and
    /// badge-to-handle on the way out, so neither direction holds an inventory
    /// of its own.
    async fn known(&self) -> Result<Vec<Entity>, super::MemoryError> {
        self.inner.list_entities(None).await
    }

    /// **Every rename event, for the reference-typed field values
    /// [`render_fact`](Self::render_fact) still resolves by hand**: those
    /// store a plain handle rather than a badge, so a rename after one was
    /// written leaves it stale in a way an edge or a ref no longer does
    /// (rule 268).
    async fn former(&self) -> Result<Vec<super::FormerHandle>, super::MemoryError> {
        self.inner.former_handles().await
    }

    /// **What a reference-typed key is, for the one thing that reads it
    /// going out**: a field value holding a plain handle is the same shape
    /// an edge's object is, so it needs the same declarations `referenced_by`
    /// already reads at write time to know which keys these are.
    async fn declared(&self) -> Result<Vec<super::types::DeclaredType>, super::MemoryError> {
        self.inner.declared_types().await
    }

    /// Rewrite a claim's text for a reader, and follow a reference-typed
    /// field value to wherever what it names answers to now.
    ///
    /// **An edge's object and a ref need no resolution here** (rule 268):
    /// both stores now store the badge the object wears and serve the
    /// current handle back on every read, so a fact handed to this
    /// function already carries the right one. A reference-typed field is
    /// the one case still a plain handle — `resolve_handle` is the same
    /// fallback a stale one gets anywhere: a direct match first, a handle's
    /// own history next, and a genuinely unknown one left as written rather
    /// than guessed at.
    fn render_fact(
        &self,
        fact: &mut super::Fact,
        known: &[Entity],
        former: &[super::FormerHandle],
        declared: &[super::types::DeclaredType],
    ) {
        fact.content = rendered(&fact.content, known);
        if let Some(details) = &fact.details {
            fact.details = Some(rendered(details, known));
        }
        Self::render_fields(&mut fact.fields, known, former, declared);
    }

    /// **Resolve every reference-typed value in a field map.**
    ///
    /// Used by [`Self::render_fact`], over a fact's own fields. A thing's folded
    /// fields come from [`Memory::fields`](super::Memory::fields) already
    /// resolved by the store, so that door resolves nothing here.
    fn render_fields(
        fields: &mut std::collections::BTreeMap<String, String>,
        known: &[Entity],
        former: &[super::FormerHandle],
        declared: &[super::types::DeclaredType],
    ) {
        for (key, value) in fields.iter_mut() {
            let Some(field) = declared
                .iter()
                .filter_map(|d| d.field(key))
                .find(|f| f.holds == super::types::ValueType::Reference)
            else {
                continue;
            };
            let resolved: Vec<String> = field
                .items(value)
                .into_iter()
                .map(|item| {
                    let id = EntityId(item.trim().to_string());
                    match super::resolve_handle(&id, known, former) {
                        Some(current) => current.id.to_string(),
                        None => item.to_string(),
                    }
                })
                .collect();
            *value = resolved.join(", ");
        }
    }

    async fn render_facts(&self, facts: &mut [super::Fact]) -> Result<(), super::MemoryError> {
        if facts.is_empty() {
            return Ok(());
        }
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        for fact in facts {
            self.render_fact(fact, &known, &former, &declared);
        }
        Ok(())
    }

    /// Rewrite a whole document for a reader: its page and every claim on it.
    fn render_scan(
        &self,
        doc: &mut super::search::DocScan,
        known: &[Entity],
        former: &[super::FormerHandle],
        declared: &[super::types::DeclaredType],
    ) {
        doc.prose = rendered(&doc.prose, known);
        for fact in &mut doc.facts {
            self.render_fact(fact, known, former, declared);
        }
    }

    /// **A write may not name something that is not there.**
    ///
    /// The same rule an edge's object faces and a record's refs face: every
    /// entity a claim names must already exist, so a mention is screened rather
    /// than stored as a spelling nobody can follow. A caller that meant the
    /// words rather than the link changes the words; a caller that meant the
    /// link creates the thing, which is two deliberate steps exactly as it is
    /// everywhere else here.
    ///
    /// **Says nothing about a mention this process cannot resolve at all** —
    /// an unloaded set makes `named` see no mentions rather than an absent
    /// one, so this never finds anything to block over it. That refusal
    /// belongs to [`resolved`], which every caller of this also calls on the
    /// same text right after: one property, checked once, rather than a copy
    /// here that would only ever agree with it.
    fn screen<T>(text: &[&str], known: &[Entity]) -> Option<super::Guarded<T>> {
        Self::first_unresolved(text, known).map(|(attempted, candidates)| super::Guarded::Blocked {
            attempted,
            candidates,
        })
    }

    /// **The same rule, answered as an error.**
    ///
    /// `retract` and `merge` return `Retraction`/`Merge` directly — neither
    /// carries a `Guarded` shape to answer blocked in. A mention naming
    /// nothing is the same near-miss the write guard already raises for
    /// any OTHER absent handle on these two verbs (an unknown address, an
    /// unknown duplicate or survivor): [`super::MemoryError::UnknownEntity`],
    /// which the surface already renders as blocked-with-candidates.
    fn screen_or_unknown(text: &[&str], known: &[Entity]) -> Option<super::MemoryError> {
        Self::first_unresolved(text, known).map(|(attempted, nearest)| {
            super::MemoryError::UnknownEntity {
                attempted: attempted.to_string(),
                nearest,
            }
        })
    }

    /// **The mention screen's shared core**: the first handle named in this
    /// text that nothing answers to, and what the guard found nearby.
    /// [`Self::screen`] and [`Self::screen_or_unknown`] each wrap this in
    /// the shape their own caller's return type can carry.
    fn first_unresolved(
        text: &[&str],
        known: &[Entity],
    ) -> Option<(EntityId, Vec<super::guard::EntityMatch>)> {
        for part in text {
            for handle in named(part) {
                if known.iter().any(|e| e.id == handle) {
                    continue;
                }
                let candidates = super::guard::screen(&handle, &[], known);
                return Some((handle, candidates));
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl super::Memory for Mentioning {
    async fn add_entity(
        &self,
        new: super::NewEntity,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.inner.add_entity(new).await
    }
    /// **The first claim resolves what it says, exactly as a capture does.** A
    /// mention that names nothing blocks the creation, and a mention that
    /// resolves is stored as the badge its row wears. Forwarded and not left to
    /// the port's default, which refuses.
    async fn add_entity_with_first_claim(
        &self,
        new: super::NewEntity,
        first: super::NewFact,
    ) -> Result<super::Guarded<(Entity, super::Fact)>, super::MemoryError> {
        refuse_forged(&[
            first.content.as_str(),
            first.details.as_deref().unwrap_or(""),
            first.drop_because.as_deref().unwrap_or(""),
        ])?;
        let known = self.known().await?;
        if let Some(blocked) = Self::screen(
            &[
                first.content.as_str(),
                first.details.as_deref().unwrap_or(""),
            ],
            &known,
        ) {
            return Ok(blocked);
        }
        let written = self
            .inner
            .add_entity_with_first_claim(
                new,
                super::NewFact {
                    content: resolved(&first.content, &known).map_err(unloaded)?,
                    details: first
                        .details
                        .as_deref()
                        .map(|d| resolved(d, &known))
                        .transpose()
                        .map_err(unloaded)?,
                    ..first
                },
            )
            .await?
            .open();
        Ok(match written {
            super::Guarded::Written((entity, mut stored)) => {
                let known = self.known().await?;
                let former = self.former().await?;
                let declared = self.declared().await?;
                self.render_fact(&mut stored, &known, &former, &declared);
                super::Guarded::Written((entity, stored))
            }
            blocked => blocked,
        })
    }
    /// **The claim written with an edit resolves what it says, exactly as a
    /// capture does**, and faces the stored-mark rule. Forwarded and not left to
    /// the port's default, which refuses.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
        claim: super::NewFact,
    ) -> Result<super::Guarded<(Entity, super::Fact)>, super::MemoryError> {
        refuse_forged(&[
            claim.content.as_str(),
            claim.details.as_deref().unwrap_or(""),
            claim.drop_because.as_deref().unwrap_or(""),
        ])?;
        let known = self.known().await?;
        if let Some(blocked) = Self::screen(
            &[
                claim.content.as_str(),
                claim.details.as_deref().unwrap_or(""),
            ],
            &known,
        ) {
            return Ok(blocked);
        }
        let written = self
            .inner
            .update_entity_with_claim(
                handle,
                patch,
                super::NewFact {
                    content: resolved(&claim.content, &known).map_err(unloaded)?,
                    details: claim
                        .details
                        .as_deref()
                        .map(|d| resolved(d, &known))
                        .transpose()
                        .map_err(unloaded)?,
                    ..claim
                },
            )
            .await?
            .open();
        Ok(match written {
            super::Guarded::Written((entity, mut stored)) => {
                let known = self.known().await?;
                let former = self.former().await?;
                let declared = self.declared().await?;
                self.render_fact(&mut stored, &known, &former, &declared);
                super::Guarded::Written((entity, stored))
            }
            blocked => blocked,
        })
    }
    async fn list_entities(
        &self,
        kind: Option<super::EntityKind>,
    ) -> Result<Vec<Entity>, super::MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<super::FormerHandle>, super::MemoryError> {
        self.inner.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: jiff::civil::Date,
        override_token: Option<&str>,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.inner
            .rename_entity(from, to, parent, date, override_token)
            .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
    ) -> Result<Entity, super::MemoryError> {
        self.inner.archive_entity(id, reason).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
    ) -> Result<(Entity, super::Archived), super::MemoryError> {
        self.inner.restore_entity(id).await
    }
    /// **Resolved on the way in.** Every handle an author wrote becomes the
    /// badge its row wears, so the claim keeps a pointer rather than a
    /// spelling.
    async fn capture(
        &self,
        fact: super::NewFact,
    ) -> Result<super::Guarded<super::Fact>, super::MemoryError> {
        refuse_forged(&[
            fact.content.as_str(),
            fact.details.as_deref().unwrap_or(""),
            // A store keeps the reason a thought is dropped as the details of the
            // claim it archives.
            fact.drop_because.as_deref().unwrap_or(""),
        ])?;
        let known = self.known().await?;
        if let Some(blocked) = Self::screen(
            &[fact.content.as_str(), fact.details.as_deref().unwrap_or("")],
            &known,
        ) {
            return Ok(blocked);
        }
        let written = self
            .inner
            .capture(super::NewFact {
                content: resolved(&fact.content, &known).map_err(unloaded)?,
                details: fact
                    .details
                    .as_deref()
                    .map(|d| resolved(d, &known))
                    .transpose()
                    .map_err(unloaded)?,
                ..fact
            })
            .await?
            .open();
        Ok(match written {
            super::Guarded::Written(mut stored) => {
                let former = self.former().await?;
                let declared = self.declared().await?;
                self.render_fact(&mut stored, &known, &former, &declared);
                super::Guarded::Written(stored)
            }
            blocked => blocked,
        })
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<super::Fact>, super::MemoryError> {
        let mut facts = self.inner.recall(subject).await?.open();
        self.render_facts(&mut facts).await?;
        Ok(facts)
    }
    /// **An edit resolves what it writes, exactly as a capture does.** A
    /// correction is where a mention is most likely to arrive, because that is
    /// where somebody rewrites the sentence.
    async fn update_fact(
        &self,
        address: &super::FactAddress,
        patch: super::FactPatch,
        caller: &super::EntityId,
    ) -> Result<super::Guarded<super::Fact>, super::MemoryError> {
        refuse_forged(&[
            patch.content.as_deref().unwrap_or(""),
            patch.details.as_deref().unwrap_or(""),
        ])?;
        let known = self.known().await?;
        if let Some(blocked) = Self::screen(
            &[
                patch.content.as_deref().unwrap_or(""),
                patch.details.as_deref().unwrap_or(""),
            ],
            &known,
        ) {
            return Ok(blocked);
        }
        let written = self
            .inner
            .update_fact(
                address,
                super::FactPatch {
                    content: patch
                        .content
                        .as_deref()
                        .map(|c| resolved(c, &known))
                        .transpose()
                        .map_err(unloaded)?,
                    details: patch
                        .details
                        .as_deref()
                        .map(|d| resolved(d, &known))
                        .transpose()
                        .map_err(unloaded)?,
                    ..patch
                },
                caller,
            )
            .await?
            .open();
        Ok(match written {
            super::Guarded::Written(mut stored) => {
                let former = self.former().await?;
                let declared = self.declared().await?;
                self.render_fact(&mut stored, &known, &former, &declared);
                super::Guarded::Written(stored)
            }
            blocked => blocked,
        })
    }
    /// **A write carries the note its record carried**, which is text, so it
    /// renders like every other piece of text a caller reads.
    async fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Vec<super::FieldWrite>, super::MemoryError> {
        let mut writes = self.inner.history(entity, key).await?.open();
        if writes.iter().all(|w| w.note.is_none()) {
            return Ok(writes);
        }
        let known = self.known().await?;
        for write in &mut writes {
            if let Some(note) = &write.note {
                write.note = Some(rendered(note, &known));
            }
        }
        Ok(writes)
    }
    /// **Every wording a claim ever had is text**, so the chain renders like
    /// the claim does — otherwise reading what a record used to say is the one
    /// place a badge leaks to a caller.
    async fn claim_history(
        &self,
        address: &super::FactAddress,
    ) -> Result<Vec<super::ClaimWrite>, super::MemoryError> {
        let mut chain = self.inner.claim_history(address).await?.open();
        if chain.is_empty() {
            return Ok(chain);
        }
        let known = self.known().await?;
        for write in &mut chain {
            write.content = rendered(&write.content, &known);
            if let Some(details) = &write.details {
                write.details = Some(rendered(details, &known));
            }
        }
        Ok(chain)
    }
    /// **Straight through**: the keys of a record are names, and no mention is
    /// rendered in one.
    async fn keys_kept_off_the_thing(
        &self,
        address: &super::FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, super::MemoryError> {
        self.inner.keys_kept_off_the_thing(address).await
    }
    /// **Every chain renders exactly as [`Self::claim_history`] renders one**,
    /// so the batched read and the single read cannot disagree about what a
    /// mention in an earlier wording says.
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::HashMap<super::FactId, Vec<super::ClaimWrite>>, super::MemoryError>
    {
        let mut chains = self.inner.claim_histories(entity).await?.open();
        if chains.values().all(Vec::is_empty) {
            return Ok(chains);
        }
        let known = self.known().await?;
        for write in chains.values_mut().flatten() {
            write.content = rendered(&write.content, &known);
            if let Some(details) = &write.details {
                write.details = Some(rendered(details, &known));
            }
        }
        Ok(chains)
    }
    /// **Straight through**: an age says nothing a mention could be inside.
    async fn field_ages(&self, entity: &EntityId) -> Result<super::FieldAges, super::MemoryError> {
        self.inner.field_ages(entity).await
    }

    /// **A thing's folded fields, as the store serves them.** A thing-scope
    /// filter compares this map and a record-scope filter compares a resolved
    /// fact's, so both have to show today's handle under a reference-typed key.
    /// Nothing is resolved here, because the layers below already do it: a store
    /// keeps the permanent id a reference names and serves the handle its thing
    /// wears now on every read, and the fold drops a cached copy that names a
    /// renamed handle. A resolution here changed no answer on either store.
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, super::MemoryError> {
        Ok(self.inner.fields(entity).await?.open())
    }
    /// **A key's backing carries the note its record carried**, the same
    /// text under the same name [`Self::history`] already renders — read
    /// through a different door, which is why a bare delegate here missed
    /// it.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, super::FieldBacking>, super::MemoryError> {
        let mut backing = self.inner.backing(entity).await?.open();
        if backing.values().all(|b| b.note.is_none()) {
            return Ok(backing);
        }
        let known = self.known().await?;
        for held in backing.values_mut() {
            if let Some(note) = &held.note {
                held.note = Some(rendered(note, &known));
            }
        }
        Ok(backing)
    }
    /// **Both claims a retraction hands back are claims**, so both render.
    /// The reason is text a caller wrote, so it resolves on the way in.
    async fn retract(
        &self,
        address: &super::FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &super::EntityId,
    ) -> Result<super::Retraction, super::MemoryError> {
        refuse_forged(&[reason.unwrap_or("")])?;
        let known = self.known().await?;
        if let Some(err) = Self::screen_or_unknown(&[reason.unwrap_or("")], &known) {
            return Err(err);
        }
        let former = self.former().await?;
        let declared = self.declared().await?;
        let reason = reason
            .map(|r| resolved(r, &known))
            .transpose()
            .map_err(unloaded)?;
        let mut done = self
            .inner
            .retract(address, reason.as_deref(), date, caller)
            .await?
            .open();
        self.render_fact(&mut done.retracted, &known, &former, &declared);
        self.render_fact(&mut done.record, &known, &former, &declared);
        Ok(done)
    }
    /// **A fold writes an account on the survivor**, which is a claim like any
    /// other and reads like one.
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
    ) -> Result<super::Merge, super::MemoryError> {
        refuse_forged(&[reason.unwrap_or("")])?;
        let known = self.known().await?;
        if let Some(err) = Self::screen_or_unknown(&[reason.unwrap_or("")], &known) {
            return Err(err);
        }
        let former = self.former().await?;
        let declared = self.declared().await?;
        let reason = reason
            .map(|r| resolved(r, &known))
            .transpose()
            .map_err(unloaded)?;
        let mut done = self
            .inner
            .merge(folded, survivor, reason.as_deref(), date, caller)
            .await?
            .open();
        self.render_fact(&mut done.record, &known, &former, &declared);
        Ok(done)
    }
    /// **A page is text, so it carries mentions too.** What comes back is the
    /// receipt for what was stored, rendered — the caller reads handles here
    /// exactly as they do everywhere else.
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
    ) -> Result<String, super::MemoryError> {
        refuse_forged(&[prose])?;
        let known = self.known().await?;
        let stored = self
            .inner
            .set_prose(entity, &resolved(prose, &known).map_err(unloaded)?)
            .await?
            .open();
        Ok(rendered(&stored, &known))
    }
    /// **The index scans through here**, so what search holds is what a reader
    /// sees: a claim mentioning a thing is findable by that thing's handle
    /// rather than by a badge nobody types.
    async fn scan(&self) -> Result<Vec<super::search::DocScan>, super::MemoryError> {
        let mut scanned = self.inner.scan().await?.open();
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        for doc in &mut scanned {
            self.render_scan(doc, &known, &former, &declared);
        }
        Ok(scanned)
    }

    /// **Rendering handles into readable text changes no stored write**, so
    /// the store underneath is the whole answer.
    async fn write_summary(&self) -> Result<Option<super::WriteSummary>, super::MemoryError> {
        self.inner.write_summary().await
    }

    /// **Straight through.** The comparison is per key against what the
    /// build ships, which only the layer beneath holds; resolving a handle
    /// into a mention changes neither side of it.
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Vec<String> {
        self.inner.echoed_defaults(entity, fields).await
    }

    async fn composed_prose(
        &self,
        entity: &EntityId,
        own: &str,
    ) -> Result<String, super::MemoryError> {
        self.inner.composed_prose(entity, own).await
    }

    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<super::search::DocScan>, super::MemoryError> {
        let Some(mut doc) = self.inner.scan_entity(entity).await?.open() else {
            return Ok(None);
        };
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        self.render_scan(&mut doc, &known, &former, &declared);
        Ok(Some(doc))
    }

    async fn built_on(
        &self,
        source: &super::FactAddress,
    ) -> Result<Vec<super::Fact>, super::MemoryError> {
        let mut facts = self.inner.built_on(source).await?.open();
        self.render_facts(&mut facts).await?;
        Ok(facts)
    }

    async fn referring_to(
        &self,
        target: &EntityId,
    ) -> Result<Vec<super::Fact>, super::MemoryError> {
        let mut facts = self.inner.referring_to(target).await?.open();
        self.render_facts(&mut facts).await?;
        Ok(facts)
    }

    /// **Straight through.** The answer is handles and renders nothing, and the
    /// default would list every bot and recall each one.
    async fn thought_writers(
        &self,
        thread: &EntityId,
    ) -> Result<Vec<EntityId>, super::MemoryError> {
        self.inner.thought_writers(thread).await
    }
    async fn declare_type(
        &self,
        declared: super::types::DeclaredType,
    ) -> Result<super::types::DeclaredType, super::MemoryError> {
        self.inner.declare_type(declared).await
    }
    async fn declared_types(&self) -> Result<Vec<super::types::DeclaredType>, super::MemoryError> {
        self.inner.declared_types().await
    }
    async fn displaced_type(
        &self,
        name: &str,
    ) -> Result<Option<super::types::Displaced>, super::MemoryError> {
        self.inner.displaced_type(name).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: super::types::Origin,
        fields: Vec<super::types::Field>,
    ) -> Result<(), super::MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), super::MemoryError> {
        self.inner.reclaim_kind(token).await
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, super::types::Origin)>, super::MemoryError> {
        self.inner.declared_kinds().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Boot;

    fn thing(handle: &str, badge: Option<&str>) -> Entity {
        Entity {
            id: EntityId(handle.into()),
            kind: EntityId(handle.into()).kind().expect("a test handle"),
            name: handle.into(),
            aliases: Vec::new(),
            source: "the roster".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
            badge: badge.map(str::to_string),
            archived: None,
        }
    }

    /// 🚨 **The stored spelling, pinned as a literal.**
    ///
    /// It is written into rows and nothing outside this process declares it, so
    /// a change to it is a change every row written before it disagrees with —
    /// and a row cannot fail a test. An assertion built from [`MARK`] would
    /// move with the constant and pin nothing.
    #[test]
    fn a_stored_mention_is_an_at_sign_and_a_hash_and_the_badge() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        assert_eq!(
            resolved("ask @person:milhouse about it", &known),
            Ok("ask @#k7h2mn about it".to_string()),
        );
    }

    /// **`@` in front of an ordinary word is ordinary text.** The
    /// discriminator is the KIND, and the set of kinds is data this process
    /// loaded — so a mention is a handle and nothing else has to be escaped.
    #[test]
    fn text_that_names_no_kind_is_left_alone() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        let plain = "mail me @ home, or @sandwich:cheese, or @person";
        assert_eq!(resolved(plain, &known), Ok(plain.to_string()));
        assert_eq!(rendered(plain, &known), plain);
    }

    /// **Punctuation after a mention is punctuation.** A handle never ends in a
    /// hyphen, and the character that ends a slug ends a badge, so a mention
    /// reads the same in both directions.
    #[test]
    fn a_mention_ends_where_the_sentence_does() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        assert_eq!(
            resolved("saw @person:milhouse, then @person:milhouse.", &known),
            Ok("saw @#k7h2mn, then @#k7h2mn.".to_string()),
        );
        assert_eq!(
            rendered("saw @#k7h2mn, then @#k7h2mn.", &known),
            "saw @person:milhouse, then @person:milhouse.",
        );
    }

    /// **A record the build supplies wears no badge**, so a mention of one is
    /// kept exactly as written: there is no row to rename, and its handle is as
    /// permanent as anything here.
    #[test]
    fn a_mention_of_something_wearing_no_badge_is_kept_as_written() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("view:loops", None)];
        assert_eq!(
            resolved("run @view:loops again", &known),
            Ok("run @view:loops again".to_string()),
        );
        assert_eq!(
            rendered("run @view:loops again", &known),
            "run @view:loops again",
        );
    }

    /// **The render follows the row, not the spelling.** One badge, two
    /// handles, and the same stored text reads as each in turn — which is the
    /// whole of what a mention buys.
    #[test]
    fn one_stored_mention_reads_as_whichever_handle_wears_the_badge() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let stored = "the survey went out on @#k7h2mn";
        assert_eq!(
            rendered(stored, &[thing("thing:handcart", Some("k7h2mn"))]),
            "the survey went out on @thing:handcart",
        );
        assert_eq!(
            rendered(stored, &[thing("work:handcart", Some("k7h2mn"))]),
            "the survey went out on @work:handcart",
        );
    }

    /// 🚨 **The two shapes `rendered` cannot make followable are excluded, and
    /// a genuine mention beside them is not** — the pairing is the point: a
    /// scanner that called nothing followable would pass on the negative
    /// halves alone.
    #[test]
    fn followable_finds_a_genuine_mention_and_leaves_the_two_dead_shapes_out() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        let served = rendered(
            "ask @#k7h2mn about @person:contract-zzz and about @#dead1",
            &known,
        );
        assert_eq!(
            served,
            "ask @person:milhouse about @person:contract-zzz (no such handle) and about @#dead1 (gone)",
        );

        let spans = followable(&served);
        assert_eq!(
            spans,
            vec![(4..20, EntityId("person:milhouse".into()))],
            "found: {spans:?}, in: {served:?}",
        );
    }

    /// The mention layer forwards which keys of a record the store holds off the
    /// thing.
    #[tokio::test]
    async fn the_mention_layer_forwards_the_keys_a_switched_store_holds_off_a_thing() {
        let store = Mentioning::new(std::sync::Arc::new(
            crate::memory::testing::InMemoryMemory::booted(),
        ));
        crate::memory::testing::contract::own_fields_stay_while_settings_and_computed_keys_reach(
            &store,
        )
        .await;
    }

    /// **The stored mark is jojobot's, so a caller's text may not carry one.**
    ///
    /// A mark followed by a badge reads back as the handle of whatever wears
    /// that badge, so text a caller wrote with one would be a link nothing
    /// checked. Claim words, claim details and a thing's prose are all
    /// refused, and the same sentence written as a handle is stored. A mark
    /// with no badge after it is ordinary text.
    #[tokio::test]
    async fn a_caller_cannot_write_the_stored_mark_into_text() {
        use crate::memory::{Memory, NewEntity, NewFact};
        let store = Mentioning::new(std::sync::Arc::new(
            crate::memory::testing::InMemoryMemory::booted(),
        ));
        let holder = EntityId("thing:contract-forged-holder".into());
        let target = EntityId("thing:contract-forged-target".into());
        for (id, name) in [(&holder, "Holder"), (&target, "Target")] {
            store
                .add_entity(NewEntity::new(id.clone(), name, "test"))
                .await
                .expect("add ok")
                .written()
                .expect("nothing collides");
        }
        let day = jiff::civil::date(2026, 8, 1);
        let refused = |what: &str, outcome: Result<_, crate::memory::MemoryError>| {
            assert!(
                matches!(outcome, Err(crate::memory::MemoryError::InvalidFact(_))),
                "{what} carrying the stored mark is refused: {:?}",
                outcome.map(|_: ()| ()),
            );
        };
        refused(
            "claim words",
            store
                .capture(NewFact::about(holder.clone(), "waits on @#k7h2mn", day))
                .await
                .map(|_| ()),
        );
        refused(
            "claim details",
            store
                .capture(NewFact {
                    details: Some("because of @#k7h2mn".to_string()),
                    ..NewFact::about(holder.clone(), "waits", day)
                })
                .await
                .map(|_| ()),
        );
        refused(
            "prose",
            store.set_prose(&holder, "see @#k7h2mn").await.map(|_| ()),
        );
        // **The reason a thought is dropped is text a caller writes**, and a store
        // keeps it as the details of the claim it archives, so it is held to the
        // same rule as the details of any claim.
        refused(
            "a drop reason",
            store
                .capture(NewFact {
                    drop: Some(crate::memory::FactAddress::new(
                        holder.clone(),
                        crate::memory::FactId("f1".to_string()),
                    )),
                    drop_because: Some("replaced by @#k7h2mn".to_string()),
                    ..NewFact::about(holder.clone(), "a new thought", day)
                })
                .await
                .map(|_| ()),
        );
        // **A claim written in the same act as a thing's creation or edit is a
        // claim**, so it is held to the same rule.
        refused(
            "a first claim",
            store
                .add_entity_with_first_claim(
                    crate::memory::NewEntity::new(
                        EntityId("thing:contract-forged-created".into()),
                        "Forged Created",
                        "test",
                    ),
                    NewFact::about(
                        EntityId("thing:contract-forged-created".into()),
                        "made with @#k7h2mn",
                        day,
                    ),
                )
                .await
                .map(|_| ()),
        );
        refused(
            "a claim written with an edit",
            store
                .update_entity_with_claim(
                    &holder,
                    crate::memory::EntityPatch::default(),
                    NewFact::about(holder.clone(), "edited with @#k7h2mn", day),
                )
                .await
                .map(|_| ()),
        );
        // The positives the refusals are measured against.
        store
            .capture(NewFact::about(
                holder.clone(),
                format!("waits on @{target}, mail me @# today"),
                day,
            ))
            .await
            .expect("a handle is written as a handle")
            .written()
            .expect("the claim lands");
        store
            .set_prose(&holder, "a mark with no badge, @# , is text")
            .await
            .expect("prose with a bare mark is stored");
    }

    /// **The first claim of a new thing is a caller's text too.** Creating a
    /// thing with its first claim refuses the stored mark in the claim's words
    /// and in its details, exactly as a capture does, and nothing is made. The
    /// same call with plain words lands the thing and its claim.
    #[tokio::test]
    async fn a_first_claim_cannot_carry_the_stored_mark() {
        use crate::memory::{Guarded, Memory, NewEntity, NewFact};
        let store = Mentioning::new(std::sync::Arc::new(
            crate::memory::testing::InMemoryMemory::booted(),
        ));
        let day = jiff::civil::date(2026, 8, 1);
        let listed = |id: &EntityId| {
            let store = &store;
            let id = id.clone();
            async move {
                store
                    .list_entities(None)
                    .await
                    .expect("the roster reads")
                    .iter()
                    .any(|entity| entity.id == id)
            }
        };
        for (what, id, first) in [
            (
                "claim words",
                EntityId("thing:contract-forged-holder".into()),
                NewFact::about(
                    EntityId("thing:contract-forged-holder".into()),
                    "waits on @#k7h2mn",
                    day,
                ),
            ),
            (
                "claim details",
                EntityId("thing:contract-forged-holder".into()),
                NewFact {
                    details: Some("because of @#k7h2mn".to_string()),
                    ..NewFact::about(
                        EntityId("thing:contract-forged-holder".into()),
                        "waits",
                        day,
                    )
                },
            ),
        ] {
            let outcome = store
                .add_entity_with_first_claim(NewEntity::new(id.clone(), "Forged", "test"), first)
                .await;
            assert!(
                matches!(outcome, Err(crate::memory::MemoryError::InvalidFact(_))),
                "a first claim's {what} carrying the stored mark is refused: {:?}",
                outcome.map(|_| ()),
            );
            assert!(!listed(&id).await, "a refused first claim made {id}");
        }

        let plain = EntityId("thing:contract-forged-holder".into());
        let Guarded::Written((entity, fact)) = store
            .add_entity_with_first_claim(
                NewEntity::new(plain.clone(), "Plain", "test"),
                NewFact::about(plain.clone(), "a mark with no badge, @# , is text", day),
            )
            .await
            .expect("plain words are a first claim")
        else {
            panic!("{plain} was expected to be written");
        };
        assert_eq!(entity.id, plain);
        assert!(fact.content.contains("is text"), "the claim reads back");
        assert!(listed(&plain).await, "the thing was made");
    }

    /// The handles `named` finds, as strings, so a case reads as a list.
    fn names_in(text: &str) -> Vec<String> {
        named(text)
            .into_iter()
            .map(|handle| handle.as_str().to_string())
            .collect()
    }

    /// **Text inside an inline code span is not a mention** (rule 450). A
    /// document that shows sample code holds handles that name nothing, and
    /// reading them as links would refuse the whole write. **Paired with the
    /// same handle outside code**, or a build that finds nothing anywhere
    /// passes the first half.
    #[test]
    fn a_handle_inside_an_inline_code_span_is_not_a_mention() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert!(names_in("write `@person:contract-zzz` to link").is_empty());
        assert!(names_in("write ``@person:contract-zzz`` to link").is_empty());
        assert_eq!(
            names_in("write @person:contract-zzz to link"),
            ["person:contract-zzz"],
        );
        assert_eq!(
            names_in("`@person:milhouse` and @person:contract-zzz"),
            ["person:contract-zzz"],
            "only the handle outside the span is a mention",
        );
    }

    /// **A fenced block is code, of either fence character and any length, and
    /// it closes only on a fence at least as long as the one that opened it.**
    #[test]
    fn a_handle_inside_a_fenced_block_is_not_a_mention() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        for text in [
            "before\n```\n@person:contract-zzz\n```\nafter",
            "before\n~~~\n@person:contract-zzz\n~~~\nafter",
            "before\n````rust\n@person:contract-zzz\n```\n@person:contract-zzz\n````\nafter",
            "before\n~~~~~\n@person:contract-zzz\n~~~\n```\n~~~~~\nafter",
            "before\n  ```\n  @person:contract-zzz\n  ```\nafter",
        ] {
            assert!(names_in(text).is_empty(), "{text:?}");
        }
        assert_eq!(
            names_in("```\ncode\n```\nthen @person:contract-zzz"),
            ["person:contract-zzz"],
            "a mention after the fence closed is a mention",
        );
    }

    /// **Code is left exactly as written in both directions, and a mention
    /// beside it still is not.** Stored and served are the same bytes for the
    /// code, and the real handle outside it is stored as a badge and served as
    /// the handle.
    #[test]
    fn resolving_and_rendering_leave_code_alone_and_the_rest_a_link() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        let written = "`@person:milhouse` and @person:milhouse\n```\n@person:milhouse\n```\n";
        let stored = resolved(written, &known).expect("kinds are loaded");
        assert_eq!(
            stored,
            "`@person:milhouse` and @#k7h2mn\n```\n@person:milhouse\n```\n",
        );
        assert_eq!(rendered(&stored, &known), written);
        // A handle that names nothing: marked outside code, untouched inside.
        assert_eq!(
            rendered("`@person:contract-zzz`", &known),
            "`@person:contract-zzz`",
        );
        assert_eq!(
            rendered("@person:contract-zzz", &known),
            "@person:contract-zzz (no such handle)",
        );
    }

    /// **A mention stored inside code before this rule still renders as its
    /// handle.** A handle in a code span used to be resolved like any other, so
    /// rows hold the badge there, and serving the badge is the failure this
    /// module exists to prevent. The mark is jojobot's own and is read
    /// wherever it sits; a handle a caller typed is what code leaves alone.
    #[test]
    fn a_mark_stored_inside_code_renders_as_its_handle() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let known = [thing("person:milhouse", Some("k7h2mn"))];
        assert_eq!(
            rendered("see `@#k7h2mn` and\n```\n@#k7h2mn\n```\n", &known),
            "see `@person:milhouse` and\n```\n@person:milhouse\n```\n",
        );
    }

    /// **The stored mark is a forgery in code as it is outside it**, because
    /// the render above reads it wherever it sits.
    #[test]
    fn a_stored_mark_is_a_forgery_inside_code_too() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert!(forged_mark("`@#k7h2mn`"));
        assert!(forged_mark("```\n@#k7h2mn\n```"));
        assert!(forged_mark("@#k7h2mn"));
        assert!(!forged_mark("`@person:milhouse` and no mark"));
    }

    /// **`followable` draws no link out of code.**
    #[test]
    fn followable_finds_nothing_inside_code() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert!(followable("`@person:milhouse`").is_empty());
        assert!(followable("```\n@person:milhouse\n```").is_empty());
        assert_eq!(followable("@person:milhouse").len(), 1);
    }

    /// **An unclosed fence or span is not code.** A stray fence that turned the
    /// rest of the text into code would drop every link after it without a
    /// word, so it is read as the words it is and a handle after it is still a
    /// mention, which a write guard then answers loudly if it names nothing.
    #[test]
    fn an_unclosed_fence_or_span_is_not_code() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert_eq!(
            names_in("```\n@person:contract-zzz"),
            ["person:contract-zzz"]
        );
        assert_eq!(
            names_in("a ` lone tick then @person:contract-zzz"),
            ["person:contract-zzz"],
        );
        assert_eq!(
            names_in("``double then `single` then @person:contract-zzz"),
            ["person:contract-zzz"],
            "a span closes on a run of the same length only",
        );
        // An unclosed fence does not hide a closed pair after it.
        assert!(names_in("```\ntext\n~~~\n@person:contract-zzz\n~~~").is_empty());
    }

    /// **A span does not cross a blank line**, so a lone tick cannot pair with a
    /// tick in the next paragraph and hide what lies between them.
    #[test]
    fn an_inline_span_does_not_cross_a_paragraph() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert_eq!(
            names_in("a `tick\n\n@person:contract-zzz then another` tick"),
            ["person:contract-zzz"],
        );
        assert!(names_in("a `tick\n@person:contract-zzz\nstill the span` ends").is_empty());
    }

    /// **An indented line is not code.** It cannot be told cheaply from the
    /// continuation of a list item, and skipping it would hide a real mention.
    #[test]
    fn an_indented_line_is_not_code() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert_eq!(
            names_in("a list:\n\n    @person:contract-zzz"),
            ["person:contract-zzz"],
        );
    }
}
