//! **The stored due moment, kept current by any write that could move it.**
//!
//! `capture` and `update_fact` both call through here after assembling the
//! fields they are about to write and before committing: if those fields (or
//! a clear) touch a key any carrier's interface names, this reads the
//! thing's current fields, projects what they will read once the write
//! lands, and hands back what should happen to [`attention::DUE_ON`] — set
//! to a new date, cleared, or left alone.

use super::*;
use jojobot_domain::attention;

/// **Which keys make a thing fall due, in words**, built from the carriers so a
/// carrier added later is named with no edit here.
///
/// A carrier whose interface is one key is a date that is the day itself, and
/// those are named together. A carrier of several keys sets the day from all of
/// them, and each group is named as a group. The last sentence says what every
/// other key is, because the whole text exists for the model that reads "a key
/// you invent is kept" and takes it to mean the key will be acted on.
///
/// Empty when there are no carriers: a sentence about no keys would teach
/// nothing, and a caller that asks for one must not publish a stub.
pub(crate) fn what_makes_a_thing_fall_due(carriers: &[&dyn attention::Carrier]) -> String {
    fn named(keys: &[String], last: &str) -> String {
        let quoted: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
        match quoted.split_last() {
            None => String::new(),
            Some((only, [])) => only.clone(),
            Some((tail, rest)) => format!("{} {last} {tail}", rest.join(", ")),
        }
    }
    let groups = attention::due_key_groups(carriers);
    if groups.iter().all(Vec::is_empty) {
        return String::new();
    }
    // **A word a carrier reads beside its keys**, named where the keys are, so a
    // caller is told which word acts on which thing: `ended` beside a date, and
    // `outcome` beside a loop's schedule. A word that is the carrier's way to end
    // ends it; any other changes it.
    let reads = attention::due_group_reads(carriers);
    let beside = |words: Vec<(String, bool)>, one: &str| -> String {
        let mut shown: Vec<(String, bool)> = Vec::new();
        for (word, ends) in words {
            if !shown.iter().any(|(seen, _)| *seen == word) {
                shown.push((word, ends));
            }
        }
        shown
            .iter()
            .map(|(word, ends)| {
                format!(
                    "; `{word}` beside {one} {} it",
                    if *ends { "ends" } else { "changes" }
                )
            })
            .collect()
    };
    let dates: Vec<String> = groups
        .iter()
        .filter(|group| group.len() == 1)
        .flatten()
        .cloned()
        .collect();
    let mut sentences: Vec<String> = Vec::new();
    if !dates.is_empty() {
        // **The bag is named in the first sentence**: a key sent under `fields`
        // stays on its claim and moves nothing, so a caller told which keys act
        // has to be told where they act.
        let single_reads: Vec<(String, bool)> = groups
            .iter()
            .zip(&reads)
            .filter(|(group, _)| group.len() == 1)
            .flat_map(|(_, words)| words.clone())
            .collect();
        sentences.push(format!(
            "A date under {} in `sets` makes a thing fall due on that day{}.",
            named(&dates, "or"),
            beside(single_reads, "one"),
        ));
    }
    for (group, words) in groups
        .iter()
        .zip(&reads)
        .filter(|(group, _)| group.len() > 1)
    {
        sentences.push(format!(
            "{} together set when a thing falls due{}.",
            named(group, "and"),
            beside(words.clone(), "them"),
        ));
    }
    // **A finished piece of work is not owed**, so `status` on those kinds acts
    // on what is owed. The kinds and the word come from the shipped kinds, the
    // same source the owed read asks.
    let finishing: Vec<String> = jojobot_domain::memory::kinds::finishing_kinds()
        .into_iter()
        .map(str::to_string)
        .collect();
    if !finishing.is_empty() {
        sentences.push(format!(
            "A {} whose `{}` is `{}` is not owed, and its dates stay on it.",
            named(&finishing, "or"),
            jojobot_domain::memory::kinds::STATUS,
            jojobot_domain::memory::kinds::FINISHED,
        ));
    }
    sentences.push(
        "Every other key is kept as written and acts on nothing: it never makes a thing fall \
         due or appear in what is owed."
            .to_string(),
    );
    sentences.join(" ")
}

