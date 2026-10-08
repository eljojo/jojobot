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
pub(crate) struct WayForward {
    text: String,
    /// The word. `None` only for a storage failure whose switch is off: see
    /// [`memory_store_failure_word`] and [`other_store_failure_word`].
    fix_by: Option<FixBy>,
}

/// **Who fixes a refusal, in one word a caller reads before the prose.** A
/// caller that cannot tell a transient failure from a refusal that will never
/// change retries the second forever, or gives up on the first. Every refusal
/// carries one of three words: in the body of a blocked answer, written by
/// [`WayForward::write_into`], and in the `data` of a protocol error, written
/// by [`stamp_error`]. Each error type picks its word in ONE exhaustive match,
/// so a kind added without a word does not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FixBy {
    /// Transient: the same call may succeed, as after a write conflict.
    Retry,
    /// The call itself must change, and the refusal names what to change. The
    /// common case, and the one a bare sentence means.
    Change,
    /// Stored damage, or a decision only a person can make. Sending the call
    /// again will not help, and neither will changing it.
    Person,
}

impl FixBy {
    /// The key the word is served under. It sorts ahead of `how_to_proceed`,
    /// so the word is read before the prose.
    pub(crate) const KEY: &'static str = "fix_by";

    /// The word on the wire.
    pub(crate) fn as_token(self) -> &'static str {
        match self {
            FixBy::Retry => "retry",
            FixBy::Change => "change",
            FixBy::Person => "person",
        }
    }
}

/// **THE SWITCHES for storage failures**, one per store, because the stores do
/// not yet tell a refused write from an outage alike. A storage failure is a
/// `retry` only where the failure really is transient.
///
/// `MEMORY_STORE_FAILURE_IS_RETRY` holds while card 2068 (a constraint
/// violation is no longer reported with the outage's words) is in the base this
/// ships on. If that fix slips, set it to `false`: every memory-store failure
/// then ships WITHOUT a word rather than with a wrong one.
pub(crate) const MEMORY_STORE_FAILURE_IS_RETRY: bool = true;

/// The session, mailbox and teaching stores still report a constraint violation
/// with the outage's words (card 2070), so their failures wear no word until it
/// lands. Set this to `true` then.
pub(crate) const OTHER_STORE_FAILURE_IS_RETRY: bool = false;

/// The word a memory-store failure wears: `retry` while its switch is on, none
/// when it is off.
pub(crate) fn memory_store_failure_word() -> Option<FixBy> {
    MEMORY_STORE_FAILURE_IS_RETRY.then_some(FixBy::Retry)
}

/// The word a session, mailbox or teaching store failure wears: none until
/// [`OTHER_STORE_FAILURE_IS_RETRY`] is turned on.
pub(crate) fn other_store_failure_word() -> Option<FixBy> {
    OTHER_STORE_FAILURE_IS_RETRY.then_some(FixBy::Retry)
}

impl WayForward {
    fn with(fix_by: Option<FixBy>, text: impl Into<String>) -> Self {
        let text = text.into();
        assert!(
            !text.trim().is_empty(),
            "a refusal was built with no way forward — every blocked result must say what it \
             unlocks (decision log 261, 262)",
        );
        WayForward { text, fix_by }
    }

    /// A refusal only a person can resolve: stored damage, or a decision that
    /// is theirs. Sending the call again will not help.
    pub(crate) fn person(text: impl Into<String>) -> Self {
        Self::with(Some(FixBy::Person), text)
    }

    /// A refusal the same call gets past: what it judged moved while it ran.
    pub(crate) fn retry(text: impl Into<String>) -> Self {
        Self::with(Some(FixBy::Retry), text)
    }

    /// A refusal because the mailbox store could not be read. It wears whatever
    /// [`other_store_failure_word`] says, which is no word until card 2070.
    pub(crate) fn mailbox_store_failure(text: impl Into<String>) -> Self {
        Self::with(other_store_failure_word(), text)
    }

