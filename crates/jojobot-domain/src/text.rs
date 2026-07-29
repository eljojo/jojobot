//! Text fitted to a field — one engine, and a **named strategy per call site**.
//!
//! Several places in this system take prose somebody wrote and put it somewhere
//! narrow: a card's title, a session's focus line, the outcome record on a
//! retired message. They all need the same mechanics — flatten to one line, cut
//! on a word boundary, say that you cut — and they had all grown their own
//! copy, which is how three call sites came to disagree about something as
//! small as whether the ellipsis counts against the budget without anybody
//! deciding that they should.
//!
//! The copies are gone; **the disagreements are not**, because they are real.
//! Each call site declares a [`Fitted`] naming exactly what its field can
//! carry, and the differences between them are now written down in one place
//! where they can be compared, rather than inferred by diffing three functions.
//!
//! **Most of these outputs are stored bytes.** A message's title sits on a page
//! and a focus sits in a session's row, so changing what those strategies
//! produce rewrites records that already exist — each is pinned by a golden at
//! its own call site, and a change to the rules has to go there and say so.
//! [`BODY_DIGEST`] is the exception: it is computed per response and stored
//! nowhere, so its golden sits here beside it rather than at a call site.

/// Whether the ellipsis a cut adds is counted against the budget.
///
/// **A real disagreement between call sites, kept rather than reconciled.** The
/// focus line's budget is the field's whole capacity — the store refuses 201
/// characters, so the ellipsis has to fit inside the 200. A card title's budget
/// is how much text is worth showing, and the ellipsis rides on top. Picking
/// one and applying it to both would have been the tidier-looking change and
/// would have rewritten stored titles on two boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ellipsis {
    /// The budget is the field's capacity: text is cut to `budget - 1` so the
    /// ellipsis fits inside it.
    WithinBudget,
    /// The budget is how much text to keep: the ellipsis is added beyond it.
    BeyondBudget,
}

/// One field's rules for taking prose and making it fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fitted {
    /// What this strategy is for. Carried so a failure names the call site
    /// rather than a line number in here.
    pub name: &'static str,
    /// How many characters the field holds — see [`Ellipsis`] for whether that
    /// includes the one a cut adds.
    pub budget: usize,
    /// Where the ellipsis is counted from.
    pub ellipsis: Ellipsis,
    /// Whether runs of whitespace collapse to one line first.
    ///
    /// Off for a field whose input is **already** one line and whose internal
    /// spacing is the writer's: an outcome record says what a person typed, and
    /// collapsing their double spaces would be an edit nobody asked for.
    pub flatten: bool,
    /// Whether backticks and control characters are removed.
    ///
    /// On only where the text rides **above a fenced machine block**: a
    /// backtick there can close the fence and turn the block into prose, which
    /// is a corrupt card rather than an ugly one.
    pub strip_unprintable: bool,
    /// What to render when nothing survives. `None` renders the empty string —
    /// right for a field that is a fragment of a longer line, wrong for one
    /// that is the whole of what a reader sees.
    pub when_empty: Option<&'static str>,
}

impl Fitted {
    /// Fit `text` to this field.
    pub fn render(&self, text: &str) -> String {
        let flat = if self.flatten {
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        } else {
            text.to_string()
        };
        let flat: String = if self.strip_unprintable {
            flat.chars()
                .filter(|c| *c != '`' && !c.is_control())
                .collect()
        } else {
            flat
        };
        let flat = flat.trim();

        if flat.is_empty()
            && let Some(fallback) = self.when_empty
        {
            return fallback.to_string();
        }
        if flat.chars().count() <= self.budget {
            return flat.to_string();
        }

        let room = match self.ellipsis {
            // Saturating because `Fitted` is public with public fields: a
            // budget of zero is no strategy anyone here declares, and it must
            // still not be an arithmetic panic in a library type.
            Ellipsis::WithinBudget => self.budget.saturating_sub(1),
            Ellipsis::BeyondBudget => self.budget,
        };
        let mut kept = String::new();
        for word in flat.split(' ') {
            if kept.chars().count() + word.chars().count() + 1 > room {
                break;
            }
            if !kept.is_empty() {
                kept.push(' ');
            }
            kept.push_str(word);
        }
        // A single word longer than the whole budget has no boundary to cut on.
        if kept.is_empty() {
            kept = flat.chars().take(room).collect();
        }
        format!("{kept}…")
    }
}

