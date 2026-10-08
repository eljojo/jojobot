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
pub(crate) struct TokenSlot(#[allow(dead_code)] Option<String>);

impl TokenSlot {
    pub(crate) fn from(args: &impl AcceptsOverride) -> Self {
        TokenSlot(args.override_token().map(str::to_string))
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
    Creating(#[allow(dead_code)] TokenSlot),
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
        Blocked::Creating(_) => format!(
            "Nothing was written. If '{attempted}' IS one of the entities above, use that handle \
             instead. If it is genuinely a different one that happens to share a name, re-call \
             add_entity with override_token: \"{token}\". That token belongs to THIS refusal and \
             lifts no other. Display names are not unique and never have to be; the handle is \
             what has to be.",
        ),
        // Says "name" rather than "rename": this gate fires on an alias write
        // too, and telling a caller nothing was renamed when they renamed
        // nothing sends them looking for a rename they never made.
        Blocked::Relabelling(_) => format!(
            "Nothing was written, and the handle '{attempted}' is unaffected either way — this \
             only moves the names it answers to. Either pick a name or alias that isn't already \
             worn, or re-call update_entity with override_token: \"{token}\" if this entity really \
             does share a name with one above: names are not unique, handles are.",
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
        | MemoryError::InvalidAddress { .. }
        | MemoryError::InvalidEntity { .. }
        | MemoryError::InvalidEdge { .. }
        | MemoryError::InvalidQuery { .. }
        | MemoryError::InvalidType { .. }
        | MemoryError::ShippedType { .. }
        | MemoryError::RepeatsShipped
        | MemoryError::BreaksFit { .. }
        | MemoryError::BreaksSchedule { .. }
        | MemoryError::BootTooHeavy { .. }
        | MemoryError::BreaksType { .. }
        | MemoryError::UnknownFact { .. }
        | MemoryError::NotYours { .. }
        | MemoryError::RoomFull { .. }
        | MemoryError::KeyNotYours { .. }
        | MemoryError::ChartCycle { .. }
        | MemoryError::MergeCarriesGuardedKeys { .. }
        | MemoryError::ThoughtTooLong { .. }
        | MemoryError::MergeOverfillsRoom { .. }
        | MemoryError::MergeThoughtTooLong { .. }
        | MemoryError::RoleFieldGuarded { .. }
        | MemoryError::RoleNotHeld { .. }
        | MemoryError::UnknownEntity { .. }
        | MemoryError::NothingToMerge { .. }
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
        | MemoryError::TestimonyRewritten { .. } => Some(FixBy::Change),
        // A store that answered and refused: a constraint the caller's input
        // broke, which sending the same call again meets again. Not an outage.
        MemoryError::Refused(_) => Some(FixBy::Change),
        // A kind set nothing re-reads, a build that collided with a stored row,
        // and a claim only the operator can bless.
        MemoryError::KindsNeverLoaded { .. }
        | MemoryError::SuppliedRecordCollidesWithStoredRow { .. }
        | MemoryError::UnconfirmedPromotion
        | MemoryError::UnconfirmedSettling => Some(FixBy::Person),
        // A held role frees when its lease ends; a collision clears on the next try.
        MemoryError::RoleTaken { .. } | MemoryError::Conflict => Some(FixBy::Retry),
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
            format!(
                "Nothing was written: {e}. The claim itself is fine — send it again with \
                 read_from naming the system, or with provenance inference."
            ),
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
        MemoryError::TestimonyRewritten { ref address } => Ok(blocked_body(
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
                 again will not change the answer. A session's chronology is its own bot's, \
                 and yours is what your own handle reaches."
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
        // **The way forward is one more key in the SAME call, or both keys
        // gone.** What is missing is the other half of the schedule, and a
        // caller who sends it now makes the loop whole in one write instead of
        // two. A loop with neither key is allowed and needs neither.
        MemoryError::BreaksSchedule {
            ref missing,
            ref accepts,
            ..
        } => Ok(blocked_body(
            &EntityId(String::new()),
            &[],
            if accepts.is_empty() {
                format!(
                    "Nothing was written: {e}. Send '{missing}' in the same call, as a number of \
                     days. To switch the schedule off, clear both keys in one call. A loop with \
                     no schedule at all is allowed and needs neither."
                )
            } else {
                format!(
                    "Nothing was written: {e}. Send '{missing}' in the same call, as {} — \
                     'due_date' keeps the loop on its own days, 'check_in_date' counts from the \
                     day each check-in happens. A loop with no 'cadence_days' at all is allowed \
                     and needs neither.",
                    accepts
                        .iter()
                        .map(|token| format!("'{token}'"))
                        .collect::<Vec<_>>()
                        .join(" or "),
                )
            },
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
                 what it declared.{}",
                // **The allowed list is a project's columns**, so a caller who
                // wants a word outside it is told the list can be extended.
                match (name.as_str(), key.as_str()) {
                    ("work" | "project", "status") => {
                        " The allowed words are the project's columns: write columns on the \
                         project to add a word of your own."
                    }
                    _ => "",
                }
            ),
        )),
        // **A different refusal, so a different way forward.** These two are
        // not malformed calls: the arguments are well-formed and jojobot is
        // declining to bless a claim the operator has not blessed. Telling a
        // caller to fix the call would invite them to set the confirmation
        // flag themselves, which is the one thing the gate exists to stop.
        MemoryError::UnconfirmedPromotion | MemoryError::UnconfirmedSettling => Ok(blocked_body(
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
                 {verb} could move."
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
                // would send the caller round a loop (rule 68). Archiving
                // a live thought and writing the thought through capture
                // are the two moves it has.
                //
                // **How many, because one is not always enough.** The edit
                // adds a thought, so the room has to end below its
                // capacity first. A room an earlier borrow left over its
                // capacity needs more than one archived, and the count is
                // the same arithmetic in every case.
                let archive = match live + 1 - capacity {
                    1 => "one of the thoughts above".to_string(),
                    n => format!("{n} of the thoughts above"),
                };
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity}, and {verb} cannot make room in it. Archive {archive} with \
                         update_fact (status: archived, and details saying why it no longer \
                         earns its slot), then re-call {verb} — or write this thought through \
                         capture, which can archive one thought as it writes this one."
                )
            } else if live > capacity {
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity} — its emergency reserve is already spent. Re-call {verb} \
                         naming drop (one of the addresses above) and drop_because (why it no \
                         longer earns its slot), or archive a live thought with update_fact to \
                         bring it back at or under {capacity} first. Borrowing again is not on \
                         offer while this debt stands."
                )
            } else {
                format!(
                    "Nothing was written: '{subject}'s room already holds {live} of \
                         {capacity}. Re-call {verb} naming drop (one of the addresses above) and \
                         drop_because (why it no longer earns its slot) — or, once and only \
                         once, borrow: true to let this one land over the ceiling anyway — or \
                         wait, and let this one go unwritten for now."
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
                "archive_needed": (verb == "update_fact" && !empty_room_without_capacity)
                    .then(|| live + 1 - capacity),
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
                "Nothing was merged: {e}. Shorten {thought} with update_fact to a pointer at or \
                 under {cap} characters (it is {len}), with the substance moved onto the thing it \
                 points at, then re-call {verb}."
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
                "Nothing was written: {e}. Re-call {verb} with the substance moved onto the \
                 thing this thought points at, and the thought itself shortened to a pointer at \
                 or under {cap} characters (this one was {len})."
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
            match may {
                jojobot_domain::memory::MayWrite::DifferentIdentity => format!(
                    "Nothing was written: {e}. A different identity has to set '{key}' on \
                     '{subject}' — ask another bot, or the operator, to raise or lower it \
                     instead."
                ),
                jojobot_domain::memory::MayWrite::Subject => {
                    format!("Nothing was written: {e}. '{subject}' has to write '{key}' itself.")
                }
                // **Set already, and no bot of this caller's standing changes
                // it.** The way forward is the head of the chart, or making one.
                jojobot_domain::memory::MayWrite::HeadOnceHeld { .. } => match allowed.is_empty() {
                    true => format!(
                        "Nothing was written: {e}. A bot has to be placed at the top of the chart \
                         first, a bot with reports and nothing above it; then that bot can write \
                         '{key}' on '{subject}'."
                    ),
                    false => format!(
                        "Nothing was written: {e}. Ask {} to write '{key}' on '{subject}'.",
                        allowed.join(" or ")
                    ),
                },
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
                    true => format!(
                        "Nothing was written: {e}. No bot is recorded above '{subject}', so \
                         '{key}' on it has to be set through whoever is at the top of the chart."
                    ),
                    // **A creation that would head a chart.** Existing records
                    // already name the new bot as their manager, so it heads a
                    // chart, and the head is the only one who may place it. It
                    // does not exist yet to do so: make it bare, then place it.
                    false if verb == "add_entity" && allowed.as_slice() == [subject.clone()] => {
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
        // caller tried to set a key: it sent no fields. Both ways forward
        // exist: a different identity performs the merge, or the key comes off
        // the duplicate first.
        MemoryError::MergeCarriesGuardedKeys {
            ref duplicate,
            ref survivor,
            ref keys,
            may: jojobot_domain::memory::MayWrite::HeadOnceHeld { .. },
            ..
        } => Ok(blocked_body(
            &EntityId(duplicate.clone()),
            &[],
            format!(
                "Nothing was written. '{duplicate}' carries {keys}, and '{survivor}' already \
                 holds it, so merging them would change it, which only the bot that heads the \
                 chart may do. Ask that bot to make this merge, or take {keys} off \
                 '{duplicate}' first: update_fact the record that sets it with clear_fields, \
                 then merge again."
            ),
        )),
        MemoryError::MergeCarriesGuardedKeys {
            ref duplicate,
            ref survivor,
            ref keys,
            may,
            ref allowed,
        } => Ok(blocked_body(
            &EntityId(duplicate.clone()),
            &[],
            match may {
                // **The chart, not a ceiling.** The merge would change who the
                // survivor reports to, which only the bots that may place it
                // may do, so they are the ones named. With nobody recorded
                // above, it says to ask whoever is at the top of the chart.
                jojobot_domain::memory::MayWrite::Ancestor
                | jojobot_domain::memory::MayWrite::Superior => {
                    let who = match allowed.is_empty() {
                        true => "whoever is at the top of the chart".to_string(),
                        false => allowed.join(" or "),
                    };
                    format!(
                        "Nothing was written. '{duplicate}' carries {keys}, and merging it into \
                         '{survivor}' would change who '{survivor}' reports to, which only \
                         {who} may do. Ask {who} to make this merge, or take {keys} off \
                         '{duplicate}' first: update_fact the record that sets it with \
                         clear_fields, then merge again."
                    )
                }
                _ => format!(
                    "Nothing was written. '{duplicate}' carries {keys}, and merging it into \
                     '{survivor}', your own bot, would raise your own ceiling, which only a \
                     different identity may do. Ask a different identity to make this merge, or \
                     take {keys} off '{duplicate}' first: update_fact the record that sets it \
                     with clear_fields, then merge again."
                ),
            },
        )),
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
        | MemoryError::BreaksSchedule { .. }
        | MemoryError::BootTooHeavy { .. }
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
        | MemoryError::AlreadyMerged { .. }
        | MemoryError::NothingToRename { .. }
        | MemoryError::HandleMoved { .. }
        | MemoryError::SuppliedHandle { .. }
        | MemoryError::UnconfirmedPromotion
        | MemoryError::UnconfirmedSettling
        | MemoryError::RoomFull { .. }
        | MemoryError::KeyNotYours { .. }
        | MemoryError::ChartCycle { .. }
        | MemoryError::MergeCarriesGuardedKeys { .. }
        | MemoryError::ThoughtTooLong { .. }
        | MemoryError::MergeOverfillsRoom { .. }
        | MemoryError::MergeThoughtTooLong { .. }
        | MemoryError::RoleFieldGuarded { .. }
        | MemoryError::RoleTaken { .. }
        | MemoryError::TestimonyRewritten { .. }
        | MemoryError::KeyInOtherBag { .. }
        | MemoryError::UnstatedProvenance => McpError::invalid_params(e.to_string(), None),
        MemoryError::Store(msg) => {
            McpError::internal_error(crate::boundary::store_failed("this call", &msg), None)
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
        // **A build-time misconfiguration, not a caller mistake.** No verb
        // produces this — it is caught at boot, before an instance ever
        // serves — so there is no way forward to hand a caller and no
        // `memory_declined` arm for it either.
        MemoryError::SuppliedRecordCollidesWithStoredRow { .. } => {
            McpError::internal_error(e.to_string(), None)
        }
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

#[cfg(test)]
mod tests {
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
            // Nobody recorded above: said, and not an empty list of people.
            let none = refused(may, &[]);
            assert!(none.contains("top of the chart"), "{none}");
            assert!(!none.contains("operator"), "{none}");
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
        let ceiling = refused(MayWrite::DifferentIdentity, &[]);
        assert!(ceiling.contains("different identity"), "{ceiling}");
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
    fn memory_refusal_rows() -> Vec<(&'static str, MemoryError, Option<&'static str>)> {
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
                "TestimonyRewritten",
                MemoryError::TestimonyRewritten {
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
                    name: s("trip"),
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
                    held: s("cadence_days"),
                    missing: s("due_date"),
                    accepts: Vec::new(),
                },
                Some("change"),
            ),
            (
                "BreaksType",
                MemoryError::BreaksType {
                    name: s("trip"),
                    key: s("starts"),
                    wanted: s("date"),
                    value: s("soon"),
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
                    keys: s("rule_seats"),
                    may: MayWrite::DifferentIdentity,
                    allowed: Vec::new(),
                },
                Some("change"),
            ),
            (
                "RoleFieldGuarded",
                MemoryError::RoleFieldGuarded {
                    role: s("dev"),
                    key: s("role/dev/holder"),
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
                "NothingToRename",
                MemoryError::NothingToRename {
                    attempted: s("person:alpha"),
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
            // Damage no caller can repair, a kind set nothing re-reads, a build
            // that collided with a stored row, and claims only the operator can
            // bless.
            (
                "KindsNeverLoaded",
                MemoryError::KindsNeverLoaded { attempted: None },
                Some("person"),
            ),
            (
                "SuppliedRecordCollidesWithStoredRow",
                MemoryError::SuppliedRecordCollidesWithStoredRow {
                    attempted: s("view:colleagues"),
                },
                Some("person"),
            ),
            (
                "UnconfirmedPromotion",
                MemoryError::UnconfirmedPromotion,
                Some("person"),
            ),
            (
                "UnconfirmedSettling",
                MemoryError::UnconfirmedSettling,
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
        ];
        rows
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
}
