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
//! **These outputs are stored bytes.** A title sits on a live board and a focus
//! sits in a card's description, so changing what a strategy produces rewrites
//! records that already exist. Every strategy is pinned by a golden test at its
//! own call site; a change to the rules has to go there and say so.

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
            flat.chars().filter(|c| *c != '`' && !c.is_control()).collect()
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
            Ellipsis::WithinBudget => self.budget - 1,
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

/// **The head of a session card's title** — the bot's focus, after `"<bot>: "`.
///
/// Identical to [`MESSAGE_TITLE`] today, and declared separately anyway: they
/// are two boards, and merging them would mean a future change to what a
/// mailbox card is called silently renaming every session card too.
pub const SESSION_TITLE: Fitted = Fitted {
    name: "session-title",
    budget: 60,
    ellipsis: Ellipsis::BeyondBudget,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The mechanics, once, where they live — each call site's own golden pins
    /// the bytes it stores.
    #[test]
    fn text_that_fits_is_returned_whole() {
        assert_eq!(MESSAGE_TITLE.render("short"), "short");
        assert_eq!(MESSAGE_TITLE.render(&"w".repeat(60)), "w".repeat(60), "to the last character");
    }

    #[test]
    fn a_cut_lands_on_a_word_and_says_it_cut() {
        let cut = MESSAGE_TITLE.render("counted the crates and reconciled them against the manifest twice over");
        assert!(cut.ends_with('…'));
        assert!(!cut.trim_end_matches('…').ends_with(' '), "on the word, not the space after it");
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
        let within = Fitted { ellipsis: Ellipsis::WithinBudget, ..MESSAGE_TITLE };
        let text = "x".repeat(200);
        assert_eq!(MESSAGE_TITLE.render(&text).chars().count(), MESSAGE_TITLE.budget + 1);
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
        assert_eq!(FOCUS_LINE.render("a `fence` and a \u{7}bell"), "a fence and a bell");
        assert_eq!(MESSAGE_TITLE.render("a `fence`"), "a `fence`");
        assert_eq!(FOCUS_LINE.render("   "), FRESH_FOCUS);
        assert_eq!(MESSAGE_TITLE.render("   "), "");
    }

    /// An outcome record keeps the writer's own spacing — it is one line
    /// already, and collapsing it would be an edit nobody asked for.
    #[test]
    fn an_outcome_record_is_not_reflowed() {
        assert_eq!(OUTCOME_NOTES.render("filed  under   shipments"), "filed  under   shipments");
        assert_eq!(MESSAGE_TITLE.render("filed  under   shipments"), "filed under shipments");
    }
}