/// The focus a session gets when it materializes with nothing better to say.
pub const FRESH_FOCUS: &str = "working";

/// **A session's focus line** — what the card says it is working on now.
///
/// The one strategy that strips, because this line rides above the card's
/// fenced machine block. Its budget is the store's hard limit, so the ellipsis
/// has to fit inside it, and it falls back rather than rendering blank: a focus
/// is the whole of what a reader sees at a glance.
pub const FOCUS_LINE: Fitted = Fitted {
    name: "focus-line",
    budget: 200,
    ellipsis: Ellipsis::WithinBudget,
    flatten: true,
    strip_unprintable: true,
    when_empty: Some(FRESH_FOCUS),
};

/// **The head of a message card's title** — the subject, or the opening of the
/// body when the poster declared none.
///
/// No fallback: this is a fragment after `"<sender>: "`, so a message whose
/// head is empty still renders a title that says who it is from.
pub const MESSAGE_TITLE: Fitted = Fitted {
    name: "message-title",
    budget: 60,
    ellipsis: Ellipsis::BeyondBudget,
    flatten: true,
    strip_unprintable: false,
    when_empty: None,
};

/// **The opening of a body, for an answer that is not shipping the body back.**
///
/// Enough to recognize which message this is beside its byte count, which is
/// what an author verifying their own write actually needs — they wrote the
/// body, so sending it back to them is the one reader it teaches nothing.
pub const BODY_DIGEST: Fitted = Fitted {
    name: "body-digest",
    budget: 120,
    ellipsis: Ellipsis::WithinBudget,
    flatten: true,
    strip_unprintable: false,
    when_empty: None,
};

/// **An outcome record** on a retired message — what a consumer says happened.
///
/// Does not flatten: the caller's spacing is theirs, and the field is validated
/// to be one line already. Generous, and a cut rather than a refusal, because
/// the crash contract asks for this record and a cap that rejected the call
/// destroyed the very thing it was policing.
pub const OUTCOME_NOTES: Fitted = Fitted {
    name: "outcome-notes",
    budget: 2000,
    ellipsis: Ellipsis::WithinBudget,
    flatten: false,
    strip_unprintable: false,
    when_empty: None,
};

