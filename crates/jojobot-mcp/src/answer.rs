//! **How jojobot answers** — the success envelope, and the one refusal that
//! belongs to no context.
//!
//! Two functions and one contract: every verb on this surface returns through
//! `json_result`, and a call whose arguments are each fine and wrong together
//! comes back through `misused`. The refusals that ARE a context's — a
//! near-miss handle, an unknown box, a closed session — live with that context.

use super::*;

/// **What a refusal unlocks — never a deficit to avoid** (decision log 261,
/// 262). Every `status: "blocked"` body on this surface carries one, built
/// through this type rather than a bare `String`, so a caller sees a next
/// move and never a rejection with nothing behind it.
///
/// **The constructor is the only door, and it is the mechanism**: nothing
/// converts an empty or blank string into a `WayForward`. A refusal built
/// with no way forward is not a smaller answer — it is the exact failure
/// this type exists to make impossible, so it panics at the moment somebody
/// tries to build one rather than shipping it to a caller who has to notice
/// on their own. A session that has never read decision log 261 still
/// cannot write a bare rejection, because the type refuses to hold one.
pub(crate) struct WayForward(String);

impl WayForward {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for WayForward {
    fn from(text: String) -> Self {
        assert!(
            !text.trim().is_empty(),
            "a refusal was built with no way forward — every blocked result must say what it \
             unlocks (decision log 261, 262)",
        );
        WayForward(text)
    }
}

impl From<&str> for WayForward {
    fn from(text: &str) -> Self {
        WayForward::from(text.to_string())
    }
}

/// **A call whose arguments are each fine and wrong together.** Not a malformed
/// call — every token parsed — so it is not a protocol error: it is a caller
/// mistake, and those are answers here.
///
/// **No `attempted` and no `candidates`, deliberately.** There is nothing that
/// was nearly right to name and nothing that nearly matched; what a caller needs
/// is the other call to make. [`session_unbound`] is the precedent — the shape
/// has always carried a candidate-free refusal, so this fits it rather than
/// stretching it into something that reads like a near miss.
pub(crate) fn misused(how_to_proceed: impl Into<WayForward>) -> CallToolResult {
    let how_to_proceed = how_to_proceed.into();
    let body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
        "how_to_proceed": how_to_proceed.as_str(),
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **Replace prose the caller just wrote with what the caller does not have.**
///
/// A write verb answers with a receipt: enough to know the write landed, to
/// recognize WHICH write it was, and to address it later — and none of the
/// body, because its author is the one reader it teaches nothing.
///
/// `post_message` is the shape this follows and the reason it is one function:
/// four keys in a fixed relation — the payload gone, a marker saying so, the
/// byte count of what was stored, and enough of the opening to tell two writes
/// apart. **Eliding is never silent**, so `how_to_read` names the call that
/// returns the whole thing; a reader that had to infer withheld-from-absent
/// would eventually infer wrong.
///
/// The byte count is of what was STORED, so a caller comparing it against what
/// it sent learns that the store trimmed it.
pub(crate) fn elide_prose(body: &mut serde_json::Value, key: &str, prose: &str, how_to_read: &str) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(key.into(), serde_json::Value::Null);
    fields.insert(format!("{key}_elided"), true.into());
    fields.insert(format!("{key}_bytes"), prose.len().into());
    fields.insert(
        format!("{key}_head"),
        jojobot_domain::text::BODY_DIGEST.render(prose).into(),
    );
    fields.insert("how_to_read".into(), how_to_read.into());
}

/// Render a JSON body as a successful tool result.
pub(crate) fn json_result(body: &serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]))
}

