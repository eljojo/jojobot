use super::super::AcceptsOverride;

/// Arguments to `rename_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameEntityArgs {
    /// The entity's current handle — what it answers to now. A handle it
    /// answered to before an earlier move still resolves in a mention, an edge, a
    /// reference-typed field value, a claim's home, a journal beat and a
    /// mailbox message — all of them stay current after a move. Given here, a
    /// stale handle is refused with the name it moved to, rather than a bare
    /// miss.
    pub(crate) handle: String,
    /// The destination, as `kind:slug`. Always fully qualified — unlike a
    /// capture's subject, a bare slug is never read as a kind by default
    /// here, because guessing one would be guessing whether this call is a
    /// reslug or a retype. Changing the kind IS a retype; changing the slug
    /// IS a reslug; the id is one string, so either, or both together, is
    /// this same call.
    ///
    /// `to` may name the same handle as `handle` when only the parent is moving,
    /// since a reparent needs no reslug; but naming the same handle with no
    /// `parent`, or the parent it already has, is refused, because that call would
    /// change nothing.
    pub(crate) to: String,
    /// Reparent onto this handle. Omit to leave the current parent alone.
    /// **There is no way to clear a parent through this verb.** `to` may
    /// name the same handle as `handle` when this is the only thing
    /// changing — a reparent with no reslug — but naming the same handle
    /// with no parent, or the parent it already has, is refused: that call
    /// would change nothing.
    ///
    /// `parent` reparents when given and leaves the current parent alone when
    /// omitted; there is no way to clear a parent through this verb.
    #[serde(default)]
    pub(crate) parent: Option<String>,
    /// The day this rename was made, `YYYY-MM-DD`. Defaults to today in the
    /// caller's own day.
    #[serde(default)]
    pub(crate) recorded_at: Option<String>,
    /// The token a previous call's refusal handed you, sent back after you
    /// read its candidates for the DESTINATION handle and judged them a
    /// different entity. It lifts only the refusal that minted it, and
    /// never an exact handle collision — a handle has exactly one owner.
    #[serde(default)]
    pub(crate) override_token: Option<String>,
    /// **Why you insist**, sent beside `override_token` when the refusal rests
    /// on the name alone: the same name under another kind, a name that holds or
    /// sits inside another, a known nickname of another, or two rare words shared
    /// with another of the same kind. Without it that refusal stands. The reason is
    /// kept on the renamed entity as `told_apart_by`, shown beside its name.
    #[serde(default)]
    pub(crate) override_reason: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. This call is made of parts, the entity, the
    /// override reason and the box: if an answer is lost or a part fails, send
    /// the same call again with the same `request_id` and the parts that landed
    /// are not written again and the rest are. It needs a `sid`, and a call that
    /// wrote nothing spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
}

/// **A deliberate, visible act**: this verb's own refusal is allowed to
/// promise an override because this impl says its `Args` really carries
/// one. See [`AcceptsOverride`].
impl AcceptsOverride for RenameEntityArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
    fn override_reason(&self) -> Option<&str> {
        self.override_reason.as_deref()
    }
}