    /// **The one place a blocked body gets its way forward and its word.** A
    /// constructor calls this and never inserts `how_to_proceed` by hand, so a
    /// refusal cannot leave without a word.
    pub(crate) fn write_into(&self, body: &mut serde_json::Value) {
        if let Some(fix_by) = self.fix_by {
            body[FixBy::KEY] = fix_by.as_token().into();
        }
        body["how_to_proceed"] = self.text.as_str().into();
    }
}

/// **Set the word on a refusal an error type produced.** An error type picks its
/// word in one exhaustive match (`fix_by_of`), and its lane's one exit stamps
/// the result here, so the word served is the word the match says and an arm
/// cannot disagree with it. A body that is not a blocked answer is left alone;
/// a `None` word takes any word off.
pub(crate) fn stamp(result: CallToolResult, word: Option<FixBy>) -> CallToolResult {
    let Some(text) = result.content.first().and_then(|b| b.as_text()) else {
        return result;
    };
    let Ok(mut body) = serde_json::from_str::<serde_json::Value>(&text.text) else {
        return result;
    };
    if body["status"] != "blocked" {
        return result;
    }
    match word {
        Some(word) => body[FixBy::KEY] = word.as_token().into(),
        None => {
            if let Some(fields) = body.as_object_mut() {
                fields.remove(FixBy::KEY);
            }
        }
    }
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **Put the word on a protocol error**, in its `data` object, beside the prose
/// the error already carries. A `None` word leaves the error as it was.
pub(crate) fn stamp_error(mut error: McpError, word: Option<FixBy>) -> McpError {
    if let Some(word) = word {
        let mut data = match error.data.take() {
            Some(serde_json::Value::Object(fields)) => fields,
            _ => serde_json::Map::new(),
        };
        data.insert(FixBy::KEY.to_string(), word.as_token().into());
        error.data = Some(serde_json::Value::Object(data));
    }
    error
}

impl From<String> for WayForward {
    /// A bare sentence means the call must change.
    fn from(text: String) -> Self {
        Self::with(Some(FixBy::Change), text)
    }
}

/// **What a refusal says first when the call did not run.** It is true of
/// every verb that reaches the gate or builder that says it, reads included: a
/// refusal shared by a read and a write cannot open with a claim about
/// writing. Where a refusal belongs to a write alone, it still says what was
/// not written.
pub(crate) const NOTHING_RAN: &str = "Nothing ran";

/// What a refusal says first when the verb writes and the call did not.
pub(crate) const NOTHING_WRITTEN: &str = "Nothing was written";

/// **What a refusal opens with, for the verb it is answering.** A builder that
/// serves a read and a write both says the true thing for each: the verbs that
/// write nothing never claim a write was skipped, and the rest still say what
/// was not written.
pub(crate) fn nothing_for(verb: &str) -> &'static str {
    match verb {
        "recall" | "search" | "list_entities" | "list_runs" | "list_sent" | "read_mailbox"
        | "read_message" | "start_here" => NOTHING_RAN,
        _ => NOTHING_WRITTEN,
    }
}

/// **The route from no handle to a handle**, in the door's own two steps. A
/// boot naming only the bot hands back a choice and no handle when the bot has
/// a run in flight, so naming the door alone sends a caller to a dead end.
pub(crate) const ROUTE_TO_A_SID: &str = "Call start_here with your bot name; if you were not told \
    one, call start_here with no bot, which boots nobody and lists the bots you could boot as. A \
    bot with a run in flight is handed a choice and no handle: call start_here again with \
    resume set to the sid of the run you are picking up, or to \"new\" for a fresh session.";

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
    let mut body = serde_json::json!({
        "status": "blocked",
        "wrote": false,
    });
    how_to_proceed.write_into(&mut body);
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

