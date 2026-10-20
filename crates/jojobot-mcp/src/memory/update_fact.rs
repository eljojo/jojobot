//! `update_fact` — Correct an addressed fact in place — the source, never an addendum.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use jojobot_domain::attention;
use jojobot_domain::memory::text_edit::{TextEdit, TextField};

use super::clauses::{happened_at_clause, stale_after_clause};
use super::*;
use crate::teaching::{
    CLAIM_DIRECTION_DOMAIN, CLAIM_DIRECTION_TEACHING, CLAIM_SUBJECT_DOMAIN, CLAIM_SUBJECT_TEACHING,
    CLAIMS_DOMAIN, CLAIMS_TEACHING, FIELD_SHADOWS_ARGUMENT_DOMAIN, RHYTHM_ARCHIVE_DOMAIN,
    RHYTHM_ARCHIVE_TEACHING,
};

/// **What a rewrite of a claim asks of the store, and what the store answered**:
/// the keys the claim must still back, and whether the store refused because the
/// claim moved (`stale`), so the caller can append instead of rewriting.
#[derive(Debug, Default)]
pub(super) struct RewriteGuard {
    pub(super) must_back: std::collections::BTreeSet<String>,
    pub(super) stale: bool,
}

/// Arguments to `update_fact`.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateFactArgs {
    /// The claim's global address, `kind:slug#local-id` — as `recall`
    /// returned it. **A claim cannot change subject:** to move one filed on the
    /// wrong thing, capture it on the right one, re-sending its dates, `fields`, `sets` and
    /// edge, with `derived_from` naming this claim, then archive this one.
    pub(crate) address: String,
    /// Replacement claim.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as the
    /// handle that thing wears today, even after a rename.
    /// Use `@<collection>/<N>` to link the member holding N; a missing number
    /// is refused with the numbers held.
    /// Text inside backticks or a fenced block is literal: a handle there is
    /// kept as written and never becomes a link.
    ///
    /// An edit that would leave the claim too big to be read back whole in one
    /// answer is refused with the claim unchanged: shorten it, split it into claims
    /// that each say one thing, or keep the long text on a document (`put_document`)
    /// and write a pointer claim.
    #[serde(default)]
    pub(crate) content: Option<String>,
    /// Replacement details; pass an empty string to clear them. They may hold
    /// paragraph breaks and read back with every one. Carries `@kind:slug`
    /// mentions exactly as `content` does. **With `status: archived` they are the
    /// reason**: the record keeps it as `archive_reason` and its own details stay.
    #[serde(default)]
    pub(crate) details: Option<String>,
    /// **The day this claim was MADE**, `YYYY-MM-DD` — the day it was said,
    /// decided or worked out. Left alone when omitted — it keeps the
    /// day of the claim it replaces, exactly as any field this patch does
    /// not name.
    ///
    /// **Give this when the correction itself was made on a different day
    /// than the one already on the claim — never the day you happen to be
    /// typing.** Rewriting content with no date given leaves the ORIGINAL day
    /// on the claim, permanently: a correction made months later would
    /// otherwise read back as if it had been made on the original day
    /// forever.
    ///
    /// **Naming a day that differs from your run's stated day is never
    /// refused** — the receipt carries `recorded_at_note`, one sentence
    /// naming both days, so a sitting acting out a different period does not
    /// silently record under the wrong one by mistake.
    #[serde(default)]
    pub recorded_at: Option<String>,
    // Described by attribute, not a doc comment: the defining clause is shared
    // with `capture` (see `memory::clauses`) and a doc comment cannot take it.
    #[schemars(description = concat!(
        happened_at_clause!(),
        "\n\n",
        "**Learning it late is ordinary**: a claim written when nobody knew the\n",
        "day gains one here. Leaving it off keeps whatever the claim says, like\n",
        "every other field this patch does not name."
    ))]
    #[serde(default)]
    pub happened_at: Option<String>,
    /// **Take the happened-on day off**, leaving a claim that says nothing
    /// about when the thing happened.
    ///
    /// Its own flag, because an absent `happened_at` means the patch does not
    /// mention it. **This is the repair for a day somebody approximated** — the
    /// guess comes off rather than being replaced by another guess.
    #[serde(default)]
    pub clear_happened_at: Option<bool>,
    /// **The far end**, `YYYY-MM-DD`, when the thing happened is a stretch of
    /// days rather than one. Leaving it off keeps whatever the claim says.
    #[serde(default)]
    pub happened_through: Option<String>,
    /// **Take the far end off**, leaving a single-day claim (or an undated
    /// one, alongside `clear_happened_at`) rather than a span.
    #[serde(default)]
    pub clear_happened_through: Option<bool>,
    /// `active` or `archived`. ARCHIVE, RETRACT OR REWRITE — one rule, stated the
    /// same in update_fact, its status argument and retract. A claim that was
    /// true and has changed, or that was never true: archive it with update_fact
    /// (status: archived, the reason in details) and capture the replacement,
    /// naming the old claim in derived_from. A claim about a past EVENT that turned
    /// out never to have happened: retract it, which also leaves a dated claim of
    /// the retraction. A mistyped or reworded claim: rewrite it. Archiving and
    /// retracting are one-way for a claim: nothing edits an archived claim,
    /// status included, so there is no un-archive (an archived ENTITY comes back
    /// with restore: true). Rewriting `content` into its own denial ("the club
    /// does NOT meet on Tuesdays" replacing "the club meets on Tuesdays") leaves a
    /// sentence about what is not so where a claim about what is true belongs. A
    /// negative stays an ordinary claim when it is not correcting anything: "the guest
    /// did not attend" stands on its own, written with `capture` like any other
    /// claim. The receipt of an archive names the keys the thing lost, under
    /// `left_the_thing`.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `testimony`, `observation` or `inference`. Moving a claim TO `testimony`
    /// needs `confirmed_by_user` whichever value it held: a claim you read
    /// somewhere is not a step towards the operator having said it. An
    /// `observation` must carry `read_from`, here as at capture — and **taking
    /// the source off a claim that stays an observation is refused for the same
    /// reason**, because both leave a machine read of a system nobody named.
    /// A claim that already carries one does not have to name it again.
    ///
    /// A session that wrote a claim keeps its provenance when it rewrites the
    /// words and sends none, and any session keeps an inference's. Another
    /// session rewriting an observation's words must restate its provenance,
    /// even when unchanged. An edit that leaves the words alone keeps it.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// `settled` or `open`. **Moving a claim that is ALREADY open to settled
    /// requires a `receipt`**, and is refused without one. Reopening is free,
    /// and takes the receipt off the claim; the claim's history keeps it.
    ///
    /// That is the whole of the gate: it is on this promotion, not on
    /// declaring a standing. A fresh `capture` may state `settled` and is
    /// taken at its word, the way it is taken at its word about provenance.
    ///
    /// Settling never changes provenance: a settled inference reads back as an
    /// inference, with its receipt. When the operator hedged a claim and no
    /// longer does, settle it with the `confirmed` receipt and leave provenance
    /// alone; the claim was theirs from the start.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// Required to promote an EXISTING claim to testimony (inference or
    /// observation alike). Set it only when the operator has actually
    /// confirmed the claim. Nothing else is gated on it — a fresh `capture`
    /// declares its provenance and its standing on honour. Settling an open
    /// claim takes a `receipt` instead, and this flag does not settle it.
    ///
    /// The flag is trusted, not checked: nothing verifies that the operator said
    /// yes.
    #[serde(default)]
    pub(crate) confirmed_by_user: Option<bool>,
    /// **Why this claim is settled**, required to move an open claim to
    /// `settled`. Exactly one of two shapes: `check`, what was checked, written
    /// so another agent can run it again; or `confirmed`, the operator said so,
    /// as `{on: YYYY-MM-DD, at: a message id or the session}`. It records that a
    /// check ran or that the operator confirmed; it never runs the check, and it
    /// never changes who backs the claim.
    #[serde(default)]
    pub(crate) receipt: Option<super::receipt::ReceiptArg>,
    /// The shape of an edge to attach: `location` · `membership` · `attendance` ·
    /// `about` · `connection` (a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone. Every read and write
    /// receipt serves the same shape word that a write takes.
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist** —
    /// `add_entity` first if it is genuinely new.
    #[serde(default)]
    pub(crate) object: Option<String>,
    /// **Fields to set**, as key/value pairs. Each key named is written; a key
    /// the claim already carries and this does not name is left alone, so an
    /// edit reaches one field without restating the rest.
    ///
    /// A field you set that equals what jojobot ships as its default today is
    /// stored exactly as you sent it either way, and the receipt's echoes_defaults
    /// names which key that was.
    #[serde(default)]
    pub(crate) fields: Option<std::collections::BTreeMap<String, String>>,
    /// **Fields to set on the thing this claim is about**, as key/value pairs,
    /// apart from `fields`, which are the claim's own. **Naming under `sets` a
    /// key the claim holds as its own field MOVES it to the settings in this
    /// call**, with the value you send, and the receipt says it moved. The other
    /// way is two calls: a setting named under `fields` is refused, so take it
    /// off with `clear_fields`, then write it under `fields`. A key that only
    /// labels the claim is never a
    /// property of the thing and is refused here by name. The keys that make a
    /// thing fall due act on it only when sent here. A list key (`depends_on`)
    /// changes by its items with `sets_add` and `sets_remove`: its whole value
    /// sent here replaces what another session added. Setting `status` to `now`
    /// on work your bot owns also points your bot's `working_on` at it.
    /// A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone. A note the receipt would have carried and a failed read kept from it is named the same way, under `reads_left`.
    #[serde(default)]
    pub(crate) sets: Option<std::collections::BTreeMap<String, String>>,
    /// **Items to add to a list key on the thing this claim is about**, as
    /// key/value pairs where the value is the items, separated by commas:
    /// `{depends_on: "work:gamma"}`. Accepted only for a key declared a list. The
    /// thing's current value is read inside this call, whichever claim wrote it,
    /// and the items are appended to the end, in the order sent; an item it
    /// already holds is not added twice and every other item stays where it was.
    /// A handle that names nothing is refused with the nearest handles. The answer
    /// is stored as an ordinary whole value. Naming the same key in `sets` is
    /// refused, and so is sending this beside `after`.
    #[serde(default)]
    pub(crate) sets_add: Option<std::collections::BTreeMap<String, String>>,
    /// **Items to take off a list key on the thing this claim is about**, in the
    /// form `sets_add` takes. The rest of the list stays where it was. An item the
    /// list does not hold is no error, and neither is a handle that no longer names
    /// anything, so a reference to a thing that is gone can come off. With
    /// `sets_add` in one call, the removals are applied first.
    #[serde(default)]
    pub(crate) sets_remove: Option<std::collections::BTreeMap<String, String>>,
    /// **Fields to remove**, by key. Its own argument rather than an empty
    /// value in `fields`: an empty value is a value somebody wrote, and
    /// setting a key to nothing and taking the key off the claim are two
    /// different edits.
    ///
    /// **It takes the key off the thing as well as off the claim**, whichever
    /// bag holds it, so this is the way to take a key off a thing. Send it to
    /// the active claim that carries the key: `recall` the thing with
    /// `backing` true and the key names that claim. A counter key names none,
    /// because its total has no single claim behind it: `recall` with
    /// `history` set to the key lists the claims that wrote it, oldest first,
    /// and `history_most` widens the list when its count says more exist.
    /// Clearing the key on one of them takes the whole total off the thing.
    ///
    /// To move a value to another key, send the new key in `sets` (or in `fields`
    /// for a key of the claim itself) and `clear_fields` with the old one in the
    /// same call.
    #[serde(default)]
    pub(crate) clear_fields: Option<Vec<String>>,
    // Described by attribute for the reason `happened_at` is.
    #[schemars(description = stale_after_clause!())]
    #[serde(default)]
    pub(crate) stale_after: Option<String>,
    /// **Take the day off**, leaving a claim that makes no promise about how
    /// long it stays good. Its own flag, because leaving it alone and removing
    /// it are two different edits.
    #[serde(default)]
    pub(crate) clear_stale_after: Option<bool>,
    /// **The claim this one was worked out from**, as its address
    /// `kind:slug#local-id`.
    ///
    /// **Lineage is learned late**, so it is set here as well as at capture: a
    /// claim is often written before anybody notices what it rests on. The
    /// named claim must exist.
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    /// **Take the lineage pointer off.** Its own flag, because leaving it alone
    /// and removing it are two different edits.
    #[serde(default)]
    pub(crate) clear_derived_from: Option<bool>,
    /// **Mark this claim as standing for the claims named here**, each as
    /// its address `kind:slug#local-id`. A synthesis, not a citation: the
    /// named claims stay exactly as they are — active, readable, untouched —
    /// and `recall` returns them with `stood_for: true`. A write
    /// replaces the whole set. Every named address must already exist;
    /// naming this claim's own address, or the same address twice, is
    /// refused — and so is an empty set (clear the mark instead) or a claim
    /// that already stands for others, because a shape cannot name a shape.
    #[serde(default)]
    pub(crate) stands_for: Option<Vec<String>>,
    /// **Take the mark off**, leaving an ordinary claim that stands for
    /// nothing. Its own flag, because leaving it alone and removing it are
    /// two different edits.
    #[serde(default)]
    pub(crate) clear_stands_for: Option<bool>,
    /// **Take the edge off**, leaving a claim that points at nothing. Its own
    /// flag for the reason the others are: an edit that names neither `shape`
    /// nor `object` says nothing about edges and leaves the one already there
    /// alone.
    ///
    /// **Reach for this when the rewrite turns a claim about what is true
    /// NOW into its current negative** — was a member and is not any more,
    /// was living somewhere and moved away. Then the edge belongs to the
    /// sentence you just erased, and leaving it standing has every walk go on
    /// answering through a claim that now denies it. **A rewrite that stays
    /// positive — who returned it, where it moved to — is not this case: the
    /// edge still describes something true.** Clearing it anyway costs a
    /// real path: the walk through it stops answering, and the entity on the
    /// other end becomes unreachable from this claim. **A PAST EVENT THAT
    /// TURNED OUT NEVER TO HAVE HAPPENED IS RETRACT'S CASE, NOT THIS ONE**:
    /// retracting marks the claim rather than rewriting it, and there is no
    /// un-retract.
    #[serde(default)]
    pub(crate) clear_edge: Option<bool>,
    /// **Keep this claim exactly as it stands — the designed way to bump when
    /// it was last touched without changing anything else.** Worth reaching
    /// for on a thought (`capture`'s `connection` on your own bot handle):
    /// one nobody touches for long enough goes quiet on its own, and this is
    /// how you say, on purpose, that one still matters. A quiet thought is
    /// outside the room's count, so keeping it into a full room is refused
    /// like a new thought, until a live thought is archived. The one call
    /// this verb refuses when nothing else is named: naming no change at all
    /// is refused unless this is `true`, and this is refused if anything
    /// else here would actually change the claim. There is no partial keep —
    /// it is this alone, or an ordinary edit naming what changes.
    #[serde(default)]
    pub(crate) keep: Option<bool>,
    /// **Words to find in the claim's text, to change just that part.** Send it
    /// with `replace` and `field`, and nothing else of the text has to be sent
    /// back. The match is exact, case-sensitive and byte for byte; no pattern.
    /// The words have to occur exactly once in the text as it stands now:
    /// none, or more than one, is refused naming the count, and nothing is
    /// written. Not combined with a whole `content` (or `details`) for the
    /// same field; the other field may be sent. Works with `expected_revision`
    /// and without it.
    #[serde(default)]
    pub(crate) find: Option<String>,
    /// **The words that take the place of `find`.** An empty string deletes
    /// them. Sent with `find`, never alone.
    #[serde(default)]
    pub(crate) replace: Option<String>,
    /// **Which text `find` searches: `content` or `details`.** Required with
    /// `find`, and no default, so the edit never lands on the wrong text by
    /// being left out. The receipt's `text_edit` says what was taken out, what
    /// was put in, and the bytes before and after.
    #[serde(default)]
    pub(crate) field: Option<String>,
    /// **Move an entry of a list: `first`, `last`, or the address of the open
    /// entry it should follow**, as `recall` serves it under `entries`. The entry
    /// reads where you put it until it is next ticked; then the list goes on as
    /// its `tick` setting says (a rotation turns, a checklist sinks the done).
    /// Only an open entry moves, and only behind an open one: a done entry sits
    /// where it was done, and a dropped one is out of view. Refused, with nothing
    /// written, and the refusal says what to send instead. The server mints the place
    /// (`order`, `held_turn`, `held_since`); a caller never sends them.
    ///
    /// **Move a section: the address of the section it should follow**, `kind:slug#local-id`
    /// of a live section of the same document. The section goes right behind it,
    /// with every chunk that carries it on. The blank lines in front of a section
    /// that now follows a different section are written (the moved one, the one
    /// after it and the one that closes the gap it left), and no section's
    /// words are.
    /// Refused, with nothing written, when this claim is not a live section of a
    /// document (an ordinary claim has no place to move), when it is a chunk that
    /// carries on the section before it (move that section), or when the target is
    /// not a live section of the same document. A section can be moved and edited
    /// in one call. A move of a long section that stops partway is finished by
    /// sending the same call again. **A move never touches the section's words**:
    /// it changes where the section sits, so it needs no say-so from the session
    /// that wrote them, and an edit of the words sent in the same call is still
    /// held to the rule for them.
    #[serde(default)]
    pub(crate) after: Option<String>,
    /// **The revision you read**, when the edit is only right if nobody has
    /// touched the claim since: the `revision_count` `recall` served for it. A
    /// claim `recall` serves without one has been written once, so it is
    /// revision 1. When the claim has been written since, the edit is refused
    /// with `retry`, naming the revision it is at now, and nothing is written.
    /// Leave it off and the edit lands as it always did. An edit's receipt
    /// carries `revision`, the claim's count after that edit: send it as the
    /// next guarded edit's `expected_revision` without a `recall` between.
    #[serde(default)]
    pub(crate) expected_revision: Option<u32>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    /// Receipt: `success`, `id`, `address`, `derived`, `changed` and warnings; the write is read back; the calling bot's setting_token_saving=on (off by default) selects only id and derived values, deferring teaching and session notices; set it with capture sets on that bot and read claims with recall facts:true.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. If an answer is lost, send the
    /// same call again with the same `request_id`: if
    /// the first call wrote, you get its receipt back marked `repeat` and nothing
    /// is written a second time, so the edit is made once. It needs a `sid`, and a call that
    /// wrote nothing spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    // **Not published**: no refusal of this verb carries a token while a type's
    // rule is off (rule 636), so the arguments would do nothing. The override
    // path below stays for the rule's return, and a call that sends them is
    // refused as one sending an argument the verb does not take.
    #[serde(skip)]
    pub(crate) override_token: Option<String>,
    #[serde(skip)]
    pub(crate) override_reason: Option<String>,
}

