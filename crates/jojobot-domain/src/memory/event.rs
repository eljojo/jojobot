//! **The event record — layer 1, and the whole of it that exists yet.**
//!
//! An event is a fact with a date and a marker: a fact is current truth and is
//! rewritten in place, an event is chronology and stays put. What makes one an
//! event is this record riding along with it.
//!
//! **Permissive and TOTAL, which is the design rather than a stage of it.** An
//! event always reads as a type NAME, a flat bag of metadata, and a list of
//! entity references — and a read NEVER fails because the reader does not know
//! the type, or because a field is there that it did not expect. There are no
//! native types, no typed fields and no schema; everything an agent records
//! comes through the open hatch, and what the real types eventually are will be
//! derived from what accumulates here. That is why the reader must not be
//! allowed to have opinions yet: it is going to be wrong about the shape, and
//! being wrong must cost nothing.
//!
//! **The typed projection is deliberately absent.** It reads this record; it
//! does not replace it, and nothing here should make it awkward to add.
//!
//! # Why the round trip is the load-bearing test
//!
//! A reader that quietly drops what it does not recognize is worse than one
//! that refuses: the loss happens on the NEXT WRITE, long after the read that
//! caused it, and nothing anywhere reports it. "A lossy record that can be
//! translated later" is only a plan if the record survives being read by
//! something that does not understand it — so [`Event::render`] of
//! [`Event::parse`] is byte-identical, including for a record carrying fields
//! this build has never heard of.
//!
//! Unknown fields are not a special case here, and that is on purpose: the bag
//! is flat `String -> String`, so an unrecognized key is simply a key. There is
//! no branch that could forget them because there is no branch.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::EntityId;

/// The metadata key reserved for the type name.
///
/// **Reserved in the code and not merely in this comment.** It used to say
/// "reserved rather than forbidden: it round-trips like any other key" and
/// nothing enforced it — a caller's `type` metadata rendered a second type
/// token, and the reader took the last one, so the event's real type was
/// destroyed and the key vanished with it. See [`reserved_key`].
pub const TYPE: &str = "type";

/// A reference to an entity, in the payload's own grammar. Reserved exactly as
/// [`TYPE`] is, and for the same reason.
pub const REF: &str = "ref";

/// **The one type name jojobot writes itself.** Everything else in this bag is
/// the caller's own words; this is not, and it is the exception that has to be
/// legible as one when the real types are eventually derived from what
/// accumulated here.
///
/// It earns the exception by being an event in its own right: taking something
/// back is a thing that happened, on a day, for a reason, and a record of it is
/// the only honest way to say so without editing the record it takes back.
pub const RETRACTION: &str = "retraction";

/// The key on a retraction record naming what it retracts — a fact ADDRESS,
/// not a handle, so it is deliberately not a walkable link: the target is a
/// row, and rows are reached by address.
const RETRACTS: &str = "retracts";

/// **Whether a metadata key is one the grammar has already spent.**
///
/// The bag is flat and free, with exactly two exceptions: the tokens this
/// record's own line format uses. A caller passing one of them is not
/// describing an event, it is writing in the grammar — and the write that
/// results destroys the field it collides with rather than the caller's key.
///
/// Refused rather than escaped or renamed. Renaming it would hand back a
/// record the caller did not write, and the type of an event is derived from
/// what accumulates here, so a silently moved key is a corrupted sample.
pub fn reserved_key(key: &str) -> bool {
    matches!(key.trim(), TYPE | REF)
}

/// **What an event is, as a record.** Not what it MEANS — nothing here
/// interprets a type name, and jojobot never guesses one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// The type NAME, free text, required, never interpreted. Ungated on
    /// purpose: a gate that every caller is trained to walk through is worse
    /// than no gate, and refusing a type only means something once there are
    /// real types to refuse against.
    pub kind: String,
    /// The flat bag. **Sorted**, because byte-identity on the way back out is
    /// what makes an unknown field survive a reader that does not know it.
    pub metadata: BTreeMap<String, String>,
    /// The entities this event points at. Links whose nature is deferred — see
    /// [`super::EdgeShape::Connection`] for why that is not the same as `about`
    /// and must not be collapsed into it.
    pub refs: Vec<EntityId>,
}

