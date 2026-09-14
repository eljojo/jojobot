//! **The session response vocabulary** — a session on the wire, one entry on
//! the wire, and the one-line form a focus is cut to.

use super::*;

/// Prose reduced to one line a display field can carry.
///
/// **A cut, never a refusal.** This is what a focus is derived from when the
/// caller offered none, and the text it is derived from is the record: an
/// entry, a story. Refusing prose because a *display* field cannot hold it
/// would throw away the thing worth keeping to protect the thing that is only a
/// glance — which is exactly what it did.
///
/// The rules — one line, no backtick or control character, cut on a word
/// boundary with an ellipsis inside the cap — are [`text::FOCUS_LINE`]. Each is
/// a rule of the *field* rather than a judgement about the text, which is why
/// they are declared there beside the other fields' and pinned by a golden.
pub(crate) fn display_line(prose: &str) -> String {
    text::FOCUS_LINE.render(prose)
}

/// **About four characters to a token** — the common rule of thumb, and the
/// only honest unit here. jojobot cannot count tokens: tokenising belongs
/// to whichever model is reading, and they differ, so borrowing one
/// tokeniser (say, an embeddings model's) would look precise while being
/// wrong for the reader actually holding this answer. An estimate that
/// says so, and shows the arithmetic behind it, is the only figure that
/// stays honest as models change.
const CHARS_PER_TOKEN: f64 = 4.0;

/// A magnitude rendered the way a reader skims it: bare under a thousand,
/// `N.Nk` above it.
fn magnitude(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// **What a session has been handed, with the estimate showing its own
/// working** (rule 264) — never a bare number, because a figure nobody can
/// re-derive is a figure nobody can recalibrate later without a caller
/// having believed a lie in the meantime.
pub(crate) fn served_json(served_chars: u64) -> serde_json::Value {
    let tokens = (served_chars as f64 / CHARS_PER_TOKEN).round() as u64;
    serde_json::json!({
        "characters": served_chars,
        "note": format!(
            "≈{} tokens, from {} characters at about {CHARS_PER_TOKEN:.0} per token",
            magnitude(tokens),
            magnitude(served_chars),
        ),
    })
}

/// One session on the wire — the record, its chronology, and where it sits.
///
/// **Deliberately carries no `served` figure.** This renders `start_here`'s
/// boot and `wrap_session`'s close, both already sized against a hard
/// character budget the chronology tail is cut to fit (rule 264's own
/// dispatch: "not a per-answer change"). `list_runs` is the read built to
/// report about sessions and is where [`served_json`] rides instead —
/// adding it here grew a boot past its own cap on the day it was tried.
pub(crate) fn session_json(session: &Session) -> serde_json::Value {
    let mut body = serde_json::json!({
        "id": session.id.as_str(),
        "bot": session.bot.as_str(),
        "focus": session.focus,
        "started_at": session.started_at.to_string(),
        "state": session.state.as_token(),
        // **The whole record's length, whatever this answer carries of it.**
        "entry_count": session.entries.len(),
    });
    if let Some(obj) = body.as_object_mut() {
        // **Sized by what it renders as, not by what somebody wrote.** An entry
        // ships as an object with an id, a timestamp and a beat around its
        // text, and on a short entry that envelope is most of the payload — so
        // measuring the text alone left the budget holding a number the answer
        // does not spend. The rendering happens twice, which a chronology is far
        // too small for anybody to notice.
        obj.extend(chronology_json(
            &text::SESSION_CHRONOLOGY.tail(&session.entries, |e| {
                entry_json(e).to_string().chars().count()
            }),
        ));
    }
    body
}

/// **The chronology a response carries, and what it left out.**
///
/// This is the only renderer for a chronology, and it takes a [`Kept`] — which
/// nothing but [`text::Capped::tail`] produces. So serving a chronology without
/// passing through the cap is not a thing that can be written here: the cap is
/// unskippable rather than remembered.
fn chronology_json(kept: &Kept<'_, JournalEntry>) -> serde_json::Map<String, serde_json::Value> {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "chronology".into(),
        kept.kept()
            .iter()
            .map(entry_json)
            .collect::<Vec<_>>()
            .into(),
    );
    fields.insert("chronology_elided".into(), kept.elided().into());
    if kept.elided() {
        fields.insert("entries_omitted".into(), kept.omitted().into());
        fields.insert(
            "chronology_note".into(),
            format!(
                "the {} OLDEST entries of this chronology are not in this answer. A chronology \
                 grows with every beat, so a boot carries the newest of it and the answer stays \
                 one you can read; `entry_count` is the length of the whole record. Nothing was \
                 changed and nothing was lost — but no verb serves the older entries, so read \
                 this tail as what a resume gives you.",
                kept.omitted(),
            )
            .into(),
        );
    }
    fields
}