impl super::declined::AcceptsOverride for UpdateFactArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
    fn override_reason(&self) -> Option<&str> {
        self.override_reason.as_deref()
    }
}

/// Edit one addressed fact in place — fix the source, never an addendum.
#[tool_router(router = update_fact_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Edit one addressed claim in place — fix the source, never an \
                       addendum. It changes `content`, `details`, `recorded_at` (the day \
                       the claim was MADE, never the day you call), `status`, \
                       `provenance`, `standing`, `fields`, `sets`, `stands_for` and the claim's edge. \
                       ARCHIVE, RETRACT OR REWRITE — one rule, stated the \
                       same in update_fact, its status argument and retract. A claim that was \
                       true and has changed, or that was never true: archive it with \
                       update_fact (status: archived, the reason in details) and capture \
                       the replacement, naming the old claim in derived_from. A claim \
                       about a past EVENT that turned out never to have happened: retract \
                       it, which also leaves a dated claim of the retraction. A mistyped \
                       or reworded claim: rewrite it. Archiving and retracting are \
                       one-way for a claim: nothing edits an archived claim, status \
                       included, so there is no un-archive (an archived ENTITY comes back \
                       with restore: true). A rewrite destroys nothing — every earlier \
                       wording stays readable (recall with history_record: the address) — \
                       so a claim you disagree with is safe to correct, except testimony \
                       from an earlier session: TESTIMONY IS REWRITTEN IN PLACE ONLY BY \
                       THE SESSION THAT WROTE IT, and any other is archived and replaced, \
                       with derived_from naming the archived claim. Two moves are \
                       gated: anything to testimony needs confirmed_by_user; open \
                       to settled needs a `receipt` (`check` or `confirmed`), which \
                       leaves provenance alone. A call naming no change is refused; keep: true alone touches it. \
                       expected_revision (recall's revision_count): a stale edit is refused with retry. find, replace, field (content or \
                       details): change one span, if find occurs once. Naming an own key in `sets` moves it. An unknown address is blocked with candidates. `sets_add` and `sets_remove` change a list key by its items. `after` moves a list entry (`first`, `last`, an address) or a section. The receipt is read back from the store; `changed` lists what moved."
    )]
    pub(crate) async fn update_fact(
        &self,
        Parameters(args): Parameters<UpdateFactArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.update_fact_checking_revision(args, None).await
    }

    /// An internal rewrite may append when its revision guard refuses it. Keep
    /// that typed refusal separate from the answer, so other failures still stop it.
    ///
    /// **A rewrite also names the keys its claim must still back**, because the
    /// revision counts the claim's own writes only, and the store checks them with
    /// the revision. See [`RewriteGuard`].
    pub(super) async fn update_fact_checking_revision(
        &self,
        args: UpdateFactArgs,
        rewrite: Option<&mut RewriteGuard>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **What this write could not work out for want of the instance record**,
        // named on the receipt.
        let unread = super::instance_unread::InstanceUnread::default();
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        // **A move of a section that an earlier call wrote under this id is being
        // finished.** The checks and the plan below judge the document as it stood
        // before the section moved; the plan the first call chose is in the note of
        // its first part, and the rest of the move follows it.
        if let Some(recorded) = jojobot_domain::request::skip("section") {
            return self.finish_move(&args, &caller, recorded).await;
        }
        // **An edit that ended what is owed and whose claim an earlier call wrote
        // is being finished**: the stored due moment comes off, and the checks
        // that judged the thing before the edit are not asked again. **The review
        // mark an archive earns is an edit of its own** inside the archive's call,
        // so it never takes the archive's parts as its own to finish: it would
        // make the mark again from inside the mark.
        if let Some(recorded) = (!super::archive_mark::within_the_mark())
            .then(|| {
                jojobot_domain::request::skip("claim")
                    .or_else(|| jojobot_domain::request::skip("archive"))
            })
            .flatten()
        {
            return self.finish_the_edit(&args, &caller, recorded).await;
        }
        // **What the call sent to lift a `must` refusal**, taken before the
        // arguments are moved into the patch.
        let slot = super::declined::TokenSlot::from(&args);
        let (override_token, override_reason) =
            (args.override_token.clone(), args.override_reason.clone());
        // **A section's place is the server's to mint.** A move changes `order`
        // without the say-so of the session that wrote the section, so a caller's
        // own value for it, which could be anything, is dropped: `after` is the
        // way to move.
        // **A move is planned before anything is written**, so a refusal leaves the
        // document as it was. The section itself takes its new place in this edit;
        // the chunks that carry it on follow once it has landed.
        // **Held from the read of the neighbours to the end of the edit**, for the
        // reason a put holds it: the key is minted from that read.
        let _placing = match args.after {
            Some(_) => match self.hold_placing(&address.home).await {
                Ok(guard) => Some(guard),
                Err(e) => return memory_declined("update_fact", e),
            },
            None => None,
        };
        // **An entry of a list is placed against the order the list reads now**,
        // and a section of a document against the sections of its document.
        let entry_move = match args.after.as_deref() {
            Some(after) if self.is_on_a_list(&address).await => {
                match self.plan_entry_move(&address, after).await {
                    Ok(plan) => Some(plan),
                    Err(refused) => return memory_declined("update_fact", refused),
                }
            }
            _ => None,
        };
        let move_plan = match args.after.as_deref() {
            Some(after) if entry_move.is_none() => match self.plan_move(&address, after).await {
                Ok(plan) => Some(plan),
                Err(refused) => return memory_declined("update_fact", refused),
            },
            _ => None,
        };
        // **A find/replace is read here for its receipt, and written as the edit
        // it is.** The layer that holds both spellings of a mention applies it
        // to the stored text, carrying the revision it read, so the store's own
        // check refuses it if the claim has been written since. The words are
        // never applied to a text the claim has moved past, and no byte outside
        // the span is rewritten.
        let text_edit = match text_edit_of(&args) {
            Ok(edit) => edit,
            Err(refused) => return memory_declined("update_fact", refused),
        };
        let mut edited_words = None;
        let mut revision_read = None;
        if let Some(edit) = &text_edit {
            match jojobot_domain::memory::text_edit::read_text_edit(
                self.memory.as_ref(),
                &address,
                edit,
                args.expected_revision.map(|revision| revision as usize),
            )
            .await
            {
                Ok(read) => {
                    revision_read = Some(read.revision);
                    edited_words = Some(read.edited);
                }
                Err(refused) => return memory_declined("update_fact", refused),
            }
        }
        let declared = Declared::of(&args);
        let mut cleared = args.clear_fields.clone().unwrap_or_default();
        let mut fields = args.fields.unwrap_or_default();
        // **The setting bag joins `fields` for everything below**, exactly as
        // `capture`'s own copy does; the keys that are the bag travel apart.
        let mut sets_sent = args.sets.unwrap_or_default();
        // **An item added to or taken off a list is resolved against the thing's
        // fold, held under its lock from the read to the write.** The thing is the
        // one the claim is about. A move already holds the lock on the claim's
        // home, which is not re-entrant, so the two are separate calls.
        let _list_lock = if Self::changes_a_list_by_item(&args.sets_add, &args.sets_remove) {
            if args.after.is_some() {
                return memory_declined(
                    "update_fact",
                    MemoryError::InvalidFact(
                        "`after` moves a section and `sets_add` or `sets_remove` change a list \
                         on the thing: they are two edits, so send them as two calls"
                            .into(),
                    ),
                );
            }
            // **The claim is found where the store edit finds it**: past a merge
            // and under the handle its thing wears now. An address from before a
            // merge names a folded thing, which holds nothing, and a list changed
            // against that would drop the items the survivor holds.
            let served = match self.served_address(&address).await {
                Ok(served) => served,
                Err(e) => return memory_declined("update_fact", e),
            };
            let subject = match self.memory.recall(&served.home).await {
                Ok(held) => held
                    .iter()
                    .find(|fact| fact.id == served.local)
                    .map(|fact| fact.subject.clone())
                    .unwrap_or_else(|| served.home.clone()),
                Err(e) => return memory_declined("update_fact", e),
            };
            let guard = match self.hold_placing(&subject).await {
                Ok(guard) => guard,
                Err(e) => return memory_declined("update_fact", e),
            };
            if let Err(refused) = self
                .resolve_list_deltas(&subject, args.sets_add, args.sets_remove, &mut sets_sent)
                .await
            {
                return memory_declined("update_fact", refused);
            }
            Some(guard)
        } else {
            None
        };
        let dropped_keys = match self
            .drop_read_keys(
                &address.home,
                Some(&address.local),
                [&mut fields, &mut sets_sent],
                &mut cleared,
            )
            .await
        {
            Ok(dropped) => dropped,
            Err(e) => return memory_declined("update_fact", e),
        };
        // **What remains of the caller's own fields**, apart from the setting
        // bag for the receipt's hint; copied server keys are not caller fields.
        let own_sent: Vec<String> = fields.keys().cloned().collect();
        if let Some(key) = sets_sent.keys().find(|key| fields.contains_key(*key)) {
            return memory_declined(
                "update_fact",
                MemoryError::InvalidFact(format!(
                    "'{key}' is named in both `fields` and `sets`. A key is one of the claim's \
                     own fields or a setting on its thing, never both: send it in one"
                )),
            );
        }
        let set_keys: std::collections::BTreeSet<String> = sets_sent.keys().cloned().collect();
        let sets_named: Vec<String> = set_keys.iter().cloned().collect();
        fields.extend(sets_sent);
        // **Captured before anything computed joins `fields`**, for the same
        // reason `capture`'s own copy is: this is about what the CALLER
        // sent, never what jojobot added on its own.
        let sent_field_keys: Vec<String> = fields.keys().cloned().collect();
        // **A planned move supplies the server's place**, after copied places
        // have been dropped from the caller's bags and clears.
        if let Some(plan) = &move_plan {
            fields.insert(
                jojobot_domain::memory::document::ORDER.to_string(),
                plan.head_order.clone(),
            );
            if let Some(edges) = &plan.head_edges {
                fields.insert(
                    jojobot_domain::memory::document::EDGES.to_string(),
                    edges.clone(),
                );
            }
        }
        if let Some(plan) = &entry_move {
            fields.extend(plan.clone());
        }
        // **The thing a ceiling binds cannot write that ceiling — checked on
        // the fold, atomically with the write, inside `self.memory.update_fact`
        // below.** A raw check here on `fields` alone would miss a clear, a
        // status change to archived, or a key that differs from the ceiling's
        // only by whitespace, because none of those name the key in what
        // THIS write sends — see `refuses_unlicensed_change`.
        // **The stored due moment is jojobot's, so a caller's own copy of it, or
        // a clear of it, is refused before anything is written** — see
        // `refuses_a_hand_written_due_moment`. Checked on what the caller sent,
        // ahead of the mover, which adds and clears its own.
        if let Some(refused) = self
            .refuses_a_hand_written_due_moment(&address.home, &fields, &cleared)
            .await
        {
            return Ok(refused);
        }
        // **The number a thing's records are held to is raised by someone above
        // the bots that filled it**, never by one of them. Asked here, ahead of the
        // store, because the store knows the thought writers of a thread and not
        // the bots that wrote its records. See `refuses_a_filler_widening`.
        if let Some(refused) = self
            .refuses_a_filler_widening("update_fact", &address.home, &caller.bot, &fields, &cleared)
            .await?
        {
            return Ok(refused);
        }
        // **The review stamp is jojobot's too**, set when the writer is defrag.
        if let Some(refused) =
            self.refuses_a_hand_written_review_stamp(&address.home, &fields, &cleared)
        {
            return Ok(refused);
        }
        // **Kept current here too.** An edit that moves a cadence, a policy or
        // a basis is a write like any other write that could move the due
        // moment — the mechanism does not care that this one is a patch
        // rather than a fresh capture.
        //
        // **Only what reaches the thing counts.** A key sent as the claim's own
        // field stays on the claim, so it moves no due moment.
        let reaching: std::collections::BTreeMap<String, String> = fields
            .iter()
            .filter(|(key, _)| set_keys.contains(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let (due_on_computed, due_on_derived) = self
            .moved_due_moment(
                &address.home,
                &reaching,
                &cleared,
                self.dated(None, args.sid.as_deref()).await?,
                &unread,
            )
            .await;
        let due_on_set = matches!(due_on_computed, attention::DueMove::Set(_)) && due_on_derived;
        let mut clears_due_on = false;
        // **The moved due moment reaches the thing**: it is jojobot's own
        // arithmetic on the thing, not words a caller wrote on a record.
        let mut derived: std::collections::BTreeSet<String> = Default::default();
        match due_on_computed {
            attention::DueMove::Set(due_on) => {
                fields.insert(attention::DUE_ON.to_string(), due_on.to_string());
                derived.insert(attention::DUE_ON.to_string());
            }
            // **The clearing path calls the mover too, and acts on what it
            // says.** The stored moment sits on the record that carried the key
            // it came from, which is not always the claim being edited, and a
            // clear is scoped to the record it addresses. So it is taken off
            // once this edit has landed, on the active record that carries it:
            // see `clear_stored_due_moment`.
            attention::DueMove::Cleared => clears_due_on = true,
            attention::DueMove::Unchanged => {}
        }
        // **A write by defrag stamps the thing in the same act**, as a key on the
        // thing riding this record.
        if let Some((key, moment)) = self.review_stamp(&caller.bot) {
            fields.insert(key.clone(), moment);
            derived.insert(key);
        }
        let content_sent = args.content.is_some();
        let patch = FactPatch {
            content: args.content,
            details: args.details,
            text_edit: text_edit.clone(),
            recorded_at: parse_date(args.recorded_at.as_deref())?,
            happened_at: parse_date(args.happened_at.as_deref())?,
            clear_happened_at: args.clear_happened_at.unwrap_or(false),
            happened_through: parse_date(args.happened_through.as_deref())?,
            clear_happened_through: args.clear_happened_through.unwrap_or(false),
            status: args.status.as_deref().map(parse_status).transpose()?,
            // **jojobot's own arithmetic is jojobot's, exactly as a
            // check-in's is on `capture`.** A moved due moment overrides
            // whatever provenance this same call asked for, so a computed
            // date is never read back with the certainty of somebody's word.
            provenance: if due_on_set {
                Some(Provenance::Inference)
            } else {
                args.provenance
                    .as_deref()
                    .map(parse_one_provenance)
                    .transpose()?
            },
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            receipt: args
                .receipt
                .as_ref()
                .map(super::receipt::parse_receipt)
                .transpose()?,
            confirmed_by_user: args.confirmed_by_user.unwrap_or(false),
            fields,
            clear_fields: cleared.clone(),
            stale_after: parse_date(args.stale_after.as_deref())?,
            clear_stale_after: args.clear_stale_after.unwrap_or(false),
            derived_from: args
                .derived_from
                .as_deref()
                .map(|address| FactAddress::parse(address).map_err(memory_error))
                .transpose()?,
            clear_derived_from: args.clear_derived_from.unwrap_or(false),
            stands_for: args
                .stands_for
                .as_ref()
                .map(|addresses| {
                    addresses
                        .iter()
                        .map(|address| FactAddress::parse(address).map_err(memory_error))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            clear_stands_for: args.clear_stands_for.unwrap_or(false),
            clear_edge: args.clear_edge.unwrap_or(false),
            edge: match parse_edge(args.shape.as_deref(), args.object.as_deref())? {
                Ok(edge) => edge,
                Err(refused) => return Ok(refused),
            },
            aged_before: None,
            role_move: None,
            // Filled in below, after the check that a call names a change: a
            // patch carrying the session would never equal the empty one.
            session: None,
            sets: set_keys,
            derived,
            expected_revision: None,
            must_back: Default::default(),
            must_override: None,
            must_day: None,
        };
        // **`keep` is the one designed way to re-assert a claim on purpose,
        // and the one thing this call refuses rather than silently
        // honouring: naming nothing at all.** A patch equal to its own
        // default changes nothing, and reaching the store with one used to
        // still append a write to the claim's history — a real
        // re-assertion nothing on the receipt named as one, discoverable
        // only by trying it and noticing the moment moved. There is no
        // partial keep: it is this alone, or an ordinary edit naming what
        // changes, never both.
        let patch_names_no_change = patch == FactPatch::default();
        let keep = args.keep.unwrap_or(false);
        match (keep, patch_names_no_change) {
            (true, false) => {
                return memory_declined(
                    "update_fact",
                    MemoryError::InvalidQuery(
                        "keep means keeping this claim exactly as it stands — a call naming a \
                         change is not a keep. Drop keep, or drop the change and send keep: true \
                         alone."
                            .into(),
                    ),
                );
            }
            (false, true) => {
                return memory_declined(
                    "update_fact",
                    MemoryError::InvalidQuery(
                        "this call names no change, and one is never made by accident. To \
                         re-assert this claim on purpose — keeping it exactly as it stands and \
                         moving when it was last touched — call again with keep: true."
                            .into(),
                    ),
                );
            }
            (true, true) | (false, false) => {}
        }
        // **A voucher is issued, never edited**: its state and its other keys are
        // the engine's, so any edit to the claim of one is turned back.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&address.home) {
            return memory_declined("update_fact", refused);
        }
        // **A role's own two fields are the boot door's, whoever is asking.**
        // The claim and renewal paths reach the Memory trait directly, never
        // through this verb, so a write that reaches here naming either
        // field — set or cleared — is, by construction, not one of those
        // two. See `refuses_role_fields` and `capture`'s own copy of this
        // check.
        if let Some(refused) = jojobot_domain::memory::refuses_role_fields(
            &address.home,
            patch.fields.keys().chain(&patch.clear_fields),
        ) {
            return memory_declined("update_fact", refused);
        }
        // **What THIS write sent, not what the fact ends up carrying.** The
        // fact's own edge can already be set from an earlier write this call
        // never touched, so the gate below reads the patch rather than the
        // result — an edit naming neither `shape` nor `object` has no side
        // to get wrong and must not spend the session's one teaching.
        let drew_or_replaced_an_edge = patch.edge.is_some();
        // **Captured before the move, exactly as the edge flag above is.**
        // Whether the rhythm still shows a due moment AFTER this write is
        // read once the write lands — see [`Jojobot::still_has_a_due_moment`].
        let archives_this_write = patch.status == Some(FactStatus::Archived);
        // **Archiving is the third way to take a role field off the fold**,
        // beside `fields` and `clear_fields`: only writes carried by an
        // active record fold at all, so archiving the record that carries a
        // role's holder or claimed_at takes it out exactly as clearing the
        // key would, without ever naming the key in this patch. Checked
        // against the record's OWN fields — the ones it was captured or last
        // edited with — because that is what would leave the fold.
        let mut held_before_archive = None;
        if archives_this_write {
            // **A store that cannot be read refuses, it does not pass** — see
            // `retract`'s own copy of this check.
            //
            // **Found by local id within the home the store resolves**, for the
            // reason `retract`'s own copy gives: an address typed under a former
            // handle never equals the address a read serves, and the store
            // resolves the typed handle itself.
            let carried = match self.memory.recall(&address.home).await {
                Ok(carried) => carried,
                Err(MemoryError::UnknownEntity { .. }) => Vec::new(),
                Err(e) => return memory_declined("update_fact", e),
            };
            let record = carried.iter().find(|fact| fact.id == address.local);
            let refused = record.and_then(|fact| {
                jojobot_domain::memory::refuses_role_fields(&address.home, fact.fields.keys())
            });
            if let Some(refused) = refused {
                return memory_declined("update_fact", refused);
            }
            // What the thing holds before the record comes off it, for the
            // receipt's account of the keys that left.
            if let Some(record) = record {
                held_before_archive =
                    Some((record.subject.clone(), self.held_by(&record.subject).await));
            }
        }
        // **No extra read: the written fact's own `subject` is what
        // `echoed_defaults` needs, and the write below already returns it.**
        // `FactPatch` cannot move a fact to a new subject, so what comes back
        // is the entity these fields were always about. Cloned here, before
        // `patch` moves into the write, for the same reason `capture`'s own
        // copy is: this is what THIS write actually sent, not what the fact
        // ends up carrying.
        let patch_fields = patch.fields.clone();
        // **The keys this call moves from the record's own fields to its settings**:
        // named in `sets` and held off the thing until now. Read before the write,
        // because after it they reach the thing and nothing tells them apart. A
        // store that cannot answer leaves the note out; the write is not held up.
        let moved_to_sets: Vec<String> = if patch.sets.is_empty() {
            Vec::new()
        } else {
            match self.memory.keys_kept_off_the_thing(&address).await {
                Ok(kept) => patch
                    .sets
                    .iter()
                    .filter(|key| kept.contains(*key))
                    .cloned()
                    .collect(),
                Err(_) => Vec::new(),
            }
        };
        // **A star or a seat count that would take the bot's boot over its
        // ceiling is refused here**, before anything lands — see
        // `refuses_a_boot_floor_over`.
        let reads = super::reads_left::ReadsLeft::default();
        if let Some(refused) = self
            .refuses_a_boot_floor_for_edit(&address, &patch, &caller.bot, &reads)
            .await
        {
            return memory_declined("update_fact", refused);
        }
        // **A star into full seats is refused here**, before anything lands —
        // see `refuses_a_star_for_edit`.
        if let Some(refused) = self.refuses_a_star_for_edit(&address, &patch).await {
            return memory_declined("update_fact", refused);
        }
        // **An edit that leaves the record too big to be read back whole is
        // refused here**, before anything lands — see
        // `refuses_a_claim_too_big_to_read`.
        if let Some(refused) = self.refuses_an_edit_too_big_to_read(&address, &patch).await {
            return memory_declined("update_fact", refused);
        }
        // **The ageing cutoff, computed here and never inside `Memory`**, for
        // the reason `capture`'s own copy is. Set after the empty-patch check
        // above, because it names no change.
        let mut patch = patch;
        patch.session = Some(caller.sid.as_str().to_string());
        // **The run's day**, which a relative day in a type's rule counts from.
        patch.must_day = Some(self.dated(None, args.sid.as_deref()).await?);
        patch.expected_revision = args
            .expected_revision
            .map(|revision| revision as usize)
            .or(revision_read);
        // **The keys a rewrite must still back** when the store writes it.
        patch.must_back = rewrite
            .as_ref()
            .map(|guard| guard.must_back.clone())
            .unwrap_or_default();
        // **Asked only when the edit could put a thought into a room**: the bot
        // has a capacity, and the record either becomes a thought by this edit
        // (it draws a connection edge or comes back to active) or already is one.
        // A thought gone quiet is outside the count, and any edit to it touches
        // it, so it rejoins the room and the store weighs it as it weighs a new
        // one. Every other edit reads no runs.
        if address.home.kind() == Some(EntityKind::BOT) {
            let fields = match self.memory.fields(&address.home).await {
                Ok(fields) => fields,
                Err(e) => return memory_declined("update_fact", e),
            };
            if fields
                .get(jojobot_domain::memory::THOUGHT_CAPACITY)
                .is_some_and(|v| v.trim().parse::<usize>().is_ok())
                && self.could_join_a_room(&address, &patch).await
            {
                match self.sessions.summaries_of(&address.home).await {
                    Ok(runs) => {
                        patch.aged_before = jojobot_domain::memory::aging_cutoff(
                            &jojobot_domain::session::history::started_ats(&runs),
                            jojobot_domain::memory::ages_after_runs_of(&fields),
                        );
                    }
                    Err(e) => return session_declined(e, caller.sid.as_str()),
                }
            }
        }
        // **Whether this edit takes the star off a rule of a bot's own**, read
        // before the write and only for an edit that names the key. Counted after
        // it lands, so a record written beside three of them can be taught.
        let names_the_star = patch.fields.contains_key("starred")
            || patch.clear_fields.iter().any(|key| key == "starred");
        let was_starred = names_the_star
            && address.home.kind() == Some(EntityKind::BOT)
            && match self.memory.recall(&address.home).await {
                Ok(held) => held
                    .iter()
                    .find(|fact| fact.id == address.local)
                    .is_some_and(|fact| fact.fields.get("starred").is_some_and(|v| v == "true")),
                Err(_) => false,
            };
        // **The thing as it stands before the write**, for the receipt's account
        // of what the write changed.
        let before = self.as_it_stands(&address.home).await;
        let moves_to_now = self.moves_to_now(&patch.fields, &patch.sets).await;
        // **A write that landed is never reported as failed** (rule 130): see
        // `capture`'s own note on the same shape.
        // **A move is made of parts under a `request_id`**: the section, and each
        // chunk that carries it on. The first keeps the plan.
        let defrag_archive =
            archives_this_write && jojobot_domain::memory::review::is_defrag(&caller.bot);
        let writing = super::must_override::update_fact_lifting(
            &*self.memory,
            &address,
            patch,
            &caller.bot,
            override_token.as_deref(),
            override_reason.as_deref(),
        );
        let writing = match &move_plan {
            Some(plan) => {
                jojobot_domain::request::armed_part(
                    "section",
                    Some(plan.frozen_for(clears_due_on)),
                    writing,
                )
                .await
            }
            None => match super::archive_mark::claim_note(clears_due_on, defrag_archive) {
                // **The claim is the first of up to three parts**: the write, the
                // stored due moment taken off, and the review mark an archive by
                // defrag earns. The note tells a resend which are still owed.
                // **An archive by defrag is a part of its own**, named for the archive: the
                // mark that follows is a nested `update_entity`, which reads a part
                // called `claim` as its own and would try to finish it.
                Some(note) => {
                    let part = if defrag_archive { "archive" } else { "claim" };
                    jojobot_domain::request::armed_part(part, Some(note), writing).await
                }
                None => jojobot_domain::request::armed(writing).await,
            },
        };
        let (written, fold_behind) = match writing {
            Ok(written) => (written, None),
            Err(MemoryError::FoldBehind {
                landed: Landed::Fact(fact),
                behind,
                ..
            }) => (Guarded::Written(*fact), Some(behind)),
            // **A find/replace whose caller sent no revision was refused on the
            // revision this call read**, so the refusal says the text changed
            // under the edit and does not talk about a revision the caller
            // never named.
            Err(MemoryError::StaleRevision {
                address, current, ..
            }) if revision_read.is_some() && args.expected_revision.is_none() => {
                return memory_declined(
                    "update_fact",
                    MemoryError::TextChangedWhileEditing {
                        address,
                        field: text_edit
                            .as_ref()
                            .map_or("text", |edit| edit.field.as_str())
                            .to_string(),
                        current,
                    },
                );
            }
            Err(e @ MemoryError::StaleRevision { .. }) => {
                if let Some(guard) = rewrite {
                    guard.stale = true;
                }
                return memory_declined("update_fact", e);
            }
            Err(e) => return super::must_override::declined_or_lifted("update_fact", e, &slot),
        };
        match written {
            Guarded::Written(fact) => {
                // **Pointing the bot at its work comes before any step that can end
                // the call for a resend to finish**: the finishing paths rebuild
                // the receipt from the claim and do not do it again.
                self.fill_working_on(&caller, &fact.subject, &moves_to_now, &unread)
                    .await;
                // **The chunks of a moved section follow it.** One write each, and a
                // store that stops partway says which moved: the same call again
                // plans from what is stored and finishes the move.
                if let Some(plan) = &move_plan {
                    let mut moved = vec![fact.address().to_string()];
                    if let Err(refused) =
                        self.move_the_chunks(&plan.rest, &caller, &mut moved).await
                    {
                        return refused;
                    }
                }
                // **An archive by defrag is a review.** The stamp this write
                // carried stays on the archived record, and an archived record's keys do
                // not reach the thing, so the thing is marked reviewed as defrag's own
                // mark marks it. The mark is a part of this call: under an id the call
                // is not finished until it is made, and without one the receipt says
                // the thing is still in the queue.
                let marked = if defrag_archive {
                    let marked = self
                        .mark_what_defrag_archived(&fact.subject, args.sid.clone())
                        .await;
                    if let super::archive_mark::Marked::NotMade(why, cause) = &marked
                        && jojobot_domain::request::carries_an_id()
                    {
                        return Ok(self.mark_still_owed(why, cause));
                    }
                    Some(marked)
                } else {
                    None
                };
                // **A move and an archive by defrag are finished by the steps after
                // the section: the mark above, then the clear below.** The id is
                // spent by the last of them, so a resend finishes whichever is owed.
                if clears_due_on {
                    if let Some(unfinished) = self
                        .end_what_is_owed("update_fact", &fact.subject, &caller)
                        .await
                    {
                        return Ok(unfinished);
                    }
                } else if move_plan.is_some() || defrag_archive {
                    jojobot_domain::request::finished();
                }
                self.beat(
                    "update_fact",
                    &fact.address().to_string(),
                    args.sid.as_deref(),
                )
                .await;
                let mut body =
                    fact_receipt_json(&fact, self.dated(None, args.sid.as_deref()).await?);
                dropped_keys.note(&mut body);
                if let Some(marked) = &marked {
                    super::archive_mark::note_the_mark(&mut body, marked);
                }
                let changed = match fold_behind {
                    Some(_) => crate::changed::Changed::not_read(),
                    None => self.changed_on(&before, &fact.subject).await,
                };
                crate::changed::note_changed(&mut body, changed);
                if let Some(behind) = fold_behind {
                    crate::answer::note_fold_behind(&mut body, behind);
                }
                let echoed_defaults = self
                    .memory
                    .echoed_defaults(&fact.subject, &patch_fields)
                    .await;
                crate::answer::note_echoes_defaults(&mut body, &echoed_defaults);
                // **Only when the caller named a day AND their run has one of
                // its own AND the two disagree.** Reparsed rather than
                // threaded through the moved patch — `parse_date` already
                // validated this same string once above, so it cannot fail
                // differently here.
                if let Some(explicit) = parse_date(args.recorded_at.as_deref())?
                    && let Some(stated) = caller.day
                    && stated != explicit
                {
                    crate::answer::note_recorded_at_mismatch(&mut body, stated, explicit);
                }
                crate::answer::note_delta(&mut body, declared.not_stored(&fact));
                if let Some(edited) = &edited_words {
                    crate::answer::note_text_edit(&mut body, edited);
                }
                // **The claim's write count after this edit**, the number `recall`
                // serves as `revision_count`, so a second guarded edit sends it
                // without a read in between. Left out when the store cannot say.
                if let Ok(chain) = self.memory.claim_history(&fact.address()).await {
                    body["revision"] = serde_json::json!(chain.len());
                    body["revision_note"] = serde_json::json!(
                        "send revision as expected_revision on the next edit of this claim, and \
                         that edit is refused if the claim has been written since"
                    );
                }
                let mut line = self
                    .what_an_update_left_standing(&fact, &cleared, keep, &mut body)
                    .await;
                if let Some(due) = crate::answer::falls_due_sentence(&fact.subject, due_on_computed)
                {
                    line.push(' ');
                    line.push_str(&due);
                }
                if !moved_to_sets.is_empty() {
                    line.push(' ');
                    line.push_str(&format!(
                        "{} moved from the claim's own fields to its settings: {} holds {} now.",
                        moved_to_sets.join(", "),
                        fact.subject,
                        if moved_to_sets.len() == 1 {
                            "it"
                        } else {
                            "them"
                        },
                    ));
                }
                crate::answer::note_postcondition(&mut body, line);
                if !moved_to_sets.is_empty() {
                    body["moved_to_sets"] = serde_json::json!(moved_to_sets);
                }
                self.note_reach_and_hint(&mut body, &fact, &own_sent, &sets_named)
                    .await;
                self.note_view_narrows_nothing(&mut body, &fact.subject)
                    .await;
                if let Some((subject, before)) = held_before_archive {
                    self.note_what_left(&mut body, &subject, before, &reads)
                        .await;
                }
                reads.note(&mut body);
                self.note_status_word(&fact, &mut body, &unread).await;
                unread.note(&mut body);
                self.note_depends_on_read(&fact, &caller, &mut body).await;
                if content_sent {
                    self.note_possible_subject(&fact, &caller, &mut body).await;
                }
                self.note_unlocks(&fact.subject, &caller, &mut body).await;
                let unstarred_this_write =
                    was_starred && fact.fields.get("starred").is_none_or(|v| v != "true");
                self.note_unstarred(&fact, was_starred, caller.sid.as_str());
                self.note_unstars_without_a_summary(
                    &fact,
                    unstarred_this_write,
                    caller.sid.as_str(),
                    &mut body,
                );
                if self.first_contact(CLAIMS_DOMAIN, Some(&caller)).await {
                    crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
                }
                if self
                    .first_contact(CLAIM_SUBJECT_DOMAIN, Some(&caller))
                    .await
                {
                    crate::answer::note_teaching(&mut body, CLAIM_SUBJECT_TEACHING);
                }
                if drew_or_replaced_an_edge
                    && self
                        .first_contact(CLAIM_DIRECTION_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, CLAIM_DIRECTION_TEACHING);
                }
                // **The gate is the aftermath, not the act.** Archiving most
                // things has nothing to do with a schedule, and archiving a
                // rhythm claim that never carried an open one leaves
                // nothing standing — so this reads the rhythm's own fields
                // AFTER the write, rather than assuming from the act alone.
                if archives_this_write
                    && fact.subject.kind() == Some(EntityKind::RHYTHM)
                    && self.still_has_a_due_moment(&fact.subject).await
                    && self
                        .first_contact(RHYTHM_ARCHIVE_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, RHYTHM_ARCHIVE_TEACHING);
                }
                // **Checked before the gate, never after** — see `capture`'s
                // own copy of this note. The schema lookup is skipped
                // entirely when no fields were sent, which is most calls.
                //
                // **Against the UNION of both verbs' arguments** — see
                // `capture`'s own copy: an update_fact carrying `check_in`,
                // capture's own argument, is caught the same way.
                //
                // **Only keys the subject's kind leaves undeclared** — a key
                // the kind declares is its own column, never a shadow.
                let undeclared_keys = crate::teaching::keys_the_kind_leaves_undeclared(
                    fact.subject.kind(),
                    &sent_field_keys,
                );
                if !undeclared_keys.is_empty()
                    && let Some(update_fact_properties) =
                        crate::teaching::published_arguments("update_fact")
                    && let Some(capture_properties) =
                        crate::teaching::published_arguments("capture")
                    && let Some((shadowed, owning_verb)) = crate::teaching::shadowed_argument_verb(
                        undeclared_keys.iter().copied(),
                        "update_fact",
                        &update_fact_properties,
                        "capture",
                        &capture_properties,
                    )
                    && self
                        .first_contact(FIELD_SHADOWS_ARGUMENT_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(
                        &mut body,
                        &crate::teaching::field_shadows_argument_teaching(shadowed, &owning_verb),
                    );
                }
                json_result(&body)
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("update_fact"),
            )),
        }
    }
}

