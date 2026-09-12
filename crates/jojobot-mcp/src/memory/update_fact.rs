//! `update_fact` — Correct an addressed fact in place — the source, never an addendum.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;
use crate::teaching::{
    CLAIM_SUBJECT_DOMAIN, CLAIM_SUBJECT_TEACHING, CLAIMS_DOMAIN, CLAIMS_TEACHING,
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
    /// Replacement details; pass an empty string to clear them. Carries
    /// `@kind:slug` mentions exactly as `content` does.
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
                       different edits and a caller means one of them. AND IT REACHES THE \
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
                       only what the claim says now.")]
    pub(crate) async fn update_fact(
        &self,
        Parameters(args): Parameters<UpdateFactArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let address = FactAddress::parse(&args.address).map_err(memory_error)?;
        let declared = Declared::of(&args);
        let cleared = args.clear_fields.clone().unwrap_or_default();
        let patch = FactPatch {
            content: args.content,
            details: args.details,
            recorded_at: parse_date(args.recorded_at.as_deref())?,
            happened_at: parse_date(args.happened_at.as_deref())?,
            clear_happened_at: args.clear_happened_at.unwrap_or(false),
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            confirmed_by_user: args.confirmed_by_user.unwrap_or(false),
            fields: args.fields.unwrap_or_default(),
            clear_fields: args.clear_fields.unwrap_or_default(),
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
        };
        let written = match self.memory.update_fact(&address, patch).await {
            Ok(written) => written,
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
                crate::answer::note_delta(&mut body, declared.not_stored(&fact));
                crate::answer::note_postcondition(
                    &mut body,
                    self.what_an_update_left_standing(&fact, &cleared).await,
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
    async fn what_an_update_left_standing(&self, fact: &Fact, cleared: &[String]) -> String {
        let address = fact.address().to_string();
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
             {beside}{removed}{instead}{source_note}"
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
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use jojobot_domain::memory::types::{Field, ValueType};

    /// 🚨 **A rewrite says what it did NOT destroy**, because a caller that
    /// does not know keeps its hands off.
    ///
    /// The line said the words a rewrite replaced were "not kept", and while
    /// that was true a session met a conflicting claim, would not overwrite it
    /// on a guess, and wrote nothing at all. A claim's writes are kept now and
    /// a caller can read them back — and a surface that does not say so leaves
    /// the caution in place with nothing behind it.
    ///
    /// The needle is the ARGUMENT that reads them, not a sentence: a phrase
    /// breaks when the wording improves and proves nothing.
    #[tokio::test]
    async fn a_rewrite_says_the_words_it_replaced_are_still_readable() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("works at the new place".into()),
                    ..update_args("person:alpha#f1")
                }))
                .await
                .expect("update ok"),
        );
        let said = edited["postcondition"]
            .as_str()
            .expect("an edit answers with a postcondition");
        assert!(
            said.contains("history_record"),
            "the receipt does not say how to read what the claim used to say: {said}"
        );
        assert!(
            said.contains("person:alpha#f1"),
            "…and it names the record to ask for: {said}"
        );
        assert!(
            !said.contains("not kept"),
            "the receipt still says the old words are gone, and they are not: {said}"
        );
    }

    /// 🚨 **Negating a claim about something that already happened names the
    /// other path, at the moment somebody is reading a receipt.**
    ///
    /// A run told that somebody was never at an event rewrote the claim in
    /// place, twice, in two paid runs — the shape rule 58 taught before it
    /// was killed. **The past does not change**, so a claim about a past
    /// event turning into its own negation is the one shape where the edit
    /// is usually the wrong verb: archive, with retract or an ordinary edit
    /// setting the status, is what either reason for it calls for now.
    ///
    /// ⛔️ **It is a line, never a gate.** A word search for a negation can be
    /// wrong in either direction, so refusing on it would be worse than the
    /// defect it catches.
    ///
    /// ⚠️ **The paired negative is what keeps the line worth reading**: an
    /// ordinary rewrite, and a negation of a claim that is not about a past
    /// event, both come back without it. A receipt that always says it is a
    /// receipt nobody reads.
    #[tokio::test]
    async fn negating_a_past_event_names_retraction_and_nothing_else_does() {
        let jojobot = handler();
        ensure(&jojobot, "event:leaving-party").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some("2026-03-01".into()),
                shape: Some("attendance".into()),
                object: Some("event:leaving-party".into()),
                ..capture_args("alpha", "was at the leaving party")
            },
        )
        .await;
        // An ordinary claim on the same thing, about no event at all.
        capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some("2026-03-01".into()),
                ..capture_args("alpha", "closes the shop at six")
            },
        )
        .await;

        let said = async |address: &str, content: &str| {
            json_of(
                &jojobot
                    .update_fact(Parameters(UpdateFactArgs {
                        content: Some(content.into()),
                        ..update_args(address)
                    }))
                    .await
                    .expect("update ok"),
            )["postcondition"]
                .as_str()
                .expect("an edit answers with a postcondition")
                .to_string()
        };

        let negated = said("person:alpha#f1", "was NOT at the leaving party").await;
        assert!(
            negated.contains("retract"),
            "a claim about a past event was negated and the other path went unnamed: {negated}"
        );

        // ⚠️ **The negative, on the same store.** A claim about no event,
        // negated: the world changed, which is what a rewrite is for.
        let ordinary = said("person:alpha#f2", "does NOT close the shop at six").await;
        assert!(
            !ordinary.contains("retract"),
            "every negation names retraction, so the line says nothing: {ordinary}"
        );

        // A contraction says no exactly as the word does, and a caller writes
        // one as readily. **On a claim of its own**, because f1 already says
        // NOT: negating what is already negative adds nothing.
        capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some("2026-03-01".into()),
                shape: Some("attendance".into()),
                object: Some("event:leaving-party".into()),
                ..capture_args("alpha", "stayed to the end of the leaving party")
            },
        )
        .await;
        let shortened = said("person:alpha#f3", "wasn't there at the end").await;
        assert!(
            shortened.contains("retract"),
            "a contraction negates and went unnoticed: {shortened}"
        );

        // …and an ordinary rewrite of the event claim, which is not a negation.
        let reworded = said("person:alpha#f1", "was at the leaving party, briefly").await;
        assert!(
            !reworded.contains("retract"),
            "a rewrite that negates nothing named retraction: {reworded}"
        );
    }

    /// **An edit says what it replaced and what it left alone.**
    ///
    /// This is the verb that makes `capture`'s line worth believing. `capture`
    /// appends and `update_fact` rewrites in place, so a postcondition that
    /// read *nothing was changed* on both would be a false promise on one of
    /// them — and a false promise in the one place a caller has been taught to
    /// trust is worse than no line at all.
    ///
    /// **Three shapes in one case, because the line is computed from the
    /// patch**: a rewrite names the record it replaced and counts what it left
    /// alone; a clear names the keys it took off; a rewrite that clears nothing
    /// names none. A constant cannot produce all three.
    #[tokio::test]
    async fn an_update_says_what_it_replaced_and_what_it_left_alone() {
        let jojobot = handler();
        let first = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("mood".to_string(), "delighted".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "said the kiln was lit")
            },
        )
        .await;
        capture_ok(
            &jojobot,
            capture_args("person:alpha", "said the flue was the problem"),
        )
        .await;
        let address = address_of(&first);

        let rewritten = update_ok(
            &jojobot,
            UpdateFactArgs {
                content: Some("said the kiln was NOT lit — confirmed".into()),
                ..update_args(&address)
            },
        )
        .await;
        let replaced = postcondition_line(&rewritten);
        assert!(
            replaced.contains(&address),
            "the line has to name the record this call rewrote: {rewritten}",
        );
        assert!(
            replaced.contains('1'),
            "…and how much on this thing it left alone: {rewritten}",
        );
        assert!(
            !replaced.contains("mood"),
            "this call took no key off, so the line names none: {rewritten}",
        );

        let cleared = update_ok(
            &jojobot,
            UpdateFactArgs {
                clear_fields: Some(vec!["mood".into()]),
                ..update_args(&address)
            },
        )
        .await;
        assert!(
            postcondition_line(&cleared).contains("mood"),
            "this call DID remove something, and a line that cannot say so is the false promise \
             the computed line exists to avoid: {cleared}",
        );
    }

    /// The postcondition line of a receipt, which every write carries.
    fn postcondition_line(body: &serde_json::Value) -> String {
        body["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {body}"))
            .to_string()
    }

    /// Update through the handler, expecting the guard to wave it through.
    async fn update_ok(jojobot: &Jojobot, args: UpdateFactArgs) -> serde_json::Value {
        let body = json_of(
            &jojobot
                .update_fact(Parameters(args))
                .await
                .expect("update_fact ok"),
        );
        assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
        body
    }

    /// **The clause about what was left alone reads as English, and vanishes
    /// when there is nothing to leave alone.**
    ///
    /// A real model read *"The 0 other records on thing:floor-pump are as they
    /// were"* off this line in a paid run — a reassurance about an empty set,
    /// on the one call in that year where the agent overwrote an account it
    /// should have kept. **Three edges, because the count decides both the
    /// noun and the verb**, and a line that announces itself as generated
    /// spends the trust it was added to build.
    #[tokio::test]
    async fn what_an_edit_left_alone_reads_as_english_at_every_count() {
        let jojobot = handler();
        let only = capture_ok(
            &jojobot,
            capture_args("person:alpha", "said the kiln was lit"),
        )
        .await;

        // Nothing else stands on the thing, so the clause is not there at all.
        let alone = update_ok(
            &jojobot,
            UpdateFactArgs {
                content: Some("said the kiln was NOT lit".into()),
                ..update_args(&address_of(&only))
            },
        )
        .await;
        let line = postcondition_line(&alone);
        assert!(
            !line.contains(" 0 "),
            "a reassurance about an empty set: {line}",
        );

        capture_ok(
            &jojobot,
            capture_args("person:alpha", "and the flue was blocked"),
        )
        .await;
        let one = postcondition_line(
            &update_ok(
                &jojobot,
                UpdateFactArgs {
                    content: Some("the kiln was cold all week".into()),
                    ..update_args(&address_of(&only))
                },
            )
            .await,
        );
        assert!(
            one.contains("1 other record is") || one.contains("1 other record on person:alpha is"),
            "singular noun with a plural verb: {one}",
        );

        capture_ok(&jojobot, capture_args("person:alpha", "and the door stuck")).await;
        let two = postcondition_line(
            &update_ok(
                &jojobot,
                UpdateFactArgs {
                    content: Some("the kiln is lit again".into()),
                    ..update_args(&address_of(&only))
                },
            )
            .await,
        );
        assert!(
            two.contains("2 other records") && two.contains("are as they were"),
            "…and the plural still has to be plural: {two}",
        );
    }

    /// **A field is set and cleared in place, and a plain recall shows it.**
    ///
    /// Edit-in-place is the surface the model puts in front of an agent: it
    /// edits and it sees the record change. This is that surface reaching the
    /// one part of a record it could not reach — a record's fields could only
    /// be written by the call that created it, so a key that turned out wrong
    /// meant a second record beside the first.
    ///
    /// **Set and clear are separate arguments** rather than one bag where an
    /// empty value means "remove". An empty value is a value somebody wrote,
    /// and the two moves must not be spelled the same.
    /// **A clear that would drop the thing below a type it answers is blocked,
    /// and the same clear on a thing that answers nothing goes through.**
    ///
    /// Strict is a floor: what a type asks for has to survive. The pairing is
    /// the point — a build that refused both would pass the first half and
    /// make a half-described thing unrepairable, which is the failure the rule
    /// is shaped to avoid.
    #[tokio::test]
    async fn a_clear_that_would_break_a_fit_is_blocked_and_says_what_it_would_cost() {
        let jojobot = handler();
        writing_as(&jojobot);
        // **A KIND, not a declared type.** What a write may take off a thing is
        // its own kind's question; a declared type is the vocabulary a caller
        // asks with and gates nothing.
        jojobot
            .memory
            .declare_kind(
                "thing",
                jojobot_domain::memory::types::Origin::Shipped,
                vec![
                    Field::required("cost", ValueType::Number),
                    Field::required("done_on", ValueType::Date),
                ],
            )
            .await
            .expect("a kind may name the keys its things keep");
        // A shipped kind rather than a new one: declaring a new kind fills the
        // set this process parses against, and every case beside this one that
        // stands a store up empties it again.

        let whole = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cost".to_string(), "40".to_string()),
                        ("done_on".to_string(), "2026-04-18".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("thing:gravel-bike", "the annual service")
            },
        )
        .await;
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["cost".into()]),
                    ..update_args(&address_of(&whole))
                }))
                .await
                .expect("a refusal is an answer, not a protocol failure"),
        );
        let advice = refused["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal carries its way forward: {refused}"));
        assert!(
            advice.contains("thing") && advice.contains("cost"),
            "the refusal names the kind and the key it would cost: {advice}"
        );
        assert_eq!(refused["wrote"], false, "{refused}");

        // The same clear, on a thing that answers no type: served. Nothing
        // here is protecting anything, and a record nobody can repair is worse
        // than a record with a key missing.
        let loose = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("cost".to_string(), "40".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("thing:road-bike", "somebody wrote down a price")
            },
        )
        .await;
        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["cost".into()]),
                    ..update_args(&address_of(&loose))
                }))
                .await
                .expect("update ok"),
        );
        assert_ne!(
            edited["status"], "blocked",
            "a thing that fits nothing has nothing to protect: {edited}"
        );
        assert_eq!(
            edited["fields_count"], 0,
            "…so the key comes off, and the count is what says so: {edited}"
        );
    }

    #[tokio::test]
    async fn update_fact_sets_and_clears_a_field() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cost".to_string(), "40".to_string()),
                        ("done_on".to_string(), "2026-04-18".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("alpha", "the annual service")
            },
        )
        .await;
        let address = address_of(&captured);

        let set = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some(
                        [("cost".to_string(), "45".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        // **The receipt counts the keys rather than reading them back.** Two
        // is the answer to both halves at once: the key named was rewritten
        // rather than added, and the key the patch did not name is still
        // there. What each one HOLDS is read below, off the record.
        assert_eq!(set["fields_count"], 2, "{set}");

        let cleared = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec!["done_on".into()]),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            cleared["fields_count"], 1,
            "the key named is gone and the other is not: {cleared}"
        );

        // …and both moves are on the record a later read takes, which is what
        // makes editing in place true rather than an answer's shape.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["fields"],
            serde_json::json!({"cost": "45"}),
            "{recalled}"
        );
    }

    /// **The key jojobot writes itself is refused here too.** A verb that
    /// could set `retracts` would be a way to mark somebody else's record
    /// taken back without going through the verb that decides whether it may
    /// be — so the gate is on both write paths, not on the one somebody
    /// thought of first.
    #[tokio::test]
    async fn update_fact_refuses_the_reserved_key() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;

        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some(
                        [("retracts".to_string(), "person:alpha#f1".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .expect("advice")
                .contains("retracts"),
            "the refusal names the key: {refused}"
        );
    }

    /// 🚨 **`clear_edge` takes the edge off, and an edit that never mentions
    /// edges leaves it alone** — through the surface a caller holds.
    ///
    /// **The second half is the load-bearing one.** A build where every rewrite
    /// silently dropped the edge would pass the first and be a worse defect
    /// than the one this fixes: correcting a typo would cost the link and
    /// nobody would be told.
    ///
    /// The rest of the patch still lands, so an empty edge is the argument
    /// doing its work rather than the whole edit failing quietly.
    #[tokio::test]
    async fn clear_edge_takes_the_edge_off_and_a_silent_edit_does_not() {
        let jojobot = handler();
        ensure(&jojobot, "event:winter-fest").await;
        let drawn = |said: &str| CaptureArgs {
            shape: Some("attendance".into()),
            object: Some("event:winter-fest".into()),
            ..capture_args("person:alpha", said)
        };

        let kept = capture_ok(&jojobot, drawn("was at the winter fest")).await;
        let reworded = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("was at the winter fest, both nights".into()),
                    ..update_args(&address_of(&kept))
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            reworded["edge"]["object"], "event:winter-fest",
            "an edit that never mentioned edges took one off: {reworded}",
        );

        let wrong = capture_ok(&jojobot, drawn("was at the winter fest")).await;
        let corrected = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("was never at the winter fest".into()),
                    clear_edge: Some(true),
                    ..update_args(&address_of(&wrong))
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            corrected["edge"].is_null(),
            "the edge is still on a claim that now denies it: {corrected}",
        );
        assert_eq!(
            corrected["status"], "active",
            "taking the edge off is not a retraction: {corrected}",
        );
        assert_eq!(
            corrected["content_bytes"].as_u64(),
            Some("was never at the winter fest".len() as u64),
            "the rewrite itself did not land: {corrected}",
        );
    }

    /// `update_fact` attaches an edge to a fact that didn't have one.
    #[tokio::test]
    async fn update_fact_attaches_an_edge() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "was at the festival")).await;
        assert!(captured["edge"].is_null());
        ensure(&jojobot, "event:winter-fest").await;

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    shape: Some("attendance".into()),
                    object: Some("event:winter-fest".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["edge"]["type"], "attendee");
        assert_eq!(updated["edge"]["object"], "event:winter-fest");
    }

    /// 🚨 **A correction can carry the day it was made, and omitting it leaves
    /// the record's day untouched.**
    ///
    /// `update_fact` had no way to say when a rewrite happened, so a claim
    /// captured under one day and corrected under a later one kept the
    /// original day forever — the one thing a later reader most wants about a
    /// correction had no argument to carry it. `retract` already took a date
    /// for exactly this reason; this is the same argument on the verb the
    /// caller is actually sent to for the common case of correcting a claim
    /// about what is true now.
    ///
    /// **Both halves.** A day given is the day carried, and no day given
    /// leaves the day the claim already held — without the second, a build
    /// that always overwrote the day with today (or dropped the argument on
    /// the floor) would satisfy the first alone.
    #[tokio::test]
    async fn a_correction_carries_the_day_it_is_given_and_leaves_it_otherwise() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                recorded_at: Some("2026-06-01".into()),
                ..capture_args("alpha", "the club meets on Tuesdays")
            },
        )
        .await;
        let address = address_of(&captured);
        assert_eq!(captured["recorded_at"], "2026-06-01");

        let redated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("the club meets on Wednesdays".into()),
                    recorded_at: Some("2026-08-15".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            redated["recorded_at"], "2026-08-15",
            "a correction given a day must carry that day rather than the day of the claim it \
             replaces: {redated}"
        );

        let untouched = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("the club meets on Thursdays".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            untouched["recorded_at"], "2026-08-15",
            "an edit naming no day must leave the record's existing day alone: {untouched}"
        );
    }

    /// **A refutation is a content edit, and `negated` is refused by name.** The
    /// rewritten row stays `active` and keeps its address — the negative truth is
    /// the current truth, so it has to be what a plain read returns. Asking for
    /// the retired status is a client error that says what to do instead, rather
    /// than an alias that would file the correction where nobody looks.
    #[tokio::test]
    async fn a_refutation_is_a_content_edit_and_negated_is_refused() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            capture_args("alpha", "a close contact of the user"),
        )
        .await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("negated".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect_err("the retired status must be refused, not aliased");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("rewrite"),
            "the error must say what to do instead: {}",
            err.message
        );

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("NOT a close contact — do not re-infer".into()),
                    ..update_args(&address_of(&captured))
                }))
                .await
                .expect("the refutation is an ordinary edit"),
        );
        assert_eq!(
            updated["status"], "active",
            "the negative truth is the truth"
        );
        assert_eq!(
            updated["content_elided"], true,
            "the refutation is not read back to whoever just wrote it: {updated}"
        );
        assert_eq!(
            updated["address"], "person:alpha#f1",
            "the row keeps its address"
        );
        // **The edit is proven where it landed, and the receipt cannot prove
        // it.** The answer says the write happened; only a read says what the
        // record now holds — and a case named for a content edit that asserts
        // nothing about the content passes on a build where the edit is
        // dropped.
        let read = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            read["objects"][0]["facts"][0]["content"], "NOT a close contact — do not re-infer",
            "the refutation is what the record says now: {read}"
        );
    }

    /// Promotion to testimony needs the explicit confirmation flag.
    #[tokio::test]
    async fn promoting_to_testimony_requires_the_confirmation_flag() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "prefers mornings")).await;
        let promote = |confirmed: Option<bool>| UpdateFactArgs {
            provenance: Some("testimony".into()),
            confirmed_by_user: confirmed,
            ..update_args(&address_of(&captured))
        };

        // Refused as a blocked ANSWER: the call is well formed and jojobot is
        // declining to bless a claim the operator has not blessed, which is a
        // next move rather than a failure (rule 68).
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(promote(None)))
                .await
                .expect("an unconfirmed promotion is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("confirmed_by_user"),
            "the way forward names the flag the operator's word unlocks: {advice}"
        );

        let ok = json_of(
            &jojobot
                .update_fact(Parameters(promote(Some(true))))
                .await
                .expect("a confirmed promotion is allowed"),
        );
        assert_eq!(ok["provenance"], "testimony");
    }

    /// **The confirmation gate is on PROMOTION, not on assertion**, and this
    /// is the scope the surface has to state.
    ///
    /// `check_standing` fires on one move only: an existing OPEN claim being
    /// made SETTLED. A fresh capture may declare `settled` on an inference and
    /// is accepted — standing is declared on honour exactly as provenance is,
    /// and the gate is on promotion rather than on assertion, by design.
    ///
    /// Both halves in one read. The refusal alone reads as "settling is
    /// guarded" and the acceptance alone reads as a hole in the gate; it is
    /// the pair that says where the line actually is, and a reader of the
    /// surface who has only one of them believes the wrong thing.
    #[tokio::test]
    async fn the_confirmation_gate_is_on_promotion_and_not_on_assertion() {
        let jojobot = handler();

        // Asserted, and ungated: nobody is asked to confirm this.
        let asserted = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("inference".into()),
                standing: Some("settled".into()),
                ..capture_args("alpha", "shuts early on sundays")
            },
        )
        .await;
        assert_eq!(asserted["provenance"], "inference", "{asserted}");
        assert_eq!(
            asserted["standing"], "settled",
            "a capture declares its standing on honour: {asserted}"
        );

        // Promoted, and gated: the operator hedged this one, so only the
        // operator withdraws the hedge.
        let hedged = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                standing: Some("open".into()),
                ..capture_args("alpha", "thinks the ferry moved")
            },
        )
        .await;
        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    standing: Some("settled".into()),
                    ..update_args(&address_of(&hedged))
                }))
                .await
                .expect("an unconfirmed settling is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// **A malformed address and a missed one are different answers**, and
    /// never a new fact. Malformed is the caller writing something that is not
    /// an address at all — a protocol error. Missed is a well-formed address
    /// naming nothing, which is the same "you named what does not exist" every
    /// gate answers, so it wears the blocked shape and carries the addresses
    /// that do exist.
    #[tokio::test]
    async fn a_malformed_address_errors_and_a_missed_one_is_blocked() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "the only fact here")).await;

        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                content: Some("nope".into()),
                ..update_args("not-an-address")
            }))
            .await
            .expect_err("a string that is no address is a malformed call");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        let missed = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("nope".into()),
                    ..update_args("person:alpha#f99")
                }))
                .await
                .expect("an address that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:alpha#f99");
        let advice = missed["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("person:alpha#f1"),
            "the addresses that DO exist are what makes this repairable: {advice}"
        );
        let body = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            body["objects"][0]["facts"].as_array().unwrap().len(),
            1,
            "nothing was created"
        );
    }

    /// An unknown status token is a client error, not a silently-active fact.
    #[tokio::test]
    async fn an_unknown_status_is_a_client_error() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "a claim")).await;
        let err = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                status: Some("retired".into()),
                ..update_args(&address_of(&captured))
            }))
            .await
            .expect_err("must reject an unknown status");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// 🚨 **The made-on field's served description must not carry the
    /// happened-on field's definition.** `recorded_at` answers when a claim
    /// was said, decided or worked out; `happened_at` answers when the thing
    /// it describes occurred. "True of [a day]" is this codebase's own
    /// phrase for the second question — a description of `recorded_at` that
    /// uses it is printing the wrong field's definition on this field's
    /// name.
    ///
    /// Checked on both surfaces a caller can read it from: the argument's own
    /// schema description and the tool-level prose.
    #[test]
    fn the_recorded_at_argument_does_not_carry_happened_ats_definition() {
        let tools = Jojobot::tool_router().list_all();
        let update_fact = tools
            .iter()
            .find(|t| t.name.as_ref() == "update_fact")
            .expect("update_fact is a tool");
        let schema =
            serde_json::to_value(&update_fact.input_schema).expect("the schema serializes");
        let recorded_at = schema["properties"]["recorded_at"]["description"]
            .as_str()
            .expect("recorded_at carries its own description")
            .to_lowercase();
        assert!(
            !recorded_at.contains("true of"),
            "the made-on field's schema description carries the happened-on field's definition: \
             {recorded_at}"
        );
        let tool_description = update_fact.description.as_deref().unwrap_or_default();
        assert!(
            !tool_description.to_lowercase().contains("true of"),
            "the tool-level description carries the happened-on field's definition on the \
             wrong field: {tool_description}"
        );
    }

    /// **A session whose whole claim-writing life is edits still learns the
    /// subject/purpose convention.** `capture` taught it; `update_fact` did
    /// not, so a session that only ever edits — the one write path that
    /// actually takes `fields`/`clear_fields` and can write those keys —
    /// never met it.
    #[tokio::test]
    async fn a_session_that_only_ever_edits_is_taught_the_subject_convention() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "plays go")).await;

        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("plays go and chess".into()),
                    sid: Some(sid),
                    ..update_args("person:alpha#f1")
                }))
                .await
                .expect("update ok"),
        );
        assert!(
            edited["teaching"]
                .as_array()
                .expect("a list")
                .contains(&serde_json::json!(CLAIM_SUBJECT_TEACHING)),
            "a session that only edits never learns the subject/purpose convention: {edited}"
        );
    }

    /// 🚨 **A mark set through `update_fact` reads back through `recall`, and
    /// an ordinary field wearing the same name does not become one.**
    ///
    /// Paired in the same case, per the dispatch: without the second half, a
    /// build where any key named `stands_for` counted as the mark would pass
    /// the first half alone.
    #[tokio::test]
    async fn a_mark_set_through_update_fact_reads_back_and_an_ordinary_field_is_not_it() {
        let jojobot = handler();
        let first = capture_ok(&jojobot, capture_args("alpha", "said the kiln was lit")).await;
        let second = capture_ok(&jojobot, capture_args("alpha", "and the flue was blocked")).await;
        let synthesis = capture_ok(
            &jojobot,
            capture_args("alpha", "the kiln trouble is resolved now"),
        )
        .await;
        let first_address = address_of(&first);
        let second_address = address_of(&second);
        let synthesis_address = address_of(&synthesis);

        let written = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    stands_for: Some(vec![first_address.clone(), second_address.clone()]),
                    ..update_args(&synthesis_address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            written["stands_for"],
            serde_json::json!([first_address, second_address]),
            "the receipt does not show the mark it was just given: {written}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        let facts = recalled["objects"][0]["facts"].as_array().expect("facts");
        let synthesis_read = facts
            .iter()
            .find(|f| f["address"] == synthesis_address)
            .expect("the synthesis record is still there");
        assert_eq!(
            synthesis_read["stands_for"],
            serde_json::json!([first_address, second_address]),
            "a plain recall does not show the mark that was set: {synthesis_read}"
        );

        // An ordinary field of the same name, on a record that never got the
        // mark: it stays a field, and the dedicated line stays empty.
        let plain = capture_ok(&jojobot, capture_args("alpha", "an unrelated claim")).await;
        let plain_address = address_of(&plain);
        let with_field = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    fields: Some(
                        [("stands_for".to_string(), "not a mark".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..update_args(&plain_address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            with_field["stands_for"],
            serde_json::json!([]),
            "an ordinary field named stands_for was read as the mark: {with_field}"
        );
    }

    /// **A mark must name claims that exist**, exactly as `derived_from` does
    /// — refused with the addresses that do exist, never a new fact.
    #[tokio::test]
    async fn a_stands_for_must_name_claims_that_exist() {
        let jojobot = handler();
        let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
        let source_address = address_of(&source);
        let synthesis =
            capture_ok(&jojobot, capture_args("alpha", "so the crossing is longer")).await;

        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    stands_for: Some(vec!["person:alpha#f99".into()]),
                    ..update_args(&address_of(&synthesis))
                }))
                .await
                .expect("a miss is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains(&source_address)),
            "the addresses that DO exist are what makes it repairable: {refused}"
        );
    }

    /// ⭐ **An empty set is refused rather than stored** — a mark with no
    /// sources would read back as an ordinary record, indistinguishable from
    /// one that never got a mark at all.
    #[tokio::test]
    async fn stands_for_refuses_an_empty_set() {
        let jojobot = handler();
        let synthesis = capture_ok(&jojobot, capture_args("alpha", "a record on its own")).await;

        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    stands_for: Some(vec![]),
                    ..update_args(&address_of(&synthesis))
                }))
                .await
                .expect("an empty set is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// A mark naming its own record's address is refused.
    #[tokio::test]
    async fn stands_for_refuses_its_own_address() {
        let jojobot = handler();
        let synthesis = capture_ok(&jojobot, capture_args("alpha", "a record on its own")).await;
        let address = address_of(&synthesis);

        let refused = blocked(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    stands_for: Some(vec![address.clone()]),
                    ..update_args(&address)
                }))
                .await
                .expect("self-reference is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// `clear_stands_for` takes the mark off, and the sources it named stay
    /// exactly as they were — active, and readable on their own address.
    #[tokio::test]
    async fn clear_stands_for_takes_the_mark_off_and_leaves_the_sources_alone() {
        let jojobot = handler();
        let source = capture_ok(&jojobot, capture_args("alpha", "said the kiln was lit")).await;
        let source_address = address_of(&source);
        let synthesis = capture_ok(&jojobot, capture_args("alpha", "resolved now")).await;
        let synthesis_address = address_of(&synthesis);

        update_ok(
            &jojobot,
            UpdateFactArgs {
                stands_for: Some(vec![source_address.clone()]),
                ..update_args(&synthesis_address)
            },
        )
        .await;

        let cleared = update_ok(
            &jojobot,
            UpdateFactArgs {
                clear_stands_for: Some(true),
                ..update_args(&synthesis_address)
            },
        )
        .await;
        assert_eq!(
            cleared["stands_for"],
            serde_json::json!([]),
            "the mark is still there after clear_stands_for: {cleared}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        let source_read = recalled["objects"][0]["facts"]
            .as_array()
            .expect("facts")
            .iter()
            .find(|f| f["address"] == source_address)
            .expect("the source record is still there");
        assert_eq!(
            source_read["status"], "active",
            "the source is untouched by the mark being cleared: {source_read}"
        );
    }

    /// 🚨 **Discoverability: the verb's own description names the
    /// argument.** A capability whose only path is that somebody read the
    /// diff has no path.
    #[test]
    fn stands_for_is_named_on_the_verbs_own_description() {
        let tools = Jojobot::tool_router().list_all();
        let update_fact = tools
            .iter()
            .find(|t| t.name.as_ref() == "update_fact")
            .expect("update_fact is a tool");
        let tool_description = update_fact.description.as_deref().unwrap_or_default();
        assert!(
            tool_description.contains("stands_for"),
            "the tool-level description does not name the argument: {tool_description}"
        );
        let schema =
            serde_json::to_value(&update_fact.input_schema).expect("the schema serializes");
        assert!(
            schema["properties"]["stands_for"]["description"]
                .as_str()
                .is_some_and(|d| !d.is_empty()),
            "stands_for carries no schema description of its own: {schema}"
        );
    }
}
