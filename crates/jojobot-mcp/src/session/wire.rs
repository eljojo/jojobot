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

/// One session on the wire — the record, its chronology, and where it sits.
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
        obj.extend(chronology_json(
            &text::SESSION_CHRONOLOGY.tail(&session.entries, |e| e.text.chars().count()),
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

#[cfg(test)]
mod tests {
    use super::*;

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