/// **The find/replace this call asks for, or the refusal of a call that asks for
/// half of one.** `find`, `replace` and `field` go together; `field` names the
/// text and has no default, and a whole replacement of that same text in the
/// same call is two edits to one text.
fn text_edit_of(args: &UpdateFactArgs) -> Result<Option<TextEdit>, MemoryError> {
    let (Some(find), Some(replace), Some(field)) = (&args.find, &args.replace, &args.field) else {
        if args.find.is_none() && args.replace.is_none() && args.field.is_none() {
            return Ok(None);
        }
        return Err(MemoryError::InvalidFact(
            "`find`, `replace` and `field` go together: send all three, or none. `replace` \
             may be an empty string, which deletes the found words"
                .into(),
        ));
    };
    let Some(field) = TextField::parse(field) else {
        return Err(MemoryError::InvalidFact(format!(
            "`field` is '{field}', and it is `content` or `details`: the text `find` searches"
        )));
    };
    let whole = match field {
        TextField::Content => args.content.is_some(),
        TextField::Details => args.details.is_some(),
    };
    if whole {
        return Err(MemoryError::InvalidFact(format!(
            "`find` edits part of the {0}, and a whole `{0}` replaces all of it: send one of the \
             two. The other text may be sent in the same call",
            field.as_str()
        )));
    }
    Ok(Some(TextEdit {
        field,
        find: find.clone(),
        replace: replace.clone(),
    }))
}

