//! **What a caller's own capture did to the thing it named**, said on the receipt.
//! Moved out of `capture.rs` so that file does not grow (rule 597).

use jojobot_domain::attention;

use super::capture::{WHY_THIS_BASIS_IS_HAND_TYPED, WHY_THIS_BASIS_WAS_DERIVED};
use super::*;

impl Jojobot {
    /// **The keys a check-in on a rhythm writes**, or the refusal that says why
    /// it cannot.
    ///
    /// **What a caller's own capture did to the thing it named.**
    ///
    /// `capture` appends. It edits no record and it removes none, which is the
    /// property an agent has been observed not to believe — one declined to
    /// record a second account of an event because it expected the first to be
    /// overwritten, and the account was lost with nothing on the surface
    /// saying otherwise.
    ///
    /// **The keys are the half that keeps the line honest.** A record carrying
    /// fields folds into what the thing holds, so what those keys answer has
    /// moved even though no record was touched. Naming them is what stops
    /// *nothing was changed* being a promise this verb cannot keep.
    ///
    /// The count is read for this line and left out when the store cannot
    /// answer, because a number nobody can stand behind is worse than the
    /// sentence without one.
    ///
    /// **`checked_in` is what tells this apart from the case
    /// [`WHY_A_CHECK_IN_DERIVES`] already covers.** A rhythm's `counts_from`
    /// sent by hand with no check-in is not a difference — the stored value
    /// IS what the caller sent, so [`Difference::between`] has nothing to
    /// compare — and it is not caught anywhere else either, since a
    /// half-taught session or one that ignored the teaching still reaches
    /// `capture` directly. This is the receipt's own chance to say so.
    pub(super) async fn what_a_capture_left_standing(
        &self,
        fact: &Fact,
        writer: &EntityId,
        checked_in: bool,
        opened_the_loop: bool,
        reads: &super::reads_left::ReadsLeft,
    ) -> String {
        // **A claim list that cannot be read is named, never counted as none**: the
        // count of accounts, and the reserve notes below, need it.
        let after = match self.memory.recall(&fact.subject).await {
            Ok(after) => Some(after),
            Err(e) => {
                reads.left(super::reads_left::ACCOUNT_COUNT, &fact.subject, &e);
                None
            }
        };
        let standing = after
            .as_ref()
            .map(|facts| {
                facts
                    .iter()
                    .filter(|f| f.status == FactStatus::Active)
                    .count()
            })
            .map_or_else(String::new, |n| {
                format!(" {n} accounts now stand on {}.", fact.subject.as_str())
            });
        // **The emergency reserve is a VISIBLE debt, not a silent one.** A
        // thought landing while its own room already holds more than its
        // ceiling is the one moment this receipt can say so — a later read
        // of the room shows the overage structurally, but only this call
        // knows it JUST happened. Asked only when this fact could be a
        // room's own member at all, the same gate the write itself ran on.
        // **Decided from the input the write used**: the caller's bot against the
        // subject, by the domain's one test. The record the store returns is read
        // back, and a read of a claim cannot always name the run that wrote it.
        let a_thought = fact
            .edge
            .as_ref()
            .is_some_and(|e| e.shape == jojobot_domain::memory::EdgeShape::Connection)
            && !jojobot_domain::memory::authorship::another_bots_claim(
                &fact.subject,
                Some(writer),
                |_| false,
            );
        let debt = if a_thought {
            let fields = match self.memory.fields(&fact.subject).await {
                Ok(fields) => Some(fields),
                Err(e) => {
                    reads.left(super::reads_left::RESERVE_NOTE, &fact.subject, &e);
                    None
                }
            };
            if after.is_none() {
                reads.left_for(super::reads_left::RESERVE_NOTE, &fact.subject);
            }
            // **The room is counted the way a room read counts it**, so a thought
            // gone quiet is outside the count here as it is there, and a write
            // that needed no reserve does not claim one.
            let counted = match (&fields, &after) {
                (Some(fields), Some(facts)) => {
                    self.count_room_of(&fact.subject, facts, fields).await
                }
                _ => None,
            };
            match counted {
                Some(view) if view.count.over() => {
                    // **The write has landed, so an unreadable run list is told
                    // and never declined** (rule 130): the count then leaves no
                    // thought out as quiet, and the receipt says so.
                    let unread = if view.ages_unread {
                        " The bot's runs could not be read, so no thought was counted as \
                         quiet: the room may hold fewer live thoughts than this."
                    } else {
                        ""
                    };
                    format!(
                        " This used {}'s emergency reserve: its room now holds {} of \
                         {}, over its own ceiling.{unread} Bring it back by archiving a live \
                         thought with update_fact — the priority now is steering back to a \
                         clean state, not borrowing again.",
                        fact.subject.as_str(),
                        view.count.live.len(),
                        view.count.capacity,
                    )
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        // **The same debt, on the record ceiling.** A write that landed on a
        // thing already holding as many live records as its ceiling allows is the
        // one moment this receipt can say it spent the reserve.
        let record_fields = match self.memory.fields(&fact.subject).await {
            Ok(fields) => Some(fields),
            Err(e) => {
                reads.left(super::reads_left::RECORD_RESERVE_NOTE, &fact.subject, &e);
                None
            }
        };
        if after.is_none() {
            reads.left_for(super::reads_left::RECORD_RESERVE_NOTE, &fact.subject);
        }
        let record_debt = match (&after, record_fields) {
            (Some(facts), Some(fields)) => {
                let ceiling =
                    jojobot_domain::memory::capacity::ceiling_of(fact.subject.kind(), &fields);
                let live = jojobot_domain::memory::capacity::live_records(facts).len();
                match ceiling {
                    Some(ceiling) if live > ceiling.limit => format!(
                        " This used {}'s emergency reserve: it now holds {} live records of \
                         {}, over its own ceiling. Bring it back by archiving a live record \
                         with update_fact — the priority now is steering back to a clean \
                         state, not borrowing again.",
                        fact.subject.as_str(),
                        live,
                        ceiling.limit,
                    ),
                    _ => String::new(),
                }
            }
            _ => String::new(),
        };
        let debt = format!("{debt}{record_debt}");
        // **Keys that reached the subject, told apart from keys that stayed.**
        // The six that describe their own claim never fold onto the thing, so a
        // receipt that said every key had moved to it told a bot that starred a
        // rule that the star was now the bot's. The set is the shipped
        // `record-labels` type, so a seventh key is placed with no edit here.
        // A key written as the record's own field stays on it too once the store
        // holds own fields off the thing: the store says which, by the class it kept.
        let reach = self.reach_of(fact).await;
        let join = |keys: &[String]| keys.join(", ");
        let keys = {
            let mut said = String::new();
            if !reach.reached.is_empty() {
                said.push_str(&format!(
                    " This record carries {}, so what those keys answer for {} has moved to \
                     it; the records that set them are untouched and still say what they said.",
                    join(&reach.reached),
                    fact.subject.as_str(),
                ));
            }
            if !reach.describing.is_empty() {
                said.push_str(&format!(
                    " {} stay on this record only: they describe it and do not fold onto {}. \
                     Read them with facts: true.",
                    join(&reach.describing),
                    fact.subject.as_str(),
                ));
            }
            if !reach.kept.is_empty() {
                said.push_str(&format!(
                    " {} stay on this record only: they were sent as its own fields, and own \
                     fields do not fold onto {}. Send a key in `sets` to put it on the thing. \
                     Read them with facts: true.",
                    join(&reach.kept),
                    fact.subject.as_str(),
                ));
            }
            said
        };
        let basis = if opened_the_loop {
            let derived = fact
                .fields
                .get(attention::COUNTS_FROM)
                .map(String::as_str)
                .unwrap_or_default();
            format!(" {WHY_THIS_BASIS_WAS_DERIVED} {derived}. Later cycles advance from it.")
        } else if !checked_in
            && fact.subject.kind() == Some(EntityKind::RHYTHM)
            && fact.fields.contains_key(attention::COUNTS_FROM)
        {
            format!(" {WHY_THIS_BASIS_IS_HAND_TYPED}")
        } else {
            String::new()
        };
        // **The policy this rhythm got because it named none.** A write that
        // sends a cadence and no `advances_from` is a whole schedule, and the
        // receipt says which default it applied so a caller is never left to
        // find out at a late check-in.
        let policy = if fact.subject.kind() == Some(EntityKind::RHYTHM)
            && (fact.fields.contains_key(attention::CADENCE_DAYS)
                || fact.fields.contains_key(attention::RRULE))
        {
            match self.held_by(&fact.subject).await {
                Ok(held) => attention::defaulted_advances_from(&held)
                    .map(|applied| {
                        format!(
                            " This rhythm names no `advances_from`, so jojobot applied the default \
                             for its cadence: {}. Send `advances_from` to choose the other.",
                            applied.as_token()
                        )
                    })
                    .unwrap_or_default(),
                // **A read that fails is named**, never a rhythm that got no policy.
                Err(e) => {
                    reads.left(super::reads_left::POLICY_NOTE, &fact.subject, &e);
                    String::new()
                }
            }
        } else {
            String::new()
        };
        // **Archive is a visibility switch, not a validity gate** — citing an
        // archived claim as derived_from is permitted, so the caller has to
        // be told rather than left to notice on a later read. Silence here
        // would be the refusal this slice removed, arriving anyway as an
        // omission.
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
            "Recorded as an additional claim.{standing} No record was edited and none was \
             removed.{keys}{basis}{policy}{source_note}{debt}"
        )
    }
}
