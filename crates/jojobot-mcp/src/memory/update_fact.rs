//! `update_fact` — Correct an addressed fact in place — the source, never an addendum.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use jojobot_domain::attention;

use super::*;
use crate::teaching::{
    CLAIM_DIRECTION_DOMAIN, CLAIM_DIRECTION_TEACHING, CLAIM_SUBJECT_DOMAIN, CLAIM_SUBJECT_TEACHING,
    CLAIMS_DOMAIN, CLAIMS_TEACHING, FIELD_SHADOWS_ARGUMENT_DOMAIN, RHYTHM_ARCHIVE_DOMAIN,
    RHYTHM_ARCHIVE_TEACHING,
};

/// Arguments to `update_fact`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateFactArgs {
    /// The fact's global address, `kind:slug#local-id` — exactly as `recall`
    /// returned it.
    pub(crate) address: String,
    /// Replacement claim.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as the
    /// handle that thing wears today, even after a rename.
    #[serde(default)]
    pub(crate) content: Option<String>,
    /// Replacement details; pass an empty string to clear them. They may hold
    /// paragraph breaks and read back with every one. Carries `@kind:slug`
    /// mentions exactly as `content` does.
    #[serde(default)]
    pub(crate) details: Option<String>,
    /// **The day this claim was MADE**, `YYYY-MM-DD` — the day it was said,
    /// decided or worked out. Left alone when omitted — the record keeps the
    /// day of the claim it replaces, exactly as any field this patch does
    /// not name.
    ///
    /// **Give this when the correction itself was made on a different day
    /// than the one already on the record — never the day you happen to be
    /// typing.** Rewriting content with no date given leaves the ORIGINAL day
    /// on the record, permanently: a correction made months later would
    /// otherwise read back as if it had been made on the original day
    /// forever.
    ///
    /// **Naming a day that differs from the day your run is in is never
    /// refused** — the receipt carries `recorded_at_note`, one sentence
    /// naming both days, so a sitting acting out a different period does not
    /// silently record under the wrong one by mistake.
    #[serde(default)]
    pub recorded_at: Option<String>,
    /// **The day the thing this claim is about HAPPENED**, `YYYY-MM-DD`.
    ///
    /// A different question from `date`, and **learning it late is ordinary**:
    /// a claim written when nobody knew the day gains one here. Leaving it off
    /// keeps whatever the record says, like every other field this patch does
    /// not name.
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
    /// days rather than one. Leaving it off keeps whatever the record says.
    #[serde(default)]
    pub happened_through: Option<String>,
    /// **Take the far end off**, leaving a single-day claim (or an undated
    /// one, alongside `clear_happened_at`) rather than a span.
    #[serde(default)]
    pub clear_happened_through: Option<bool>,
    /// `active` or `archived`. **Archive a claim that changed or was never
    /// true — do not negate it.** Rewriting `content` into its own denial
    /// ("the club does NOT meet on Tuesdays" replacing "the club meets on
    /// Tuesdays") leaves a sentence about what is not so where a claim about
    /// what is true belongs. Set `status: archived` and give `details`
    /// saying why, then capture the true claim as a new record — name this
    /// one as its `derived_from` when there is a direct replacement. A
    /// negative is still an ordinary fact when it is not correcting
    /// anything: "he did not attend" stands on its own, written with
    /// `capture` like any other claim.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `testimony`, `observation` or `inference`. Moving a claim TO `testimony`
    /// needs `confirmed_by_user` whichever value it held: a claim you read
    /// somewhere is not a step towards the user having said it. An
    /// `observation` must carry `read_from`, here as at capture — and **taking
    /// the source off a claim that stays an observation is refused for the same
    /// reason**, because both leave a machine read of a system nobody named.
    /// A claim that already carries one does not have to name it again.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// `settled` or `open`. **Moving a claim that is ALREADY open to settled
    /// requires `confirmed_by_user`** — the operator hedged that claim, and
    /// only the operator can withdraw the hedge. Reopening is free.
    ///
    /// That is the whole of the gate: it is on this promotion, not on
    /// declaring a standing. A fresh `capture` may state `settled` and is
    /// taken at its word, the way it is taken at its word about provenance.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// Required for two promotions of an EXISTING claim: anything → testimony
    /// (inference or observation alike), and an open standing → settled. Set it only when the user has actually
    /// confirmed the claim. Nothing else is gated on it — a fresh `capture`
    /// declares its provenance and its standing on honour.
    #[serde(default)]
    pub(crate) confirmed_by_user: Option<bool>,
    /// The shape of an edge to attach: `location` · `membership` · `attendance` ·
    /// `about` · `connection` (a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone.
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist** —
    /// `add_entity` first if it is genuinely new.
    #[serde(default)]
    pub(crate) object: Option<String>,
    /// **Fields to set**, as key/value pairs. Each key named is written; a key
    /// the record already carries and this does not name is left alone, so an
    /// edit reaches one field without restating the rest.
    #[serde(default)]
    pub(crate) fields: Option<std::collections::BTreeMap<String, String>>,
    /// **Fields to remove**, by key. Its own argument rather than an empty
    /// value in `fields`: an empty value is a value somebody wrote, and
    /// setting a key to nothing and taking the key off the record are two
    /// different edits.
    #[serde(default)]
    pub(crate) clear_fields: Option<Vec<String>>,
    /// **The day after which this reading stops being good**, `YYYY-MM-DD`.
    ///
    /// It is a fact about jojobot's knowledge rather than about the world: a
    /// pass that runs out on a date is a claim about the world and belongs in
    /// the content. Past this day a read SAYS SO and **nothing else happens** —
    /// no sweep, no reminder, nobody is coming to check it.
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
    /// **Mark this record as standing for the claims named here**, each as
    /// its address `kind:slug#local-id`. A synthesis, not a citation: the
    /// named claims stay exactly as they are — active, readable, untouched —
    /// so the full picture is still there for anyone who reads them. A write
    /// replaces the whole set. Every named address must already exist;
    /// naming this record's own address, or the same address twice, is
    /// refused — and so is an empty set, because a mark with no sources
    /// would read back as an ordinary record, indistinguishable from one a
    /// session never finished marking.
    #[serde(default)]
    pub(crate) stands_for: Option<Vec<String>>,
    /// **Take the mark off**, leaving an ordinary record that stands for
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
    /// other end becomes unreachable from this record. **A PAST EVENT THAT
    /// TURNED OUT NEVER TO HAVE HAPPENED IS RETRACT'S CASE, NOT THIS ONE**:
    /// retracting marks the record rather than rewriting it, and there is no
    /// un-retract.
    #[serde(default)]
    pub(crate) clear_edge: Option<bool>,
    /// **Keep this claim exactly as it stands — the designed way to bump when
    /// it was last touched without changing anything else.** Worth reaching
    /// for on a thought (`capture`'s `connection` on your own bot handle):
    /// one nobody touches for long enough goes quiet on its own, and this is
    /// how you say, on purpose, that one still matters. The one call
    /// this verb refuses when nothing else is named: naming no change at all
    /// is refused unless this is `true`, and this is refused if anything
    /// else here would actually change the claim. There is no partial keep —
    /// it is this alone, or an ordinary edit naming what changes.
    #[serde(default)]
    pub(crate) keep: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Edit one addressed fact in place — fix the source, never an addendum.