impl Event {
    /// **Every entity this record points at — whatever key it sits under.**
    ///
    /// What makes a payload value a walkable reference is that it IS an entity
    /// handle, not the key it happens to have. `ref=person:alpha` is the
    /// unnamed case, used when there is nothing to call the relationship; a
    /// later `mechanic=person:alpha` is the same link with the key doing the
    /// annotating. **The key is the annotation** — so a projection keyed on the
    /// literal word `ref` would silently miss every named reference the day the
    /// first real type ships, and miss it invisibly, because nothing today has
    /// a named field to notice with.
    ///
    /// That is also the boundary: a key annotates a link cheaply, and it does
    /// not make the link a place to keep things. An edge growing its own fields
    /// is a node that has not admitted it yet.
    ///
    /// Deduplicated and ordered, so two spellings of the same answer cannot
    /// come back as two answers.
    pub fn linked(&self) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = self.refs.clone();
        found.extend(
            self.metadata
                .values()
                .map(|v| EntityId(v.trim().to_string()))
                .filter(|id| super::validate_subject(id).is_ok()),
        );
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        found.dedup();
        found
    }

    /// An event of this type, with nothing else recorded.
    pub fn of(kind: &str) -> Self {
        Event {
            kind: kind.trim().to_string(),
            metadata: BTreeMap::new(),
            refs: Vec::new(),
        }
    }

    /// The record of a retraction: what it takes back, named by address.
    ///
    /// **The link is written here, by jojobot, and never by a caller** — a
    /// retraction that pointed wherever its author said would be a way to mark
    /// somebody else's record taken back.
    pub fn retraction_of(target: &str) -> Self {
        Event {
            metadata: [(RETRACTS.to_string(), target.to_string())]
                .into_iter()
                .collect(),
            ..Event::of(RETRACTION)
        }
    }

    /// Whether this record IS a retraction — the question [`Fact`] asks to
    /// refuse retracting one, so the answer lives with the grammar that spells
    /// it rather than being re-derived from a string comparison elsewhere.
    ///
    /// [`Fact`]: super::Fact
    pub fn is_retraction(&self) -> bool {
        self.kind == RETRACTION
    }

    /// The address this retraction takes back, if it is one and says so.
    pub fn retracts(&self) -> Option<&str> {
        self.is_retraction()
            .then(|| self.metadata.get(RETRACTS).map(String::as_str))
            .flatten()
    }

    /// The record as one line of text — **deterministic**, so the same record
    /// always renders the same bytes.
    ///
    /// The grammar is the edges cell's, generalized: space-separated `key=value`
    /// with the value escaped, `type` first because a reader wants it first, and
    /// `ref` repeated once per reference. Ordering is not cosmetic: it is what
    /// makes the round trip byte-identical rather than merely equivalent.
    ///
    /// The space and the `=` in here are **structure**: [`escape`] guarantees
    /// neither can appear inside a key or a value, so splitting needs no
    /// lookahead and a value can never break out of its own token.
    pub fn render(&self) -> String {
        let mut out = vec![format!("{TYPE}={}", escape(&self.kind))];
        for (key, value) in &self.metadata {
            out.push(format!("{}={}", escape(key), escape(value)));
        }
        for object in &self.refs {
            out.push(format!("{REF}={}", escape(object.as_str())));
        }
        out.join(" ")
    }

    /// Read a record back. **Total on anything with a type**: an unrecognized
    /// key is kept as metadata, because the alternative is a reader deciding
    /// which of the operator's records deserve to survive it.
    ///
    /// `None` only where there is no event at all — an empty cell, or one with
    /// no `type`. That is absence, not failure: a fact without this payload is
    /// an ordinary fact, which is the common case and not an error.
    pub fn parse(cell: &str) -> Option<Self> {
        let mut kind: Option<String> = None;
        let mut metadata = BTreeMap::new();
        let mut refs = Vec::new();
        for token in cell.split_whitespace() {
            let Some((key, value)) = token.split_once('=') else {
                continue;
            };
            let (key, value) = (unescape(key), unescape(value));
            match key.as_str() {
                TYPE => kind = Some(value),
                REF => refs.push(EntityId(value)),
                _ => {
                    metadata.insert(key, value);
                }
            }
        }
        Some(Event {
            kind: kind?,
            metadata,
            refs,
        })
    }
}

