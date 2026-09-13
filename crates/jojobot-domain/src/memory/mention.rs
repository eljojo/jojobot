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

use super::{Entity, EntityId, kinds};

/// **What marks a resolved mention in stored text.**
///
/// Two characters that no handle can start with, so a stored mention can never
/// be read as one somebody typed. **Pinned by a test rather than derived**: the
/// spelling is STORED and nothing outside this process declares it, so a change
/// to it is a change every row written before it disagrees with — and a row
/// cannot fail a test.
pub const MARK: &str = "@#";

/// Every entity an author named in this text, in the order they wrote them.
///
/// **Shape and kind only.** Whether the handle names anything is the caller's
/// question, asked against the store: this says what was written, and a write
/// guard says whether it is there.
pub fn named(text: &str) -> Vec<EntityId> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices('@') {
        // **A stored mark needs no skip of its own.** What follows it is a
        // badge rather than a kind, so the read below turns it down for the
        // reason it turns down any other word.
        if let Some((handle, _)) = handle_at(text, at) {
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
pub fn resolved(text: &str, known: &[Entity]) -> String {
    rewrite(text, |at| {
        let (handle, end) = handle_at(text, at)?;
        let badge = known
            .iter()
            .find(|e| e.id == handle)?
            .badge
            .as_deref()
            .filter(|badge| !badge.is_empty())?;
        Some((format!("{MARK}{badge}"), end))
    })
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
    rewrite(text, |at| {
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
        let (handle, end) = handle_at(text, at)?;
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
    for (at, _) in text.match_indices('@') {
        // **A stored mark parses as no handle at all**, so it is already
        // excluded — `handle_at` needs `kind:` right after the `@`, and `#`
        // is never that.
        let Some((handle, end)) = handle_at(text, at) else {
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

/// Walk every `@` in the text and let `at` decide what replaces it.
///
/// **One walk for both directions**, so the two can never disagree about where
/// a mention starts or ends.
fn rewrite(text: &str, at: impl Fn(usize) -> Option<(String, usize)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cut = 0;
    for (start, _) in text.match_indices('@') {
        if start < cut {
            continue;
        }
        let Some((replacement, end)) = at(start) else {
            continue;
        };
        out.push_str(&text[cut..start]);
        out.push_str(&replacement);
        cut = end;
    }
    out.push_str(&text[cut..]);
    out
}

/// The handle written at `at`, and where it ends.
///
/// **A kind this instance does not hold is not a mention at all**, so `@` in
/// front of an ordinary word is ordinary text. That is the discriminator: a
/// mention names a kind, and the set of kinds is data this process loaded.
fn handle_at(text: &str, at: usize) -> Option<(EntityId, usize)> {
    let rest = &text[at + 1..];
    let (kind, _) = rest.split_once(':')?;
    if kinds::resolve(kind).is_err() {
        return None;
    }
    // **The run of slug bytes, whole.** A trailing hyphen is not trimmed: a
    // slug may end in one, so trimming would read a mention as naming a
    // different thing than the author wrote.
    let slug = run_of(text, at + 1 + kind.len() + 1, is_slug_byte);
    if slug.is_empty() {
        return None;
    }
    let end = at + 1 + kind.len() + 1 + slug.len();
    Some((EntityId(format!("{kind}:{slug}")), end))
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
    inner: std::sync::Arc<dyn super::Memory>,
}

impl Mentioning {
    pub fn new(inner: std::sync::Arc<dyn super::Memory>) -> Self {
        Mentioning { inner }
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
    /// Shared by [`Self::render_fact`], over a fact's own fields, and by
    /// [`Memory::fields`](super::Memory::fields), over a thing's folded
    /// fields — both are `BTreeMap<String, String>`, and the same
    /// reference-typed key holds a plain handle in either. **Both need this,
    /// or `graph::walk`'s thing-scope filter and record-scope filter compare
    /// today's handle against two different maps agreeing on every key but
    /// this one.**
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
    fn screen<T>(text: &[&str], known: &[Entity]) -> Option<super::Guarded<T>> {
        for part in text {
            for handle in named(part) {
                if known.iter().any(|e| e.id == handle) {
                    continue;
                }
                let candidates = super::guard::screen(&handle, &[], known);
                return Some(super::Guarded::Blocked {
                    attempted: handle,
                    candidates,
                });
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
    /// **Resolved on the way in.** Every handle an author wrote becomes the
    /// badge its row wears, so the claim keeps a pointer rather than a
    /// spelling.
    async fn capture(
        &self,
        fact: super::NewFact,
    ) -> Result<super::Guarded<super::Fact>, super::MemoryError> {
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
                content: resolved(&fact.content, &known),
                details: fact.details.as_deref().map(|d| resolved(d, &known)),
                ..fact
            })
            .await?;
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
        let mut facts = self.inner.recall(subject).await?;
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
    ) -> Result<super::Guarded<super::Fact>, super::MemoryError> {
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
                    content: patch.content.as_deref().map(|c| resolved(c, &known)),
                    details: patch.details.as_deref().map(|d| resolved(d, &known)),
                    ..patch
                },
            )
            .await?;
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
        let mut writes = self.inner.history(entity, key).await?;
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
        let mut chain = self.inner.claim_history(address).await?;
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
    /// **Resolved exactly as a fact's own fields are.** A thing-scope filter
    /// compares this map, and a record-scope filter compares a resolved
    /// fact's — both need to see today's handle under a reference-typed key,
    /// or the two scopes disagree about the same key on the same thing.
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, super::MemoryError> {
        let mut fields = self.inner.fields(entity).await?;
        if fields.is_empty() {
            return Ok(fields);
        }
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        Self::render_fields(&mut fields, &known, &former, &declared);
        Ok(fields)
    }
    /// **A key's backing carries the note its record carried**, the same
    /// text under the same name [`Self::history`] already renders — read
    /// through a different door, which is why a bare delegate here missed
    /// it.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, super::FieldBacking>, super::MemoryError> {
        let mut backing = self.inner.backing(entity).await?;
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
    ) -> Result<super::Retraction, super::MemoryError> {
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        let reason = reason.map(|r| resolved(r, &known));
        let mut done = self.inner.retract(address, reason.as_deref(), date).await?;
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
    ) -> Result<super::Merge, super::MemoryError> {
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        let reason = reason.map(|r| resolved(r, &known));
        let mut done = self
            .inner
            .merge(folded, survivor, reason.as_deref(), date)
            .await?;
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
        let known = self.known().await?;
        let stored = self
            .inner
            .set_prose(entity, &resolved(prose, &known))
            .await?;
        Ok(rendered(&stored, &known))
    }
    /// **The index scans through here**, so what search holds is what a reader
    /// sees: a claim mentioning a thing is findable by that thing's handle
    /// rather than by a badge nobody types.
    async fn scan(&self) -> Result<Vec<super::search::DocScan>, super::MemoryError> {
        let mut scanned = self.inner.scan().await?;
        let known = self.known().await?;
        let former = self.former().await?;
        let declared = self.declared().await?;
        for doc in &mut scanned {
            self.render_scan(doc, &known, &former, &declared);
        }
        Ok(scanned)
    }

    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<super::search::DocScan>, super::MemoryError> {
        let Some(mut doc) = self.inner.scan_entity(entity).await? else {
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
        let mut facts = self.inner.built_on(source).await?;
        self.render_facts(&mut facts).await?;
        Ok(facts)
    }

    async fn referring_to(
        &self,
        target: &EntityId,
    ) -> Result<Vec<super::Fact>, super::MemoryError> {
        let mut facts = self.inner.referring_to(target).await?;
        self.render_facts(&mut facts).await?;
        Ok(facts)
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
            "ask @#k7h2mn about it",
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
        assert_eq!(resolved(plain, &known), plain);
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
            "saw @#k7h2mn, then @#k7h2mn.",
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
            "run @view:loops again",
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
        let served = rendered("ask @#k7h2mn about @person:zzz and about @#dead1", &known);
        assert_eq!(
            served,
            "ask @person:milhouse about @person:zzz (no such handle) and about @#dead1 (gone)",
        );

        let spans = followable(&served);
        assert_eq!(
            spans,
            vec![(4..20, EntityId("person:milhouse".into()))],
            "found: {spans:?}, in: {served:?}",
        );
    }
}