#[tool_router(router = update_fact_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(description = "Edit an addressed fact in place \
                       (content/details/date/status/provenance/standing). To record that something \
                       is NOT so, rewrite content to state the negative truth — that is an \
                       ordinary edit and the fact stays active; there is no negated status. \
                       NOT FOR A PAST EVENT: turning a claim about one into its negation is \
                       never this rewrite — archive it instead (status: archived, with a note \
                       saying why), or retract it. \
                       DATE REWRITES THE DAY THIS CLAIM WAS MADE — the day it was said, decided \
                       or worked out, YYYY-MM-DD — the same argument retract carries, and it is \
                       never the day the call happens to be made on. Omit it and the record \
                       keeps the day of the claim it replaces; give it when the correction \
                       itself was made on a different day, or a rewrite made long after the \
                       fact keeps the ORIGINAL day forever, reading back as if it had always \
                       been made on a day it was never made on. \
                       TWO MOVES NEED confirmed_by_user, and they are different: moving a \
                       claim TO testimony (who backs it), from inference or from observation \
                       alike — a claim you read in a system of record is not a step towards the \
                       operator having said it — and settling a claim that is already open (how \
                       sure anyone is). THIS IS HOW A HEDGE IS CONFIRMED — the \
                       operator hedged the claim and no longer does, so set standing settled \
                       and leave provenance alone; the claim was theirs from the start. \
                       Reopening is free. THE GATE IS ON PROMOTION, NOT ON ASSERTION: a fresh \
                       capture may declare standing settled and nobody is asked to confirm it, \
                       exactly as it declares its provenance — what needs the operator's word \
                       is moving a claim they hedged. IT ALSO REACHES THE RECORD'S FIELDS: \
                       fields sets the keys you name and leaves every other key alone, and \
                       clear_fields takes keys off. Those are two arguments rather than one, \
                       because setting a key to an empty value and removing the key are \
                       different edits and a caller means one of them. A field you set that \
                       equals what jojobot ships as its default today is stored exactly as you \
                       sent it either way, and the receipt's echoes_defaults names which key that \
                       was. TO MOVE A VALUE TO ANOTHER KEY, send `fields` with the new key and \
                       `clear_fields` with the old one in the same call. A CLAIM CANNOT CHANGE \
                       SUBJECT: to move one filed on the wrong thing, capture it on the right \
                       one, re-sending its dates, fields and edge, with `derived_from` naming \
                       this claim, then archive this one. AND IT REACHES THE \
                       EDGE BOTH WAYS: shape with object draws or replaces one, and clear_edge \
                       takes it off. Reach for clear_edge when the rewrite turns a claim about \
                       what is true NOW into its current negative — was a member and is not any \
                       more, was living somewhere and moved away — because then the edge belongs \
                       to the sentence you just erased. A rewrite that stays positive is not this \
                       case: clearing the edge there stops every walk through it and makes the \
                       entity on the other end unreachable from this record, so leave it alone. \
                       A PAST EVENT THAT TURNED OUT NEVER TO HAVE HAPPENED IS RETRACT'S CASE, \
                       NOT THIS ONE: retracting marks the record rather than rewriting it, and \
                       there is no un-retract. \
                       stands_for MARKS THIS RECORD AS STANDING FOR THE CLAIMS NAMED HERE, each \
                       as its address: a synthesis, never a citation — the named claims stay \
                       active and readable exactly as they were, so recall still shows the full \
                       picture. clear_stands_for takes the mark off. A write replaces the whole \
                       set; naming an address that does not exist, this record's own address, \
                       the same address twice, or an empty set is refused. \
                       An address that \
                       names no fact comes back status: blocked with the addresses that do \
                       exist — it never creates. IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the \
                       address, the date, the provenance, the standing, the status and how many \
                       keys the record now carries, with the claim itself elided and said to be. \
                       The write is still verified against the store before it is called a \
                       success; what stops is shipping you the words you just sent. recall the \
                       subject to read the record back. AND A REWRITE DESTROYS NOTHING: every \
                       write of a claim is kept, so what it said before this call is still \
                       readable — recall the subject with history_record: the address, and you \
                       get every version of it, oldest first. So a claim you disagree with is \
                       safe to correct: you are not deciding whether the old wording survives, \
                       only what the claim says now. \
                       NAMING NOTHING TO CHANGE IS REFUSED, NOT SILENTLY HONOURED: a call that \
                       sets none of the arguments above still reaches this verb, and a claim is \
                       never re-asserted by accident. keep IS THE DESIGNED WAY TO DO IT ON \
                       PURPOSE — set keep: true and nothing else to keep this claim exactly as it \
                       stands while moving when it was last touched. keep IS REFUSED IF ANYTHING \
                       ELSE HERE WOULD ACTUALLY CHANGE THE CLAIM: there is no partial keep, only \
                       this alone or an ordinary edit naming what changes.")]
    pub(crate) async fn update_fact(
        &self,
        Parameters(args): Parameters<UpdateFactArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let declared = Declared::of(&args);
        let mut cleared = args.clear_fields.clone().unwrap_or_default();
        let mut fields = args.fields.unwrap_or_default();
        // **Captured before anything computed joins `fields`**, for the same
        // reason `capture`'s own copy is: this is about what the CALLER
        // sent, never what jojobot added on its own.
        let sent_field_keys: Vec<String> = fields.keys().cloned().collect();
        // **The thing a ceiling binds cannot write that ceiling — checked on
        // the fold, atomically with the write, inside `self.memory.update_fact`
        // below.** A raw check here on `fields` alone would miss a clear, a
        // status change to archived, or a key that differs from the ceiling's
        // only by whitespace, because none of those name the key in what
        // THIS write sends — see `refuses_own_ceiling_change`.
        // **Kept current here too.** An edit that moves a cadence, a policy or
        // a basis is a write like any other write that could move the due
        // moment — the mechanism does not care that this one is a patch
        // rather than a fresh capture.
        let due_on_computed = self
            .moved_due_moment(&address.home, &fields, &cleared)
            .await;
        let due_on_set = matches!(due_on_computed, attention::DueMove::Set(_));
        match due_on_computed {
            attention::DueMove::Set(due_on) => {
                fields.insert(attention::DUE_ON.to_string(), due_on.to_string());
            }
            // **The clearing path calls the mover too, and acts on what it
            // says.** This is the case `capture` cannot reach: a clear is
            // the one write shape that can actually take a key off a
            // record, so it is the one that can take the stale due moment
            // off with it — onto the same `clear_fields` list this patch
            // already carries, so it is removed in the same write that
            // made it stale rather than in a write of its own.
            attention::DueMove::Cleared => cleared.push(attention::DUE_ON.to_string()),
            attention::DueMove::Unchanged => {}
        }
        let patch = FactPatch {
            content: args.content,
            details: args.details,
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
        // **A role's own two fields are the boot door's, whoever is asking.**
        // The claim and renewal paths reach the Memory trait directly, never
        // through this verb, so a write that reaches here naming either
        // field — set or cleared — is, by construction, not one of those
        // two. See `refuses_role_fields` and `capture`'s own copy of this
        // check.
        if let Some(refused) = jojobot_domain::memory::refuses_role_fields(
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
            let refused = carried
                .iter()
                .find(|fact| fact.id == address.local)
                .and_then(|fact| jojobot_domain::memory::refuses_role_fields(fact.fields.keys()));
            if let Some(refused) = refused {
                return memory_declined("update_fact", refused);
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
        // **A star or a seat count that would take the bot's boot over its
        // ceiling is refused here**, before anything lands — see
        // `refuses_a_boot_floor_over`.
        if let Some(refused) = self
            .refuses_a_boot_floor_for_edit(&address, &patch, &caller.bot)
            .await
        {
            return memory_declined("update_fact", refused);
        }
        // **The ageing cutoff, computed here and never inside `Memory`**, for
        // the reason `capture`'s own copy is. Asked only when this edit could
        // newly make the record a thought: it draws a connection edge or
        // brings the record back to active. Set after the empty-patch check
        // above, because it names no change.
        let mut patch = patch;
        if address.home.kind() == Some(EntityKind::BOT)
            && (patch
                .edge
                .as_ref()
                .is_some_and(|e| e.shape == EdgeShape::Connection)
                || patch.status == Some(FactStatus::Active))
        {
            match self.sessions.summaries_of(&address.home).await {
                Ok(runs) => {
                    patch.aged_before = jojobot_domain::memory::aging_cutoff(
                        &runs.iter().map(|r| r.started_at).collect::<Vec<_>>(),
                    );
                }
                Err(e) => return session_declined(e, caller.sid.as_str()),
            }
        }
        // **A write that landed is never reported as failed** (rule 130): see
        // `capture`'s own note on the same shape.
        let (written, fold_behind) =
            match self.memory.update_fact(&address, patch, &caller.bot).await {
                Ok(written) => (written, None),
                Err(MemoryError::FoldBehind {
                    landed: Landed::Fact(fact),
                    behind,
                    ..
                }) => (Guarded::Written(*fact), Some(behind)),
                Err(e) => return memory_declined("update_fact", e),
            };
        match written {
            Guarded::Written(fact) => {
                self.beat(
                    "update_fact",
                    &fact.address().to_string(),
                    args.sid.as_deref(),
                )
                .await;
                let mut body = fact_receipt_json(&fact, self.dated(None, args.sid.as_deref())?);
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
                crate::answer::note_postcondition(
                    &mut body,
                    self.what_an_update_left_standing(&fact, &cleared, keep)
                        .await,
                );
                self.note_seat_pushed_off(&fact, &mut body).await;
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
                if !sent_field_keys.is_empty()
                    && let Some(update_fact_properties) =
                        crate::teaching::published_arguments("update_fact")
                    && let Some(capture_properties) =
                        crate::teaching::published_arguments("capture")
                    && let Some((shadowed, owning_verb)) = crate::teaching::shadowed_argument_verb(
                        sent_field_keys.iter().map(String::as_str),
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
    /// `update_fact` rewrites one record in place and touches no other. The
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
        format!(
            "{address} now states what this call sent, in place of what it said before.{kept}\
             {beside}{removed}{half_cleared}{instead}{source_note}"
        )
    }

    /// **When a rewrite was probably the wrong move, say what the right one
    /// is — once, on the receipt, and never as a refusal.**
    ///
    /// Neither reason a past-event claim turns into its negation is a
    /// rewrite. A record that was never true is archived, with a note
    /// saying why — `retract` does the same and keeps a dated account
    /// beside it. A record that was true and changed is archived too, and
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
             is a rewrite. If the record was never true, archive it instead — status: archived, \
             details saying why — or retract, which does the same and keeps a dated account \
             beside it. If it was true and has changed, archive it and capture the new claim; \
             overwriting it here loses the day it stopped being true.",
        )
    }

    /// **Reads the rhythm's own fields, after this write landed.** A
    /// schedule is fields folded from whichever of a rhythm's records last
    /// wrote them, so this asks the thing itself rather than the record
    /// just archived — the same question `moved_due_moment` computes
    /// forward, asked backward: is a due moment still standing now.
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

#[cfg(test)]
mod tests;