/// Whether a character may ride into the record raw.
///
/// **An allowlist, and the reason it is one is empirical.** This record lands
/// in a markdown table cell in a store that rewrites markdown on every save,
/// and a denylist can only name the rewrites somebody thought to look for. The
/// real store was found doing two of them: it re-serializes a literal
/// backslash as `\\`, and it INSERTS an escape of its own in front of
/// characters it reads as syntax (`~` became `\~` with nobody asking). The
/// first corrupted every escape sequence this module wrote; the second
/// corrupts characters it never touched at all.
///
/// So nothing rides raw on the strength of an argument about what markdown
/// means. Letters and digits do — including non-ASCII ones, so an ordinary
/// word stays an ordinary word on a page the operator reads — plus four
/// punctuation marks that carry the handles this record is mostly made of
/// (`kind:slug`, `some-slug`, a date, a path). Everything else is encoded,
/// including characters that are probably fine: probably-fine is what a
/// polite fake tells you.
fn rides_raw(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '.' | ':' | '/')
}

/// Escape a value into one token that survives both this grammar and the
/// store under it.
///
/// **Percent-encoding, not backslashes**, and the swap is not cosmetic. The
/// grammar needs the space and the `=` back — they separate tokens and split a
/// key from its value — and it used a backslash to buy them. A backslash is
/// the one character a markdown store is guaranteed to have opinions about, so
/// the escape mechanism was the thing that could not survive being stored. A
/// `%` is inert everywhere the record travels, and the encoding is
/// self-terminating: two hex digits, no lookahead, nothing a later character
/// can change the meaning of.
///
/// Bytes rather than chars, so a multi-byte character encodes and decodes as
/// exactly the bytes it is made of.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if rides_raw(c) {
            out.push(c);
            continue;
        }
        let mut buf = [0u8; 4];
        for byte in c.encode_utf8(&mut buf).as_bytes() {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The inverse of [`escape`]. A `%` that does not begin a well-formed pair is
/// kept **as written** rather than dropped: losing a byte to tidy up a
/// malformed one is the failure this whole module is against, and a stray `%`
/// is exactly what a hand edit leaves behind.
///
/// Decoded as BYTES and validated once at the end, because a multi-byte
/// character arrives as several pairs and decoding them one at a time cannot
/// see the character they spell. A sequence that is not valid UTF-8 falls back
/// to the token as written — mangled input comes back mangled rather than
/// silently becoming a replacement glyph.
fn unescape(token: &str) -> String {
    let raw = token.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let pair = (raw[i] == b'%')
            .then(|| raw.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match pair {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(raw[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(kind: &str, pairs: &[(&str, &str)], refs: &[&str]) -> Event {
        Event {
            kind: kind.to_string(),
            metadata: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            refs: refs.iter().map(|r| EntityId((*r).to_string())).collect(),
        }
    }

    /// **The load-bearing one.** Render, parse, render again — byte-identical,
    /// including for a record carrying fields this build has never heard of.
    ///
    /// Without this, "a lossy record that can be translated later" is a slogan.
    /// The failure it catches is not a read that errors — it is a read that
    /// silently succeeds having dropped something, so the loss lands on the
    /// NEXT WRITE, long after the read that caused it, with nothing anywhere
    /// reporting it.
    #[test]
    fn a_record_survives_a_reader_that_does_not_understand_it() {
        let strange = recorded(
            "some-type-from-a-later-build",
            &[
                ("a-field-this-build-never-heard-of", "and its value"),
                ("mood", "delighted"),
                ("nested-looking", "{\"not\":\"parsed\"}"),
                ("punctuation", "spaces, = signs, and a \\ backslash"),
            ],
            &["person:alpha", "place:north-trail"],
        );

        let once = strange.render();
        let back = Event::parse(&once).expect("a record with a type reads");
        assert_eq!(back, strange, "the record itself survived");
        assert_eq!(
            back.render(),
            once,
            "…and re-renders to the same bytes, which is what the next write puts back"
        );
        // Named individually, because "equal" can hide a key that was dropped
        // and re-added by a default somewhere.
        assert_eq!(
            back.metadata.get("a-field-this-build-never-heard-of"),
            Some(&"and its value".to_string()),
            "an unknown field is kept as written: {back:?}"
        );
    }

    /// **A type nobody knows is still a record.** The invariant rule 99 states
    /// outright: a read never fails because of a type the reader does not know.
    /// This build knows no types at all, which is the point — it must not
    /// acquire an opinion later without somebody deciding to give it one.
    #[test]
    fn a_type_this_build_has_never_seen_reads_anyway() {
        let read = Event::parse("type=a-type-invented-next-year mood=curious")
            .expect("an unknown type is a record, not a failure");
        assert_eq!(read.kind, "a-type-invented-next-year");
        assert_eq!(read.metadata.get("mood"), Some(&"curious".to_string()));
    }

    /// Absence is not failure: a fact with no payload is an ordinary fact, which
    /// is the common case and must not read as a broken event.
    #[test]
    fn a_cell_with_no_type_is_no_event_rather_than_a_broken_one() {
        for empty in ["", "   ", "mood=curious", "not-even-a-pair"] {
            assert_eq!(
                Event::parse(empty),
                None,
                "{empty:?} is a fact without an event, not a damaged one"
            );
        }
    }

    /// **What makes a payload value an edge is the VALUE, not the key.**
    ///
    /// `ref` is the unnamed member of a family, not the only member: when a
    /// type eventually names its fields, `mechanic=person:x` is the same link
    /// with the key doing the annotating. A projection that keyed on the
    /// literal word `ref` would work perfectly today — nothing has named fields
    /// yet — and would silently drop every named reference the day one ships.
    /// That invisibility is the whole reason this is asserted now.
    #[test]
    fn any_value_that_is_a_handle_is_a_link_whatever_its_key() {
        let recorded_event = recorded(
            "a-thing-that-happened",
            &[
                // The named case, which does not exist yet and must still work.
                ("mechanic", "person:milhouse"),
                // Ordinary metadata: not a handle, so not a link.
                ("mood", "delighted"),
                ("count", "3"),
                // Shaped like a handle but not a well-formed one.
                ("nearly", "person:"),
            ],
            &["place:north-trail"],
        );

        assert_eq!(
            recorded_event.linked(),
            vec![
                EntityId("person:milhouse".into()),
                EntityId("place:north-trail".into()),
            ],
            "the named field links exactly as the unnamed one does"
        );
    }

    /// The same entity named twice — once unnamed, once under a key — is one
    /// link, not two. A reader asking what touches an entity must not have to
    /// dedupe an answer.
    #[test]
    fn one_entity_named_twice_is_one_link() {
        let twice = recorded(
            "a-thing",
            &[("mechanic", "person:milhouse")],
            &["person:milhouse"],
        );
        assert_eq!(twice.linked(), vec![EntityId("person:milhouse".into())]);
    }

    /// **The rendered record is made of characters the store cannot rewrite.**
    ///
    /// The property the round-trip tests cannot see: they hand the record
    /// straight back to this module, so a grammar that is byte-perfect here and
    /// unstorable in production passes every one of them. It did. The previous
    /// grammar escaped with backslashes, real Outline re-serialized every one
    /// of them as `\\`, and the first event ever written through the real store
    /// came back with `a\sb` where the operator had typed `a b`.
    ///
    /// So this asserts the shape of the OUTPUT rather than the round trip: only
    /// the allowlist reaches the page. It is the one test in this module that
    /// would have failed before the store was ever called.
    #[test]
    fn a_rendered_record_carries_only_characters_that_survive_a_markdown_store() {
        let nasty = recorded(
            "a type with spaces",
            &[
                ("equation", "a = b"),
                ("path", "c:\\dir"),
                // The two the real store rewrote, one it inserted an escape in
                // front of, and a sample of the punctuation nobody has probed.
                ("tilde", "a~b~c"),
                ("emphasis", "_underscored_ and *starred*"),
                ("markup", "<b>bold</b> & 'quoted'"),
                ("unicode", "café — ünïcode ✓"),
                ("empty", ""),
            ],
            &["person:alpha", "place:north-trail"],
        );

        // **The safe set is written out here, not read off `rides_raw`.** An
        // assertion phrased in terms of the allowlist it is guarding is not an
        // assertion: widening the allowlist widens the test in lockstep, and
        // adding one character to `rides_raw` passed this test while putting
        // that character straight onto the page. So the alphabet is named
        // literally, and the only way to widen what ships is to widen it here,
        // in a diff a reviewer reads as exactly what it is.
        //
        // Letters and digits by Rust's own definition (`café` stays `café`),
        // the four punctuation marks handles are made of, the two structural
        // characters, and `%` — which is the escape and therefore the point.
        let safe = |c: char| c.is_alphanumeric() || "-.:/ =%".contains(c);

        let rendered = nasty.render();
        let stray: Vec<char> = rendered.chars().filter(|c| !safe(*c)).collect();
        assert!(
            stray.is_empty(),
            "these reached the page raw, and the store gets a say in every one of them: \
             {stray:?} in {rendered}"
        );
        // …and it is still the caller's text underneath.
        assert_eq!(Event::parse(&rendered).as_ref(), Some(&nasty));
        assert_eq!(
            Event::parse(&rendered).expect("reads").render(),
            rendered,
            "byte-identical, punctuation and all"
        );
    }

    /// A `%` that begins nothing is kept as written — this module writes no
    /// such token itself, but a hand edit leaves them behind and losing a byte
    /// to tidy one up is the failure the whole module is against.
    #[test]
    fn a_malformed_escape_survives_as_the_bytes_it_is() {
        for (token, expected) in [
            ("100%", "100%"),
            ("%zz", "%zz"),
            ("%", "%"),
            ("%2", "%2"),
            // The first `%` begins nothing and stays; the second is a real pair.
            ("a%%20b", "a% b"),
        ] {
            let read = Event::parse(&format!("type=t v={token}")).expect("reads");
            assert_eq!(
                read.metadata.get("v").map(String::as_str),
                Some(expected),
                "a malformed escape is data, not an error: {token}"
            );
        }
    }

    /// The characters that would otherwise break the grammar survive it — a
    /// record is the operator's text, and a value with a space or an `=` in it
    /// is ordinary rather than exotic.
    #[test]
    fn a_value_carrying_the_grammars_own_characters_round_trips() {
        let awkward = recorded(
            "a type with spaces",
            &[("equation", "a = b"), ("path", "c:\\\\dir"), ("empty", "")],
            &[],
        );
        let rendered = awkward.render();
        // **One token per field, whatever is inside the values.** A value that
        // leaked a raw space would still round-trip by luck on some inputs and
        // silently split a neighbour's on others, so the count is asserted
        // rather than the absence of any particular character — an empty value
        // legitimately ends its token with the separator.
        assert_eq!(
            rendered.split_whitespace().count(),
            1 + awkward.metadata.len() + awkward.refs.len(),
            "a value broke out of its own token: {rendered}"
        );
        assert_eq!(Event::parse(&rendered).as_ref(), Some(&awkward));
        assert_eq!(
            Event::parse(&rendered).expect("reads").render(),
            rendered,
            "byte-identical, punctuation and all"
        );
    }
}