/// **The one line an empty answer carries: what it looked through.**
///
/// An empty answer and a narrowed or withheld one look identical to a caller,
/// and a caller acts on "nothing among these" as if it were "nothing". The
/// line names the population searched, what the read left out and why, and the
/// call that widens it. A verb adds it only when it returns nothing, so an
/// answer that holds something carries no extra bytes.
pub(crate) fn population_line(looked_through: &str, left_out: &[String], widen: &str) -> String {
    let left_out = if left_out.is_empty() {
        "nothing".to_string()
    } else {
        left_out.join("; ")
    };
    format!("Looked through {looked_through}. Left out: {left_out}. To widen: {widen}")
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

/// **The nulls that mean something, kept by the name of the key.** Every other
/// null-valued key is left out of an answer: an absent key says what a null one
/// said, and costs nothing. These stay because a reader is told what the null
/// means and an absent key would say something else:
///
/// - `sender_mail_waiting_at_send`: null is "could not tell", never zero.
/// - `overdue_by_days`: present and null on the one object the distance cannot
///   be measured for; absent would read as "you did not ask".
/// - `ended`: whether the posting run has ended, null when the store could not
///   say.
/// - `sid`: a boot's choice carries no handle until it is answered.
/// - `name`: a null name on a hit means the handle names nothing, a defect to
///   report.
pub(crate) const KEPT_NULLS: &[&str] = &[
    "sender_mail_waiting_at_send",
    "overdue_by_days",
    "ended",
    "sid",
    "name",
];

/// **Remove every null-valued key, at every depth, except [`KEPT_NULLS`].** A null
/// inside an array is a position and stays.
pub(crate) fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.retain(|key, held| !held.is_null() || KEPT_NULLS.contains(&key.as_str()));
            for held in fields.values_mut() {
                strip_nulls(held);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_nulls(item);
            }
        }
        _ => {}
    }
}

/// Apply [`strip_nulls`] to the body of a completed answer. An answer this
/// cannot read as a JSON object is left exactly as the verb wrote it, for the
/// reason [`structure`] leaves one alone.
fn strip_nulls_from(answered: &mut CallToolResponse) {
    let CallToolResponse::Complete(result) = answered else {
        return;
    };
    for block in &mut result.content {
        let Some(text) = block.as_text() else {
            continue;
        };
        let Ok(mut body) = serde_json::from_str::<serde_json::Value>(&text.text) else {
            continue;
        };
        if !body.is_object() {
            continue;
        }
        strip_nulls(&mut body);
        *block = ContentBlock::text(body.to_string());
    }
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
        // Before the answer is counted and before it moves: what is charged to
        // the session and what ships are the same text.
        strip_nulls_from(answered);
        if let Some(sid) = sid {
            self.registry.note_call(sid);
        }
        self.record_served(answered, sid).await;
        if structured {
            structure(answered);
        }
    }

    /// **Add this answer's own size to the running total of what this
    /// session has been handed** (rule 264) — characters, counted from the
    /// same text this call actually ships, after the status bar joins it and
    /// before `structure` moves it anywhere else.
    ///
    /// **Silent when there is nothing to charge it to.** An anonymous
    /// caller, or a caller whose first write has not landed yet, has no card
    /// to hold a total on — accounting attaches to a session that already
    /// exists, exactly as the status bar and every beat do, and a boot that
    /// does nothing still leaves nothing behind. A store that could not take
    /// the write is swallowed here for the same reason a mail count is: an
    /// accounting side-channel must never be why an otherwise-good answer
    /// fails to reach its caller.
    async fn record_served(&self, answered: &CallToolResponse, sid: Option<&str>) {
        let Ok(Some(caller)) = self.caller(sid) else {
            return;
        };
        let Some(card) = caller.card else {
            return;
        };
        let CallToolResponse::Complete(result) = answered else {
            return;
        };
        let chars: u64 = result
            .content
            .iter()
            .filter_map(|block| block.as_text())
            .map(|text| text.text.chars().count() as u64)
            .sum();
        if chars == 0 {
            return;
        }
        let _ = self.sessions.add_served(&card, chars).await;
    }
}

