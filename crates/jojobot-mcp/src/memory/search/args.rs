use super::*;

/// The `edge` filter of a `search` — a shape and the entity it points at.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeFilterArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about` ·
    /// `connection`), or the word a read served for it (`memberOf`, `attendee`,
    /// `relatedTo`): either is understood.
    /// Omit for **any** edge pointing at `object` — "what's connected to X".
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// The entity the edge must point at, as `kind:slug`.
    pub(crate) object: String,
}

/// Arguments to `search`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Free text over entity handles/names, fact claims and details, and an
    /// entity's prose. **Words are matched by stem, and a record needs at least
    /// half of the query's terms, rounded up: two terms need one.** Optional when at least one filter
    /// below is given.
    #[serde(default)]
    pub(crate) query: Option<String>,
    /// Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose prose matched.
    ///
    /// A `kind` filter leaves mail out even when you ask for it, since a message
    /// belongs to no entity and so has no kind to match.
    #[serde(default)]
    pub(crate) kind: Option<String>,
    /// `active` or `archived`. **`status`, `provenance`, `standing`, `subject` and
    /// `edge` ask about facts, so a search that uses one returns facts only;
    /// `answers_type` and `fits_type` ask about things and return things only.
    /// `left_out` does not list the kinds of hit they leave out.**
    /// **Left unset, the search behaves like a mail
    /// client**: active claims first, then archived ones — a claim that was
    /// true once and was replaced or moved past — ranked below every active
    /// one, and a hit that was replaced names the claim that replaced it in
    /// `superseded_by` when include_links is set. A claim taken back as never true (retracted) is left
    /// out. `active` is current truth only. `archived` reads every archived
    /// claim, the retracted ones too.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `testimony`, `observation` or `inference` — keep only the claims backed
    /// that way. `observation` is a claim an AI read out of a system of record,
    /// and it names the system it was read from.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// `settled` or `open` — **the other certainty axis, and the one that
    /// answers *which of these am I not sure about***. `provenance` says who
    /// backs a claim; this says how sure anybody is of it. A hedge the operator
    /// gave you, and an inference nobody confirmed, are both `open`.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// Facts about this entity, as `kind:slug`.
    #[serde(default)]
    pub(crate) subject: Option<String>,
    /// Facts drawing a matching edge. With `kind`, this is how a cross-entity
    /// question ("which people are in X") is answered in one call.
    #[serde(default)]
    pub(crate) edge: Option<EdgeFilterArgs>,
    /// **THINGS that answer this type, by name.** Matching is STRUCTURAL: a
    /// thing carrying the type's keys comes back whether or not anybody
    /// declared it to be one, so this finds things nobody filed under it. It is
    /// asked of the thing rather than of one of its records — what a thing is
    /// gets written down a piece at a time, and every write on it counts.
    ///
    /// A thing carrying only some of the keys comes back too, saying which it
    /// lacks — partial matches are the ones usually worth finding, so they are
    /// reported and never filtered out. Each hit carries an `answers_type`
    /// saying which keys it holds, which it lacks by name, and any whose value
    /// is not what the type said it holds.
    ///
    /// ⚠️ **Some of the keys is enough here.** If you want only the things
    /// with no gaps — *which of these ARE services*, rather than *which are
    /// described like one, and what is missing* — that is `fits_type`, below.
    ///
    /// A name no type answers to comes back blocked, naming the types that do
    /// exist.
    ///
    /// An answers_type that matches nothing carries `type_keys`, the type's keys,
    /// and the first such answer in a session under a type the software ships, with
    /// nothing else narrowing the search, also carries a teaching. In `type_keys`,
    /// `required: true` marks a key the type counts as its floor and never means
    /// required to write: a record that holds some of the keys is kept and found here.
    /// `fits_type` keeps only the things that hold every key, optional ones too. Reach for
    /// answers_type unless you have a reason not to: a thing that arrives with its
    /// gaps named can be judged, and a thing fits_type leaves out looks exactly
    /// like a thing that is not there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) answers_type: Option<String>,
    /// **Only the things that FIT this type**, by name — the ones carrying
    /// EVERY key it names, counted over every write on each.
    ///
    /// The strict half of the same question, and which one you are asking is
    /// yours to choose: `answers_type` asks *which of these are described like
    /// a service, and what is missing*, and this asks *which of these ARE
    /// services*.
    ///
    /// Reach for `answers_type` unless you have a reason not to. A thing that
    /// arrives with its gaps named can be judged; a thing this filter leaves
    /// out looks exactly like a thing that is not there.
    ///
    /// Pass one or the other, never both. A name no type answers to comes back
    /// blocked, naming the types that do exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fits_type: Option<String>,
    /// Whether messages left in mailboxes are searched too. **Defaults to
    /// false, and worth passing true** when you are looking for what a session
    /// knows: a report filed for another session is exactly the context you
    /// would not know to go looking for, and this is the one call that finds
    /// it. It is off unless you ask, because a message hit carries somebody's
    /// box, sender and a snippet, and this verb is the one to reach for first.
    ///
    /// Mail is off unless you ask, because a message hit carries somebody's box,
    /// sender and a snippet and this is the verb to reach for first. Reach for it
    /// when you want what a session knows: a report filed for another session is
    /// exactly the context you would not know to go looking for. `mail` in the
    /// answer says whether messages were searched (`searched: false` means none
    /// was) and can still be partial after a degraded start, where the hits are
    /// real but anything older than this server's start is missing. `memory` says
    /// the same for entities, facts and prose, and with a note the index is behind
    /// the store, where `behind` says how much: `unscanned` (the index holds only
    /// what jojobot has written since this server started) or `stale` (the index
    /// holds an older version of one entity than the store, so a hit may be a
    /// version the store has moved past). The hits are real either way, and
    /// `recall` is complete.
    #[serde(default)]
    pub(crate) include_mail: Option<bool>,
    /// Include relationships and references on results. Omitted by default.
    #[serde(default)]
    pub(crate) include_links: Option<bool>,
    /// **Whether matching also reaches a claim's earlier wordings.** Defaults
    /// to false: an ordinary search matches only the current wording of a
    /// claim, exactly as the corpus note says. Worth passing true when a
    /// query on the current wording came back thin — a claim that was
    /// corrected may hold, in what it used to say, the words you are
    /// looking for. A hit that matches through this is the same current
    /// record every other fact hit is; reading the earlier wording itself is
    /// still recall with history_record.
    #[serde(default)]
    pub(crate) include_history: Option<bool>,
    /// **The size of a page**: how many results come back at most; defaults to
    /// 5. An answer carries the best of them that fit under the answer ceiling
    /// and says how many it left out; `offset` reads on from there.
    #[serde(default)]
    pub(crate) limit: Option<u32>,
    /// **How many of the ranked results you have already read**, which is where
    /// the next page starts. The answer stops at the ceiling and names the
    /// `offset` that returns the next part; repeat the same call with it. The
    /// same `limit` with a larger `offset` walks to the end of the same ranking
    /// and returns every hit exactly once. Defaults to 0.
    #[serde(default)]
    pub(crate) offset: Option<u32>,
    /// **Whether each claim comes back with its details.** Off by default: a hit
    /// leads with the matching passage, its address, provenance and standing; the
    /// details (the reasons under a claim) are a paragraph the reader has not
    /// asked for. An answer that left details out says once, in
    /// `details_left_out`, how many it left out, the first twenty addresses and how to ask. Send `true` to
    /// read them. Words in a claim's details still match.
    #[serde(default)]
    pub(crate) details: Option<bool>,
    /// **Whether a thing's review mark is among the hits.** Off by default: the
    /// mark is the one record defrag leaves on a thing it has reviewed, which is
    /// bookkeeping to every other reader. The answer names it in `left_out` while
    /// it is left out. Send `true` to read it. `recall` takes the same argument.
    /// A bot's record of the release notes it was told is jojobot's bookkeeping
    /// too, left out and read back the same way.
    #[serde(default)]
    pub(crate) review_marks: Option<bool>,
    /// **The recency tie-break day: `recorded_on`** (the default) or `happened_at`.
    /// More matching query terms and closer words lead before recency is considered.
    ///
    /// ⚠️ **A claim with no `happened_at` ranks by `recorded_on` instead**,
    /// and the answer's own `rank_fallbacks` says how many that happened to
    /// — an incomplete answer, accepted and explained, never a silent mix
    /// of two orderings.
    ///
    /// `clock` picks the day ranking reads: `recorded_on` (the default) is the
    /// day in a claim's `recorded_at`, and `happened_at` is the day the thing
    /// itself did.
    #[serde(default)]
    pub(crate) clock: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled. With one, your own past runs also come
    /// back as hits, ranked last; without one, no run does.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}
