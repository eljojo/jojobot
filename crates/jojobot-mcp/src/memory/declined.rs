//! **Memory's refusals** — the guard's answers, and the line between a refusal
//! and a failure.
//!
//! A blocked result is a SUCCESS whose body says `status: "blocked"`,
//! `wrote: false`: the caller named something that resembles what exists, or
//! named something that is not there, and nothing was written. A plain error is
//! a malformed call or the store itself failing. Callers branch on `status`;
//! they should never have to parse a failure.

use super::*;

/// **A verb whose own `Args` genuinely carries an `override_token` field.**
/// The only way to implement this is to write `fn override_token` naming a
/// real field, in a visible `impl` beside that struct's own definition — so
/// the three verbs that accept one implement it and nothing else does, on
/// purpose, in a place a reader finds. [`TokenSlot::from`] is the only
/// thing that reads this trait, and it is the only way to build a
/// [`TokenSlot`].
pub(crate) trait AcceptsOverride {
    fn override_token(&self) -> Option<&str>;
    /// The reason sent beside the token. It is what a refusal that rests on
    /// the name alone asks for (rule 498).
    fn override_reason(&self) -> Option<&str> {
        None
    }
}

/// **Evidence that a promising arm is being built from a verb that actually
/// accepts an override.** Its field is private to this module, so nothing
/// outside `TokenSlot::from` can construct one — not `None`, not
/// `Default::default()`, not a bare literal. The only door in is a
/// reference to something implementing [`AcceptsOverride`], and the only
/// things that implement it are the three `Args` structs with a real
/// `override_token` field.
///
/// Owns its value rather than borrowing it: a call site builds this once,
/// from `args`, and then goes on to move `args`' other fields into the
/// write it is about to make — a borrow would still be alive for that and
/// refuse to compile for an unrelated reason, which would make the error a
/// caller sees here about lifetimes rather than about the guard.
///
/// The value inside plays no role in [`blocked_result`] — the token it
/// mints is a pure function of `attempted` and `candidates`, unchanged by
/// what rides here. The slot's only job is existing: a verb with no
/// `override_token` field has no `AcceptsOverride` impl to read through, so
/// it has no way to build one at all.
///
/// **This is not unforgeable, and does not claim to be.** Nothing stops a
/// hurried author from writing a fresh `impl AcceptsOverride for
/// SomeArgs { fn override_token(&self) -> Option<&str> { None } }` beside
/// the call and wiring a promising arm to a verb that still has no real
/// slot — the type system cannot see that the `None` is invented. What it
/// buys is that doing so is never silent: it is a whole new `impl` block,
/// naming the trait, in a place a diff shows and a reader can ask "why does
/// this verb suddenly accept overrides?" — never a one-line change to an
/// existing call, which is what the gap before this looked like.
pub(crate) struct TokenSlot {
    /// Whether the call already sent the token this refusal could mint. The
    /// advice reads only this, never the token: a refusal for a reason still owed
    /// says so rather than repeating the first advice.
    token_sent: bool,
    reason_sent: bool,
}

impl TokenSlot {
    /// Whether the call sent a token and no reason beside it.
    pub(crate) fn reason_owed(&self) -> bool {
        self.token_sent && !self.reason_sent
    }

    pub(crate) fn from(args: &impl AcceptsOverride) -> Self {
        TokenSlot {
            token_sent: args.override_token().is_some(),
            reason_sent: args
                .override_reason()
                .is_some_and(|reason| !reason.trim().is_empty()),
        }
    }
}

impl Jojobot {
    /// **Each candidate with what tells it apart from a thing that resembles it**,
    /// read from the thing's own fields. A candidate nobody said how to tell
    /// apart, or whose fields cannot be read, comes back as it was: the refusal is
    /// served either way.
    pub(crate) async fn told_apart(&self, candidates: Vec<EntityMatch>) -> Vec<EntityMatch> {
        let mut told = Vec::with_capacity(candidates.len());
        for mut candidate in candidates {
            candidate.told_apart_by = self
                .memory
                .fields(&candidate.handle)
                .await
                .ok()
                .and_then(|fields| fields.get(guard::TOLD_APART_BY).cloned())
                .filter(|reason| !reason.trim().is_empty());
            told.push(candidate);
        }
        told
    }
}

/// **A thought ceiling exists on purpose**, so the refusal of a write to one
/// points at the route that same refusal names and treats a change to the
/// ceiling as the exception. It claims no earlier refusal, because a first
/// attempt has none. Any other key's refusal is returned as it is.
fn thought_ceiling_note(key: &str, subject: &str, said: String) -> String {
    use jojobot_domain::memory::{THOUGHT_AGES_AFTER_RUNS, THOUGHT_BODY_CAP, THOUGHT_CAPACITY};
    if matches!(
        key,
        THOUGHT_CAPACITY | THOUGHT_BODY_CAP | THOUGHT_AGES_AFTER_RUNS
    ) {
        format!(
            "{said} Use the route named above, and ask for a change only when \
             the job of '{subject}' has changed."
        )
    } else {
        said
    }
}

/// **The reason a call carries, when it carries one that counts**: present,
/// not blank once trimmed, and sent beside the token it explains. A reason
/// alone overrides nothing, so it is not kept.
pub(crate) fn kept_override_reason<'a>(
    token: Option<&str>,
    reason: Option<&'a str>,
) -> Option<&'a str> {
    reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty() && token.is_some())
}

/// **Whether a handle names a week of usage**, the one question every refusal
/// that words a week differently from a thing asks (rule 498).
fn is_a_week_of_usage(handle: &EntityId) -> bool {
    handle.kind() == Some(EntityKind::USAGE)
}

/// **Whether a candidate is a near slug that differs from the attempt only by a
/// number or a date**, the pair the screen lets stand when both sit under one
/// parent. The screen's own test answers (rule 498).
fn is_a_series_made_without_a_parent(attempted: &EntityId, candidates: &[EntityMatch]) -> bool {
    candidates.iter().any(|c| {
        c.reason == guard::MatchReason::NearSlug
            && guard::numbered_apart(attempted.slug(), c.handle.slug())
    })
}

/// **Whether any candidate is a resemblance of the name alone**, which takes a
/// reason beside the token to lift (rule 498).
fn rests_on_the_name_alone(candidates: &[EntityMatch]) -> bool {
    candidates.iter().any(|c| c.reason.needs_a_reason())
}