/// **The room an answer keeps for what `finish` adds after the verb has
/// counted its own text**: the status bar. A verb that fills a collection
/// against the answer ceiling subtracts this along with the rest of its answer,
/// so the answer that reaches the client is under the ceiling and not merely
/// the verb's part of it.
pub(crate) const STATUS_BAR_ROOM: usize = 200;

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
    // **The fields only.** `delta` already holds what was sent, what was kept
    // and why, whole, so a line that printed them again would put the caller's
    // own text in the answer twice.
    fields.insert(
        "delta_note".into(),
        format!(
            "stored differs from sent on: {}; see delta",
            differences
                .iter()
                .map(|d| d.field.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into(),
    );
}

/// **Say when an explicit `recorded_at` differs from the day this run is
/// in.** Call it only when both are true — the caller sent one and it does
/// not match the run's own stated day — so the note stays silent on an
/// omitted date, a matching one, and a run with no stated day to differ
/// from. A different question from [`note_delta`]: that one is about what
/// the STORE kept versus what was sent; this is about the caller's own
/// argument versus the frame their run is already working in.
pub(crate) fn note_recorded_at_mismatch(
    body: &mut serde_json::Value,
    run_day: jiff::civil::Date,
    recorded_at: jiff::civil::Date,
) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(
        "recorded_at_note".into(),
        format!(
            "recorded_at names {recorded_at}, which differs from the day this run is in, \
             {run_day}. Leaving recorded_at off would record this under {run_day}."
        )
        .into(),
    );
}

/// **Say which written fields equal what this build ships as their default
/// today.** Never a refusal and never a reason the write skipped storing
/// anything — the value lands exactly as sent either way; this only says
/// what the coincidence means. Silent when nothing echoes, which is every
/// ordinary call: most fields have no shipped default to echo, and most
/// callers are not sending one back.
pub(crate) fn note_echoes_defaults(body: &mut serde_json::Value, keys: &[String]) {
    if keys.is_empty() {
        return;
    }
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    let notes: Vec<serde_json::Value> = keys
        .iter()
        .map(|key| {
            format!(
                "{key} equals what this build ships as its default today. Stored as you sent \
                 it, it keeps this value through a later upgrade; left out, it would have \
                 followed the shipped default instead."
            )
            .into()
        })
        .collect();
    fields.insert("echoes_defaults".into(), notes.into());
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

/// **State that a write landed even though the in-process fold behind it
/// could not confirm it** (rule 130) — the write-path sibling of memory's own
/// coverage note: one vocabulary for "an in-process projection is behind the
/// store it mirrors", reused rather than reinvented (rule 51).
///
/// **Never [`Behind::Unscanned`].** [`jojobot_domain::memory::MemoryError::FoldBehind`]
/// is raised only after a write already landed, which is the write-path
/// route into [`Behind::Stale`] alone — that type's own doc names it.
pub(crate) fn note_fold_behind(body: &mut serde_json::Value, behind: Behind) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    let note = match behind {
        Behind::Stale => {
            "This write landed and is in the store. The in-process fold that backs \
                          a candidate-picking read of this entity could not be refreshed \
                          afterward, so such a read may still answer with what stood before this \
                          write until the entity is next touched. recall and history read the \
                          store directly and already reflect it."
        }
        Behind::Unscanned => {
            "unreachable: a write-path fold gap is always reported as stale, never unscanned"
        }
    };
    fields.insert(
        "fold".into(),
        serde_json::json!({ "behind": behind.as_token(), "note": note }),
    );
}

/// **Say what day a resumed run is in, when the resuming call named none of
/// its own.** A call that states its own day answers in that frame and needs
/// no note — this is for the caller who sent nothing and would otherwise
/// have no way to tell "the run already has a day" from "nothing has a day."
pub(crate) fn note_resumed_day(
    body: &mut serde_json::Value,
    started_on: Option<jiff::civil::Date>,
) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    let note = match started_on {
        Some(day) => format!("This run is in {day}, set by an earlier call."),
        None => "This run has no stated day. The server's clock is used.".to_string(),
    };
    fields.insert("day_note".into(), note.into());
}

