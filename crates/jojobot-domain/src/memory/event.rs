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

/// The metadata key reserved for the type name. Reserved rather than forbidden:
/// it round-trips like any other key, it just has a field of its own.
const TYPE: &str = "type";

/// A reference to an entity, in the payload's own grammar.
const REF: &str = "ref";

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
    /// An event of this type, with nothing else recorded.
    pub fn of(kind: &str) -> Self {
        Event {
            kind: kind.trim().to_string(),
            metadata: BTreeMap::new(),
            refs: Vec::new(),
        }
    }

    /// The record as one line of text — **deterministic**, so the same record
    /// always renders the same bytes.
    ///
    /// The grammar is the edges cell's, generalized: space-separated `key=value`
    /// with the value escaped, `type` first because a reader wants it first, and
    /// `ref` repeated once per reference. Ordering is not cosmetic: it is what
    /// makes the round trip byte-identical rather than merely equivalent.
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

/// Escape a value into one whitespace-free token.
///
/// Four characters cannot ride raw: the backslash that does the escaping, the
/// space that separates tokens, the `=` that separates a key from its value,
/// and a newline. Everything else is left exactly as the caller wrote it — a
/// record is the operator's text, not this module's.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ' ' => out.push_str("\\s"),
            '=' => out.push_str("\\e"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// The inverse of [`escape`]. A trailing lone backslash, or an escape this
/// build does not know, is kept **as written** rather than dropped: losing a
/// byte to tidy up a malformed one is the failure this whole module is against.
fn unescape(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('s') => out.push(' '),
            Some('e') => out.push('='),
            Some('n') => out.push('\n'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
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
