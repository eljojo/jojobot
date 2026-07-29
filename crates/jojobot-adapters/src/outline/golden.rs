//! **Golden pages for the mail and session rails, recorded from real Outline.**
//!
//! The fact table got these first, and the two rails that then failed in
//! production never had them. That is the first root cause in the incident:
//! nothing measured what the store does to a mailbox page or a session page,
//! so nobody could have known — the store's rewriting is **not uniform across
//! surfaces**, and measuring one says nothing about another.
//!
//! # What these record, and why not through the ports
//!
//! The fact-table goldens are written through `capture`, because that write
//! survives. These cannot be: the read-back guard refuses exactly the writes
//! worth measuring, so a recorder that went through `post_message` would
//! record only the cases that were never in question, and the interesting page
//! would never reach disk.
//!
//! So the recorder puts the page up **as the codec renders it** and reads the
//! bytes back. What is recorded is the store's own transformation of the exact
//! text this code writes — which is the measurement the comparison has to be
//! designed from, and it is unavailable from reasoning about markdown.
//!
//! The battery is deliberately the punctuation that has already cost this
//! project something: a tilde (three surfaces), a list marker at line start
//! (two failed sends, an orphaned body, a consumed id), plus the characters
//! markdown gives meaning to and the pipe the cell grammar uses.

use super::*;

/// Where a recorded page and its expectation live.
fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub(super) fn fixture(rail: &str, name: &str) -> String {
    let path = fixture_dir().join(rail).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the recorded fixture {} must be readable: {e}",
            path.display()
        )
    })
}

