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
    let dates: Vec<String> = groups
        .iter()
        .filter(|group| group.len() == 1)
        .flatten()
        .cloned()
        .collect();
    let mut sentences: Vec<String> = Vec::new();
    if !dates.is_empty() {
        sentences.push(format!(
            "A date under {} makes a thing fall due on that day.",
            named(&dates, "or"),
        ));
    }
    for group in groups.iter().filter(|group| group.len() > 1) {
        sentences.push(format!(
            "{} together set when a thing falls due.",
            named(group, "and"),
        ));
    }
    let reads = attention::due_reads(carriers);
    if !reads.is_empty() {
        sentences.push(format!(
            "{} set no day and still change what a thing owes.",
            named(&reads, "and"),
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
        "Every other key is kept as you wrote it and nothing acts on it: it never makes a \
         thing fall due or appear in what is owed."
            .to_string(),
    );
    sentences.join(" ")
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
            Err(_) => return (attention::DueMove::Unchanged, false),
        };
        let existing_due_on = projected
            .get(attention::DUE_ON)
            .and_then(|v| v.trim().parse().ok());
        for key in cleared {
            projected.remove(key);
        }
        projected.extend(incoming.iter().map(|(k, v)| (k.clone(), v.clone())));
        (
            attention::moved_due_moment_as(&kind, &carriers, existing_due_on, &projected),
            attention::due_is_derived(&carriers, &projected),
        )
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
        let way_out = match ending {
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
                 that set a due day are {}. To move it, change one of them. {way_out}",
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