/// **Move the body to where the agreed revision reads it.**
///
/// Every verb writes one JSON body into a text block, which is the only place
/// a client older than `2026-07-28` can read it. That revision added
/// `structuredContent`, where the same body arrives as an object rather than
/// as a string the caller has to parse.
///
/// **The body moves rather than being copied.** The spec permits a server to
/// send both, and most do; here it would put the whole body on the wire twice
/// for a reader that has it already. Nothing else this surface does ships a
/// caller what it demonstrably holds, and a duplicate of the answer is the
/// plainest case of that.
///
/// **An answer this cannot read is left exactly as the verb wrote it**, for the
/// reason the status bar leaves one alone: rewriting a body it does not
/// understand is worse than not touching it.
pub(crate) fn structure(answered: &mut CallToolResponse) {
    // Only a completed call carries a body. The other responses are the
    // protocol asking the client for something.
    let CallToolResponse::Complete(result) = answered else {
        return;
    };
    let Some(found) = result.content.iter().position(|block| {
        block.as_text().is_some_and(|text| {
            serde_json::from_str::<serde_json::Value>(&text.text).is_ok_and(|body| body.is_object())
        })
    }) else {
        return;
    };
    let block = result.content.remove(found);
    let text = block.as_text().expect("the block just matched as text");
    result.structured_content = serde_json::from_str(&text.text).ok();
}

impl Jojobot {
    /// **The last two things that happen to every answer, in the one order
    /// that works.** The status bar is written into the body, so it has to be
    /// added while the body is still where the verb put it; moving the body to
    /// `structuredContent` first would leave the bar with nothing to ride on.
    ///
    /// They are one call so that order is not a thing a caller can get wrong,
    /// and so a verb added tomorrow gets both by doing nothing.
    pub(crate) async fn finish(
        &self,
        answered: &mut CallToolResponse,
        sid: Option<&str>,
        structured: bool,
    ) {
        self.add_status_bar(answered, sid).await;
        if structured {
            structure(answered);
        }
    }
}

// ── what a write says about itself ──────────────────────────────────────────