/// **Every value this edit declared that the record does not carry**, and the
/// snapshot it is read from — taken before the patch is assembled, because
/// assembling it consumes what the caller sent.
struct Declared {
    provenance: Option<String>,
    standing: Option<String>,
    status: Option<String>,
    recorded_at: Option<String>,
}

impl Declared {
    fn of(args: &UpdateFactArgs) -> Self {
        Self {
            provenance: args.provenance.clone(),
            standing: args.standing.clone(),
            status: args.status.clone(),
            recorded_at: args.recorded_at.clone(),
        }
    }

    fn not_stored(&self, fact: &Fact) -> Vec<crate::answer::Difference> {
        use crate::answer::Difference;
        [
            Difference::between(
                "provenance",
                self.provenance.as_deref(),
                fact.provenance.as_token(),
            ),
            Difference::between(
                "standing",
                self.standing.as_deref(),
                fact.standing.as_token(),
            ),
            Difference::between("status", self.status.as_deref(), fact.status.as_token()),
            Difference::between(
                "recorded_at",
                self.recorded_at.as_deref(),
                &fact.recorded_at.to_string(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Jojobot {
    /// **What a caller's own edit did to the thing the record belongs to.**
    ///
    /// `update_fact` rewrites one record in place. A move of a document section
    /// with `after` also writes the blank-line lead of each section it puts beside
    /// another, and no word of any section changes. The
    /// record now states what it was sent and does not keep what it said
    /// before — the store holds current truth, never a correction trail — so
    /// this line says which record moved and how much beside it did not.
    ///
    /// ⚠️ **A clear removes.** The keys it took off are named, because this is
    /// the one write on this surface that can take something away, and a line
    /// claiming otherwise would be false exactly here.
    ///
    /// The count is read for this line and left out when the store cannot
    /// answer, since a number nobody can stand behind is worse than the
    /// sentence without one.
    async fn what_an_update_left_standing(
        &self,
        fact: &Fact,
        cleared: &[String],
        keep: bool,
        body: &mut serde_json::Value,
    ) -> String {
        let address = fact.address().to_string();
        // **A keep is not an edit with nothing to say about — it is its own
        // shape**, and the other sentences below (what moved beside it,
        // what a clear removed, the archive-only path) all answer questions
        // that only make sense for a write that changed something. Answered
        // once, plainly, and never falls through to the rest.
        if keep {
            return format!(
                "{address} is unchanged: this call kept it exactly as it stood and moved when \
                 it was last touched. Recall {} with history_record: {address} to see the new \
                 write beside every one before it.",
                fact.subject.as_str(),
            );
        }
        let beside = self
            .memory
            .recall(&fact.subject)
            .await
            .ok()
            .map(|facts| {
                facts
                    .iter()
                    .filter(|f| f.status == FactStatus::Active && f.address() != fact.address())
                    .count()
            })
            .map_or_else(String::new, |n| {
                match n {
                    // **Nothing else on the thing, so there is nothing to
                    // reassure anybody about.** "The 0 other records are as
                    // they were" is a sentence about an empty set, and a real
                    // model read it in a paid run.
                    0 => String::new(),
                    1 => format!(
                        " The 1 other record on {} is as it was.",
                        fact.subject.as_str()
                    ),
                    n => format!(
                        " The {n} other records on {} are as they were.",
                        fact.subject.as_str()
                    ),
                }
            });
        let removed = if cleared.is_empty() {
            String::new()
        } else {
            format!(
                " It no longer carries {}, so what those keys answer for {} falls back to the \
                 records that set them.",
                cleared.join(", "),
                fact.subject.as_str(),
            )
        };
        // **A clear that took off part of a rhythm's schedule, not all of
        // it, does not quiet the loop — it leaves the loop reading overdue
        // instead.** Say what is left and the act that finishes the job,
        // rather than letting the caller believe a half-clear silenced it.
        let half_cleared = match self.memory.fields(&fact.subject).await {
            Ok(current) => attention::half_cleared_schedule(cleared, &current)
                .map(|remaining| {
                    format!(
                        " This cleared part of {}'s schedule, not all of it: it still carries \
                         {}, which reads overdue rather than never-due. Clear {} too to stop it \
                         falling due at all.",
                        fact.subject.as_str(),
                        remaining.join(", "),
                        remaining.join(", "),
                    )
                })
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        if !half_cleared.is_empty() {
            crate::write_receipts::warnings(body, [half_cleared.trim().into()]);
        }
        // **What this write did NOT destroy.** A session that met a
        // conflicting claim, would not overwrite it on a guess, and wrote
        // nothing at all had good reason while the old words were gone. They
        // are kept now, so the receipt says so and names the call that reads
        // them — the caution goes away because its reason does.
        let kept = format!(
            " What it said before is kept: recall {} with history_record: {address} to read \
             every version of it, oldest first.",
            fact.subject.as_str(),
        );
        // **The other path, named at the moment somebody is already reading.**
        let instead = self.the_other_path(fact).await;
        if !instead.is_empty() {
            body["event_negation_note"] = serde_json::json!(format!(
                "{} Previous versions remain readable with recall history_record: the claim address.",
                instead.trim()
            ));
        }
        // **Archive is a visibility switch, not a validity gate** — citing an
        // archived claim as derived_from is permitted, so the caller has to
        // be told rather than left to notice on a later read.
        let source_note = match &fact.derived_from {
            Some(source) => self
                .memory
                .recall(&source.home)
                .await
                .ok()
                .and_then(|facts| facts.into_iter().find(|f| f.id == source.local))
                .filter(|f| f.status == FactStatus::Archived)
                .map(|_| {
                    format!(
                        " The claim this was derived from, {source}, is archived — read it to \
                         judge whether the citation still holds."
                    )
                })
                .unwrap_or_default(),
            None => String::new(),
        };
        if crate::write_receipts::single_write() && !source_note.is_empty() {
            crate::write_receipts::warnings(body, [source_note.trim().into()]);
        }
        format!(
            "{address} now states what this call sent, in place of what it said before.{kept}\
             {beside}{removed}{half_cleared}{instead}{source_note}"
        )
    }

    /// **When a rewrite was probably the wrong move, say what the right one
    /// is — once, on the receipt, and never as a refusal.**
    ///
    /// Neither reason a past-event claim turns into its negation is a
    /// rewrite. An event that never happened is retracted, which leaves a
    /// dated record of the retraction beside the claim. A record that was
    /// true and changed is archived, with a note saying why, and
    /// the new claim is a fresh capture, not this row overwritten: rule 58,
    /// which told an agent to rewrite a disproved fact into its own denial,
    /// is gone, and an in-place negation is the shape it took. Two paid runs
    /// were told somebody had never been at an event and reached for the
    /// edit both times.
    ///
    /// ⚠️ **It names the FORK and never a diagnosis.** Nothing on the wire says
    /// which of the two acts a caller means — a guest who was never there and
    /// one who cancelled write the same sentence — so the line gives both and
    /// lets the caller pick. **Asserting the first would point a cancelled
    /// attendance at a verb that does not fit it**, which is worse than
    /// silence.
    ///
    /// ⛔️ **The claim's own date does not decide it.** A record of last
    /// month's party captured today carries today's date, so a past-only test
    /// would miss exactly the case this was built for.
    ///
    /// ⛔️ **Not a gate, because the heuristic is a guess.** A word search for a
    /// negation can be wrong in both directions, and refusing on a guess costs
    /// more than the defect it would catch. **Empty everywhere else**, because
    /// a line on every receipt is a line nobody reads.
    ///
    /// **The previous wording comes from the claim's own writes**, which is
    /// also why the edge is read from the write BEFORE this one: a caller
    /// following this verb's own advice clears the edge in the same call that
    /// negates the sentence, and the record in front of us no longer says what
    /// it was about.
    async fn the_other_path(&self, fact: &Fact) -> String {
        let Ok(chain) = self.memory.claim_history(&fact.address()).await else {
            return String::new();
        };
        // The write before this one. A claim written once has no before, and
        // nothing was replaced.
        let Some(previous) = chain.iter().rev().nth(1) else {
            return String::new();
        };
        let at_an_event = |edge: Option<&Edge>| {
            edge.is_some_and(|edge| {
                edge.shape == EdgeShape::Attendance || edge.object.kind() == Some(EntityKind::EVENT)
            })
        };
        let about_an_event = fact.subject.kind() == Some(EntityKind::EVENT)
            || at_an_event(previous.edge.as_ref())
            || at_an_event(fact.edge.as_ref());
        if !about_an_event || !adds_a_negation(&previous.content, &fact.content) {
            return String::new();
        }
        String::from(
            " This turns a claim about an event into its negation, and neither reason for that \
             is a rewrite. If the event never happened, retract the claim, which also leaves a \
             dated record of the retraction. If it was true and has changed, archive it \
             (status: archived, details saying why) and capture the new claim; overwriting it \
             here loses the day it stopped being true.",
        )
    }

    /// **Reads the rhythm's own fields, after this write landed.** A
    /// schedule is the rhythm's fields, each key the newest write of it from
    /// whichever record made that write, so this asks the thing itself rather
    /// than the record just archived — the same question `moved_due_moment`
    /// computes forward, asked backward: is a due moment still standing now.
    ///
    /// A store failure reads as no due moment, for the same reason
    /// [`Jojobot::first_contact`] answers `false` on one: a teaching that
    /// cannot confirm its own premise must not spend the session's one
    /// contact on a guess.
    async fn still_has_a_due_moment(&self, subject: &EntityId) -> bool {
        self.memory
            .fields(subject)
            .await
            .is_ok_and(|fields| fields.contains_key(attention::DUE_ON))
    }
}

/// **Does the new wording say no where the old one did not?**
///
/// Word-wise and case-insensitive, over a small set of plain negations plus
/// any contraction ending in `n't`. It is a heuristic and it is allowed to be:
/// what rides on it is one advisory line, so a miss costs nothing a caller had
/// before and a false positive costs a sentence.
fn adds_a_negation(before: &str, after: &str) -> bool {
    let negations = |text: &str| {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .any(|word| matches!(word, "not" | "never" | "no" | "nor") || word.ends_with("n't"))
    };
    negations(after) && !negations(before)
}

impl Jojobot {
    /// **The chunks of a moved section follow it.** One write each, a part of
    /// its own under a `request_id`. A store that stops partway says which moved:
    /// the same call again plans from what is stored, or, with an id, finishes
    /// the plan the first call chose. `Err` is the answer the call ends with.
    async fn move_the_chunks(
        &self,
        rest: &[(FactAddress, &'static str, String)],
        caller: &crate::caller::Caller,
        moved: &mut Vec<String>,
    ) -> Result<(), Result<CallToolResult, McpError>> {
        use jojobot_domain::request::{Part, part};
        for (at, (chunk, key, value)) in rest.iter().enumerate() {
            let patch = FactPatch {
                fields: [((*key).to_string(), value.clone())].into(),
                session: Some(caller.sid.as_str().to_string()),
                ..Default::default()
            };
            match part(
                &format!("chunk-{at}"),
                None,
                self.memory.update_fact(chunk, patch, &caller.bot),
            )
            .await
            {
                Part::Ran(Ok(_))
                | Part::Ran(Err(MemoryError::FoldBehind { .. }))
                | Part::Recorded(_) => moved.push(chunk.to_string()),
                Part::Ran(Err(e)) => {
                    let still = rest.len() + 1 - moved.len();
                    let to_finish = format!(
                        "{} of {} parts of the section moved and stay. Send the same call again \
                         to finish the move",
                        moved.len(),
                        rest.len() + 1
                    );
                    // **With an id the same call finishes the plan it began.**
                    if jojobot_domain::request::carries_an_id() {
                        return Err(Ok(crate::caller::handle_declined(
                            "update_fact",
                            crate::stopped::Stopped::of_memory(&e).unfinished(
                                &format!(
                                    "{} of {} parts of the section moved and stay.",
                                    moved.len(),
                                    rest.len() + 1
                                ),
                                "Send the same call again with the same request_id: the move is \
                                 finished in the places the first call chose.",
                            ),
                        )));
                    }
                    return Err(super::documents::stopped_with(
                        memory_declined("update_fact", e),
                        serde_json::json!({
                            "moved": moved,
                            "not_moved": still,
                            "to_finish": to_finish,
                        }),
                        format!("the store stopped partway: {to_finish}"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// **The end of an edit whose claim an earlier call wrote**: the stored due
    /// moment is taken off the finished thing, as the plan in the claim's note
    /// says, and the receipt is built from the claim as it was written.
    async fn finish_the_edit(
        &self,
        args: &UpdateFactArgs,
        caller: &crate::caller::Caller,
        recorded: jojobot_domain::request::Recorded,
    ) -> Result<CallToolResult, McpError> {
        let gone = |what: &str| {
            Ok(crate::caller::handle_declined(
                "update_fact",
                crate::answer::WayForward::person(format!(
                    "The edit was written under this request_id, but {what}, so the rest \
                     cannot be finished."
                )),
            ))
        };
        let (_, address) = match self.recorded_claim_target(&recorded).await {
            Ok(target) => target,
            Err(what) => return gone(what),
        };
        let address = match self.served_address(&address).await {
            Ok(served) => served,
            Err(_) => return gone("its claim cannot be found"),
        };
        let Some(fact) = self
            .memory
            .recall(&address.home)
            .await
            .ok()
            .and_then(|facts| facts.into_iter().find(|fact| fact.id == address.local))
        else {
            return gone("it cannot be read back");
        };
        // **The review mark an archive by defrag earns is made first**, so a mark that
        // fails again leaves the call open for the next resend.
        let marked = if super::archive_mark::note_holds(
            recorded.note.as_deref(),
            super::archive_mark::MARKS_REVIEWED,
        ) {
            let marked = self
                .mark_what_defrag_archived(&fact.subject, args.sid.clone())
                .await;
            if let super::archive_mark::Marked::NotMade(why, cause) = &marked {
                return Ok(self.mark_still_owed(why, cause));
            }
            Some(marked)
        } else {
            None
        };
        if super::archive_mark::note_holds(recorded.note.as_deref(), super::due::CLEARS_DUE_ON) {
            if let Some(unfinished) = self
                .end_what_is_owed("update_fact", &fact.subject, caller)
                .await
            {
                return Ok(unfinished);
            }
        } else {
            jojobot_domain::request::finished();
        }
        self.beat("update_fact", &address.to_string(), args.sid.as_deref())
            .await;
        let mut body = fact_receipt_json(&fact, self.dated(None, args.sid.as_deref()).await?);
        if let Some(marked) = &marked {
            super::archive_mark::note_the_mark(&mut body, marked);
        }
        crate::request::note_resumed(&mut body);
        crate::changed::note_changed(&mut body, crate::changed::Changed::not_read());
        json_result(&body)
    }

    /// **The end of a move whose section an earlier call wrote**: the chunks that
    /// did not move follow it, in the places the plan in the note chose.
    async fn finish_move(
        &self,
        args: &UpdateFactArgs,
        caller: &crate::caller::Caller,
        recorded: jojobot_domain::request::Recorded,
    ) -> Result<CallToolResult, McpError> {
        let gone = |what: &str| {
            Ok(crate::caller::handle_declined(
                "update_fact",
                crate::answer::WayForward::person(format!(
                    "The section was moved under this request_id, but {what}, so the rest of \
                     the move cannot be finished."
                )),
            ))
        };
        let Some(rest) = recorded
            .note
            .as_deref()
            .and_then(super::documents::MovePlan::thawed)
        else {
            return gone("the plan it kept cannot be read");
        };
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let Some(fact) = self
            .memory
            .recall(&address.home)
            .await
            .ok()
            .and_then(|facts| facts.into_iter().find(|fact| fact.id == address.local))
        else {
            return gone("the section cannot be read back");
        };
        let mut moved = vec![address.to_string()];
        if let Err(ended) = self.move_the_chunks(&rest, caller, &mut moved).await {
            return ended;
        }
        // **The review mark an archive by defrag earns follows the move**, as it
        // does in the call that began it; a mark an earlier call made is not made
        // again.
        let archived_by_defrag = args.status.as_deref().map(parse_status).transpose()?
            == Some(FactStatus::Archived)
            && jojobot_domain::memory::review::is_defrag(&caller.bot);
        let marked = if archived_by_defrag {
            let marked = self
                .mark_what_defrag_archived(&fact.subject, args.sid.clone())
                .await;
            if let super::archive_mark::Marked::NotMade(why, cause) = &marked
                && jojobot_domain::request::carries_an_id()
            {
                return Ok(self.mark_still_owed(why, cause));
            }
            Some(marked)
        } else {
            None
        };
        // **What the edit ended is taken off once the move is whole**, as the plan
        // in the note says.
        if recorded
            .note
            .as_deref()
            .is_some_and(super::documents::MovePlan::thawed_clears_due_on)
        {
            if let Some(unfinished) = self
                .end_what_is_owed("update_fact", &fact.subject, caller)
                .await
            {
                return Ok(unfinished);
            }
        } else {
            jojobot_domain::request::finished();
        }
        self.beat("update_fact", &address.to_string(), args.sid.as_deref())
            .await;
        let mut body = fact_receipt_json(&fact, self.dated(None, args.sid.as_deref()).await?);
        body["moved"] = moved.into();
        if let Some(marked) = &marked {
            super::archive_mark::note_the_mark(&mut body, marked);
        }
        crate::request::note_resumed(&mut body);
        crate::changed::note_changed(&mut body, crate::changed::Changed::not_read());
        json_result(&body)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "update_fact/legacy_resume_tests.rs"]
mod legacy_resume_tests;