/// **Whether a cell came back as the same VALUE, allowing for the store
/// rewriting it.**
///
/// The read-back guard asserts that what was stored reads back as what was
/// written. That is right, and comparing bytes is the wrong way to ask it: the
/// store is a markdown editor, it re-serializes what it saves, and
/// re-serialization is not identity. It escapes what it reads as syntax —
/// measured, not assumed: a tilde, a leading `#`, a leading `-`, `<`, `\`,
/// `|`, emphasis marks, and jojobot's own `-` placeholder, which comes back
/// `\-`.
///
/// So four writes that SUCCEEDED were refused in production, and every byte of
/// real damage came from rolling them back: orphaned bodies, consumed ids, a
/// hand repair, a partial commit behind a flat error. The guard was firing on
/// successes.
///
/// **Cells only.** Fenced content — a message body, a chronology entry — comes
/// back from the store byte-identical, measured across two rails and twelve
/// adversarial cases, so it stays byte-exact and gets nothing. The axis is
/// where a value SITS, not what it is: a subject is prose and lives in a cell.
///
/// **What it forgives is a backslash the store put in front of punctuation,
/// and nothing else.** A dropped character, a changed word, a truncation, a
/// swapped value: all still fail, which is the half of the guard that has
/// twice caught real data loss.
pub fn same_cell_value(wrote: &str, read: &str) -> bool {
    fn without_added_escapes(cell: &str) -> String {
        let mut out = String::with_capacity(cell.len());
        let mut chars = cell.chars().peekable();
        while let Some(c) = chars.next() {
            // A backslash before punctuation is the store's; a backslash
            // before anything else — a letter, a digit, the end of the cell —
            // is the writer's and stays.
            if c == '\\'
                && chars
                    .peek()
                    .is_some_and(|n| !n.is_alphanumeric() && !n.is_whitespace())
            {
                continue;
            }
            out.push(c);
        }
        out
    }
    /// **One named case, and the goldens are its floor.** The store respells
    /// underscore-emphasis as asterisk-emphasis: `_under_` comes back
    /// `*under*`. Nothing is escaped and nothing is lost — it is the same
    /// emphasis run wearing the other marker.
    ///
    /// **Intraword underscores are left alone, and that is measured rather
    /// than assumed.** `parse_bodies` comes back untouched, so a `_` with a
    /// letter on both sides is not a delimiter and is not respelled. That
    /// matters more than it looks: our own subjects are full of identifiers,
    /// and forgiving those would be forgiving a difference in a name.
    ///
    /// This is deliberately not a model of markdown. It forgives what the
    /// recording shows and nothing else, and a rule that cannot point at a
    /// golden does not belong here.
    fn as_one_emphasis_marker(cell: &str) -> String {
        let chars: Vec<char> = cell.chars().collect();
        chars
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if *c != '_' {
                    return *c;
                }
                let inside_a_word = i > 0
                    && chars[i - 1].is_alphanumeric()
                    && chars.get(i + 1).is_some_and(|n| n.is_alphanumeric());
                if inside_a_word { '_' } else { '*' }
            })
            .collect()
    }

    let same_after = |f: fn(&str) -> String| f(wrote) == f(read);
    wrote == read
        || same_after(without_added_escapes)
        || same_after(|c| as_one_emphasis_marker(&without_added_escapes(c)))
}

/// How a field's two sides are compared — the caller knows which its field is,
/// and this is the only place the choice is spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// A value that rides in a table cell, so the store's own escaping and
    /// marker respelling do not count as a change. See [`same_cell_value`].
    Cell,
    /// Machinery, or content the store leaves alone: an id, a date, a state, a
    /// fenced body. Byte-exact.
    Exact,
}

/// **The one field that did not survive, and the bytes on each side.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// What the field is called to a caller — never where it is kept.
    pub field: &'static str,
    /// What was written.
    pub wrote: String,
    /// What came back.
    pub read: String,
}

impl std::fmt::Display for Changed {
    /// **Names the field and shows the bytes, and says nothing about where the
    /// value sits.** A page, a table, a row or a cell in this sentence would be
    /// jojobot telling a caller about its store, which is the one thing no
    /// answer does.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the {} came back changed: wrote {:?}, read {:?}",
            self.field, self.wrote, self.read
        )
    }
}