/// **A count and its noun, agreeing.** A real model read "1 entries long" off
/// a postcondition in a paid run. These lines are read by something that
/// reasons about what they say, so prose that announces itself as generated
/// spends the trust the line was added to build.
pub(crate) fn counted(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

/// **One field the store did not keep as the caller sent it.**
///
/// `sent` is what the caller wrote and `stored` is what the record carries.
/// **A value the caller never sent is not a difference** — it was defaulted,
/// and the receipt already states every defaulted value on its own key. The
/// distinction is the whole point: a default is jojobot filling a gap, and a
/// difference is jojobot overruling a choice.
pub(crate) struct Difference {
    /// **Owned rather than static**, because a verb may substitute inside a
    /// structure a caller named: a closed set belongs to one key, and a line
    /// that could not say which key would leave a reader to guess.
    pub(crate) field: String,
    pub(crate) sent: String,
    pub(crate) stored: String,
    /// **Why this verb stores something else here**, where it has a reason.
    ///
    /// ⚠️ **A difference is not a fault**, and without a reason it reads as
    /// one. The check-in path substitutes because the record it builds mixes a
    /// caller's sentence with a computed schedule, and a caller told only that
    /// its value was replaced learns to distrust a verb that did the right
    /// thing.
    ///
    /// **It says what the stored value is and why the record can carry no
    /// other — a fact about the record, never about how the server went about
    /// the write** (rule 158). `None` where nothing needs explaining: a
    /// sentence restating the comparison is noise wearing the shape of help.
    pub(crate) because: Option<&'static str>,
}

impl Difference {
    /// The difference between what a caller sent for `field` and what was
    /// stored, or `None` when the caller sent nothing or the two agree.
    ///
    /// **Deterministic, and a comparison over declared data**: two tokens are
    /// equal or they are not, and nothing here judges whether the substitution
    /// was right.
    pub(crate) fn between(
        field: impl Into<String>,
        sent: Option<&str>,
        stored: &str,
    ) -> Option<Self> {
        Self::converted(field, sent, stored, None)
    }

    /// The same comparison, carrying the reason this verb stores something
    /// else here.
    pub(crate) fn converted(
        field: impl Into<String>,
        sent: Option<&str>,
        stored: &str,
        because: Option<&'static str>,
    ) -> Option<Self> {
        let sent = sent?;
        (!sent.eq_ignore_ascii_case(stored)).then(|| Self {
            field: field.into(),
            sent: sent.to_string(),
            stored: stored.to_string(),
            because,
        })
    }
}

/// **Name the values the store did not keep as they were sent.**
///
/// **Silent when nothing differs**, and that is not a saving: a line printed on
/// every write is one a reader learns to skip, and it would be gone from view
/// on the write that needed it. The key is absent rather than empty for the
/// same reason a reader must never infer withheld from missing — here there is
/// nothing withheld to tell them about.
pub(crate) fn note_delta(body: &mut serde_json::Value, differences: Vec<Difference>) {
    if differences.is_empty() {
        return;
    }
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(
        "delta".into(),
        differences
            .iter()
            .map(|d| {
                serde_json::json!({
                    "field": d.field.as_str(),
                    "sent": d.sent,
                    "stored": d.stored,
                    "because": d.because,
                })
            })
            .collect::<Vec<_>>()
            .into(),
    );
    fields.insert(
        "delta_note".into(),
        format!(
            "stored differs from sent. {}",
            differences
                .iter()
                .map(|d| {
                    let because = d.because.map(|w| format!(" — {w}")).unwrap_or_default();
                    format!(
                        "{}: stored {}, you sent {}{because}",
                        d.field, d.stored, d.sent
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
        .into(),
    );
}

/// **State what now stands, and what this write left alone.**
///
/// A fact about the CALLER'S OWN EFFECT on the store, derivable from the verb's
/// contract — never how the server went about it. *"Two accounts now stand on
/// this thing"* is where a caller stands; *"we read it back to check"* is the
/// server's business and does not appear here (rule 158).
///
/// **The line is computed per write and never a constant.** A write that
/// displaced something says so: *"nothing was removed"* on a write that removed
/// something is a false promise in the one place a caller has been taught to
/// trust, which is worse than saying nothing at all.
pub(crate) fn note_postcondition(body: &mut serde_json::Value, line: String) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert("postcondition".into(), line.into());
}

/// **Ride a teaching on the answer that triggered it**, rather than a
/// separate call the caller has to know to make — see
/// [`crate::teaching`].
///
/// **Appends, never overwrites.** More than one domain can reach its first
/// contact on the same call — a session's very first `capture` can be the
/// first time it has ever touched two domains at once — and a single key a
/// second call could overwrite would silently drop one of them. `first_contact`
/// has already consumed that domain's row by the time this runs, so a
/// teaching lost here is lost for good: the ledger would truthfully report it
/// was taught, and it never was. A list makes that collision impossible on
/// the wire rather than something a caller has to avoid by construction.
pub(crate) fn note_teaching(body: &mut serde_json::Value, content: &str) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields
        .entry("teaching")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .expect("teaching is always written as an array")
        .push(content.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🚨 **An empty way forward cannot be built at all** (decision log 261,
    /// 262) — the constructor is the one door, and it panics rather than
    /// handing back a `WayForward` that says nothing. This is the mechanism
    /// itself, independent of any one refusal that uses it.
    #[test]
    #[should_panic(expected = "way forward")]
    fn a_blank_way_forward_cannot_be_built() {
        let _: WayForward = "   ".into();
    }

    /// The ordinary case: real prose survives the constructor unchanged.
    #[test]
    fn a_real_way_forward_survives_construction() {
        let built: WayForward = "call add_entity first".into();
        assert_eq!(built.as_str(), "call add_entity first");
    }

    /// **`misused` is wired to the mechanism, not merely beside it.** A
    /// refusal built through the actual verb-facing function — not the type
    /// in isolation — must panic on an empty way forward, or the type is a
    /// guarantee nobody is holding.
    #[test]
    #[should_panic(expected = "way forward")]
    fn misused_cannot_be_built_with_an_empty_way_forward() {
        misused(String::new());
    }

    /// 🚨 **Two teachings on one body must both survive.** A single `"teaching"`
    /// key that a second call overwrites drops the first one silently — no
    /// error, no signal — which is worse than never teaching it, because the
    /// domain's ledger row is already spent by the time this runs.
    #[test]
    fn two_teachings_on_one_body_both_survive() {
        let mut body = serde_json::json!({});
        note_teaching(&mut body, "first domain's content");
        note_teaching(&mut body, "second domain's content");
        assert_eq!(
            body["teaching"],
            serde_json::json!(["first domain's content", "second domain's content"]),
            "both teachings must be readable, in the order they were recorded: {body}"
        );
    }
}