/// **Name the starred rule a write just pushed off the boot's seats.** A
/// bot's boot carries a fixed number of marked rules; starring one past that
/// number does not refuse the write — see
/// [`crate::Jojobot::note_seat_pushed_off`] — it names, by address, the
/// oldest starred rule that no longer rides, so a caller learns of the gap
/// here rather than only at the bot's next boot.
pub(crate) fn note_seat_dropped(body: &mut serde_json::Value, dropped: &str, sentence: &str) {
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    fields.insert(
        "seats".into(),
        serde_json::json!({ "dropped": dropped, "note": sentence }),
    );
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

/// **Say when a type NAME this call resolved has a caller's own declaration
/// displaced underneath it.** A caller who names a type — in `answers_type`,
/// `fits_type`, or any other argument that resolves one — gets matched
/// against the type the software ships now, and if that used to be the
/// caller's own, with different keys, matching silently against the wrong
/// shape is the same silence `declare_type`'s own refusal used to carry
/// before it named this too.
///
/// **Once per name, never per object matched against it.** `declare_type`'s
/// own refusal already covers a caller who tries to redeclare the name; this
/// is for the query path that never gets that far, because it never tries to
/// write.
///
/// **Silent when nothing was ever displaced under this name** — the ordinary
/// case, and the positive this note rests on. Appends rather than overwrites,
/// the way `note_teaching` does: `answers_type` and `fits_type` can name two
/// different displaced types in the same call, and a single key either could
/// overwrite would silently drop one.
pub(crate) fn note_type_displaced(
    body: &mut serde_json::Value,
    name: &str,
    displaced: Option<&Displaced>,
) {
    let Some(displaced) = displaced else {
        return;
    };
    let Some(fields) = body.as_object_mut() else {
        return;
    };
    let note = format!(
        "'{name}' held keys {}, declared by a caller, until {}, when the software's own \
         declaration took the name over. What follows is matched against the software's keys, \
         not those.",
        displaced
            .fields
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        displaced.replaced_on,
    );
    fields
        .entry("type_displaced")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .expect("type_displaced is always written as an array")
        .push(note.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use rmcp::handler::server::wrapper::Parameters;

    /// 🚨 **An empty way forward cannot be built at all** (decision log 261,
    /// 262) — the constructor is the one door, and it panics rather than
    /// handing back a `WayForward` that says nothing. This is the mechanism
    /// itself, independent of any one refusal that uses it.
    #[test]
    #[should_panic(expected = "way forward")]
    fn a_blank_way_forward_cannot_be_built() {
        let _: WayForward = "   ".into();
    }

    /// The ordinary case: real prose survives the constructor unchanged, and a
    /// bare sentence means the call must change.
    #[test]
    fn a_real_way_forward_survives_construction() {
        let built: WayForward = "call add_entity first".into();
        let mut body = serde_json::json!({});
        built.write_into(&mut body);
        assert_eq!(body["how_to_proceed"], "call add_entity first");
        assert_eq!(body[FixBy::KEY], "change");
    }

    /// **The three words, each from its own constructor, and the word is the
    /// key's value on the wire.** Pinned as literals because the spelling is
    /// served and nothing outside this process declares it: a caller branches on
    /// these three strings.
    #[test]
    fn each_constructor_writes_its_own_word_beside_the_prose() {
        for (way, word) in [
            (
                WayForward::from("send a different call".to_string()),
                "change",
            ),
            (WayForward::person("a person has to repair it"), "person"),
        ] {
            let mut body = serde_json::json!({"status": "blocked"});
            way.write_into(&mut body);
            assert_eq!(body["fix_by"], word, "{body}");
            assert!(body["how_to_proceed"].is_string(), "{body}");
        }
        assert_eq!(FixBy::KEY, "fix_by");
        assert_eq!(FixBy::Retry.as_token(), "retry");
        let mut body = serde_json::json!({"status": "blocked"});
        WayForward::mailbox_store_failure("the store failed").write_into(&mut body);
        assert_eq!(
            body["fix_by"].as_str(),
            other_store_failure_word().map(FixBy::as_token),
            "a mailbox storage failure wears whatever its switch says: {body}"
        );
    }

    /// **The switch and the stamp.** With no word a stamped refusal carries
    /// none (the shape the store-failure switch produces when it is off); with
    /// one it carries exactly that one, replacing any other; a body that is no
    /// refusal is left alone; and a protocol error carries the word in `data`
    /// beside its message.
    #[test]
    fn the_stamp_sets_replaces_and_removes_the_word_and_leaves_other_answers_alone() {
        let refusal = || {
            CallToolResult::success(vec![ContentBlock::text(
                serde_json::json!({"status": "blocked", "wrote": false, "fix_by": "change"})
                    .to_string(),
            )])
        };
        assert_eq!(
            json_of(&stamp(refusal(), Some(FixBy::Person)))["fix_by"],
            "person"
        );
        assert!(json_of(&stamp(refusal(), None)).get("fix_by").is_none());
        let receipt = CallToolResult::success(vec![ContentBlock::text(
            serde_json::json!({"id": "x"}).to_string(),
        )]);
        assert!(
            json_of(&stamp(receipt, Some(FixBy::Retry)))
                .get("fix_by")
                .is_none()
        );

        let error = stamp_error(
            McpError::internal_error("the store failed", None),
            Some(FixBy::Retry),
        );
        assert_eq!(error.message, "the store failed");
        assert_eq!(error.data.expect("data")["fix_by"], "retry");
        assert!(
            stamp_error(McpError::internal_error("x", None), None)
                .data
                .is_none()
        );
        assert_eq!(
            memory_store_failure_word(),
            MEMORY_STORE_FAILURE_IS_RETRY.then_some(FixBy::Retry),
            "the switch and the word it yields agree"
        );
        assert_eq!(
            other_store_failure_word(),
            OTHER_STORE_FAILURE_IS_RETRY.then_some(FixBy::Retry),
            "the switch and the word it yields agree"
        );
    }

    /// **Two different names can each carry a displaced record in one
    /// answer, and neither overwrites the other** — the same
    /// append-not-overwrite property `note_teaching` already guards,
    /// proven here for its sibling. Mechanism level, independent of any one
    /// verb that calls this.
    #[test]
    fn note_type_displaced_appends_rather_than_overwrites() {
        use jiff::civil::date;
        use jojobot_domain::memory::types::{Displaced, Field, ValueType};

        let mut body = serde_json::json!({});
        note_type_displaced(
            &mut body,
            "roster-a",
            Some(&Displaced {
                name: "roster-a".into(),
                fields: vec![Field::new("shift_lead", ValueType::Reference)],
                replaced_on: date(2026, 4, 18),
            }),
        );
        note_type_displaced(
            &mut body,
            "roster-b",
            Some(&Displaced {
                name: "roster-b".into(),
                fields: vec![Field::new("cover", ValueType::Reference)],
                replaced_on: date(2026, 4, 18),
            }),
        );

        let notes = body["type_displaced"]
            .as_array()
            .expect("both notes are recorded");
        assert_eq!(
            notes.len(),
            2,
            "the second call must not overwrite the first: {body}"
        );
        let joined = notes
            .iter()
            .filter_map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            joined.contains("roster-a") && joined.contains("shift_lead"),
            "{joined}"
        );
        assert!(
            joined.contains("roster-b") && joined.contains("cover"),
            "{joined}"
        );
    }

    /// **`None` adds nothing at all** — the positive above rests on this,
    /// and it is the ordinary case: most names were never displaced.
    #[test]
    fn note_type_displaced_is_silent_on_none() {
        let mut body = serde_json::json!({});
        note_type_displaced(&mut body, "never-displaced", None);
        assert!(body.get("type_displaced").is_none(), "{body}");
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

    /// **A session handed several answers totals them, and an untouched
    /// session reads zero — in the same read** (rule 264).
    ///
    /// Both cases matter together: a build that never called `add_served`
    /// would still pass the untouched half, and a build that always
    /// reported zero would still pass a case that never checked a positive
    /// total. Driven through `record_served` itself — the one seat every
    /// answer passes through via `finish` — rather than through `add_served`
    /// directly, which the domain and adapter suites already cover on their
    /// own layers.
    ///
    /// One real write materializes each session's card first — `finish` is
    /// not on the path a test calls a verb through directly, so neither
    /// materializing write is itself counted, exactly as production never
    /// counts the write that opens a session before `finish` runs on its
    /// own answer.
    #[tokio::test]
    async fn several_answers_total_and_an_untouched_session_reads_zero() {
        let jojobot = handler();
        let handed_sid = jojobot
            .registry
            .mint_with(&EntityId("bot:milhouse".into()), None, || {
                "svmm".to_string()
            })
            .expect("a free handle")
            .0;
        let untouched_sid = jojobot
            .registry
            .mint_with(&EntityId("bot:gamma".into()), None, || "svbt".to_string())
            .expect("a free handle")
            .0;

        jojobot
            .journal(Parameters(JournalArgs {
                entry: "first beat".into(),
                focus: None,
                sid: handed_sid.clone(),
            }))
            .await
            .expect("journal ok");
        jojobot
            .journal(Parameters(JournalArgs {
                entry: "first beat".into(),
                focus: None,
                sid: untouched_sid.clone(),
            }))
            .await
            .expect("journal ok");

        let first: CallToolResponse =
            CallToolResult::success(vec![ContentBlock::text("a".repeat(120))]).into();
        let second: CallToolResponse =
            CallToolResult::success(vec![ContentBlock::text("b".repeat(340))]).into();
        jojobot.record_served(&first, Some(&handed_sid)).await;
        jojobot.record_served(&second, Some(&handed_sid)).await;

        let handed_card = jojobot
            .caller(Some(&handed_sid))
            .expect("resolved")
            .expect("bound")
            .card
            .expect("materialized by the journal write above");
        let untouched_card = jojobot
            .caller(Some(&untouched_sid))
            .expect("resolved")
            .expect("bound")
            .card
            .expect("materialized by the journal write above");

        let handed = jojobot
            .sessions
            .read_session(&handed_card)
            .await
            .expect("read ok");
        let untouched = jojobot
            .sessions
            .read_session(&untouched_card)
            .await
            .expect("read ok");
        assert_eq!(
            handed.served_chars, 460,
            "the two recorded answers total: {handed:?}"
        );
        assert_eq!(
            untouched.served_chars, 0,
            "a session record_served was never called for reads zero: {untouched:?}"
        );
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
    /// **The refusals every lane shares**, built outside any error enum: a call
    /// whose arguments are each fine and wrong together, a call with no session,
    /// and a boot the door declined. Each is a call that has to change.
    #[test]
    fn the_refusals_every_lane_shares_wear_the_change_word() {
        assert_fix_by("misused", &misused("send one of the two"), "change");
        assert_fix_by(
            "session_unbound",
            &crate::caller::session_unbound(),
            "change",
        );
        assert_fix_by(
            "handle_declined",
            &crate::caller::handle_declined("zz99", "boot as a bot that exists".to_string()),
            "change",
        );
    }
    /// **A key whose value is null is not sent**, at every depth, in an answer
    /// and in a refusal alike: an absent key says the same as a null one and
    /// costs nothing. The few nulls that MEAN something stay, and stay by name
    /// (see [`KEPT_NULLS`]). A null inside an array is a position and stays.
    #[test]
    fn a_null_key_is_removed_at_every_depth_and_the_documented_ones_stay() {
        let mut body = serde_json::json!({
            "id": "x",
            "details": null,
            "claim": {"edge": null, "words": "kept", "deep": {"derived_from": null, "n": 1}},
            "rows": [{"happened_at": null, "k": 1}, null, {"sender_mail_waiting_at_send": null}],
            "overdue_by_days": null,
        });
        strip_nulls(&mut body);
        assert_eq!(
            body,
            serde_json::json!({
                "id": "x",
                "claim": {"words": "kept", "deep": {"n": 1}},
                "rows": [{"k": 1}, null, {"sender_mail_waiting_at_send": null}],
                "overdue_by_days": null,
            })
        );
        // The kept names are the documented ones, pinned as literals because
        // the spelling is served and a caller reads the difference.
        for kept in [
            "sender_mail_waiting_at_send",
            "overdue_by_days",
            "ended",
            "sid",
            "name",
        ] {
            assert!(KEPT_NULLS.contains(&kept), "{kept}");
        }
    }
}