/// **The punctuation this project has actually been hurt by**, plus the
/// characters markdown assigns meaning to.
///
/// Every entry is `(name, text)`, and the names are what the fixtures are
/// keyed by, so a case that stops being recorded is visible as a missing file
/// rather than as a shorter list nobody counted.
pub(super) const BATTERY: &[(&str, &str)] = &[
    // The one that hit three surfaces before anybody knew it was general: the
    // store inserts an escape in front of it that nothing wrote.
    ("tilde", "a ~ b ~ c"),
    // The one that cost two failed sends and an orphaned body: a LETTERED list
    // marker at line start, which the store reads as an ordered list and
    // renumbers.
    ("lettered-list", "a) first\nb) second\nc) third"),
    ("numbered-list", "1. first\n2. second\n7. out of order"),
    ("bulleted-list", "- first\n* second\n+ third"),
    // A heading, a quote and a rule: all line-start syntax, all re-serialized.
    ("line-start-syntax", "# heading\n> quoted\n---"),
    ("emphasis", "_under_ *star* **bold** `tick`"),
    ("angle-brackets", "<b>bold</b> & an <email@example.test>"),
    ("backslash", "c:\\dir\\file and a trailing \\"),
    ("pipe", "a | b | c"),
    ("indented", "    four spaces\n\tand a tab"),
    ("unicode", "café — ünïcode ✓ 🎯"),
    ("blank-lines", "first\n\n\nlast"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// What one recorded page parses into, per position.
    #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Parsed {
        /// The table cell — a subject, or a session's focus.
        cell: String,
        /// The fenced block — a message body, or a chronology entry.
        block: String,
    }

    fn parse_mail(page: &str) -> Parsed {
        let (rows, quarantined) = mailbox_codec::parse_rows(page);
        assert!(
            quarantined.is_empty(),
            "the store's rewriting cost a whole ROW, which is loss rather than formatting: \
             {quarantined:?}"
        );
        let row = rows.first().expect("one message");
        let bodies = mailbox_codec::parse_bodies(page);
        Parsed {
            cell: row.subject.clone().unwrap_or_default(),
            block: bodies
                .iter()
                .find(|(id, _)| id == &row.id)
                .map(|(_, b)| b.clone())
                .expect("the message keeps its body"),
        }
    }

    fn parse_session(page: &str) -> Parsed {
        let rows = session_codec::parse_rows(page);
        let row = rows.first().expect("one session");
        let entries = session_codec::parse_entries(page);
        Parsed {
            cell: row.focus.clone(),
            block: entries
                .iter()
                .find(|(id, _)| id == &row.id)
                .map(|(_, e)| e.text.clone())
                .expect("the session keeps its entry"),
        }
    }

    fn parse(rail: &str, page: &str) -> Parsed {
        match rail {
            "mailboxes" => parse_mail(page),
            _ => parse_session(page),
        }
    }

    /// **What each recorded page reads back as, checked in and compared.**
    ///
    /// The recorder cannot produce this half — it lives outside the crate and
    /// cannot see a codec — and its first two attempts to approximate it with
    /// a substring check were both wrong in the same direction, answering "not
    /// rewritten" for text the store had escaped. A `contains` cannot see a
    /// prefix escape: `\# heading` contains `# heading`.
    ///
    /// So the artefact is the PARSED value, restated by
    /// [`restate_the_rail_measurements`] and reviewed as a diff. Comparing
    /// against a checked-in value is what makes this a golden rather than a
    /// derivation agreeing with itself.
    #[test]
    fn the_rails_read_back_as_what_was_recorded() {
        for rail in ["mailboxes", "sessions"] {
            for (name, _) in BATTERY {
                let expected: Parsed =
                    serde_json::from_str(&fixture(rail, &format!("{name}.parsed.json")))
                        .unwrap_or_else(|e| panic!("{rail}/{name}'s expectation: {e}"));
                assert_eq!(
                    parse(rail, &fixture(rail, &format!("{name}.md"))),
                    expected,
                    "{rail}/{name}: the recorded page no longer reads as what it read as"
                );
            }
        }
    }

    /// **A fenced block survives THE STORE verbatim; a table cell does not.**
    ///
    /// This is the finding the rest of the slice is designed from, and it is
    /// asserted against the recorded PAGE — the store's own bytes — rather
    /// than against what this crate then parses out of it. The two are
    /// different claims and conflating them hid a defect of ours behind a
    /// statement about Outline: the first version of this test compared the
    /// PARSED body and failed on the indented case, which the store had in
    /// fact preserved perfectly.
    ///
    /// It is the whole argument for fixing the comparison rather than escaping
    /// every cell: the store's damage is confined to cells, and the operator's
    /// own prose — a message body, a journal entry — is already safe where it
    /// sits.
    #[test]
    fn a_fence_survives_the_store_and_a_cell_does_not() {
        let mut rewritten_cells = 0;
        for rail in ["mailboxes", "sessions"] {
            for (name, wrote) in BATTERY {
                let page = fixture(rail, &format!("{name}.md"));
                assert!(
                    page.contains(wrote),
                    "{rail}/{name}: the store changed text inside a FENCE. Every comparison \
                     this slice builds assumes fences are safe, and that assumption just moved."
                );
                if parse(rail, &page).cell != wrote.replace('\n', " ") {
                    rewritten_cells += 1;
                }
            }
        }
        // Named as a count rather than a list, because which cases the store
        // rewrites is its business and may change; that it rewrites SOME is
        // the thing the design depends on, and a zero here would mean the
        // problem this slice exists for had quietly gone away.
        assert!(
            rewritten_cells > 0,
            "no recorded cell came back changed, which contradicts four production \
             incidents — the recording is measuring the wrong thing"
        );
    }

    /// **A body's leading indentation survives the reader.**
    ///
    /// It did not: the four spaces were on the recorded page and absent from
    /// what `parse_bodies` returned, because the reader trimmed the whole
    /// joined body rather than the blank lines around it. The store had kept
    /// them perfectly. Found by the goldens on their first run, which is the
    /// argument for having built them.
    #[test]
    fn a_bodys_leading_indentation_survives_the_reader() {
        let page = fixture("mailboxes", "indented.md");
        assert!(
            page.contains("    four spaces"),
            "the store kept the indentation: {page}"
        );
        assert_eq!(
            parse("mailboxes", &page).block,
            "    four spaces\n\tand a tab",
            "…and so must the reader"
        );
    }

    /// **Restate what the recorded pages parse into.** Ignored and gated like
    /// the recorders: it rewrites checked-in expectations, so running it is a
    /// decision and the diff is what a reviewer reads.
    ///
    /// Separate from the recorder because only this side can see a codec, and
    /// separate from the tests above because a test that rewrites its own
    /// expectation asserts nothing.
    #[test]
    #[ignore]
    fn restate_the_rail_measurements() {
        if std::env::var("JOJOBOT_RECORD_GOLDENS")
            .ok()
            .filter(|v| !v.is_empty())
            .is_none()
        {
            println!("SKIPPED: set JOJOBOT_RECORD_GOLDENS=1 to rewrite the checked-in values.");
            return;
        }
        for rail in ["mailboxes", "sessions"] {
            for (name, _) in BATTERY {
                let parsed = parse(rail, &fixture(rail, &format!("{name}.md")));
                let path = fixture_dir().join(rail).join(format!("{name}.parsed.json"));
                std::fs::write(
                    &path,
                    format!(
                        "{}\n",
                        serde_json::to_string_pretty(&parsed).expect("it serializes")
                    ),
                )
                .expect("write the expectation");
                println!("RESTATED {rail}/{name}");
            }
        }
    }
}