/// **The keys that describe their own claim, named for the model writing one.**
/// They never fold onto the thing the claim is about, so the sentence also says
/// how to read them back and how to find a claim by them. The names come from
/// the shipped `record-labels` type, so a seventh is named with no edit.
///
/// Empty when there are none: a sentence about no keys would teach nothing.
pub(crate) fn what_describes_its_claim(keys: &[String]) -> String {
    let quoted: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
    let Some((tail, rest)) = quoted.split_last() else {
        return String::new();
    };
    let named = if rest.is_empty() {
        tail.clone()
    } else {
        format!("{} and {tail}", rest.join(", "))
    };
    format!(
        "{named} describe their own claim and never fold onto the thing: read them back with \
         `facts: true`, and find by them with a `fields` filter with `scope: record`."
    )
}

/// **Where each key of one record went**: see [`Jojobot::reach_of`].
#[derive(Debug, Default)]
pub(crate) struct Reach {
    /// Keys that describe their record and never fold onto the thing.
    pub(crate) describing: Vec<String>,
    /// Keys written as the record's own fields, which the store holds off the thing.
    pub(crate) kept: Vec<String>,
    /// Keys that reached the thing.
    pub(crate) reached: Vec<String>,
}

impl Jojobot {
    /// **What this write should do to the stored due moment.**
    /// [`attention::DueMove::Unchanged`] covers both "this write cannot move
    /// anything" (skipped before any read) and "it would not move it",
    /// since neither has anything for a caller to add or take off.
    ///
    /// **The second answer is whether the moved day is jojobot's arithmetic**
    /// rather than a copy of a date the caller wrote: only then does the record
    /// carrying it stop being the caller's word.
    ///
    /// A read failure here answers `Unchanged` too, silently: this is
    /// jojobot's own bookkeeping riding along on somebody else's write, and
    /// it must never be the reason that write fails.
    pub(crate) async fn moved_due_moment(
        &self,
        subject: &EntityId,
        incoming: &std::collections::BTreeMap<String, String>,
        cleared: &[String],
        today: jiff::civil::Date,
    ) -> (attention::DueMove, bool) {
        let carriers = self.carriers();
        // **The keys a carrier's answer reads, not only the ones that make a
        // thing its business** — see `Carrier::also_reads`.
        let watched = |c: &dyn attention::Carrier, key: &str| {
            c.interface().fields.iter().any(|f| f.key == key) || c.also_reads().contains(&key)
        };
        // **A work item or a project is finished at `done`**, so its status is
        // watched too: a write that moves it into or out of `done` moves the
        // stored moment off or back.
        let kind = subject
            .kind()
            .map(|kind| kind.as_token().to_string())
            .unwrap_or_default();
        let finishes = jojobot_domain::memory::kinds::holds_columns(&kind);
        let status = jojobot_domain::memory::kinds::STATUS;
        let touches_incoming = carriers
            .iter()
            .any(|c| incoming.keys().any(|key| watched(*c, key)))
            || (finishes && incoming.contains_key(status));
        let touches_cleared = carriers
            .iter()
            .any(|c| cleared.iter().any(|key| watched(*c, key)))
            || (finishes && cleared.iter().any(|key| key == status));
        if !touches_incoming && !touches_cleared {
            return (attention::DueMove::Unchanged, false);
        }
        let mut projected = match self.memory.fields(subject).await {
            Ok(fields) => fields,
            // **A thing that does not exist yet holds nothing**, so the first claim
            // of a creation is worked out over what it sends alone. A capture on a
            // handle nobody holds is refused by the store, so this changes no
            // answer there.
            Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }) => {
                std::collections::BTreeMap::new()
            }
            Err(_) => return (attention::DueMove::Unchanged, false),
        };
        let held = projected.clone();
        let instance = self.instance_fields().await;
        let existing_due_on = projected
            .get(attention::DUE_ON)
            .and_then(|v| v.trim().parse().ok());
        for key in cleared {
            projected.remove(key);
        }
        projected.extend(incoming.iter().map(|(k, v)| (k.clone(), v.clone())));
        (
            attention::moved_due_moment_as(
                &kind,
                &carriers,
                existing_due_on,
                &held,
                &projected,
                &instance,
                today,
            ),
            attention::due_is_derived(&carriers, &projected),
        )
    }

    /// **Take the stored due moment off a thing, through the edit path's own
    /// `clear_fields`**, on the active record that carries it.
    ///
    /// A clear is scoped to the record it addresses, and a capture's own record
    /// does not carry the key, so the clear goes to the record that does. A fold
    /// keeps the newest write of a key and a clear is a write, so the moment is
    /// off the thing from this one on while every earlier write of it stays
    /// readable. **It never fails the capture that asked for it**: the capture
    /// has landed, and this is jojobot's own bookkeeping riding along on
    /// somebody else's write, as `moved_due_moment` is. A failure is logged.
    pub(crate) async fn clear_stored_due_moment(
        &self,
        subject: &EntityId,
        bot: &EntityId,
        session: &str,
    ) -> bool {
        let carrier = match self.memory.recall(subject).await {
            Ok(facts) => facts.into_iter().find(|fact| {
                fact.status == FactStatus::Active && fact.fields.contains_key(attention::DUE_ON)
            }),
            Err(e) => {
                tracing::warn!(error = %e, %subject, "the stored due moment could not be found");
                return false;
            }
        };
        let Some(carrier) = carrier else { return true };
        let patch = FactPatch {
            clear_fields: vec![attention::DUE_ON.to_string()],
            session: Some(session.to_string()),
            ..Default::default()
        };
        // **The second part of a capture that finishes a thing**, written once
        // under a `request_id`.
        use jojobot_domain::request::{Part, part};
        match part(
            "clear_due",
            None,
            self.memory.update_fact(&carrier.address(), patch, bot),
        )
        .await
        {
            Part::Ran(Err(e)) => {
                tracing::warn!(
                    error = %e,
                    address = %carrier.address(),
                    "the stored due moment could not be taken off a finished thing",
                );
                false
            }
            Part::Ran(Ok(_)) | Part::Recorded(_) => true,
        }
    }

    /// **What a write's receipt says about reach.** The keys of the claim that
    /// reached the thing it is about, told apart from the ones that stayed on the
    /// record because they only describe it or were sent as its own fields; and,
    /// for a key the caller sent as one of the claim's own fields that the store
    /// held off the thing and that the thing's kind or a carrier reads off the
    /// thing, a hint that it belongs in `sets`.
    ///
    /// `own_sent` is the keys this call sent as the claim's own fields, so a key
    /// already in the setting bag, or one a call did not send, is never hinted. A
    /// key the store let reach (a record from before own fields stayed) is not
    /// hinted either: it reached.
    pub(crate) async fn note_reach_and_hint(
        &self,
        body: &mut serde_json::Value,
        fact: &Fact,
        own_sent: &[String],
    ) {
        let reach = self.reach_of(fact).await;
        let stayed: Vec<String> = reach
            .describing
            .iter()
            .chain(&reach.kept)
            .cloned()
            .collect();
        let read = self.keys_read_off_a_thing(&fact.subject);
        let to_move: Vec<String> = own_sent
            .iter()
            .filter(|key| reach.kept.contains(*key) && read.contains(*key))
            .cloned()
            .collect();
        // **Told first, for every key this call sent as an own field that stayed**
        // — the keys that only label a claim are not in `kept`, and "send it in
        // sets" would be wrong for them.
        let told: Vec<&String> = own_sent
            .iter()
            .filter(|key| reach.kept.contains(*key))
            .collect();
        crate::answer::note_keys_stayed_off_the_thing(body, &fact.subject, &told);
        crate::answer::note_reach(body, reach.reached, stayed);
        if to_move.is_empty() {
            return;
        }
        let named = to_move
            .iter()
            .map(|key| key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        crate::answer::note_hint(
            body,
            format!(
                "{named} sent as the claim's own fields, and the thing's kind or a carrier reads \
                 {} off the thing. A claim's own fields stay on the claim, so {} did not reach \
                 the thing: send {} in `sets` instead.",
                if to_move.len() == 1 { "it" } else { "them" },
                if to_move.len() == 1 { "it" } else { "they" },
                if to_move.len() == 1 { "it" } else { "them" },
            ),
        );
    }

    /// **Says on the receipt that a view narrows nothing on its own.** A write
    /// that leaves a view holding none of the keys that narrow it still lands: a
    /// caller's own kind, subject or fields win beside a view, so a view that only
    /// shows or orders is a call a caller can make. Asked as it stands it is
    /// refused, and this tells the writer so while the keys can still be added.
    /// The keys are the ones the refusal reads (`graph::Narrowing`). Nothing is
    /// said about any other kind, nor when the store cannot answer.
    pub(crate) async fn note_view_narrows_nothing(
        &self,
        body: &mut serde_json::Value,
        subject: &EntityId,
    ) {
        use jojobot_domain::memory::graph::{Narrowing, view_narrows};
        if subject.kind() != Some(EntityKind::VIEW) {
            return;
        }
        let Ok(held) = self.memory.fields(subject).await else {
            return;
        };
        if view_narrows(&held) {
            return;
        }
        if let Some(fields) = body.as_object_mut() {
            fields.insert(
                "narrows_nothing_note".into(),
                format!(
                    "{subject} holds none of the keys that narrow a view ({}), so asked as it \
                     is, it is refused. A kind, subject, parent, answers_type or fields sent \
                     beside it narrow it for that call.",
                    Narrowing::view_keys().join(", ")
                )
                .into(),
            );
        }
    }

    /// **What a thing holds right now**, read before a record is taken off it so
    /// [`Jojobot::note_what_left`] can say what the taking-off cost. `None` when
    /// the store could not answer: a receipt then says nothing about it, because
    /// an empty list would claim that no key left.
    pub(crate) async fn held_by(
        &self,
        subject: &EntityId,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        self.memory.fields(subject).await.ok()
    }

    /// **The keys the thing held before a record was archived or retracted and
    /// does not hold now.** A key the record shared with an earlier one goes back
    /// to that record's value and has not left; a key the record alone set is
    /// gone; a key that stayed on the record was never on the thing.
    pub(crate) async fn note_what_left(
        &self,
        body: &mut serde_json::Value,
        subject: &EntityId,
        before: Option<std::collections::BTreeMap<String, String>>,
    ) {
        let (Some(before), Some(after)) = (before, self.held_by(subject).await) else {
            return;
        };
        let left = before
            .keys()
            .filter(|key| !after.contains_key(*key))
            .cloned()
            .collect();
        crate::answer::note_left_the_thing(body, left);
    }

    /// **Where each key of a record went**, from what the store kept rather than a
    /// list of names: the keys that only describe the record, the keys the store
    /// holds off the thing because they were written as its own fields, and the
    /// keys that reached.
    ///
    /// A store that cannot answer leaves the second group empty and says so in the
    /// log: the write has landed (rule 130), and a receipt that failed the call
    /// would report a landed write as lost.
    pub(crate) async fn reach_of(&self, fact: &Fact) -> Reach {
        let describing = crate::seed::describing_keys();
        let kept_off = match self.memory.keys_kept_off_the_thing(&fact.address()).await {
            Ok(kept) => kept,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    address = %fact.address(),
                    "the keys held off the thing could not be read for a receipt",
                );
                Default::default()
            }
        };
        let mut reach = Reach::default();
        for key in fact.fields.keys() {
            if describing.contains(key) {
                reach.describing.push(key.clone());
            } else if kept_off.contains(key) {
                reach.kept.push(key.clone());
            } else {
                reach.reached.push(key.clone());
            }
        }
        reach
    }

    /// **The keys a thing of this subject's kind, or a carrier, reads off the
    /// thing**: the keys its kind declares, and the keys that make it fall due.
    fn keys_read_off_a_thing(&self, subject: &EntityId) -> std::collections::BTreeSet<String> {
        let mut keys: std::collections::BTreeSet<String> = subject
            .kind()
            .map(|kind| {
                jojobot_domain::memory::kinds::keys_of(kind.as_token())
                    .into_iter()
                    .map(|field| field.key)
                    .collect()
            })
            .unwrap_or_default();
        let carriers = self.carriers();
        keys.extend(attention::due_keys(&carriers));
        for carrier in &carriers {
            keys.extend(carrier.also_reads().iter().map(|key| key.to_string()));
        }
        keys
    }

    /// **The refusal for a caller that writes or clears the stored due moment.**
    ///
    /// It is jojobot's own: set from the keys that make a thing fall due, kept
    /// current by every write to them. A hand-written copy is not read by the
    /// per-kind question and a cleared one drops the thing out of the cross-kind
    /// one, so the two reads then disagree about what is owed. The way to move
    /// or remove it is to change or clear the key it comes from.
    ///
    /// `sent` and `cleared` are what the CALLER sent, taken before jojobot adds
    /// its own computed copy.
    ///
    /// **A clear is let through when no carrier answers for the thing.** The
    /// refusal tells the caller to clear the key the moment came from, and a
    /// thing holding a `due_on` with no such key has none to clear: the stored
    /// moment is a leftover of a write the deployed code once allowed, and the
    /// advice would loop. A write of it is refused either way. A store that
    /// cannot be read answers as before, with the refusal.
    pub(crate) async fn refuses_a_hand_written_due_moment(
        &self,
        subject: &EntityId,
        sent: &std::collections::BTreeMap<String, String>,
        cleared: &[String],
    ) -> Option<CallToolResult> {
        let key = attention::DUE_ON;
        let writes = sent.contains_key(key);
        if !writes && !cleared.iter().any(|k| k == key) {
            return None;
        }
        let held = self.memory.fields(subject).await.ok();
        let carriers = self.carriers();
        if !writes
            && let Some(held) = &held
            && !carriers
                .iter()
                .any(|carrier| carrier.interface().matched_by(held).is_some())
        {
            return None;
        }
        // **The carrier that answers for what the thing holds once this write
        // lands**, so the way out named is the one that works for it.
        let mut projected = held.unwrap_or_default();
        projected.extend(sent.iter().map(|(k, v)| (k.clone(), v.clone())));
        let ending = carriers
            .iter()
            .find(|carrier| carrier.interface().matched_by(&projected).is_some())
            .and_then(|carrier| carrier.ending());
        let setting = attention::due_keys(&carriers);
        let finished = if jojobot_domain::memory::kinds::holds_columns(subject.kind_token()) {
            format!(
                " If the work is finished, set `{}` to `{}`: it keeps its dates and stops \
                 falling due.",
                jojobot_domain::memory::kinds::STATUS,
                jojobot_domain::memory::kinds::FINISHED,
            )
        } else {
            String::new()
        };
        let way_out = match ending {
            // **The day is optional, so both routes work and only one keeps it.**
            // Ending is named first, because clearing the source erases the date.
            Some(ending) if ending.any_word => format!(
                "To end the owing and keep the day, write '{}' with a word saying how it \
                 ended, for example {}: the day stays on the thing and jojobot removes '{key}' \
                 with it. To remove it and erase the day too, clear the one it came from, and \
                 jojobot removes '{key}' with it.",
                ending.key,
                ending.words.join(", "),
            ),
            Some(ending) => format!(
                "To remove it, write '{}' as one of {}: this one keeps its day, so ending it is \
                 how it stops falling due, and jojobot removes '{key}' with it.",
                ending.key,
                ending.words.join(", "),
            ),
            None => format!(
                "To remove it, clear the one it came from, and jojobot removes '{key}' with it."
            ),
        };
        Some(blocked_body(
            subject,
            &[],
            format!(
                "Nothing was written. '{key}' is jojobot's own key and a caller does not {}. \
                 jojobot sets it from the day a thing carries and keeps it current. The keys \
                 that set a due day are {}. To move it, change one of them. {way_out}{finished}",
                if writes { "write it" } else { "clear it" },
                setting.join(", "),
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    use std::collections::BTreeMap;

    /// **The text says which word acts beside which keys.** `ended` is named
    /// beside the dates it ends, `outcome` beside the loop's schedule, and a
    /// word that is not the carrier's way to end is not said to end it. A
    /// generator that listed the words together would name both and pass a
    /// bare containment check, so the position is read.
    #[test]
    fn each_word_is_named_beside_the_keys_it_acts_with() {
        let shipped = attention::shipped();
        let carriers: Vec<&dyn attention::Carrier> =
            shipped.iter().map(std::convert::AsRef::as_ref).collect();
        let text = what_makes_a_thing_fall_due(&carriers);
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle}: {text}"))
        };
        let dates_end = at("`ended` beside one ends it");
        assert!(
            at("`runs_out`") < dates_end,
            "the dates come before their word: {text}"
        );
        let loop_word = at("`outcome` beside them changes it");
        assert!(
            at("`snoozed_until`") < loop_word && dates_end < loop_word,
            "a loop's word follows the loop's keys: {text}"
        );
    }

    /// A carrier the build does not ship, with one date key of its own.
    struct Pledges;

    impl attention::Carrier for Pledges {
        fn interface(&self) -> DeclaredType {
            DeclaredType::new("pledged", vec![Field::new("pledged_for", ValueType::Date)])
        }
        fn due(&self, _: &BTreeMap<String, String>) -> attention::Due {
            attention::Due::Never
        }
    }

    /// **A carrier added later is named with no edit to the text.**
    ///
    /// The shipped carriers are named, the carrier the build does not ship is
    /// named beside them, and a build with no carriers publishes nothing rather
    /// than a sentence about no keys. Both halves, so a generator that ignored
    /// its input and one that typed the shipped keys each fail.
    #[test]
    fn a_carrier_added_later_is_named_with_no_edit() {
        let shipped = attention::shipped();
        let mut carriers: Vec<&dyn attention::Carrier> =
            shipped.iter().map(std::convert::AsRef::as_ref).collect();
        let without = what_makes_a_thing_fall_due(&carriers);
        assert!(
            !without.is_empty() && !without.contains("`pledged_for`"),
            "the shipped carriers alone name the shipped keys and no other: {without}",
        );
        carriers.push(&Pledges);
        let with = what_makes_a_thing_fall_due(&carriers);
        assert!(
            with.contains("`pledged_for`"),
            "a carrier added to the list is named in the text built from it: {with}",
        );
        for key in attention::due_keys(&carriers) {
            assert!(
                with.contains(&format!("`{key}`")),
                "`{key}` is named: {with}"
            );
        }
        // **The keys that change what is owed without setting a day are named
        // too**, from the carriers and not typed here.
        for key in ["ended", "outcome"] {
            assert!(
                with.contains(&format!("`{key}`")) && without.contains(&format!("`{key}`")),
                "`{key}` changes what is owed, so the text names it: {with}",
            );
        }
        assert_eq!(
            what_makes_a_thing_fall_due(&[]),
            "",
            "no carriers, no sentence",
        );
    }
}