/// **The advice for a refusal that rests on the name alone.** It names the
/// candidates the reason is about. A call that already sent the token is told
/// the token was not enough, rather than handed the first advice again.
fn reason_advice(
    verb: &str,
    nothing: &str,
    attempted: &EntityId,
    candidates: &[EntityMatch],
    slot: &TokenSlot,
    token: &str,
) -> String {
    let by_name = candidates
        .iter()
        .filter(|c| c.reason.needs_a_reason())
        .map(|c| c.handle.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if slot.token_sent && !slot.reason_sent {
        format!(
            "{nothing} The override_token was not enough: '{attempted}' resembles {by_name} by \
             name alone, and lifting that takes a reason. Re-call {verb} with override_token: \
             \"{token}\" and override_reason: \"<why these are different things>\". The reason \
             is kept as told_apart_by on the thing, and shown beside its name."
        )
    } else {
        format!(
            "{nothing} If '{attempted}' IS one of the entities above, use that one instead. \
             '{attempted}' resembles {by_name} by name alone. If it is genuinely a different \
             thing, re-call {verb} with override_token: \"{token}\" and override_reason: \
             \"<why these are different things>\". The reason is kept as told_apart_by, shown beside the \
             name. That token \
             belongs to THIS refusal and lifts no other."
        )
    }
}

/// Which gate stopped a write — because the way out of each one is different,
/// and one copy-pasted paragraph telling a rename to "pick a more qualified
/// slug" is worse than no advice at all.
///
/// **The three arms that promise an `override_token` each take a
/// [`TokenSlot`], never the value itself.** Before this, nothing tied the
/// choice of arm to what the verb actually accepts, and the one place that
/// went wrong (`add_entity`'s parent refusal, 5e2d18a7) was fixed by
/// routing around the shared body rather than by anything that would have
/// stopped a second one.
pub(crate) enum Blocked {
    /// A creation: the handle is being minted here, so an exact collision is
    /// unforgivable and the token covers only a shared *name*. The
    /// [`TokenSlot`] is never read — see its own doc for why holding one at
    /// all is the point.
    Creating(TokenSlot),
    /// A relabel — a change to a name or an alias. No handle is moving, so
    /// nothing here is unforgivable.
    Relabelling(#[allow(dead_code)] TokenSlot),
    /// A rename's destination handle. Unlike [`Relabelling`](Self::Relabelling)
    /// a handle IS moving here — that is the whole verb — so the advice must
    /// not say otherwise; unlike [`Creating`](Self::Creating) the thing is
    /// not new, it already answers to a different name.
    Renaming(#[allow(dead_code)] TokenSlot),
    /// A write that only **names** an entity (a capture's subject, an edge's
    /// object). It cannot create one, so there is no token to hand back and no
    /// `override_token` on the verb.
    MustExist(&'static str),
}

/// **Does a candidate stand for the very record the call edits.** A stored
/// thing is never its own near miss, so this is true only when the layer that
/// holds what the build supplies refused the edit of that record itself.
fn names_itself(attempted: &EntityId, candidates: &[EntityMatch]) -> bool {
    candidates
        .iter()
        .any(|c| c.reason == guard::MatchReason::ExactHandle && &c.handle == attempted)
}

/// The write guard's answer: **nothing was written**, and here is what jojobot
/// suspects you meant.
///
/// A **successful** result carrying a structured payload, not a protocol error.
/// The guard doing its job is an answer the caller has to act on — jojobot
/// detects, the AI decides — and dressing it as an exception made a working
/// feature read like a broken server: clients that retry on error retry it, and
/// clients that unwrap on error handle it exactly wrong. `status` and `wrote`
/// are what stop it reading as a completed write.
pub(crate) fn blocked_result(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    gate: Blocked,
) -> CallToolResult {
    let exact = candidates
        .iter()
        .any(|c| c.reason == guard::MatchReason::ExactHandle);
    // **The token this refusal mints, and the only thing that lifts it** (rule
    // 75). It rides the advice rather than sitting in a field of its own,
    // because a secret a caller has to be told separately about is one nobody
    // uses — the sentence that says what to do names the thing to do it with
    // (rule 68). An exact collision mints none: there is nothing to lift.
    let token = guard::override_token(attempted, candidates);
    let how_to_proceed = match gate {
        Blocked::Creating(_) if exact => format!(
            "Nothing was written. The handle '{attempted}' is already taken, and that cannot be \
             forced — a handle has exactly one owner. Either this IS the entity above (use its \
             handle and carry on), or it is a different one and needs a more qualified slug.",
        ),
        // **A resemblance of the name alone is lifted with a reason** (rule 498).
        Blocked::Creating(slot) if rests_on_the_name_alone(candidates) => reason_advice(
            "add_entity",
            "Nothing was written.",
            attempted,
            candidates,
            &slot,
            &token,
        ),
        // **Two roots numbered apart are the entries of one series**, which lands
        // one call each under one parent. The screen skips such a pair only under
        // one parent (rule 498), so a first entry that is already a root is moved
        // under it with `rename_entity`: naming the parent on the new call alone
        // is refused again.
        Blocked::Creating(_) if is_a_series_made_without_a_parent(attempted, candidates) => {
            format!(
                "Nothing was written. If '{attempted}' IS one of the entities above, use that \
                 handle instead. If '{attempted}' and an entity above are entries of one \
                 numbered or dated series, make the series under one parent: call add_entity for \
                 each entry with the same `parent`, and each one lands the first time. An entry \
                 above that sits under no parent, or under another, is moved first: call \
                 rename_entity with its handle, `to` the same handle and that `parent`; the \
                 move does not change the handle. If it is genuinely a different thing, re-call add_entity with override_token: \
                 \"{token}\". That token belongs to THIS refusal and lifts no other.",
            )
        }
        Blocked::Creating(_) => format!(
            "Nothing was written. If '{attempted}' IS one of the entities above, use that handle \
             instead. If it is genuinely a different one that happens to share a name, re-call \
             add_entity with override_token: \"{token}\". That token belongs to THIS refusal and \
             lifts no other. Display names are not unique and never have to be; the handle is \
             what has to be.",
        ),
        // **The record the call edits is the build's own.** Its name, aliases,
        // source and crm come from the build, and no token lifts that, so none is
        // offered. The way that works is a write on top of it: `capture`, or
        // `update_entity` with `sets` alone.
        Blocked::Relabelling(_) if names_itself(attempted, candidates) => format!(
            "Nothing was written: '{attempted}' is a record the build supplies, so its name, \
             aliases, source and crm come from the build and are not edited here. Facts and \
             settings written to it with capture, or with update_entity and only `sets`, are \
             kept on top of what the build ships. No token changes this refusal.",
        ),
        // Says "name" rather than "rename": this gate fires on an alias write
        // too, and telling a caller nothing was renamed when they renamed
        // nothing sends them looking for a rename they never made.
        Blocked::Relabelling(slot) if rests_on_the_name_alone(candidates) => reason_advice(
            "update_entity",
            "Nothing was written.",
            attempted,
            candidates,
            &slot,
            &token,
        ),
        Blocked::Relabelling(_) => format!(
            "Nothing was written, and the handle '{attempted}' is unaffected either way — this \
             only moves the names it answers to. Either pick a name or alias that isn't already \
             worn, or re-call update_entity with override_token: \"{token}\" if this entity really \
             does share a name with one above: names are not unique, handles are.",
        ),
        // **A run is never created by hand, and its handle is not the sid a
        // caller holds.** A caller that names `session:<sid>` is sent to the
        // read that does find a run by its sid, not to a verb that would make
        // an entity of the wrong kind.
        Blocked::MustExist(verb) if attempted.kind() == Some(EntityKind::SESSION) => format!(
            "{}. '{attempted}' is not a run jojobot knows, and the handle of a run is not its \
             sid. {}To find a run by the sid you hold, call {verb} with kind: session and \
             fields: [{{\"key\": \"sid\", \"value\": \"<the sid>\"}}]; a run is made by \
             booting, never by hand.",
            nothing_for(verb),
            if candidates.is_empty() {
                ""
            } else {
                "If one of the handles above is the run you meant, use that. "
            },
        ),
        // **A week of usage is never made by hand, and a week the bot did nothing
        // in has no record.** A caller that named one is sent to the read that
        // lists a bot's weeks, never to a verb that would make an entity of a kind
        // that is computed at the read.
        Blocked::MustExist(verb) if is_a_week_of_usage(attempted) => format!(
            "{}. '{attempted}' is not a week jojobot has a record of: a week in which the bot \
             had no run, compaction or call has no record, and no week is made by hand. {}To \
             list a bot's weeks, call recall with view: \"usage\" (inputs: {{\"bot\": \
             \"bot:<slug>\"}}), or with kind: usage and parent: \"bot:<slug>\".",
            nothing_for(verb),
            if candidates.is_empty() {
                ""
            } else {
                "If one of the handles above is the week you meant, use that. "
            },
        ),
        // **A read of a thing that is not there creates nothing, so it is not
        // sent to the verb that creates it.** Reading a missing document runs
        // nothing; the handle is what to check.
        Blocked::MustExist(verb) if nothing_for(verb) == NOTHING_RAN => format!(
            "{}. '{attempted}' is not an entity jojobot knows. {}{verb} only reads what is \
             there: check the handle with search or list_entities.",
            nothing_for(verb),
            if candidates.is_empty() {
                ""
            } else {
                "If one of the handles above is what you meant, use that. "
            },
        ),
        // The candidate list is often empty here — this gate fires on any
        // unrecognized handle, not only a near miss — so the advice must not
        // point at "the handles above" when there are none.
        Blocked::MustExist(verb) if candidates.is_empty() => format!(
            "{}. '{attempted}' is not an entity jojobot knows, and nothing \
             resembles it. {verb} cannot create an entity: call add_entity to create \
             '{attempted}' first, then re-call {verb}.",
            nothing_for(verb),
        ),
        Blocked::MustExist(verb) => format!(
            "{}. '{attempted}' is not an entity jojobot knows. If one of the \
             handles above is what you meant, use that. Otherwise {verb} cannot create it for \
             you — call add_entity to create '{attempted}' first, then re-call {verb}.",
            nothing_for(verb),
        ),
        Blocked::Renaming(_) if exact => format!(
            "Nothing was renamed. The handle '{attempted}' is already taken, and that cannot be \
             forced — a handle has exactly one owner. Either this IS the entity above (nothing \
             to do — it already has this name), or it is a different thing and the destination \
             needs a more qualified slug.",
        ),
        Blocked::Renaming(slot) if rests_on_the_name_alone(candidates) => reason_advice(
            "rename_entity",
            "Nothing was renamed.",
            attempted,
            candidates,
            &slot,
            &token,
        ),
        Blocked::Renaming(_) => format!(
            "Nothing was renamed. If '{attempted}' IS one of the entities above, renaming onto \
             it would collide with a thing that already exists there. If it is genuinely a \
             different thing that happens to share a name, re-call rename_entity with \
             override_token: \"{token}\". That token belongs to THIS refusal and lifts no other. \
             Display names are not unique and never have to be; the handle is what has to be.",
        ),
    };
    blocked_body(attempted, candidates, how_to_proceed)
}

/// The blocked envelope itself, once — so every gate's advice arrives in one
/// shape and a client branches on `status`, never on which gate fired.
pub(crate) fn blocked_body(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    how_to_proceed: impl Into<WayForward>,
) -> CallToolResult {
    let how_to_proceed = how_to_proceed.into();
    let mut body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
    });
    how_to_proceed.write_into(&mut body);
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **The refusal for something no reader could be handed whole**, with the
/// numbers as keys: what it would measure as a read serves it, the most one
/// answer can carry of it, and the overage, so a caller cuts by the amount that
/// is needed instead of guessing.
fn too_big_to_read(
    subject: &str,
    what_is_over: &str,
    size: usize,
    room: usize,
    how_to_proceed: impl Into<WayForward>,
) -> CallToolResult {
    let mut body = serde_json::json!({
        "status": "blocked",
        "attempted": subject,
        "wrote": false,
        "candidates": [],
        // **Which one is over**, as a word a caller branches on: the record being
        // written, or the thing's own name, aliases and keys, which no claim written on it can fix.
        "what_is_over": what_is_over,
        "size": size,
        "room": room,
        "over": size - room,
    });
    how_to_proceed.into().write_into(&mut body);
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

/// **The word every memory refusal wears, in one exhaustive match.** There is
/// no wildcard arm: a kind added to [`MemoryError`] without a word does not
/// compile, so a refusal cannot ship without one. [`memory_declined`] and
/// [`memory_error`] stamp this word on whatever they answer, and the cases below
/// check what is served against it.
///
/// `change` is the call itself; `person` is damage or a decision only a person
/// can make; `retry` is a failure the same call may get past. `None` is not a
/// refusal: a fold that is behind says the write LANDED.
pub(crate) fn memory_fix_by(e: &MemoryError) -> Option<FixBy> {
    match e {
        MemoryError::InvalidFact { .. }
        | MemoryError::InvalidSubject { .. }
        | MemoryError::WeekOfUsageWritten { .. }
        | MemoryError::InvalidAddress { .. }
        | MemoryError::InvalidEntity { .. }
        | MemoryError::InvalidEdge { .. }
        | MemoryError::InvalidQuery { .. }
        | MemoryError::InvalidType { .. }
        | MemoryError::ShippedType { .. }
        | MemoryError::RepeatsShipped
        | MemoryError::BreaksFit { .. }
        | MemoryError::BreaksSchedule { .. }
        | MemoryError::ViewNarrowsNothing { .. }
        | MemoryError::NumberTaken { .. }
        | MemoryError::NumberFreed { .. }
        | MemoryError::NotANumber { .. }
        | MemoryError::MembersBreakNumbering { .. }
        | MemoryError::BootTooHeavy { .. }
        | MemoryError::ClaimTooLarge { .. }
        | MemoryError::ThingTooLarge { .. }
        | MemoryError::BreaksType { .. }
        | MemoryError::RemapsShippedWord { .. }
        | MemoryError::UnknownFact { .. }
        | MemoryError::NotYours { .. }
        | MemoryError::RoomFull { .. }
        | MemoryError::OverCapacity { .. }
        | MemoryError::KeyNotYours { .. }
        | MemoryError::ChartCycle { .. }
        | MemoryError::MergeCarriesGuardedKeys { .. }
        | MemoryError::ThoughtTooLong { .. }
        | MemoryError::MergeOverfillsRoom { .. }
        | MemoryError::MergeThoughtTooLong { .. }
        | MemoryError::RoleFieldGuarded { .. }
        | MemoryError::VoucherIssued { .. }
        | MemoryError::ChallengeIssued { .. }
        | MemoryError::UnknownRight { .. }
        | MemoryError::RoleNotHeld { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::NothingToMerge { .. }
        | MemoryError::DocumentMergedWithAThing { .. }
        | MemoryError::MergeIntoDescendant { .. }
        | MemoryError::AlreadyMerged { .. }
        | MemoryError::NothingToRename { .. }
        | MemoryError::HandleMoved { .. }
        | MemoryError::SuppliedHandle { .. }
        | MemoryError::AlreadyRetracted { .. }
        | MemoryError::AlreadyArchived { .. }
        | MemoryError::NotArchived { .. }
        | MemoryError::NotRetractable { .. }
        | MemoryError::UnsourcedObservation
        | MemoryError::UnstatedProvenance
        | MemoryError::KeyInOtherBag { .. }
        | MemoryError::UnreceiptedSettling
        | MemoryError::ReceiptWithoutSettling
        | MemoryError::TestimonyRewritten { .. }
        | MemoryError::AppClaimRewritten { .. } => Some(FixBy::Change),
        // A store that answered and refused: a constraint the caller's input
        // broke, which sending the same call again meets again. Not an outage.
        MemoryError::Refused(_) => Some(FixBy::Change),
        // A kind set nothing re-reads, and a claim only the operator can bless.
        MemoryError::KindsNeverLoaded { .. } | MemoryError::UnconfirmedPromotion => {
            Some(FixBy::Person)
        }
        // Stored damage, which sending the call again or changing it cannot help:
        // only a person repairs it.
        MemoryError::StoredDamage { .. } => Some(FixBy::Person),
        // A held role frees when its lease ends; a collision clears on the next try.
        MemoryError::RoleTaken { .. } | MemoryError::Conflict => Some(FixBy::Retry),
        // The claim moved on after the caller read it: read it again, then send the
        // edit against the revision now current.
        MemoryError::StaleRevision { .. } => Some(FixBy::Retry),
        MemoryError::TextChangedWhileEditing { .. } => Some(FixBy::Retry),
        // The words do not occur once in the text: the call has to name them differently.
        MemoryError::TextEditMatches { .. } => Some(FixBy::Change),
        MemoryError::Store(_) => memory_store_failure_word(),
        MemoryError::FoldBehind { .. } => None,
    }
}

/// The answer for a refusal: the arms below build the body and the word comes
/// from [`memory_fix_by`], stamped here once for every one of them.
pub(crate) fn memory_declined(
    verb: &'static str,
    e: MemoryError,
) -> Result<CallToolResult, McpError> {
    let word = memory_fix_by(&e);
    memory_declined_arms(verb, e).map(|answered| stamp(answered, word))
}

/// **What to do about a number a write would free**, in the words of the verb
/// that asked. One place decides the text, so the verbs cannot drift.
///
/// An edit has a number to send, so it is sent back to the number the member
/// holds. A move out of the collection has none, and it is told to leave the
/// member where it is or to archive it, which keeps its number. Only a rename
/// with a new parent reaches the numbering guard that frees a number by a move. A merge has none: the survivor serves what the duplicate held, so two
/// numbered members cannot be folded into one, and a numbered member cannot be
/// folded out of its collection. The ways that work for a merge are the two that
/// leave no number to lose: take back the claim that gave one of the numbers
/// (a retraction is not held to the numbering) and merge again, or archive the
/// duplicate, which keeps its number.
fn number_freed_advice(
    verb: &str,
    reads: &str,
    collection: &str,
    holder: &str,
    number: u64,
) -> String {
    let head = format!(
        "Nothing was written: {reads}. A number names one member for ever, so it stays with \
         {holder} while it is a member of {collection}, and no token overrides this."
    );
    let tail = format!("To read the holder, recall {collection}/{number}.");
    match verb {
        "merge_entities" => format!(
            "{head} A merge makes the survivor serve what the duplicate held, so it cannot \
             carry two numbers, and it cannot carry a number out of {collection}. Two ways go \
             on. Take back the claim that gave one of the numbers with retract (a retraction \
             is allowed, and the number is free again), then send the merge again. Or leave \
             the duplicate unmerged and archive it with archive_entity: an archived member \
             keeps its number. {tail}"
        ),
        "rename_entity" => format!(
            "{head} A move with a new `parent` takes {holder} out of {collection}, and a \
             number stays with a member only while it is a member. Two ways go on. Leave it \
             where it is: send the rename again without the new `parent`. Or, if it is \
             retired, archive it with archive_entity: an archived member keeps its number \
             and stays in {collection}. {tail}"
        ),
        _ => format!(
            "{head} Send the number it holds, or change something else. An archived member \
             keeps its number. A number given by mistake is taken back by retracting the \
             claim that gave it, and then the right one can be given. {tail}"
        ),
    }
}

/// **A miss and a block speak one shape.** An id, handle or address that names
/// nothing is not a malformed call and not a server failure: it is jojobot
/// declining because what the caller named is not there — the same answer the
/// resemblance and existence gates give — so it comes back as a *successful*
/// result whose body says `status: blocked`, `wrote: false`, with whatever is
/// nearby and what to do next.
///
/// Two shapes for one idea meant a client had to branch twice to learn the same
/// thing, and the error half read as a broken server: clients that retry on
/// error retry it, and clients that unwrap on error handle it exactly wrong.
///
/// Everything that is genuinely a caller mistake (a malformed address, an
/// unknown kind token) or genuinely a failure (the store is down) stays an
/// error. `Ok` here is the refusal; `Err` is still an error.
fn memory_declined_arms(verb: &'static str, e: MemoryError) -> Result<CallToolResult, McpError> {
    match e {
        MemoryError::UnknownEntity { attempted, nearest } => Ok(blocked_result(
            &EntityId(attempted),
            &nearest,
            Blocked::MustExist(verb),
        )),
        // A fact miss has no entity candidates — its near misses are the live
        // addresses in the same doc, which is what makes it repairable.
        MemoryError::UnknownFact { attempted, nearest } => {
            let live = if nearest.is_empty() {
                "That entity holds no facts at all yet, so there is nothing here to edit — \
                 capture one first."
                    .to_string()
            } else {
                format!(
                    "The addresses that do exist here are: {}.",
                    nearest.join(", ")
                )
            };
            Ok(blocked_body(
                &EntityId(attempted.clone()),
                &[],
                format!(
                    "Nothing was written. '{attempted}' addresses no fact jojobot holds, and this \
                     verb never creates one. {live} Recall the entity if none of them is what you \
                     meant — every fact comes back carrying the address that edits it."
                ),
            ))
        }
        // **A refusal, not a failure**: the row is there and the caller named
        // it correctly — jojobot is declining to do this to THAT row. The
        // domain already wrote the sentence that says which of the three
        // reasons it is and what to do instead, so it is carried through
        // rather than re-worded here, where it would drift from the rule it
        // describes.
        // **Already done is not the same answer as cannot be done.** The
        // record the caller asked to take back is archived already, whether
        // by this verb or by an ordinary edit; what this call wrote is
        // nothing, because there was nothing left to write. Saying "cannot
        // be retracted" here denies a state the store is holding, and a
        // caller who believes it treats an archived record as live.
        MemoryError::AlreadyRetracted { attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "'{attempted}' is already archived — the record jojobot holds is the one you \
                 asked for. This call wrote nothing because there was nothing left to write, and \
                 a further attempt would say the same. Archiving is one-way: nothing takes a \
                 record back out of it. If that was itself a mistake, capture what is so now as \
                 a new record."
            ),
        )),
        // **Already done is not the same answer as cannot be done**, the same
        // split `AlreadyRetracted` draws for a claim. The entity jojobot
        // holds is already archived, whichever call did it, so a second
        // archive writes nothing rather than overwriting the reason already
        // on record.
        MemoryError::AlreadyArchived { attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "'{attempted}' is already archived — the entity jojobot holds is the one you \
                 asked for. This call wrote nothing because there was nothing left to write, and \
                 a further attempt would say the same. Recall it by handle to read why and \
                 when it was archived."
            ),
        )),
        // **The mirror of the answer above:** the entity jojobot holds is
        // already the one the caller wanted back.
        MemoryError::NotArchived { attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "'{attempted}' is not archived — it is already in every default read. This call \
                 wrote nothing because there was nothing to restore, and a further attempt \
                 would say the same."
            ),
        )),
        // **Half an attribution is what is missing, and the caller has both
        // ways out** (rule 68): name where it was read, or file it as the
        // derivation it would otherwise be.
        MemoryError::UnsourcedObservation => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            // **Worded by the verb**: put_document takes the system as `source`
            // and stores it as `read_from`, so naming `read_from` there tells
            // the caller to send an argument the verb does not have.
            if verb == "put_document" {
                "Nothing was written: a claim read out of a system of record has to name it: \
                 give `source` the system that was read — and `source_ref` what was read there, \
                 if you have it — or put it as an inference instead. The claim itself is fine — \
                 send it again with source naming the system, or with provenance inference."
                    .to_string()
            } else {
                format!(
                    "Nothing was written: {e}. The claim itself is fine — send it again with \
                     `read_from` in `fields` naming the system, or with provenance inference."
                )
            },
        )),
        MemoryError::NotRetractable { attempted, why } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!("Nothing was written. '{attempted}' cannot be retracted: {why}."),
        )),
        // **A malformed argument is a caller mistake, so it is an answer**
        // (rule 68). A thrown error is not a value: the model on the other end
        // gets a failure where it should get a next move, and the sentence
        // saying what to do lands in a channel nothing branches on.
        //
        // One arm for all of them, interpolating the validator's own sentence
        // rather than restating it. Each of these faults has several causes
        // and the validators gain more; naming them here would be a catalogue
        // that goes stale on the day it is added to (rule 106).
        // **A refusal nothing in the call can reach** (rule 68). The set of
        // kinds is loaded at startup and no verb re-reads it, so the advice
        // every other malformed call gets — send it again with that fixed —
        // is advice that cannot succeed here, and a model follows it round a
        // loop with no end. This says who repairs it instead.
        MemoryError::KindsNeverLoaded { .. } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. The call is not what is wrong, and sending it again \
                 will not help: jojobot loaded no kinds when it started, and nothing a caller \
                 does re-reads them. This one needs the operator."
            ),
        )),
        // **The way forward is a different pair of calls, not this one again**: the
        // key stays in the bag it was first written under until it is cleared.
        MemoryError::KeyInOtherBag { ref key, .. } => Ok(blocked_body(
            &EntityId(key.clone()),
            &[],
            format!(
                "Nothing was written: {e}. Nothing is missing from the store and nothing here \
                 needs the operator."
            ),
        )),
        // **The way forward is a different call, not this one again.** The words
        // are not rewritten in place, so "send it again" would loop; the sentence
        // names the two calls that do it and says the original stays readable, so
        // a caller can tell the operator so.
        // **The caller named no revision, so the sentence does not talk about one.**
        // It says the text changed under the edit and names the read.
        MemoryError::TextChangedWhileEditing { ref address, .. } => Ok(blocked_body(
            &EntityId(address.clone()),
            &[],
            WayForward::retry(format!(
                "Nothing was written: {e}. Read the claim again with recall (history_record: \
                 \"{address}\") to see what changed, then send the edit again."
            )),
        )),
        // **The count is a key**, so a caller branches on a number and does not read
        // a sentence for it. Two or more means the words name more than one place;
        // none means the text is not what the caller thinks it is.
        MemoryError::TextEditMatches {
            ref address,
            ref field,
            found,
        } => {
            let way = if found == 0 {
                let subject = address.split('#').next().unwrap_or(address);
                format!(
                    "Nothing was written: {e}. Read the {field} with recall (subject: \
                     \"{subject}\", facts: true) to see the text as it stands, then send `find` \
                     with words that occur in it. The match is exact and case-sensitive."
                )
            } else {
                format!(
                    "Nothing was written: {e}. Send a longer `find`, with the words around the \
                     place you mean, so it names one place."
                )
            };
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": address,
                "wrote": false,
                "candidates": [],
                "matches": found,
            });
            WayForward::from(way).write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **The way forward is the read, then the same edit against the revision the
        // read shows.** The sentence names the revision the claim is at now and the
        // call that returns what the other write changed.
        MemoryError::StaleRevision {
            ref address,
            current,
            ..
        } => Ok(blocked_body(
            &EntityId(address.clone()),
            &[],
            WayForward::retry(format!(
                "Nothing was written: {e}. Read the claim again with recall (history_record: \
                 \"{address}\") to see what changed, then send the edit again with \
                 expected_revision: {current} if it still stands."
            )),
        )),
        MemoryError::TestimonyRewritten { ref address }
        | MemoryError::AppClaimRewritten { ref address } => Ok(blocked_body(
            &EntityId(address.clone()),
            &[],
            format!(
                "Nothing was written: {e}. Nothing is missing from the store and nothing here \
                 needs the operator."
            ),
        )),
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
        | MemoryError::InvalidType(_)
        | MemoryError::UnstatedProvenance => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "{}: {e}. Nothing is missing from the store and nothing here \
                 needs the operator — the call itself is what jojobot cannot carry out. Send the \
                 same {verb} call again with that fixed.",
                nothing_for(verb)
            ),
        )),
        // **A write onto a week of usage is answered with the read and nothing
        // else.** The message names the route; the tail every other malformed
        // call gets would tell the caller to send this one again.
        MemoryError::WeekOfUsageWritten(ref subject) => Ok(blocked_body(
            subject,
            &[],
            format!("{}: {e}.", nothing_for(verb)),
        )),
        // **A well-formed call against a name that is not the caller's.** Not
        // a malformed declaration and not a missing one: the type is there and
        // the software owns it.
        //
        // So the way forward cannot be "send it again with that fixed" —
        // nothing the caller can put in this call reaches a shipped type, and
        // advice that implies otherwise sends a model round a loop with no
        // end. What it can do is declare under a name of its own, and the
        // sentence says so and names the verb to do it with (rule 68).
        // **The caller does not know a mechanism is here, so the sentence does
        // not name one.** No layer, no core, no composition: what they did is
        // send back text that repeats what is already there, and what they can
        // do is send only the part they are adding. A reader who has never
        // heard of any of this can act on that, which is the whole bar.
        //
        // The address is on the answer because a caller writing several things
        // needs to know which one came back.
        // **Refused because it is somebody else's, which is not the same
        // answer as absent.** A caller told a handle names nothing cannot tell
        // a typo from a thing it may not read, and would retry the first
        // forever. This says which it is and that retrying will not help.
        MemoryError::NotYours { ref attempted, .. } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was read: {e}. It is there, and it is not yours — sending this \
                 again will not change the answer. {}",
                if is_a_week_of_usage(&EntityId(attempted.clone())) {
                    "A week of usage is read by its own bot, by the bots above that bot on \
                     the chart, and by the assistant."
                } else {
                    "A session's chronology is its own bot's, and yours is what your own \
                     handle reaches."
                }
            ),
        )),
        MemoryError::RepeatsShipped => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Sending this again will not change the answer. Send \
                 only the part you are adding — what you write is kept beside what is already \
                 there, and a read hands back both."
            ),
        )),
        MemoryError::ShippedType {
            ref name,
            ref displaced,
        } => Ok(blocked_body(
            &EntityId(name.clone()),
            &[],
            format!(
                "Nothing was written: {e}. A shipped type cannot be extended, shrunk or replaced \
                 from here, so sending this call again will not change the answer — changing one \
                 is a change to the software. To see the keys '{name}' already has, call recall \
                 with answers_type: '{name}'. To declare a type of your own instead, call {verb} \
                 with a different name, and that type is yours to declare and redeclare as you \
                 like.{}",
                match displaced {
                    None => String::new(),
                    Some(d) => format!(
                        " Before this: '{name}' held keys {}, declared by a caller, until {}, \
                         when the software's own declaration took the name over. Those keys are \
                         gone from '{name}'; declare them under a name of your own to keep them.",
                        d.fields
                            .iter()
                            .map(|f| f.key.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        d.replaced_on,
                    ),
                }
            ),
        )),
        // **The write is well formed and the record is real** — what it would
        // cost is a key the thing needs to go on being what it is. Re-sending
        // it cannot change that, so the way forward is the choice the caller
        // actually has: leave the key, or say the same thing somewhere the
        // type can still see it.
        MemoryError::BreaksFit { ref name, ref keys } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This thing answers '{name}' now, and {verb} would take \
                 {} off it, so sending this call again will not change the answer. Leave the key \
                 where it is, or write what you meant under a key of your own — adding keys is \
                 never refused, and a thing may carry anything beyond what a type asks for.",
                keys.join(", ")
            ),
        )),
        MemoryError::BootTooHeavy {
            ref subject,
            floor,
            budget,
            ref parts,
            stamp_margin,
        } => {
            let largest = parts
                .first()
                .map(|(part, n)| format!("{part} ({n} characters)"))
                .unwrap_or_default();
            let how_to_proceed = WayForward::from(match verb {
                "set_charter" => format!(
                    "Nothing was written: {e}. Send a shorter charter. The largest part of \
                         the floor is {largest}; floor_parts lists every part. A rule that \
                         binds at one moment is better carried by a skill than by the charter."
                ),
                // **A creation made nothing, so there is nothing to unstar.**
                // What a new bot's boot carries from the call is its name, its
                // aliases and its source; what the call sets does not ride in
                // it. So those are the three to shorten.
                "add_entity" => format!(
                    "Nothing was created: {e}. The largest part of the floor is {largest}; \
                         floor_parts lists every part. Send a shorter name, shorter aliases or a \
                         shorter source in the same call: they are what the new bot's boot \
                         carries."
                ),
                // **An edit to a bot may carry a name, aliases, a star or a
                // seat count**, so it is told the cheaper cut first and the
                // rest after.
                "update_entity" => format!(
                    "Nothing was written: {e}. The largest part of the floor is {largest}; \
                         floor_parts lists every part. Send a shorter name or shorter aliases, \
                         or: {}",
                    crate::orientation::floor::ways_down(subject),
                ),
                // **Three ways down, and the caller picks the one that
                // costs least.** Which rules are starred and how many seats
                // a bot has are data, so none of them is chosen for the
                // caller.
                _ => format!(
                    "Nothing was written: {e}. The largest part of the floor is {largest}; \
                         floor_parts lists every part. {}",
                    crate::orientation::floor::ways_down(subject),
                ),
            });
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
                "candidates": [],
                "floor": floor,
                "budget": budget,
                "over": floor - budget,
                // **What the floor is made of, largest first**, so the cut is
                // made where it helps rather than where it is easy.
                "floor_parts": parts
                    .iter()
                    .map(|(part, n)| serde_json::json!({"part": part, "characters": n}))
                    .collect::<Vec<_>>(),
            });
            how_to_proceed.write_into(&mut body);
            // **Said only when the write was measured with a stamp.** The check
            // holds back room for the timestamp the store gives a new record, so
            // a writer about this many characters short of the ceiling is refused
            // for room the record may not use. A charter or an edit measures no
            // new stamp, and a margin there would be untrue.
            if stamp_margin > 0
                && let Some(object) = body.as_object_mut()
            {
                object.insert("stamp_margin".into(), stamp_margin.into());
                if let Some(how) = object.get_mut("how_to_proceed")
                    && let Some(said) = how.as_str()
                {
                    *how = format!(
                        "{said} Part of the margin is kept for the timestamp the store gives \
                         the new record: the check holds back {stamp_margin} characters for \
                         it, so a rule that short of the ceiling is refused."
                    )
                    .into();
                }
            }
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **The way forward is a cadence in the SAME call, or both keys gone.**
        // What is missing is the cadence, and there are two: a number of days or a
        // calendar rule. A caller who sends one now makes the loop whole in one
        // write instead of two. A loop with no schedule at all is allowed.
        MemoryError::BreaksSchedule { ref missing, .. } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Send '{missing}' in `sets` in the same call, as a \
                 number of days, or '{}' in `sets`, as a calendar rule such as \
                 FREQ=MONTHLY;BYMONTHDAY=1 (the rhythms skill has examples). To switch the \
                 schedule off, call update_fact with clear_fields naming '{}': it is the one \
                 verb that clears a key, and on capture, add_entity or update_entity the way \
                 through is to send the cadence. A loop with no schedule at all is allowed and \
                 needs neither.",
                jojobot_domain::attention::RRULE,
                jojobot_domain::attention::ADVANCES_FROM,
            ),
        )),
        // **The way forward is something that narrows, on the view or beside
        // it.** The view is named, and so is every key that would narrow it,
        // built from the one list a recall's own refusal reads
        // (`graph::Narrowing`). A key written `$name` narrows only when its input
        // is sent, which is how a view that does narrow comes to be refused.
        MemoryError::ViewNarrowsNothing { ref view } => Ok(blocked_body(
            &EntityId(view.clone()),
            &[],
            super::view_run::narrows_nothing_words(view),
        )),
        // **The number is held and stays held**, archived or not, so the way
        // forward is a different number, and the refusal hands the caller the
        // next free one and the handle that reads the holder. There is no
        // override: this is an integrity guard (rule 472).
        MemoryError::NumberTaken {
            ref collection,
            ref holder,
            number,
            next,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. A number is never reused, even when its holder is \
                 archived, and no token overrides this. The next free number is {next}: send \
                 that. To read the holder, recall {collection}/{number} (it is {holder})."
            ),
        )),
        // **The number stays with its holder**, so the way forward is a write that
        // leaves the member holding it. No override: this is the same integrity
        // guard as the one above (rule 472).
        MemoryError::NumberFreed {
            ref collection,
            ref holder,
            number,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            number_freed_advice(verb, &e.to_string(), collection, holder, number),
        )),
        MemoryError::NotANumber { ref collection, .. } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. Members of {collection} hold a positive whole \
                 number written in digits, with no sign, no point and no leading zero, such as \
                 5, and below 18446744073709551615. Send one. To see which number is free, \
                 recall {collection}: it says next_number."
            ),
        )),
        // **The members are repaired first**, so the way forward is a write on each
        // member named, or another key. No override: the same integrity guard as
        // the ones above (rule 472).
        MemoryError::MembersBreakNumbering {
            ref collection,
            ref key,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. {collection} would promise that every member holds \
                 a positive whole number under {key} and that no two hold the same one, and \
                 the members named already break that. Give each a number nobody else holds \
                 (a number given by mistake is taken back by retracting the claim that gave \
                 it), or number {collection} by a key they do not hold, and then send \
                 numbers_by again."
            ),
        )),
        // **The key stays and the value has to change**, which is what makes
        // this a different way forward from the one above. The caller is told
        // what the key holds in the words a declaration uses, so the repair is
        // a value it can write rather than a rule it has to infer.
        MemoryError::BreaksType {
            ref name,
            ref key,
            ref wanted,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This thing is a '{name}' now, so '{key}' has to go on \
                 holding {wanted} — sending the same value again will not change the answer. \
                 Write a value that holds {wanted}, or say what you meant under a key of your \
                 own: adding keys is never refused, and only the keys a type names are held to \
                 what it declared."
            ),
        )),
        // **A shipped word means itself, so the way forward is a word of the
        // caller's own** (rule 261). The five are named so the mapping can be
        // written whole in the next call.
        MemoryError::RemapsShippedWord { ref word, .. } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. '{word}' is one of the five words the software ships \
                 and it means itself on every instance, so sending this again will not change \
                 the answer. Map a word of your own instead: write `{}<your word>` in `sets`, set to the \
                 shipped word it should mean, one of {}.",
                jojobot_domain::memory::kinds::STATUS_WORD,
                jojobot_domain::memory::kinds::WORK_STATUSES.join(", "),
            ),
        )),
        // **A receipt without the move it records is a call that must change**: the
        // standing is what the receipt is for.
        MemoryError::ReceiptWithoutSettling => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. The claim is unchanged. Send {verb} again with \
                 `standing` set to `settled` beside the receipt, or without the receipt."
            ),
        )),
        // **Settling is a call that must change, not a decision for a person**:
        // a check that ran is the caller's to record, and the operator's word is
        // recorded the same way, as the `confirmed` shape.
        MemoryError::UnreceiptedSettling => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. The claim is unchanged. Send {verb} again with \
                 `receipt` set to `{{check: ...}}` or `{{confirmed: {{on: YYYY-MM-DD, at: ...}}}}`."
            ),
        )),
        // **A different refusal, so a different way forward.** This is not a
        // malformed call: the arguments are well-formed and jojobot is declining
        // to bless a claim the operator has not blessed. Telling a caller to fix
        // the call would invite them to set the confirmation flag themselves,
        // which is the one thing the gate exists to stop.
        MemoryError::UnconfirmedPromotion => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            format!(
                "Nothing was written: {e}. This is not a malformed call and re-sending it will \
                 not change the answer — what is missing is the operator's word. Ask, and re-call \
                 {verb} with confirmed_by_user only once they have actually said so."
            ),
        )),
        // **Three of `rename_entity`'s own misses**, each a caller mistake
        // and none of them a resemblance or an ordinary absence, so none of
        // them fits `blocked_result`'s gates. Answered here instead of
        // falling through to `memory_error`, which turned all three into a
        // plain protocol error — false against this verb's own argument
        // documentation, which promises a stale handle comes back "with the
        // name it moved to, rather than a bare miss."
        //
        // **Two sides of one handle** is not a resemblance: the destination
        // is fine, it names exactly what the source already answers to.
        MemoryError::NothingToRename { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was renamed: {e}. Sending this again with the same handle, the same \
                 parent and nothing else different will not change the answer — name a \
                 destination that differs from '{attempted}', or a parent that differs from the \
                 one it already has."
            ),
        )),
        // **Stale, not absent** (rule 261): the caller's evidence is real, it
        // is just out of date. The way forward is the handle it wears now,
        // never a bare miss that reads the same as a handle that never
        // existed.
        MemoryError::HandleMoved {
            ref attempted,
            ref now,
        } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was renamed: {e}. Re-call {verb} with handle: \"{now}\" — that is what \
                 '{attempted}' answers to now."
            ),
        )),
        // **Real, and with nothing stored underneath it.** Neither a create
        // (it already exists) nor an ordinary miss (it is not gone): a
        // build-supplied record has nothing for `rename_entity` or `merge`
        // to move, on either side of a fold.
        MemoryError::SuppliedHandle { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing changed: {e}. Sending this again will not change the answer — \
                 '{attempted}' is part of the software rather than something stored that \
                 {verb} could move. Facts and settings written to this record with capture \
                 are kept on top of what the build ships; only renaming, archiving or \
                 merging the record itself is refused."
            ),
        )),
        // **Two of `merge`'s own misses**, answered here for the same reason
        // `rename_entity`'s three are: neither is a resemblance or an
        // ordinary absence, and the tool's own published description
        // promises both come back `status: blocked` naming a way forward —
        // a promise the code was breaking rather than the description
        // (rule 199).
        //
        // **Two sides of one handle** is not a resemblance: the survivor is
        // fine, it names exactly what the duplicate already answers to.
        MemoryError::NothingToMerge { ref attempted } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Sending this again with the same handle on both \
                 sides will not change the answer — name a survivor that differs from \
                 '{attempted}'."
            ),
        )),
        // **A document merges only into another document.** Both handles are
        // real; the call has to change, and the route is named: keep the two as
        // they are, or carry the words over by hand and put one away.
        MemoryError::DocumentMergedWithAThing {
            ref document,
            ref other,
        } => Ok(blocked_body(
            &EntityId(document.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Merge two documents to get one that holds both. To \
                 bring '{other}' and '{document}' together anyway, carry over by hand what you \
                 want, in whichever direction: read '{document}' with read_document and \
                 capture what is wanted as claims on '{other}', or write what is wanted from \
                 '{other}' into '{document}' with put_document. Then put the one you no longer \
                 need away with archive_entity."
            ),
        )),
        // **A thing does not fold into its own descendant.** Both handles are
        // real; the call has to change. Fold the other way, or move the
        // survivor out from under the thing first and fold after.
        MemoryError::MergeIntoDescendant {
            ref folded,
            ref survivor,
        } => Ok(blocked_body(
            &EntityId(folded.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. To fold them anyway, either call {verb} the other \
                 way, folding '{survivor}' into '{folded}', or move '{survivor}' out from \
                 under '{folded}' first with rename_entity and its parent argument, then fold."
            ),
        )),
        // **Stale, not absent** (rule 261): the row named is real — it is a
        // forwarding row rather than a side to fold or a survivor to fold
        // into — so the way forward is the handle it forwards to, on
        // whichever side it was named.
        MemoryError::AlreadyMerged {
            ref attempted,
            ref into,
        } => Ok(blocked_body(
            &EntityId(attempted.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Re-call {verb} naming '{into}' instead of \
                 '{attempted}' — that is where it resolves now."
            ),
        )),
        // **The room rides on the refusal**, structured rather than folded
        // into prose, so naming a drop never costs a second round trip to
        // see what is in it. `blocked_body`'s fixed shape has no room for
        // this, so this is its own small body rather than a fourth
        // parameter every other caller would carry and never use.
        MemoryError::RoomFull {
            ref subject,
            live,
            capacity,
            ref room,
            aged_out,
        } => {
            let empty_room_without_capacity = live == 0 && capacity == 0;
            let how_to_proceed = WayForward::from(if empty_room_without_capacity {
                // **A room with no capacity holds nothing, so there is no
                // thought to archive or to drop** — naming either points at
                // nothing. What is true is that the ceiling has to move
                // first, and it is not this caller's to move.
                if verb == "update_fact" {
                    format!(
                        "Nothing was written: '{subject}'s room has a capacity of 0, so it \
                             holds no thoughts and {verb} cannot make this claim one. A \
                             different identity has to give '{subject}' a thought_capacity above \
                             0 first — ask another bot, or the operator — then re-call {verb}."
                    )
                } else {
                    format!(
                        "Nothing was written: '{subject}'s room has a capacity of 0, so no \
                             thought can be written in it and there is none to give up. A \
                             different identity has to give '{subject}' a thought_capacity above \
                             0 — ask another bot, or the operator — then re-call {verb}. Once \
                             and only once, borrow: true lets this one land over the ceiling \
                             anyway."
                    )
                }
            } else if verb == "update_fact" {
                // **An edit has no drop and no borrow**, so naming either
                // would send the caller round a loop (rule 68). Archiving a
                // live thought is the move it has. Writing the thought
                // through capture is the other, but only while the room is
                // not over its capacity: over it, capture can only swap one
                // thought for another, which leaves the debt where it was.
                //
                // **How many, because one is not always enough.** The edit
                // adds a thought, so the room has to end below its
                // capacity first. A room an earlier borrow left over its
                // capacity needs more than one archived, and the count is
                // the same arithmetic in every case.
                let archive = thoughts_to_archive(live, capacity);
                let through_capture = if live > capacity {
                    ""
                } else {
                    " — or write this thought through capture, which can archive one \
                         thought as it writes this one"
                };
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity}, and {verb} cannot make room in it. Archive {archive} with \
                         update_fact (status: archived, and details saying why it no longer \
                         earns its slot), then re-call {verb}{through_capture}. The \
                         room's size is deliberate: choosing what to give up is the work."
                )
            } else if live > capacity {
                // **Only archiving repays the debt.** A drop swaps one thought
                // for the one written and leaves the room as far over as it
                // was, so it is not on offer here.
                let archive = thoughts_to_archive(live, capacity);
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity} — its emergency reserve is already spent. Archive \
                         {archive} with update_fact (status: archived, and details saying why \
                         it no longer earns its slot), then re-call {verb}. Borrowing again is \
                         not on offer while this debt stands. The room's size is deliberate: \
                         choosing what to give up is the work."
                )
            } else {
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity}. Re-call {verb} naming drop (one of the addresses above) and \
                         drop_because (why it no longer earns its slot) — or, once and only \
                         once, borrow: true to let this one land over the ceiling anyway — or \
                         wait, and let this one go unwritten for now. The room's size is \
                         deliberate: choosing what to give up is the work."
                )
            });
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
                "room": room
                    .iter()
                    .map(|f| serde_json::json!({
                        "address": f.address().to_string(),
                        "content": f.content,
                    }))
                    .collect::<Vec<_>>(),
                // **Said, not silently withheld** — a thought aged out of
                // this count is still there and still active, just not
                // counted against the room. A caller weighing whether to
                // wait instead of dropping needs to know that count is
                // already excluding some.
                "aged_out": aged_out,
                // **Where that count's threshold comes from.** The number of
                // runs a thought may go untouched is the bot's own setting,
                // which a different identity writes, and the default when it
                // carries none.
                "ageing": {
                    "setting": jojobot_domain::memory::THOUGHT_AGES_AFTER_RUNS,
                    "default": jojobot_domain::memory::AGES_AFTER_RUNS,
                },
                // **How many thoughts an edit has to archive before it can
                // land**, as a number a caller does not have to read out of
                // a sentence. `null` for a verb that has other ways forward.
                "archive_needed": ((verb == "update_fact" || live > capacity)
                    && !empty_room_without_capacity)
                    .then(|| thoughts_to_archive_count(live, capacity)),
                // **The offer names the state it is actually in.** A room
                // already OVER its capacity is a borrow already outstanding
                // — offering another would stack debt the ceiling exists to
                // bound, so the way forward is repaying it, never spending
                // the reserve again. Exactly at capacity is the one moment
                // `borrow` is genuinely on the table, so it is the one
                // moment the refusal names it.
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A thing holds as many live records as its ceiling allows**, and the
        // write would add past it. The live records ride on the refusal, so the
        // choice costs no second read, and the four routes are named as
        // identifiers a caller branches on beside the sentence that explains them.
        MemoryError::OverCapacity {
            ref subject,
            live,
            limit,
            adding,
            from_setting,
            ref records,
        } => {
            let did = match verb {
                "merge_entities" => "Nothing was merged",
                "rename_entity" => "Nothing was renamed",
                _ => "Nothing was written",
            };
            let excess = (live + adding).saturating_sub(limit);
            let archive = match excess {
                0 | 1 => "one of the records above".to_string(),
                n => format!("{n} of the records above"),
            };
            let how_to_proceed = WayForward::from(format!(
                "{did}: '{subject}' holds {live} live records, its ceiling is {limit}{}, and {} \
                Make room by one of four routes. REWRITE: edit one of the records above in \
                place with update_fact, because an edit adds no record. ARCHIVE: archive \
                {archive} with update_fact (status: archived, and details saying why it no \
                longer earns its slot), then re-call {verb}. SPLIT: when what is held has grown \
                into a second line, make a child of '{subject}' with add_entity (its parent \
                set to '{subject}') and capture the detail there. DOCUMENT: when the weight is \
                long text, put it in a document with put_document and keep one short record \
                here that points at it. Raising `record_capacity` (a bot above the writers \
                into '{subject}' sets it) is for a thing whose job has grown, never a way \
                around these four routes.{}",
                if from_setting {
                    " (set on the thing)"
                } else {
                    " (the default for a thread)"
                },
                match verb {
                    "merge_entities" => format!(
                        "this merge brings {adding} more. The merged thing folds to the lower \
                         of the two things' own numbers, and the ceiling named here is that one."
                    ),
                    "rename_entity" =>
                        "this change would make it a thread holding that many.".to_string(),
                    _ => format!("this write adds {adding} more."),
                },
                // **The reserve is on offer at the one moment it lands**: a
                // capture that adds one record to a thing exactly full.
                if verb == "capture" && adding == 1 && live == limit {
                    " Or send this capture again with `borrow: true`, which lands the one \
                     record over the ceiling, once; archive a record afterwards to repay it."
                } else {
                    ""
                },
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
                "live": live,
                "ceiling": limit,
                "ceiling_from": if from_setting { "setting" } else { "default" },
                "adding": adding,
                "archive_needed": excess,
                "records": records
                    .iter()
                    .map(|f| serde_json::json!({
                        "address": f.address().to_string(),
                        "content": f.content,
                    }))
                    .collect::<Vec<_>>(),
                "routes": ["rewrite", "archive", "split", "document"],
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A merge that would overfill the survivor's room.** The room rides
        // on the refusal exactly as `RoomFull`'s does. A merge has no drop and
        // no borrow, so the way forward is archiving, and the count says how
        // many: the room has to end with space for everything that arrives.
        MemoryError::MergeOverfillsRoom {
            ref subject,
            live,
            capacity,
            incoming,
            ref room,
        } => {
            // **A room with no capacity holds nothing**, so there is no thought
            // to archive. The ceiling has to move first, and it is not this
            // caller's to move — the branch `RoomFull` takes for the same room.
            let empty_room_without_capacity = live == 0 && capacity == 0;
            let archive_needed = live + incoming - capacity;
            let how_to_proceed = WayForward::from(if empty_room_without_capacity {
                format!(
                    "Nothing was merged: '{subject}'s room has a capacity of 0, so it holds \
                         no thoughts and this merge brings {incoming}. There is none to archive. \
                         A different identity has to give '{subject}' a thought_capacity above \
                         0 — ask another bot, or the operator — then re-call {verb}."
                )
            } else {
                format!(
                    "Nothing was merged: '{subject}'s room holds {live} of {capacity}, and \
                         this merge brings {incoming} more thoughts. Archive {archive_needed} of \
                         the thoughts above with update_fact (status: archived, and details \
                         saying why it no longer earns its slot), then re-call {verb}."
                )
            });
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
                "room": room
                    .iter()
                    .map(|f| serde_json::json!({
                        "address": f.address().to_string(),
                        "content": f.content,
                    }))
                    .collect::<Vec<_>>(),
                "incoming": incoming,
                "archive_needed": (!empty_room_without_capacity).then_some(archive_needed),
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A record no reader could be handed whole.** The three ways to say it
        // are all the caller's to choose between, so the refusal names them and
        // picks none: shorter, several claims that each say one thing, or the long
        // text kept outside the store behind a claim that points at it.
        MemoryError::ClaimTooLarge {
            ref subject,
            size,
            room,
        } => Ok(too_big_to_read(
            subject,
            "the record",
            size,
            room,
            format!(
                "Nothing was written: {e}. Shorten it ({} characters over); or split it into \
                 claims that each say one thing, which {verb} can write one after another; or \
                 keep the long text on a document (add_entity of kind document, then \
                 put_document) and write a pointer claim that names it.",
                size - room
            ),
        )),
        // **A thing whose own row cannot be read whole.** What grows a row is
        // its aliases and the keys its claims set, so the refusal names both.
        MemoryError::ThingTooLarge {
            ref subject,
            size,
            room,
        } => Ok(too_big_to_read(
            subject,
            "the thing's name, aliases and keys",
            size,
            room,
            // **The thing's own name, aliases and keys are what is over, so nothing about the claim
            // being written helps.** Every route named here shrinks them, and
            // the last sentence says why no claim is written until they have shrunk.
            match verb {
                "add_entity" => format!(
                    "Nothing was created: {e}. Send a shorter name, fewer or shorter aliases, or \
                     fewer keys in `sets` ({} characters over).",
                    size - room
                ),
                _ => format!(
                    "Nothing was written: {e}. The thing's own name, aliases and keys are what is over ({} \
                     characters), not the claim you sent, and a read could not serve any claim \
                     on it until they are smaller. Shrink them: shorten its name or \
                     aliases with update_entity, take keys off with update_fact and \
                     clear_fields, move detail to the thing it is about, or keep the long text \
                     on a document (put_document) behind a pointer claim on another thing. Then \
                     send {verb} again.",
                    size - room
                ),
            },
        )),
        // **A merge that would bring a thought over the survivor's body cap.**
        // The way forward names where the thought is now, because the caller
        // never wrote it on the survivor.
        MemoryError::MergeThoughtTooLong {
            ref subject,
            ref thought,
            len,
            cap,
        } => Ok(blocked_body(
            &EntityId(subject.clone()),
            &[],
            format!(
                "Nothing was merged: {e}. Move the substance of {thought} onto the thing it \
                 points at with capture, archive {thought} with update_fact (status: archived, \
                 details saying where the substance went), and capture a pointer at or under \
                 {cap} characters (it is {len}) with {thought} as its derived_from. Then \
                 re-call {verb}. The cap is deliberate, because a thought is a pointer: do not \
                 ask for a bigger cap or spread the text over several thoughts."
            ),
        )),
        // **A thought earns its place by staying a pointer.** The way
        // forward names the repair the brief itself describes: the
        // substance belongs on the thing the thought points at, and the
        // thought stays a short claim naming that it is live.
        MemoryError::ThoughtTooLong {
            ref subject,
            len,
            cap,
        } => Ok(blocked_body(
            &EntityId(subject.clone()),
            &[],
            format!(
                "Nothing was written: {e}. Re-call {verb} with the substance captured onto the \
                 thing this thought points at, and the thought as a pointer at or under {cap} \
                 characters (this one was {len}). If you were editing a thought that already \
                 stands, a shorter edit in place with update_fact lands, except over another \
                 session's testimony; or archive it (status: archived) and capture the pointer \
                 as a new claim with the old one as its derived_from. The cap is deliberate, because a thought is a pointer: do not ask \
                 for a bigger cap or spread the text over several thoughts."
            ),
        )),
        // **The thing a ceiling binds cannot write that ceiling.** A
        // refusal, not a failure — the caller named a real subject and a
        // real key, and jojobot is declining to let it raise or lower its
        // own bound rather than reporting anything wrong with the call's
        // shape.
        MemoryError::KeyNotYours {
            ref subject,
            ref key,
            may,
            ref allowed,
        } => Ok(blocked_body(
            &EntityId(subject.clone()),
            &[],
            // **The bots that may make the write are named** (rule 261), and
            // the operator never is: the operator acts through the assistant,
            // which is above every bot that has a chain. The four ceilings keep
            // the words they have always had.
            thought_ceiling_note(
                key,
                subject,
                match may {
                    jojobot_domain::memory::MayWrite::DifferentIdentity => format!(
                        "Nothing was written: {e}. A different identity has to set '{key}' on \
                     '{subject}' — ask another bot, or the operator, to raise or lower it \
                     instead."
                    ),
                    jojobot_domain::memory::MayWrite::Subject => {
                        format!(
                            "Nothing was written: {e}. '{subject}' has to write '{key}' itself."
                        )
                    }
                    // **Set already, and no bot of this caller's standing changes
                    // it.** The way forward is the head of the chart, or making one.
                    jojobot_domain::memory::MayWrite::HeadOnceHeld { .. } => {
                        match allowed.is_empty() {
                            true => format!(
                                "Nothing was written: {e}. A bot has to be placed at the top of the chart \
                         first, a bot with reports and nothing above it; then that bot can write \
                         '{key}' on '{subject}'."
                            ),
                            false => format!(
                                "Nothing was written: {e}. Ask {} to write '{key}' on '{subject}'.",
                                allowed.join(" or ")
                            ),
                        }
                    }
                    // **A thread's ceiling binds the bots that write into it**, so
                    // the way forward is a bot above one of them, never the writer.
                    jojobot_domain::memory::MayWrite::WritersSuperior => match allowed.is_empty() {
                        true => format!(
                            "Nothing was written: {e}. None of the bots that write into '{subject}' \
                         has a manager recorded, so '{key}' on it cannot be set yet: place one of \
                         them under a manager first."
                        ),
                        false => format!(
                            "Nothing was written: {e}. Ask {} to write '{key}' on '{subject}'.",
                            allowed.join(" or ")
                        ),
                    },
                    jojobot_domain::memory::MayWrite::Ancestor
                    | jojobot_domain::memory::MayWrite::Superior => match allowed.is_empty() {
                        // **Nobody may make this write**: it names no bot as the
                        // manager, and the thing has none to change, so no bot is
                        // above it and none is named. The way on is the handle.
                        true => format!(
                            "Nothing was written: {e}. The value sent for '{key}' on '{subject}' \
                         names no bot and '{subject}' has no manager recorded to change, so no \
                         bot may write it: send the handle of the bot it reports to."
                        ),
                        // **A creation that would head a chart.** Existing records
                        // already name the new bot as their manager, so it heads a
                        // chart, and the head is the only one who may place it. It
                        // does not exist yet to do so: make it bare, then place it.
                        false
                            if verb == "add_entity" && allowed.as_slice() == [subject.clone()] =>
                        {
                            format!(
                                "Nothing was written: {e}. Existing records already name '{subject}' \
                             as their manager, so it heads a chart, and a head is placed by \
                             itself: create it without {key}, then write {key} on it as itself."
                            )
                        }
                        false => format!(
                            "Nothing was written: {e}. Ask {} to write '{key}' on '{subject}'.",
                            allowed.join(" or ")
                        ),
                    },
                },
            ),
        )),
        // **A chart write that would put a bot under one of its own reports.**
        // Refused whoever asks, so the way forward is a different manager, not a
        // different caller.
        MemoryError::ChartCycle {
            ref subject,
            ref manager,
        } => Ok(blocked_body(
            &EntityId(subject.clone()),
            &[],
            format!(
                "Nothing was written: {e}. '{manager}' is at or below '{subject}' in the chart. \
                 Name a manager that is not, or move '{manager}' out from under '{subject}' \
                 first."
            ),
        )),
        // **A merge into the caller's own bot that would carry a ceiling onto
        // it.** The refusal says the MERGE was refused and why, not that the
        // caller tried to set a key: it sent no fields. The ways forward are
        // the bots that may make the merge, and taking the key off the
        // duplicate first when the caller may.
        MemoryError::MergeCarriesGuardedKeys {
            ref duplicate,
            ref survivor,
            ref keys,
            may,
            ref allowed,
            clear_opens,
        } => Ok(blocked_body(&EntityId(duplicate.clone()), &[], {
            // **The route of taking the keys off the duplicate is named only
            // when the caller may take them off**: the domain asked the
            // licence a clear meets. Named to a caller it refuses, the clear
            // would loop back here (rule 68).
            let clear = if clear_opens {
                format!(
                    " Or take {keys} off '{duplicate}' first: update_fact the record that \
                         sets it with clear_fields, then merge again."
                )
            } else {
                String::new()
            };
            let who = allowed.join(" or ");
            // **The assistant holds the head of the chart**, so a refusal that
            // names it says so: that is true on every instance, whoever it
            // acts for.
            let operator = if allowed
                .iter()
                .any(|bot| bot == jojobot_domain::memory::ASSISTANT)
            {
                " The assistant holds the head of the chart."
            } else {
                ""
            };
            match may {
                jojobot_domain::memory::MayWrite::HeadOnceHeld { .. } => format!(
                    "Nothing was written. '{duplicate}' carries {keys}, and '{survivor}' \
                         already holds it, so merging them would change it, which only the bot \
                         that heads the chart may do. Ask that bot to make this merge.{clear}"
                ),
                // **A thread, judged by the bots that write into it.** The
                // survivor's key is the writers' superiors', and on a record
                // ceiling the assistant's too, so they are the ones named. A
                // different identity is not enough here, and the survivor is
                // not the caller's own bot.
                jojobot_domain::memory::MayWrite::WritersSuperior if allowed.is_empty() => {
                    format!(
                        "Nothing was written. '{duplicate}' carries {keys}, and no bot is \
                             recorded above the bots that write into '{survivor}', so no bot may \
                             make this merge yet. Place one of them under a manager, and that \
                             manager can.{clear}"
                    )
                }
                jojobot_domain::memory::MayWrite::WritersSuperior => format!(
                    "Nothing was written. '{duplicate}' carries {keys}, and merging it into \
                         '{survivor}' would set it there, which none of the bots that write into \
                         '{survivor}' may do, only {who}. Ask {who} to make this merge.\
                         {operator}{clear}"
                ),
                // **The chart, not a ceiling.** The merge would change who the
                // survivor reports to, which only the bots that may place it
                // may do, so they are the ones named.
                jojobot_domain::memory::MayWrite::Ancestor
                | jojobot_domain::memory::MayWrite::Superior
                    if allowed.is_empty() =>
                {
                    format!(
                        "Nothing was written. '{duplicate}' carries {keys}, which names no \
                             bot, and '{survivor}' has no manager recorded to change, so no bot \
                             may make this merge.{clear}"
                    )
                }
                jojobot_domain::memory::MayWrite::Ancestor
                | jojobot_domain::memory::MayWrite::Superior => format!(
                    "Nothing was written. '{duplicate}' carries {keys}, and merging it into \
                         '{survivor}' would change who '{survivor}' reports to, which only {who} \
                         may do. Ask {who} to make this merge.{clear}"
                ),
                _ => format!(
                    "Nothing was written. '{duplicate}' carries {keys}, and merging it into \
                         '{survivor}', your own bot, would raise your own ceiling, which only a \
                         different identity may do. Ask a different identity to make this \
                         merge.{clear}"
                ),
            }
        })),
        // **A role's own two fields, named on the ordinary surface.** The
        // subject named here is not an entity handle — `blocked_body` wants
        // one to check for candidates, and a role has none — so this is its
        // own small body, the same shape `RoomFull`'s is for the same
        // reason: what it carries has no `EntityId` to hang the shared
        // shape on.
        MemoryError::RoleFieldGuarded { ref role, ref key } => {
            let how_to_proceed = WayForward::from(format!(
                "Nothing was written: {e}. Claim the '{role}' role through start_here's own \
                     claim argument — that is the only door either of a role's own two fields \
                     opens through."
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": key,
                "wrote": false,
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A voucher is issued, never written.** The subject is a voucher's
        // handle, which may not exist as an entity, so this is its own small
        // body for the reason the role refusal above gives.
        MemoryError::VoucherIssued { ref subject } => {
            let how_to_proceed = WayForward::from(format!(
                "Nothing was written: {e}. A voucher is a grant the engine issues for the \
                 operator's code; no record a caller writes is a grant. '{subject}' stays as \
                 it is, and nothing here creates, edits or takes one back. A session that needs \
                 one asks for a code with start_here's ask_for_code and boots with the code the \
                 operator types in."
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A challenge is issued, never written**, for the reason a voucher is.
        MemoryError::ChallengeIssued { ref subject } => {
            let how_to_proceed = WayForward::from(format!(
                "Nothing was written: {e}. A challenge holds the hash of a code jojobot sent to \
                 the operator, and only the engine writes it. '{subject}' stays as it is; a boot \
                 asks for a new code with start_here's ask_for_code."
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": subject,
                "wrote": false,
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **A right no code knows.** Names the words that are known, so the way
        // forward is in the refusal.
        MemoryError::UnknownRight {
            ref right,
            ref known,
        } => {
            let how_to_proceed = WayForward::from(format!(
                "Nothing was written: {e}. A right grants only what code gives it, so a \
                 voucher for a word nothing implements is refused. The rights known here are: {}.",
                known.join(", ")
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": right,
                "wrote": false,
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        // **The claim path's own refusal, not a caller mistake about shape.**
        // A second claimant told no while the lease is fresh — the same
        // information `start_here`'s own `claim` answer carries when this is
        // reached through the boot door, served here for whatever other
        // path reaches this refusal.
        MemoryError::RoleTaken {
            ref role,
            ref holder,
            until,
        } => {
            let how_to_proceed = WayForward::from(format!(
                "Nothing was written: {e}. {}",
                role_taken_way_forward(role, holder, &until.to_string())
            ));
            let mut body = serde_json::json!({
                "status": "blocked",
                "attempted": role,
                "wrote": false,
                "holder": holder,
                "until": until.to_string(),
            });
            how_to_proceed.write_into(&mut body);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                body.to_string(),
            )]))
        }
        other => Err(memory_error(other)),
    }
}

/// **What to do about a role somebody else holds** — said once, for the
/// refusal a write gets and for the `refused` outcome of a claim at the boot
/// door, so the two cannot come to say different things.
pub(crate) fn role_taken_way_forward(role: &str, holder: &str, until: &str) -> String {
    format!("Wait until {until}, or ask '{holder}' to release '{role}' by wrapping its session.")
}

/// **What to do about a role that belongs to another bot.** The refusal a claim
/// at the boot door gets when the role is carried by, or sits under, a bot
/// other than the one booting.
pub(crate) fn role_owned_way_forward(role: &str, owner: &str) -> String {
    let slug = owner.split_once(':').map_or(owner, |(_, slug)| slug);
    format!(
        "'{role}' belongs to {owner}. To claim it, call start_here with bot set to '{slug}'. \
         Or claim a name no bot owns. Nothing was written."
    )
}

/// **What to do about a role whose object is archived.** The refusal a claim at
/// the boot door gets when the object `role:<role>` stands archived: the way
/// forward is to restore the object, which only the bot it belongs to or a bot
/// heading the chart may do, and then claim again.
pub(crate) fn role_archived_way_forward(
    role: &str,
    object: &str,
    may_restore: &[String],
) -> String {
    format!(
        "'{role}' was not claimed: its role object {object} is archived. Restore it with \
         archive_entity, handle '{object}' and restore true; only {} may. Then claim '{role}' \
         again. Nothing was written.",
        may_restore.join(" or "),
    )
}

/// Map a domain [`MemoryError`] to an MCP error, splitting client mistakes
/// (invalid params) from server-side failures.
pub(crate) fn memory_error(e: MemoryError) -> McpError {
    let word = memory_fix_by(&e);
    stamp_error(memory_error_arms(e), word)
}

fn memory_error_arms(e: MemoryError) -> McpError {
    match e {
        // **Backstops, not the intended answer.** Every one of these is a
        // caller mistake and `memory_declined` answers all of them as blocked
        // results with a way forward (rule 68). They are reached only by a verb
        // that surfaces an error without going through that path, and they stay
        // client errors rather than 500s for that case.
        MemoryError::InvalidFact(_)
        | MemoryError::InvalidSubject(_)
        | MemoryError::WeekOfUsageWritten(_)
        | MemoryError::KindsNeverLoaded { .. }
        | MemoryError::InvalidAddress(_)
        | MemoryError::InvalidEntity(_)
        | MemoryError::InvalidEdge(_)
        | MemoryError::InvalidQuery(_)
        | MemoryError::InvalidType(_)
        | MemoryError::ShippedType { .. }
        | MemoryError::RepeatsShipped
        | MemoryError::BreaksFit { .. }
        | MemoryError::BreaksType { .. }
        | MemoryError::RemapsShippedWord { .. }
        | MemoryError::BreaksSchedule { .. }
        | MemoryError::ViewNarrowsNothing { .. }
        | MemoryError::NumberTaken { .. }
        | MemoryError::NumberFreed { .. }
        | MemoryError::NotANumber { .. }
        | MemoryError::MembersBreakNumbering { .. }
        | MemoryError::BootTooHeavy { .. }
        | MemoryError::ClaimTooLarge { .. }
        | MemoryError::ThingTooLarge { .. }
        | MemoryError::RoleNotHeld { .. }
        | MemoryError::UnknownFact { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::NotYours { .. }
        | MemoryError::NotRetractable { .. }
        | MemoryError::UnsourcedObservation
        | MemoryError::AlreadyRetracted { .. }
        | MemoryError::AlreadyArchived { .. }
        | MemoryError::NotArchived { .. }
        | MemoryError::NothingToMerge { .. }
        | MemoryError::DocumentMergedWithAThing { .. }
        | MemoryError::MergeIntoDescendant { .. }
        | MemoryError::AlreadyMerged { .. }
        | MemoryError::NothingToRename { .. }
        | MemoryError::HandleMoved { .. }
        | MemoryError::SuppliedHandle { .. }
        | MemoryError::UnconfirmedPromotion
        | MemoryError::UnreceiptedSettling
        | MemoryError::ReceiptWithoutSettling
        | MemoryError::RoomFull { .. }
        | MemoryError::OverCapacity { .. }
        | MemoryError::KeyNotYours { .. }
        | MemoryError::ChartCycle { .. }
        | MemoryError::MergeCarriesGuardedKeys { .. }
        | MemoryError::ThoughtTooLong { .. }
        | MemoryError::MergeOverfillsRoom { .. }
        | MemoryError::MergeThoughtTooLong { .. }
        | MemoryError::RoleFieldGuarded { .. }
        | MemoryError::VoucherIssued { .. }
        | MemoryError::ChallengeIssued { .. }
        | MemoryError::UnknownRight { .. }
        | MemoryError::RoleTaken { .. }
        | MemoryError::StaleRevision { .. }
        | MemoryError::TextChangedWhileEditing { .. }
        | MemoryError::TextEditMatches { .. }
        | MemoryError::TestimonyRewritten { .. }
        | MemoryError::AppClaimRewritten { .. }
        | MemoryError::KeyInOtherBag { .. }
        | MemoryError::UnstatedProvenance => McpError::invalid_params(e.to_string(), None),
        MemoryError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
        }
        // **Damage is not an outage.** The store answered and what it holds is
        // wrong; the sentence names the damage and says a person must repair it,
        // and never says to try again.
        MemoryError::StoredDamage { what } => {
            McpError::internal_error(crate::boundary::damaged("this call", &what), None)
        }
        // **A refusal is not an outage** — see the domain's own doc on
        // [`MemoryError::Refused`]. The sentence says the store answered and that
        // sending the same call again meets the same refusal.
        MemoryError::Refused(rule) => {
            McpError::internal_error(crate::boundary::refused("this call", &rule), None)
        }
        // **A conflict is not a failure** — see the domain's own doc on
        // [`MemoryError::Conflict`]. It reaches the caller through the same
        // JSON-RPC error shape as `Store` (rated with the same severity,
        // because a payload a client cannot act on is a server fault
        // whatever the underlying cause), but the sentence itself says the
        // opposite of `store_failed`'s: retry, not escalate.
        MemoryError::Conflict => McpError::internal_error(
            crate::boundary::conflict("this call", &MemoryError::Conflict.to_string()),
            None,
        ),
        // **Reaching this arm is itself the bug.** `FoldBehind` says the write
        // landed — every verb that can raise it (`capture`, `update_fact`,
        // `retract`, `merge`) catches it before its error ever reaches this
        // mapper, and answers with the record it carries plus a fold-behind
        // note (rule 130). An `McpError` here would tell the caller the write
        // failed, which is exactly false; there is no channel through this
        // function that says otherwise, so a call site missing that handling
        // is what needs fixing, not this arm.
        MemoryError::FoldBehind { .. } => McpError::internal_error(
            format!(
                "{e}. This is not a failed write — the record landed. The verb that raised this \
                 is missing its fold-behind handling; that is the defect to fix."
            ),
            None,
        ),
    }
}

/// **How many thoughts a room must archive to take one more.** The write adds a
/// thought, so the room has to end below its capacity first; a room an earlier
/// borrow left over its capacity needs more than one archived. The one count
/// (rule 498) behind the edit's refusal, the over-capacity refusal and the
/// `archive_needed` they carry.
fn thoughts_to_archive_count(live: usize, capacity: usize) -> usize {
    (live + 1).saturating_sub(capacity).max(1)
}

/// The count as a phrase: "one of the thoughts above", "2 of the thoughts above".
fn thoughts_to_archive(live: usize, capacity: usize) -> String {
    match thoughts_to_archive_count(live, capacity) {
        1 => "one of the thoughts above".to_string(),
        n => format!("{n} of the thoughts above"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// **Wired to the mechanism, not merely beside it** (decision log 261,
    /// 262). Every arm of `memory_declined` funnels through `blocked_body`,
    /// so proving this one site panics on an empty way forward is proving it
    /// for all of them at once.
    #[test]
    #[should_panic(expected = "way forward")]
    fn blocked_body_cannot_be_built_with_an_empty_way_forward() {
        blocked_body(&EntityId("person:homer".into()), &[], String::new());
    }

    /// **A caller mistake never leaves this rail through the error channel**
    /// (rule 68). It comes back as a blocked answer carrying what is wrong and
    /// what to do about it.
    ///
    /// Driven through the verbs a caller calls, because these faults arrive by
    /// two different routes and only one of them is the fall-through the
    /// mapper is blamed for. `capture`, `add_entity` and `search` never reach
    /// the declined path at all: they hand the domain's error straight to the
    /// mapper, so an arm added there does nothing for them until the call site
    /// is routed too. Asking the mapper directly would pass on a build where
    /// none of them is wired to anything.
    #[tokio::test]
    async fn a_malformed_memory_write_is_an_answer_rather_than_an_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args("person", "person:alpha", "Alpha")))
            .await
            .expect("add ok");

        // Refused inside the domain's own write.
        let empty_claim = jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(sid.clone()),
                ..capture_args("person:alpha", "   ")
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        let bad_entity = jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(sid.clone()),
                ..add_args("person", "person:beta", "   ")
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        // Refused by the query's own validation, before the index is read.
        let empty_query = jojobot
            .search(Parameters(SearchArgs {
                sid: Some(sid),
                ..search_args()
            }))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");

        for (what, result) in [
            ("an empty claim", &empty_claim),
            ("an entity with no name", &bad_entity),
            ("a search that narrows nothing", &empty_query),
        ] {
            let body = blocked(result);
            assert_eq!(body["wrote"], false, "{what} wrote something: {body}");
            assert!(
                body["how_to_proceed"]
                    .as_str()
                    .is_some_and(|advice| !advice.is_empty()),
                "{what} came back with no way forward: {body}"
            );
        }
    }

    /// **A refused key write names the bots that may make it** (rule 261), for
    /// every relation a key can declare, and never "the operator": the
    /// operator acts through the assistant, which is above every bot that has a
    /// chain. The two relations no ordinary caller reaches by a key of its own
    /// are reached here, and the cycle refusal beside them.
    #[test]
    fn a_refused_key_write_names_the_bots_that_may_make_it_for_every_relation() {
        use jojobot_domain::memory::MayWrite;
        let refused = |may, allowed: &[&str]| {
            let result = memory_declined(
                "capture",
                MemoryError::KeyNotYours {
                    subject: "bot:sigma".into(),
                    key: "a_key".into(),
                    may,
                    allowed: allowed.iter().map(|m| m.to_string()).collect(),
                },
            )
            .expect("a refusal is an answer");
            blocked(&result)["how_to_proceed"]
                .as_str()
                .expect("a way forward")
                .to_string()
        };
        for may in [MayWrite::Ancestor, MayWrite::Superior] {
            // The bots that may are named in the way forward, and the operator
            // is not named at all.
            let said = refused(may, &["bot:gamma", "bot:omega"]);
            for who in ["bot:gamma", "bot:omega"] {
                assert!(said.contains(who), "{who} may make it: {said}");
            }
            assert!(!said.contains("operator"), "{said}");
            // Nobody may make the write: said in one line, with the reason and
            // the way on, and never an unnamed somebody to go and find.
            let none = refused(may, &[]);
            assert!(
                none.contains("a_key") && none.contains("bot:sigma"),
                "{none}"
            );
            assert!(none.contains("handle"), "the way on is a handle: {none}");
            assert!(!none.contains("whoever"), "no unnamed somebody: {none}");
            assert!(!none.contains("operator"), "{none}");
            assert!(!none.contains('\n'), "one line: {none}");
        }
        // The thing itself, and a different identity (whose words are the four
        // ceilings' own and are unchanged).
        assert!(refused(MayWrite::Subject, &[]).contains("bot:sigma"));
        assert!(refused(MayWrite::DifferentIdentity, &[]).contains("different identity"));
    }

    /// **A merge refused for a chart key names the bots that may make it, and a
    /// merge refused for a ceiling still says a different identity must.** The
    /// two keys have different relations, and the refusal for the second was
    /// worded for the first: a stranger folding in a duplicate that holds a
    /// manager was told the merge would raise its own ceiling, and nobody was
    /// named. Paired, so neither wording stands in for the other.
    #[test]
    fn a_merge_refused_for_a_chart_key_names_who_may_make_it() {
        use jojobot_domain::memory::MayWrite;
        let refused = |may: MayWrite, allowed: &[&str]| {
            let result = memory_declined(
                "merge_entities",
                MemoryError::MergeCarriesGuardedKeys {
                    duplicate: "bot:epsilon".into(),
                    survivor: "bot:psi".into(),
                    keys: "reports_to".into(),
                    may,
                    allowed: allowed.iter().map(ToString::to_string).collect(),
                    clear_opens: true,
                },
            )
            .expect("a refusal is an answer");
            blocked(&result)["how_to_proceed"]
                .as_str()
                .expect("a way forward")
                .to_string()
        };
        let chart = refused(MayWrite::Superior, &["bot:omega", "bot:sigma"]);
        for who in ["bot:omega", "bot:sigma", "bot:epsilon", "bot:psi"] {
            assert!(chart.contains(who), "{who}: {chart}");
        }
        assert!(
            !chart.contains("your own ceiling"),
            "the chart is not a ceiling: {chart}"
        );
        // Nobody may make the merge: it carries no manager that names a bot,
        // and the survivor has none to change. The way on is the one that
        // always exists, taking the key off the duplicate, and no unnamed
        // somebody is sent for.
        let none = refused(MayWrite::Superior, &[]);
        for named in ["bot:epsilon", "bot:psi", "reports_to", "clear_fields"] {
            assert!(none.contains(named), "{named}: {none}");
        }
        assert!(!none.contains("whoever"), "no unnamed somebody: {none}");
        assert!(!none.contains("operator"), "{none}");
        assert!(!none.contains('\n'), "one line: {none}");
        // A list of nobody rendered as a gap reads like a sentence with a word
        // missing, so a double space is the tell.
        assert!(!none.contains("  "), "no empty list of bots: {none}");
        let ceiling = refused(MayWrite::DifferentIdentity, &[]);
        assert!(ceiling.contains("different identity"), "{ceiling}");
    }

    /// **A merge into a thread, refused for the thread's writers, names the bots
    /// above them and does not send for "a different identity".** The relation is
    /// the writers', so any other bot is not enough, and the caller's own bot is
    /// not what is merged into. With no bot above the writers, nobody is sent for
    /// and the way on is the key taken off the duplicate.
    #[test]
    fn a_merge_into_a_thread_refused_for_its_writers_names_who_may_make_it() {
        use jojobot_domain::memory::MayWrite;
        let refused = |allowed: &[&str], clear_opens: bool| {
            let result = memory_declined(
                "merge_entities",
                MemoryError::MergeCarriesGuardedKeys {
                    duplicate: "thread:simpson-kitchen".into(),
                    survivor: "thread:milhouse-moves".into(),
                    keys: "record_capacity".into(),
                    may: MayWrite::WritersSuperior,
                    allowed: allowed.iter().map(ToString::to_string).collect(),
                    clear_opens,
                },
            )
            .expect("a refusal is an answer");
            blocked(&result)["how_to_proceed"]
                .as_str()
                .expect("a way forward")
                .to_string()
        };
        let named = refused(&["bot:otto", "bot:lisa"], true);
        for who in [
            "bot:otto",
            "bot:lisa",
            "thread:milhouse-moves",
            "record_capacity",
        ] {
            assert!(named.contains(who), "{who}: {named}");
        }
        assert!(!named.contains("different identity"), "{named}");
        assert!(!named.contains("your own bot"), "{named}");
        let nobody = refused(&[], true);
        for word in ["thread:milhouse-moves", "record_capacity", "clear_fields"] {
            assert!(nobody.contains(word), "{word}: {nobody}");
        }
        assert!(!nobody.contains("different identity"), "{nobody}");
        assert!(!nobody.contains('\n'), "one line: {nobody}");
        assert!(!nobody.contains("  "), "no empty list of bots: {nobody}");
        // **No clear the caller may not make**: the arm with nobody allowed
        // names the way on, a manager, and no clear.
        let closed = refused(&[], false);
        for word in ["thread:milhouse-moves", "record_capacity", "manager"] {
            assert!(closed.contains(word), "{word}: {closed}");
        }
        assert!(!closed.contains("clear_fields"), "{closed}");
        assert!(!closed.contains("  "), "no empty list of bots: {closed}");
    }

    /// **A chart cycle is refused with a way forward that names a different
    /// manager**, since no different caller helps.
    #[test]
    fn a_chart_cycle_is_refused_naming_both_bots() {
        let result = memory_declined(
            "capture",
            MemoryError::ChartCycle {
                subject: "bot:assistant".into(),
                manager: "bot:sigma".into(),
            },
        )
        .expect("a refusal is an answer");
        let said = blocked(&result)["how_to_proceed"]
            .as_str()
            .expect("a way forward")
            .to_string();
        for who in ["bot:assistant", "bot:sigma"] {
            assert!(said.contains(who), "{who}: {said}");
        }
    }

    /// **The store being down is a failure, not a blocked answer** — the other
    /// half of the line this module draws, over the one read both typed verbs
    /// make before anything else.
    ///
    /// `declared_types` has exactly one fallible step, so every error reachable
    /// through it is the store. A client following the published instructions
    /// branches on `status`, and `blocked` tells it the call was ITS mistake:
    /// it fixes the type name it got right, and never retries the outage.
    ///
    /// Driven through both verbs that take `answers_type`, because the resolve
    /// is shared and a channel is chosen at each call site — one of them
    /// swallowing the error back into `Ok` is exactly the drift this pins.
    #[tokio::test]
    async fn a_store_that_cannot_list_types_is_a_failure_rather_than_the_callers_mistake() {
        let jojobot = Jojobot::new(
            Arc::new(DownMemory(
                Down::TypeRoster,
                Arc::new(InMemoryMemory::booted()),
            )),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let sid = writing_as(&jojobot);

        let recalled = jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid.clone()),
                answers_type: Some("pet".into()),
                ..recall_args("person:bart")
            }))
            .await;
        let searched = jojobot
            .search(Parameters(SearchArgs {
                sid: Some(sid),
                answers_type: Some("pet".into()),
                ..search_args()
            }))
            .await;

        for (verb, result) in [("recall", recalled), ("search", searched)] {
            let err = match result {
                Err(err) => err,
                Ok(ok) => panic!(
                    "{verb} handed an outage back as a caller-fixable answer: {}",
                    text_of(&ok)
                ),
            };
            // The adapter's own words stay inside, as everywhere else on this
            // rail — what crosses is that it was the store and to try again.
            assert!(
                !err.message.contains("the type roster cannot be read"),
                "{verb} let the adapter's own words cross: {}",
                err.message
            );
            assert!(
                err.message.contains("Try once more"),
                "{verb} left the caller without its next move: {}",
                err.message
            );
        }
    }

    /// A store failure's own account must not reach the caller — the same
    /// invariant the mailbox and session rails hold, through the same
    /// function.
    #[test]
    fn a_store_failure_does_not_carry_the_adapters_own_words() {
        let leaky = "the page for gamma has no table, and the row vanished from the document";
        let err = memory_error(MemoryError::Store(leaky.into()));
        assert!(
            !err.message.contains(leaky),
            "the adapter's own words crossed: {}",
            err.message
        );
        assert!(
            err.message.contains("Try once more"),
            "a caller needs its next move: {}",
            err.message
        );
    }

    /// **Stored damage is told as damage a person repairs, and never as an
    /// outage.** The caller is told which things are damaged, in the handles it
    /// knows, and that sending the call again meets the same damage; the
    /// sentence an outage gets (try once more) is nowhere in it. **Paired** with
    /// the outage, which still says to try once more and wears `retry`.
    #[test]
    fn stored_damage_names_the_damage_and_never_says_to_try_again() {
        let what = "the merge events in the store lead back to where they began: \
                    person:alpha, person:beta";
        let damage = memory_error(MemoryError::StoredDamage { what: what.into() });
        assert!(
            damage.message.contains("person:alpha") && damage.message.contains("person:beta"),
            "the damaged things are named: {}",
            damage.message
        );
        assert!(
            !damage.message.contains("Try once more"),
            "damage was told the way an outage is: {}",
            damage.message
        );
        assert_error_word("StoredDamage", &damage, Some("person"));
        let outage = memory_error(MemoryError::Store("the store is gone".into()));
        assert!(
            outage.message.contains("Try once more"),
            "{}",
            outage.message
        );
        assert_error_word("Store", &outage, Some("retry"));
    }

    /// **`memory_error` routes `Refused` through `refused`, not `store_failed`**
    /// — the wiring proof beside `boundary`'s own sentence. A write the store
    /// refused reaches the caller as its own answer, naming the kind of rule,
    /// and never as the answer a store that could not be reached gets.
    #[test]
    fn a_refused_write_is_mapped_through_its_own_sentence_not_the_failure_one() {
        let refused = memory_error(MemoryError::Refused("a key held twice".into()));
        let outage = memory_error(MemoryError::Store("the store is gone".into()));
        assert_ne!(
            refused.message, outage.message,
            "a refusal and an outage reached the caller as one answer"
        );
        assert!(
            refused.message.contains("a key held twice"),
            "the caller is told which kind of rule refused it: {}",
            refused.message
        );
    }

    /// **`memory_error` routes `Conflict` through `conflict`, not
    /// `store_failed`** — the wiring proof beside `boundary`'s own case for
    /// the sentence's shape. A caller reading this through the served
    /// surface must see retry advice, never the "tell the operator" line a
    /// bare store failure carries.
    #[test]
    fn a_conflict_is_mapped_through_the_conflict_sentence_not_the_failure_one() {
        let err = memory_error(MemoryError::Conflict);
        assert!(
            !err.message.contains("tell the operator"),
            "a conflict routed through the failure sentence rather than its own: {}",
            err.message
        );
        assert!(
            err.message.contains("retry") || err.message.contains("retrying"),
            "the caller's next move must be named: {}",
            err.message
        );
    }
    /// One row per refusal kind: its name, an error the domain raises for it,
    /// and the word it wears. `memory_fix_by` is an exhaustive match, so a kind
    /// added without a word does not compile; the rows are what the served
    /// answers are held against.
    pub(crate) fn memory_refusal_rows() -> Vec<(&'static str, MemoryError, Option<&'static str>)> {
        use jojobot_domain::memory::MayWrite;

        let s = |text: &str| text.to_string();
        let rows: Vec<(&str, MemoryError, Option<&str>)> = vec![
            (
                "InvalidFact",
                MemoryError::InvalidFact(s("empty")),
                Some("change"),
            ),
            (
                "InvalidSubject",
                MemoryError::InvalidSubject(s("x")),
                Some("change"),
            ),
            (
                "WeekOfUsageWritten",
                MemoryError::WeekOfUsageWritten(EntityId(s("usage:bart-2026-w41"))),
                Some("change"),
            ),
            (
                "InvalidAddress",
                MemoryError::InvalidAddress(s("x")),
                Some("change"),
            ),
            (
                "InvalidEntity",
                MemoryError::InvalidEntity(s("x")),
                Some("change"),
            ),
            (
                "InvalidEdge",
                MemoryError::InvalidEdge(s("x")),
                Some("change"),
            ),
            (
                "InvalidQuery",
                MemoryError::InvalidQuery(s("x")),
                Some("change"),
            ),
            (
                "InvalidType",
                MemoryError::InvalidType(s("x")),
                Some("change"),
            ),
            (
                "UnstatedProvenance",
                MemoryError::UnstatedProvenance,
                Some("change"),
            ),
            (
                "UnknownEntity",
                MemoryError::UnknownEntity {
                    attempted: s("person:lisa"),
                    nearest: Vec::new(),
                },
                Some("change"),
            ),
            (
                "UnknownFact",
                MemoryError::UnknownFact {
                    attempted: s("person:alpha#f9"),
                    nearest: Vec::new(),
                },
                Some("change"),
            ),
            (
                "NotYours",
                MemoryError::NotYours {
                    attempted: s("zz99"),
                },
                Some("change"),
            ),
            (
                "AlreadyRetracted",
                MemoryError::AlreadyRetracted {
                    attempted: s("person:alpha#f1"),
                },
                Some("change"),
            ),
            (
                "AlreadyArchived",
                MemoryError::AlreadyArchived {
                    attempted: s("person:alpha"),
                },
                Some("change"),
            ),
            (
                "NotArchived",
                MemoryError::NotArchived {
                    attempted: s("person:alpha"),
                },
                Some("change"),
            ),
            (
                "UnsourcedObservation",
                MemoryError::UnsourcedObservation,
                Some("change"),
            ),
            (
                "NotRetractable",
                MemoryError::NotRetractable {
                    attempted: s("person:alpha#f1"),
                    why: s("a rule"),
                },
                Some("change"),
            ),
            (
                "TextChangedWhileEditing",
                MemoryError::TextChangedWhileEditing {
                    address: s("person:alpha#f1"),
                    field: s("details"),
                    current: 2,
                },
                Some("retry"),
            ),
            (
                "TextEditMatches",
                MemoryError::TextEditMatches {
                    address: s("person:alpha#f1"),
                    field: s("details"),
                    found: 2,
                },
                Some("change"),
            ),
            (
                "StaleRevision",
                MemoryError::StaleRevision {
                    address: s("person:alpha#f1"),
                    expected: 1,
                    current: 2,
                },
                Some("retry"),
            ),
            (
                "TestimonyRewritten",
                MemoryError::TestimonyRewritten {
                    address: s("person:alpha#f1"),
                },
                Some("change"),
            ),
            (
                "AppClaimRewritten",
                MemoryError::AppClaimRewritten {
                    address: s("person:alpha#f1"),
                },
                Some("change"),
            ),
            (
                "RepeatsShipped",
                MemoryError::RepeatsShipped,
                Some("change"),
            ),
            (
                "ShippedType",
                MemoryError::ShippedType {
                    name: s("entitlement"),
                    displaced: None,
                },
                Some("change"),
            ),
            (
                "BreaksFit",
                MemoryError::BreaksFit {
                    name: s("pet"),
                    keys: vec![s("owner")],
                },
                Some("change"),
            ),
            (
                "BreaksSchedule",
                MemoryError::BreaksSchedule {
                    held: s("advances_from"),
                    missing: s("cadence_days"),
                },
                Some("change"),
            ),
            (
                "ViewNarrowsNothing",
                MemoryError::ViewNarrowsNothing {
                    view: s("view:plain-listing"),
                },
                Some("change"),
            ),
            (
                "BreaksType",
                MemoryError::BreaksType {
                    name: s("journey"),
                    key: s("starts"),
                    wanted: s("date"),
                    value: s("soon"),
                },
                Some("change"),
            ),
            (
                "RemapsShippedWord",
                MemoryError::RemapsShippedWord {
                    word: s("done"),
                    value: s("someday"),
                },
                Some("change"),
            ),
            (
                "ThingTooLarge",
                MemoryError::ThingTooLarge {
                    subject: s("thing:gamma"),
                    size: 29_000,
                    room: 27_000,
                },
                Some("change"),
            ),
            (
                "ClaimTooLarge",
                MemoryError::ClaimTooLarge {
                    subject: s("thing:gamma"),
                    size: 30_000,
                    room: 27_000,
                },
                Some("change"),
            ),
            (
                "BootTooHeavy",
                MemoryError::BootTooHeavy {
                    subject: s("bot:gamma"),
                    floor: 30_000,
                    budget: 28_000,
                    parts: vec![(s("charter"), 20_000)],
                    stamp_margin: 0,
                },
                Some("change"),
            ),
            (
                "KeyNotYours",
                MemoryError::KeyNotYours {
                    subject: s("bot:gamma"),
                    key: s("reports_to"),
                    may: MayWrite::Subject,
                    allowed: Vec::new(),
                },
                Some("change"),
            ),
            (
                "ChartCycle",
                MemoryError::ChartCycle {
                    subject: s("bot:gamma"),
                    manager: s("bot:delta"),
                },
                Some("change"),
            ),
            (
                "MergeCarriesGuardedKeys",
                MemoryError::MergeCarriesGuardedKeys {
                    duplicate: s("bot:gamma"),
                    survivor: s("bot:delta"),
                    keys: "rule_seats".into(),
                    may: MayWrite::DifferentIdentity,
                    allowed: Vec::new(),
                    clear_opens: true,
                },
                Some("change"),
            ),
            (
                "RoleFieldGuarded",
                MemoryError::RoleFieldGuarded {
                    role: s("dev"),
                    key: s("holder"),
                },
                Some("change"),
            ),
            (
                "VoucherIssued",
                MemoryError::VoucherIssued {
                    subject: s("voucher:boot-as-patty"),
                },
                Some("change"),
            ),
            (
                "ChallengeIssued",
                MemoryError::ChallengeIssued {
                    subject: s("challenge:abcd"),
                },
                Some("change"),
            ),
            (
                "UnknownRight",
                MemoryError::UnknownRight {
                    right: s("fly"),
                    known: vec![s("boot-as")],
                },
                Some("change"),
            ),
            (
                "RoomFull",
                MemoryError::RoomFull {
                    subject: s("bot:gamma"),
                    live: 3,
                    capacity: 3,
                    room: Vec::new(),
                    aged_out: 0,
                },
                Some("change"),
            ),
            (
                "MergeOverfillsRoom",
                MemoryError::MergeOverfillsRoom {
                    subject: s("bot:gamma"),
                    live: 3,
                    capacity: 3,
                    incoming: 1,
                    room: Vec::new(),
                },
                Some("change"),
            ),
            (
                "ThoughtTooLong",
                MemoryError::ThoughtTooLong {
                    subject: s("bot:gamma"),
                    len: 900,
                    cap: 500,
                },
                Some("change"),
            ),
            (
                "MergeThoughtTooLong",
                MemoryError::MergeThoughtTooLong {
                    subject: s("bot:gamma"),
                    thought: s("x"),
                    len: 900,
                    cap: 500,
                },
                Some("change"),
            ),
            (
                "NothingToMerge",
                MemoryError::NothingToMerge {
                    attempted: s("person:alpha"),
                },
                Some("change"),
            ),
            (
                "DocumentMergedWithAThing",
                MemoryError::DocumentMergedWithAThing {
                    document: s("document:alpha"),
                    other: s("thing:handcart"),
                },
                Some("change"),
            ),
            (
                "NumberFreed",
                MemoryError::NumberFreed {
                    collection: s("topic:widgets"),
                    key: s("number"),
                    number: 2,
                    holder: s("thing:handcart"),
                },
                Some("change"),
            ),
            (
                "MembersBreakNumbering",
                MemoryError::MembersBreakNumbering {
                    collection: s("topic:widgets"),
                    key: s("number"),
                    members: vec![s("thing:handcart (5)"), s("thing:handcart-2 (5)")],
                },
                Some("change"),
            ),
            (
                "NothingToRename",
                MemoryError::NothingToRename {
                    attempted: s("person:alpha"),
                },
                Some("change"),
            ),
            (
                "MergeIntoDescendant",
                MemoryError::MergeIntoDescendant {
                    folded: s("org:springfield-town-council"),
                    survivor: s("org:springfield-cyclery"),
                },
                Some("change"),
            ),
            (
                "AlreadyMerged",
                MemoryError::AlreadyMerged {
                    attempted: s("person:alpha"),
                    into: s("person:beta"),
                },
                Some("change"),
            ),
            (
                "HandleMoved",
                MemoryError::HandleMoved {
                    attempted: s("person:alpha"),
                    now: s("person:beta"),
                },
                Some("change"),
            ),
            (
                "SuppliedHandle",
                MemoryError::SuppliedHandle {
                    attempted: s("view:colleagues"),
                },
                Some("change"),
            ),
            (
                "UnreceiptedSettling",
                MemoryError::UnreceiptedSettling,
                Some("change"),
            ),
            (
                "ReceiptWithoutSettling",
                MemoryError::ReceiptWithoutSettling,
                Some("change"),
            ),
            // Damage no caller can repair, a kind set nothing re-reads, and
            // claims only the operator can bless.
            (
                "KindsNeverLoaded",
                MemoryError::KindsNeverLoaded { attempted: None },
                Some("person"),
            ),
            (
                "UnconfirmedPromotion",
                MemoryError::UnconfirmedPromotion,
                Some("person"),
            ),
            // A held role frees when its lease ends; a collision clears on the
            // next try; a storage failure is a retry while the store-failure
            // switch is on. `FoldBehind` is not in the table: the write LANDED,
            // so it has no refusal word (`memory_fix_by` says `None`), and
            // building its landed record here would test the fixture.
            (
                "RoleTaken",
                MemoryError::RoleTaken {
                    role: s("dev"),
                    holder: s("4nfx"),
                    until: jiff::Timestamp::UNIX_EPOCH,
                },
                Some("retry"),
            ),
            ("Conflict", MemoryError::Conflict, Some("retry")),
            // **A store that could not be reached is an outage, so the same call
            // may get past it; a store that answered and refused is a constraint
            // the caller's input broke, and the same call meets it again.** The
            // two words are pinned as words, not through the switch.
            (
                "Refused",
                MemoryError::Refused(s("a key held twice")),
                Some("change"),
            ),
            (
                "Store",
                MemoryError::Store(s("connection refused")),
                Some("retry"),
            ),
            // **Damage a person repairs, set beside the outage it is not.**
            (
                "StoredDamage",
                MemoryError::StoredDamage {
                    what: s(
                        "the merge events in the store lead back to where they began: person:alpha, person:beta",
                    ),
                },
                Some("person"),
            ),
        ];
        rows
    }

    /// 🚨 **The refusal of half a schedule says where the other half goes.** A
    /// caller told only to send the key "in the same call" sent it under
    /// `fields`, where it stays on the claim and the loop is still half a loop.
    /// The refusal names `sets` and both ways to give a schedule.
    #[test]
    fn the_refusal_of_half_a_schedule_says_to_send_the_key_in_sets() {
        let refused = memory_declined(
            "capture",
            MemoryError::BreaksSchedule {
                held: "advances_from".into(),
                missing: "cadence_days".into(),
            },
        )
        .expect("an answer");
        let body = json_of(&refused);
        let way = body["how_to_proceed"].as_str().expect("a way forward");
        assert!(way.contains("`sets`"), "{way}");
        assert!(
            way.contains("cadence_days") && way.contains("rrule"),
            "{way}"
        );
        // **"Clear it" names the verb that clears.** Only `update_fact` takes
        // `clear_fields`; on the other verbs a key cannot be cleared.
        assert!(
            way.contains("update_fact") && way.contains("clear_fields"),
            "the way to switch the schedule off names its verb: {way}"
        );
    }

    /// 🚨 **Every refusal kind this lane answers wears the word its fix needs.**
    /// One row per kind, each reddening alone: the row names the kind, builds
    /// the error the domain raises for it, and asserts the word on the answer
    /// that comes back. `change` is the call itself; `person` is damage a
    /// person has to repair, where sending the call again or changing it
    /// cannot help. A kind that answers as a protocol error rather than as a
    /// blocked body (a storage failure or a collision) is not in this table.
    #[test]
    fn every_refusal_kind_in_the_memory_lane_wears_its_word() {
        for (label, error, word) in memory_refusal_rows() {
            assert_eq!(
                memory_fix_by(&error).map(FixBy::as_token),
                word,
                "{label}: the match and the row disagree"
            );
        }
        // Served as a blocked answer where the lane answers one, or as a
        // protocol error carrying the word in `data` where it does not.
        for (label, error, word) in memory_refusal_rows() {
            match memory_declined("capture", error) {
                Ok(answered) => assert_word(label, &answered, word),
                Err(raised) => assert_error_word(label, &raised, word),
            }
        }
        // And every kind as the protocol error the backstop raises.
        for (label, error, word) in memory_refusal_rows() {
            let raised = memory_error(error);
            assert_error_word(label, &raised, word);
        }
    }

    /// **The advice for a document merged with a thing is true whichever side was
    /// folded.** The error carries the document and the other side, not the
    /// direction, so the advice names each by its handle, names only verbs the
    /// surface serves, and never calls the thing a document.
    #[test]
    fn the_advice_for_a_document_merged_with_a_thing_is_true_in_both_directions() {
        let answered = json_of(
            &memory_declined(
                "merge_entities",
                MemoryError::DocumentMergedWithAThing {
                    document: "document:alpha".into(),
                    other: "thing:handcart".into(),
                },
            )
            .expect("a blocked answer"),
        );
        let how = answered["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("document:alpha") && how.contains("thing:handcart"),
            "{how}"
        );
        let served: Vec<String> = crate::Jojobot::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        for verb in ["read_document", "put_document", "archive_entity", "capture"] {
            assert!(how.contains(verb), "the advice does not name {verb}: {how}");
            assert!(
                served.iter().any(|name| name == verb),
                "{verb} is not served"
            );
        }
        assert!(!how.contains("the other document"), "{how}");
    }

    /// **A number a write would free is answered in the words of the verb that
    /// asked.** An edit is sent back to the number the member holds. A merge has
    /// no number to send, so it is told the two ways that work, each by a verb the
    /// surface serves. The two answers read the same facts, so they name the same
    /// collection, holder and number.
    #[test]
    fn a_number_a_write_would_free_is_answered_in_the_words_of_the_verb() {
        let freed = || MemoryError::NumberFreed {
            collection: "topic:widgets".into(),
            key: "number".into(),
            number: 2,
            holder: "thing:handcart".into(),
        };
        let advice = |verb: &'static str| {
            json_of(&memory_declined(verb, freed()).expect("a blocked answer"))["how_to_proceed"]
                .as_str()
                .expect("advice")
                .to_string()
        };
        let (merge, edit, moved) = (
            advice("merge_entities"),
            advice("capture"),
            advice("rename_entity"),
        );
        for how in [&merge, &edit, &moved] {
            assert!(
                how.contains("topic:widgets/2") && how.contains("thing:handcart"),
                "{how}"
            );
        }
        let served: Vec<String> = crate::Jojobot::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        for verb in ["retract", "archive_entity"] {
            assert!(
                merge.contains(verb),
                "the merge advice does not name {verb}"
            );
            assert!(
                served.iter().any(|name| name == verb),
                "{verb} is not served"
            );
        }
        // A move leaves the member where it is or archives it; it is not told to
        // retract, and it names the `parent` to leave off.
        assert!(
            moved.contains("archive_entity") && moved.contains("`parent`"),
            "the move advice does not name archive_entity and the parent: {moved}"
        );
        assert!(
            !moved.contains("retract"),
            "a move is not told to retract: {moved}"
        );
        assert!(
            !edit.contains("archive_entity"),
            "an edit is not told to archive: {edit}"
        );
        assert_eq!(advice("update_fact"), edit, "the edit verbs share one text");
    }

    /// **A thought too long is moved, never compressed.** Both refusals that
    /// name a thought over its cap say how to relocate it: the substance goes
    /// onto the thing the thought points at as a new claim, the original is
    /// archived, and a short pointer is captured in its place. Rewriting the
    /// same words shorter in place is the one repair they must not offer.
    #[test]
    fn a_thought_over_its_cap_is_told_to_relocate_and_archive() {
        let refusals = [
            MemoryError::ThoughtTooLong {
                subject: "bot:gamma".into(),
                len: 900,
                cap: 500,
            },
            MemoryError::MergeThoughtTooLong {
                subject: "bot:gamma".into(),
                thought: "bot:gamma#f1".into(),
                len: 900,
                cap: 500,
            },
        ];
        let mut closings = Vec::new();
        for error in refusals {
            let answered = json_of(&memory_declined("capture", error).expect("a blocked answer"));
            let how = answered["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("archived") && how.contains("capture"),
                "the advice has to name archiving the thought and capturing its replacement: {how}"
            );
            assert!(
                !how.contains("Shorten") && !how.contains("shortened"),
                "the advice must not offer compressing the thought in place: {how}"
            );
            closings.push(
                how.rsplit(". ")
                    .next()
                    .expect("a last sentence")
                    .to_string(),
            );
        }
        // Both refusals close on the one sentence that says the cap is on purpose, so a
        // caller meets the same word whichever way it ran into the cap.
        assert!(
            closings.len() == 2 && closings[0] == closings[1] && !closings[0].is_empty(),
            "the two thought refusals share their closing sentence: {closings:?}"
        );
    }

    /// **A limit that exists on purpose says so, and a room that holds nothing
    /// does not.** The three rooms that hold thoughts (an edit, a debt, a full
    /// room) say their size is deliberate and keep the moves they had. The room
    /// of capacity 0 says the ceiling has to move first, so it does not claim
    /// its size is the point. The pair is what makes the word mean something.
    #[test]
    fn a_full_room_says_its_size_is_deliberate_and_an_empty_one_does_not() {
        let room = |live, capacity| MemoryError::RoomFull {
            subject: "bot:gamma".into(),
            live,
            capacity,
            room: Vec::new(),
            aged_out: 0,
        };
        let advice = |verb, error| {
            json_of(&memory_declined(verb, error).expect("a blocked answer"))["how_to_proceed"]
                .as_str()
                .expect("advice")
                .to_string()
        };
        for (what, how) in [
            ("a full room", advice("capture", room(3, 3))),
            ("a room in debt", advice("capture", room(4, 3))),
            (
                "an edit into a full room",
                advice("update_fact", room(3, 3)),
            ),
        ] {
            assert!(
                how.contains("deliberate"),
                "{what} says its size is on purpose: {how}"
            );
        }
        let full = advice("capture", room(3, 3));
        assert!(
            full.contains("drop") && full.contains("borrow") && full.contains("wait"),
            "the moves a full room had stay: {full}"
        );
        let none = advice("capture", room(0, 0));
        assert!(
            !none.contains("deliberate"),
            "a room that holds nothing is not told its size is the point: {none}"
        );
    }

    /// **A record ceiling keeps its four routes and says raising it is not one of
    /// them.** The sentence about raising is for a thing whose job has grown.
    #[test]
    fn a_record_ceiling_names_raising_it_as_for_a_grown_job_and_not_a_route() {
        let how = json_of(
            &memory_declined(
                "capture",
                MemoryError::OverCapacity {
                    subject: "bot:gamma".into(),
                    live: 3,
                    limit: 3,
                    adding: 1,
                    from_setting: false,
                    records: Vec::new(),
                },
            )
            .expect("a blocked answer"),
        )["how_to_proceed"]
            .as_str()
            .expect("advice")
            .to_string();
        for route in ["REWRITE", "ARCHIVE", "SPLIT", "DOCUMENT"] {
            assert!(how.contains(route), "the four routes stay: {how}");
        }
        assert!(
            how.contains("job has grown") && how.contains("never a way around"),
            "raising the ceiling is for a grown job and never a way around the routes: {how}"
        );
    }

    /// **A write to a thought ceiling is sent to the route the refusal itself
    /// names, and a write to another guarded key is not.** The three thought
    /// ceilings carry the sentence; `rule_seats` has the same guard and does not.
    /// **It holds on a first attempt**: no earlier refusal is claimed, because
    /// there may have been none.
    #[test]
    fn a_refused_thought_ceiling_points_at_its_own_route_and_other_keys_do_not() {
        use jojobot_domain::memory::{
            MayWrite, THOUGHT_AGES_AFTER_RUNS, THOUGHT_BODY_CAP, THOUGHT_CAPACITY,
        };
        let advice = |key: &str, may| {
            json_of(
                &memory_declined(
                    "capture",
                    MemoryError::KeyNotYours {
                        subject: "bot:gamma".into(),
                        key: key.into(),
                        may,
                        allowed: vec!["bot:delta".into()],
                    },
                )
                .expect("a blocked answer"),
            )["how_to_proceed"]
                .as_str()
                .expect("advice")
                .to_string()
        };
        // The key's own name is the only thing a refusal spells from the key, so the
        // advice is compared with it taken out.
        let keyless = |key: &str, may| advice(key, may).replace(key, "<key>");
        for key in [THOUGHT_CAPACITY, THOUGHT_BODY_CAP, THOUGHT_AGES_AFTER_RUNS] {
            for may in [MayWrite::DifferentIdentity, MayWrite::WritersSuperior] {
                let base = keyless("rule_seats", may);
                let how = keyless(key, may);
                let tail = how.strip_prefix(base.as_str()).unwrap_or_else(|| {
                    panic!("{key}: the advice is another key's advice and a sentence more: {how}")
                });
                assert!(
                    tail.contains("bot:gamma"),
                    "{key}: the added sentence names the subject: {tail:?}"
                );
            }
        }
        for may in [MayWrite::DifferentIdentity, MayWrite::WritersSuperior] {
            assert_eq!(
                keyless("rule_seats", may),
                keyless("bot_seats", may),
                "another guarded key keeps the words it had"
            );
        }
    }

    /// **The pair beside the table: an answer that is not a refusal carries no
    /// word.** A valid capture lands, and the word is a property of a refusal,
    /// so it is absent from the receipt.
    #[tokio::test]
    async fn an_answer_that_is_not_a_refusal_carries_no_word() {
        let jojobot = handler();
        ensure(&jojobot, "person:alpha").await;
        let landed = json_of(
            &jojobot
                .capture(Parameters(capture_args("person:alpha", "likes the diner")))
                .await
                .expect("capture ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert!(landed.get("fix_by").is_none(), "{landed}");
    }
    /// **The resemblance and existence gates, which no error enum carries**,
    /// each wear `change`: the answer is a different call, or the same call with
    /// the token the refusal hands back. One row per gate and per case of it.
    #[test]
    fn every_gate_that_blocks_a_write_wears_the_change_word() {
        use jojobot_domain::memory::guard::{EntityMatch, MatchReason};
        let slot = || TokenSlot::from(&add_args("person", "alpha", "Alpha"));
        let attempted = EntityId("person:alpha".into());
        let near = |reason| {
            vec![EntityMatch {
                handle: EntityId("person:alphaa".into()),
                kind: EntityKind::PERSON,
                name: "Alphaa".into(),
                source: "user-named".into(),
                reason,
                told_apart_by: None,
            }]
        };
        let rows: Vec<(&str, CallToolResult)> = vec![
            (
                "creating a near miss",
                blocked_result(
                    &attempted,
                    &near(MatchReason::Contains),
                    Blocked::Creating(slot()),
                ),
            ),
            (
                "creating an exact handle",
                blocked_result(
                    &attempted,
                    &near(MatchReason::ExactHandle),
                    Blocked::Creating(slot()),
                ),
            ),
            (
                "relabelling",
                blocked_result(
                    &attempted,
                    &near(MatchReason::Contains),
                    Blocked::Relabelling(slot()),
                ),
            ),
            (
                "renaming a near miss",
                blocked_result(
                    &attempted,
                    &near(MatchReason::Contains),
                    Blocked::Renaming(slot()),
                ),
            ),
            (
                "renaming an exact handle",
                blocked_result(
                    &attempted,
                    &near(MatchReason::ExactHandle),
                    Blocked::Renaming(slot()),
                ),
            ),
            (
                "naming an unknown entity",
                blocked_result(&attempted, &[], Blocked::MustExist("capture")),
            ),
            (
                "naming a near miss",
                blocked_result(
                    &attempted,
                    &near(MatchReason::Contains),
                    Blocked::MustExist("capture"),
                ),
            ),
        ];
        for (label, answered) in rows {
            assert_fix_by(label, &answered, "change");
        }
    }

    /// **A refusal for a record the build supplies points at the write that
    /// does work**, for every verb that gives it: the rename, the archive and
    /// either side of a fold.
    ///
    /// Each is driven through the shipped stack, where `view:loops` is a
    /// supplied record. A record the build supplies is extended in place, so the
    /// refusal must not send the caller to a record of their own. The write it
    /// points at is then made on the same record, and answered: a way forward
    /// the engine refused would be worse than none.
    #[tokio::test]
    async fn a_supplied_records_refusal_points_at_the_write_that_extends_it() {
        use crate::memory::archive_entity::ArchiveEntityArgs;
        use crate::memory::merge_entities::MergeArgs;
        use crate::memory::rename_entity::RenameEntityArgs;

        let jojobot = handler_shipped();
        let sid = writing_as(&jojobot);

        let renamed = jojobot
            .rename_entity(Parameters(RenameEntityArgs {
                request_id: None,
                handle: "view:loops".into(),
                to: "view:my-bikes".into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("a supplied record's rename is an answer");
        let archived = jojobot
            .archive_entity(Parameters(ArchiveEntityArgs {
                request_id: None,
                handle: "view:loops".into(),
                reason: "a test".into(),
                restore: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("a supplied record's archive is an answer");
        let folded_away = jojobot
            .merge_entities(Parameters(MergeArgs {
                request_id: None,
                duplicate: "view:loops".into(),
                survivor: "view:colleagues".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("a supplied record folded away is an answer");
        let survivor = jojobot
            .merge_entities(Parameters(MergeArgs {
                request_id: None,
                duplicate: "view:colleagues".into(),
                survivor: "view:loops".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("a supplied record kept as the survivor is an answer");

        for (label, answered) in [
            ("rename", &renamed),
            ("archive", &archived),
            ("fold away", &folded_away),
            ("survivor", &survivor),
        ] {
            let body = blocked(answered);
            let way = body["how_to_proceed"].as_str().unwrap_or_default();
            assert!(
                way.contains("capture"),
                "{label}: the refusal names the write that extends the record: {body}",
            );
            assert!(
                !way.contains("add_entity"),
                "{label}: the refusal sends the caller to a record of their own: {body}",
            );
        }

        // The write it names, made on the same supplied record: it lands and
        // reads back on top of what the build ships.
        let wrote = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(sid.clone()),
                    sets: Some([("note".to_string(), "kept on top".to_string())].into()),
                    ..capture_args("view:loops", "A caller's own note on the loops view")
                }))
                .await
                .expect("a write to a supplied record answers"),
        );
        assert_ne!(wrote["status"], "blocked", "{wrote}");
        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    sid: Some(sid),
                    ..recall_args("view:loops")
                }))
                .await
                .expect("recall answers"),
        );
        let loops = &read["objects"][0];
        assert_eq!(loops["id"], "view:loops", "{read}");
        assert_eq!(loops["fields"]["note"], "kept on top", "{read}");
        assert!(
            loops["fields"]["selects"].is_string(),
            "what the build ships is still there under the overlay: {read}",
        );
    }
}