/// One chronology entry. `beat` names the verb class for an entry **jojobot**
/// wrote and is null for one the session wrote — a reader weighing a chronology
/// has to tell an account of intent from a tally of calls.
pub(crate) fn entry_json(entry: &JournalEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id.as_str(),
        "at": entry.at.to_string(),
        "text": entry.text,
        "beat": entry.beat,
    })
}

/// **The receipt for a beat somebody just wrote**: the same entry with its
/// text replaced by what the caller does not have.
///
/// A chronology READ carries the text, because its reader was not there when
/// it was written. The answer to the write is read by its own author, in the
/// call that carried it, so the text is the one thing in it that teaches
/// nothing — and an entry is prose, once per beat, all run long.
pub(crate) fn entry_receipt_json(entry: &JournalEntry) -> serde_json::Value {
    let mut body = entry_json(entry);
    crate::answer::elide_prose(
        &mut body,
        "text",
        &entry.text,
        "you wrote this entry. The whole chronology comes back from start_here when you resume \
         this session, and from wrap_session when you close it.",
    );
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Never a bare number** (rule 264): the served figure shows the
    /// arithmetic that produced it, so a reader can re-derive it and a later
    /// recalibration of the divisor is one line with no lie standing in the
    /// meantime.
    #[test]
    fn the_served_figure_shows_its_own_working() {
        let body = served_json(49_623);
        assert_eq!(body["characters"], 49_623, "{body}");
        let note = body["note"].as_str().expect("a note string");
        assert!(
            note.contains("49.6k") && note.contains("characters"),
            "the raw character count is named, not just the estimate: {note}"
        );
        assert!(
            note.contains("≈") && note.contains("tokens"),
            "the figure is marked as an estimate rather than read as exact: {note}"
        );
        assert!(
            note.contains("per token"),
            "the divisor itself is shown, so it can be recalibrated later: {note}"
        );
    }

    /// **Zero is an ordinary answer and reads as one**, not as a missing
    /// field — an untouched session's figure is still a real number with
    /// its own working, never elided into absence.
    #[test]
    fn an_untouched_session_shows_a_real_zero() {
        let body = served_json(0);
        assert_eq!(body["characters"], 0, "{body}");
        assert!(
            body["note"].as_str().is_some_and(|n| n.contains('0')),
            "{body}"
        );
    }

    /// **The golden: every byte the derived focus has ever been given.** A focus
    /// is stored in a session card's description on a live board, so this
    /// strategy's output is the product. Recorded literally so the shared text
    /// engine underneath can only pass by producing the same bytes.
    ///
    /// This is the one strategy that strips: a focus rides above a fenced
    /// machine block, so a backtick in it can close the fence, and it has an
    /// empty fallback because a card with a blank description says nothing.
    #[test]
    fn the_focus_line_golden() {
        let w200 = "w".repeat(200);
        let w199 = "w".repeat(199);
        let words = format!("{} tail", "word ".repeat(45));
        let x400 = "x".repeat(400);
        let cases: [(&str, String); 9] = [
            ("short one", "short one".into()),
            (
                "read the hand-off\n\nthen scoped the slice",
                "read the hand-off then scoped the slice".into(),
            ),
            (
                "started on `working_session`, which was the wrong shape",
                "started on working_session, which was the wrong shape".into(),
            ),
            (&w200, w200.clone()),
            (&w199, w199.clone()),
            (&words, format!("{}word…", "word ".repeat(39))),
            (&x400, format!("{}…", "x".repeat(199))),
            ("   ", "working".into()),
            ("bell\u{7}char", "bellchar".into()),
        ];
        for (input, expected) in cases {
            assert_eq!(
                display_line(input),
                expected,
                "the stored focus changed for {input:?}"
            );
        }
    }
}