/// **The first field that changed, or `None` if the record survived.**
///
/// One mechanism for every guard on every rail, rather than one per rail —
/// three copies of this decision is how one of them comes to disagree about
/// what counts as a change.
///
/// It exists because of what a refusal used to say. A read-back mismatch
/// interpolated two whole records into a sentence and left the reader to diff
/// them: in production that cost two failed writes, a page a person had to
/// repair by hand, and a wrong cause passed on as established, all because
/// nobody could see WHICH field differed. One field and its two values is the
/// whole of what a caller needs to act.
///
/// First rather than all, deliberately: the caller's next move is the same
/// whichever one it is, and a list invites diffing again.
pub fn first_changed(fields: &[(&'static str, Compare, String, String)]) -> Option<Changed> {
    fields
        .iter()
        .find(|(_, how, wrote, read)| match how {
            Compare::Cell => !same_cell_value(wrote, read),
            Compare::Exact => wrote != read,
        })
        .map(|(field, _, wrote, read)| Changed {
            field,
            wrote: wrote.clone(),
            read: read.clone(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mechanics, once, where they live — each call site's own golden pins
    /// the bytes it stores.
    #[test]
    fn text_that_fits_is_returned_whole() {
        assert_eq!(MESSAGE_TITLE.render("short"), "short");
        assert_eq!(
            MESSAGE_TITLE.render(&"w".repeat(60)),
            "w".repeat(60),
            "to the last character"
        );
    }

    #[test]
    fn a_cut_lands_on_a_word_and_says_it_cut() {
        let cut = MESSAGE_TITLE
            .render("counted the crates and reconciled them against the manifest twice over");
        assert!(cut.ends_with('…'));
        assert!(
            !cut.trim_end_matches('…').ends_with(' '),
            "on the word, not the space after it"
        );
    }

    /// **Where the ellipsis is counted from is the difference the strategies
    /// exist to keep.** Same text, same budget, two fields, two answers — and
    /// the one character between them is the whole disagreement: a field whose
    /// store refuses `budget + 1` needs the ellipsis to fit inside.
    ///
    /// It shows on the unbroken word, where the cut is exactly at the limit
    /// rather than at whatever word boundary happens to precede it.
    #[test]
    fn the_two_ellipsis_conventions_differ_by_the_ellipsis() {
        let within = Fitted {
            ellipsis: Ellipsis::WithinBudget,
            ..MESSAGE_TITLE
        };
        let text = "x".repeat(200);
        assert_eq!(
            MESSAGE_TITLE.render(&text).chars().count(),
            MESSAGE_TITLE.budget + 1
        );
        assert_eq!(within.render(&text).chars().count(), within.budget);
    }

    /// A word with no boundary inside the budget is cut anyway — a field that
    /// grew without limit would be worse than one that cut mid-word.
    #[test]
    fn an_unbroken_word_is_cut_rather_than_running_forever() {
        let cut = MESSAGE_TITLE.render(&"x".repeat(200));
        assert_eq!(cut, format!("{}…", "x".repeat(60)));
    }

    /// Only the fenced-block neighbour strips, and only it falls back.
    #[test]
    fn stripping_and_the_empty_fallback_belong_to_the_strategy() {
        assert_eq!(
            FOCUS_LINE.render("a `fence` and a \u{7}bell"),
            "a fence and a bell"
        );
        assert_eq!(MESSAGE_TITLE.render("a `fence`"), "a `fence`");
        assert_eq!(FOCUS_LINE.render("   "), FRESH_FOCUS);
        assert_eq!(MESSAGE_TITLE.render("   "), "");
    }

    /// **The body digest's golden**, kept here because this strategy is the one
    /// whose output is never stored — it is computed per response, so it has no
    /// call site to pin it at.
    #[test]
    fn the_body_digest_golden() {
        assert_eq!(
            BODY_DIGEST.render("the shipment landed"),
            "the shipment landed"
        );
        assert_eq!(
            BODY_DIGEST.render("the shipment landed\n\nand the crates are stacked"),
            "the shipment landed and the crates are stacked",
            "a multi-line body reads as one line of preview"
        );
        assert_eq!(
            BODY_DIGEST.render(&"counted the crates. ".repeat(20)),
            format!("{}counted the crates.…", "counted the crates. ".repeat(5)),
            "…and a long one is cut inside the budget"
        );
        assert!(BODY_DIGEST.render(&"x".repeat(500)).chars().count() <= BODY_DIGEST.budget);
        assert_eq!(
            BODY_DIGEST.render("   "),
            "",
            "an empty body previews as nothing"
        );
    }

    /// A budget of zero is no strategy declared here, and a public type with
    /// public fields must still not panic on one.
    #[test]
    fn a_zero_budget_cuts_rather_than_underflowing() {
        let nothing = Fitted {
            budget: 0,
            ..BODY_DIGEST
        };
        assert_eq!(nothing.render("anything at all"), "…");
    }

    /// **The cells real Outline actually rewrote**, taken from the recorded
    /// goldens rather than from an idea of what markdown escapes.
    ///
    /// The first draft of this test did the opposite and invented one: it
    /// asserted that `_under_` comes back `\_under\_`. The store does not
    /// escape it, it NORMALIZES it, to `*under*` — which the recorded golden
    /// said all along and which this comparison does not forgive. That case is
    /// deliberately absent here rather than hand-waved: an escape and a
    /// semantic rewrite are different problems.
    #[test]
    fn a_cell_the_store_escaped_is_the_same_value() {
        for (wrote, read) in [
            ("a ~ b ~ c", "a \\~ b \\~ c"),
            // Read from fixtures/mailboxes/emphasis.parsed.json, not from an
            // idea of what markdown does — the first draft of this test
            // invented the wrong transformation for exactly this case.
            (
                "_under_ *star* **bold** `tick`",
                "*under* *star* **bold** `tick`",
            ),
            ("# heading > quoted ---", "\\# heading > quoted ---"),
            (
                "<b>bold</b> & an <email@example.test>",
                "<b>bold</b> & an <email@example.test>",
            ),
            // jojobot's own placeholder, which the store escapes on every row
            // it writes.
            ("-", "\\-"),
            ("c:\\dir\\file", "c:\\dir\\file"),
        ] {
            assert!(
                same_cell_value(wrote, read),
                "the store's own escaping must not read as a changed value: \
                 {wrote:?} vs {read:?}"
            );
        }
    }

    /// **And the half that has to keep failing.** Forgiving an escape is not
    /// forgiving a difference: this guard has caught real loss twice, and a
    /// comparison that waved everything through would be worse than none.
    #[test]
    fn a_cell_that_really_changed_is_not_the_same_value() {
        for (wrote, read) in [
            ("moved to the 14th", "moved to the 15th"),
            ("a value with spaces", "a value with"),
            ("something", ""),
            ("", "something"),
            ("person:alpha", "person:beta"),
            // An underscore INSIDE a word is a name, not a marker, and the
            // store leaves it alone — measured in
            // fixtures/mailboxes/snake-case.parsed.json. So a difference in
            // one is a difference in a name.
            ("parse_bodies", "parse*bodies"),
            ("same_cell_value", "same_cell_valve"),
            // A backslash the WRITER put before a letter is theirs, and losing
            // it is loss.
            ("c:\\dir", "c:dir"),
        ] {
            assert!(
                !same_cell_value(wrote, read),
                "a changed value must still be caught: {wrote:?} vs {read:?}"
            );
        }
    }

    /// **A refusal names one field and its two values.**
    #[test]
    fn the_first_changed_field_is_named_with_its_bytes() {
        let survived = first_changed(&[
            ("subject", Compare::Cell, "a ~ b".into(), "a \\~ b".into()),
            (
                "body",
                Compare::Exact,
                "unchanged".into(),
                "unchanged".into(),
            ),
        ]);
        assert_eq!(survived, None, "the store's own escaping is not a change");

        let changed = first_changed(&[
            ("subject", Compare::Cell, "a ~ b".into(), "a \\~ b".into()),
            (
                "body",
                Compare::Exact,
                "as written".into(),
                "as stored".into(),
            ),
        ])
        .expect("a changed field is found");
        assert_eq!(changed.field, "body");
        assert_eq!(
            changed.to_string(),
            "the body came back changed: wrote \"as written\", read \"as stored\""
        );
    }

    /// **A refusal says nothing about where the value is kept.** The sentence
    /// reaches an agent, and jojobot's storage is not an agent's business.
    #[test]
    fn a_refusal_names_no_part_of_the_store() {
        let said = first_changed(&[("focus", Compare::Exact, "before".into(), "after".into())])
            .expect("a change")
            .to_string()
            .to_lowercase();
        for leak in [
            "page", "table", "row", "cell", "column", "document", "fence",
        ] {
            assert!(
                !said.contains(leak),
                "a refusal must not name {leak:?}: {said}"
            );
        }
    }

    /// An outcome record keeps the writer's own spacing — it is one line
    /// already, and collapsing it would be an edit nobody asked for.
    #[test]
    fn an_outcome_record_is_not_reflowed() {
        assert_eq!(
            OUTCOME_NOTES.render("filed  under   shipments"),
            "filed  under   shipments"
        );
        assert_eq!(
            MESSAGE_TITLE.render("filed  under   shipments"),
            "filed under shipments"
        );
    }
}
