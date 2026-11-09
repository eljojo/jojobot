//! **The one owner of what a capture receipt says beyond the facts of the
//! write.** The receipt's facts (what landed, what was not stored, what changed)
//! are its core and always ship. Every advisory part a capture adds is a named
//! layer here, and the composer decides which ship: at most one suggestion,
//! ranked, inside the bot's budget. A teaching note is outside that budget, so
//! a receipt whose core and teaching pass it ships both and drops the
//! suggestion. Teaching notes ride one array in ranked order.

use std::borrow::Cow;
use std::collections::BTreeSet;

use serde_json::Value;

use super::possible_subject::PossibleSubject;
use super::unlocks::OwedUnlock;
use crate::answer::compose::{
    Answer, Budget, Composed, Form, Kind, LEFT_OUT_KEY, Layer, LeftOut, Tier, account,
    compose_within,
};
use crate::{Jojobot, caller::Caller};

/// **A teaching note a capture would add, not yet spent.** Its gate has held;
/// whether it ships is the composer's choice, and its domain is spent only if
/// it does.
pub(crate) struct TeachingNote {
    /// The name an omission gives it.
    pub(crate) name: &'static str,
    /// The ledger domain spent when the note is served.
    pub(crate) domain: Cow<'static, str>,
    pub(crate) text: String,
    pub(crate) tier: Tier,
}

impl TeachingNote {
    pub(crate) fn new(
        name: &'static str,
        domain: impl Into<Cow<'static, str>>,
        text: impl Into<String>,
        tier: Tier,
    ) -> Self {
        TeachingNote {
            name,
            domain: domain.into(),
            text: text.into(),
            tier,
        }
    }
}

/// The key the teaching notes ride under, as every other verb serves them.
const TEACHING_SLOT: &str = "teaching";

impl Jojobot {
    /// **The skill offers an act makes, as teaching notes**: one per skill, the
    /// least urgent kind, each spent under its own domain only if served.
    pub(crate) async fn skill_offer_notes(
        &self,
        act: crate::orientation::skills::Act,
        caller: &Caller,
        notes: &mut Vec<TeachingNote>,
    ) {
        for (skill, domain, line) in self.skill_offers_for(act, caller).await {
            notes.push(TeachingNote::new(skill, domain, line, Tier::Low));
        }
    }

    /// **What a capture receipt serves, and the teachings it spends for it.** A
    /// part that is dropped spends nothing: a possible-subject note spends the
    /// rows of the things it names, a carry hint its domain, a type step its
    /// ledger key and a teaching note its domain, each only if the receipt still
    /// carries it once the composer has chosen. If another receipt claimed the
    /// teaching first, or its ledger write fails, the part comes out and the
    /// receipt is composed again. **Every spend happens after the last compose**,
    /// in one loop, since a smaller receipt can let a part ship that the first
    /// compose dropped; a part that already spent stays served, since a smaller
    /// set never drops a served layer.
    pub(crate) async fn own_capture_receipt(
        &self,
        mut body: Value,
        caller: &Caller,
        mut carry_hint: bool,
        owed: &[OwedUnlock],
        mut possible: Option<PossibleSubject>,
        mut notes: Vec<TeachingNote>,
    ) -> Value {
        // The type steps by ledger key and sentence, owned, so no borrow of the
        // caller's steps is held across a spend.
        let mut owed: Vec<(String, String)> = owed
            .iter()
            .map(|step| (step.key.clone(), step.sentence.clone()))
            .collect();
        let mut spent: BTreeSet<String> = BTreeSet::new();
        loop {
            let composed = compose_receipt(body.clone(), &notes);
            if let Some(current) = possible.clone()
                && composed.served.get("possible_subject_note").is_some()
            {
                let keys: Vec<String> = current.keys().map(str::to_string).collect();
                let lost = self.spend(keys, caller, &mut spent).await;
                if !lost.is_empty() {
                    let object = body
                        .as_object_mut()
                        .expect("a capture receipt is an object");
                    possible = current.without(&lost);
                    match &possible {
                        Some(rest) => {
                            object.insert("possible_subject_note".into(), rest.note().into());
                        }
                        None => {
                            object.remove("possible_subject_note");
                        }
                    }
                    continue;
                }
            }
            if carry_hint && composed.served.get("hint").is_some() {
                let key = super::list_carry::PLAN_CARRY_DOMAIN.to_string();
                if !self.spend(vec![key], caller, &mut spent).await.is_empty() {
                    body.as_object_mut()
                        .expect("a capture receipt is an object")
                        .remove("hint");
                    carry_hint = false;
                    continue;
                }
            }
            if composed.served.get("unlocks").is_some() && !owed.is_empty() {
                let keys: Vec<String> = owed.iter().map(|(key, _)| key.clone()).collect();
                let lost = self.spend(keys, caller, &mut spent).await;
                if !lost.is_empty() {
                    let sentences: Vec<String> = owed
                        .iter()
                        .filter(|(key, _)| lost.contains(key))
                        .map(|(_, sentence)| sentence.clone())
                        .collect();
                    let steps = body
                        .get_mut("unlocks")
                        .and_then(|block| block.get_mut("steps"))
                        .and_then(Value::as_array_mut)
                        .expect("the served block came from the body");
                    // One step goes for each lost key, so a sentence two keys share
                    // leaves the step of the key that was not lost.
                    for sentence in &sentences {
                        if let Some(at) = steps
                            .iter()
                            .position(|step| step["unlocks"].as_str() == Some(sentence))
                        {
                            steps.remove(at);
                        }
                    }
                    if steps.is_empty() {
                        body.as_object_mut()
                            .expect("a capture receipt is an object")
                            .remove("unlocks");
                    }
                    owed.retain(|(key, _)| !lost.contains(key));
                    continue;
                }
            }
            let left: BTreeSet<String> = composed
                .left_out
                .iter()
                .map(|omitted| omitted.layer.to_string())
                .collect();
            let mut lost_note: Option<usize> = None;
            let served_notes: Vec<(usize, String)> = notes
                .iter()
                .enumerate()
                .filter(|(_, note)| !left.contains(note.name))
                .map(|(at, note)| (at, note.domain.to_string()))
                .collect();
            for (at, domain) in served_notes {
                if !self
                    .spend(vec![domain], caller, &mut spent)
                    .await
                    .is_empty()
                {
                    lost_note = Some(at);
                    break;
                }
            }
            match lost_note {
                None => return composed.served,
                Some(at) => {
                    notes.remove(at);
                }
            }
        }
    }

    /// **Spend each ledger key this receipt has not spent**, and say which
    /// another receipt had spent first. A key this receipt spent stays spent.
    async fn spend(
        &self,
        keys: Vec<String>,
        caller: &Caller,
        spent: &mut BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut lost = BTreeSet::new();
        for key in keys {
            if spent.contains(&key) {
                continue;
            }
            if self.first_contact(&key, Some(caller)).await {
                spent.insert(key);
            } else {
                lost.insert(key);
            }
        }
        lost
    }
}

/// The advisory parts of a capture receipt, by the key each is served under.
/// The composer ranks by tier, then by cost, so inside a tier the cheaper
/// layer ranks first and declaration order breaks only an equal cost. A wrong
/// subject outranks every other suggestion, and what the write unlocked is the
/// least urgent.
/// **A fact about what the write did is not here**: a status outside the
/// vocabulary and a view that narrows nothing say how the write stands, so they
/// stay in the core and always ship.
const SUGGESTIONS: [(&str, Tier); 3] = [
    ("possible_subject_note", Tier::High),
    ("hint", Tier::Medium),
    ("unlocks", Tier::Low),
];

/// **The suggestion a ledger domain was spent for**, by the prefix each
/// suggestion's rows are written under. A domain no suggestion owns, such as a
/// teaching note's, answers `None`.
fn suggestion_of(domain: &str) -> Option<&'static str> {
    if domain.starts_with(super::possible_subject::NOTED_DOMAIN) {
        Some("possible_subject_note")
    } else if domain.starts_with(super::unlocks::TYPE_UNLOCKS_DOMAIN) {
        Some("unlocks")
    } else if domain == super::list_carry::PLAN_CARRY_DOMAIN {
        Some("hint")
    } else {
        None
    }
}

/// **The layers an account of what was left out names**, whichever of its three
/// forms it is written in.
fn layers_named(account: &Value) -> Vec<String> {
    let mut layers = Vec::new();
    let mut take = |value: &Value| {
        if let Some(layer) = value.as_str() {
            layers.push(layer.to_string());
        }
    };
    match account {
        Value::Array(entries) => {
            for entry in entries {
                take(&entry["layer"]);
                for layer in entry["layers"].as_array().into_iter().flatten() {
                    take(layer);
                }
            }
        }
        Value::Object(_) => {
            for layer in account["layers"].as_array().into_iter().flatten() {
                take(layer);
            }
        }
        _ => {}
    }
    layers
}

/// **A batch's merged receipt, composed once**: the items' notes were each
/// composed by their own item, so the merge can hold more than one suggestion.
/// The owner keeps the top-ranked one. The spends the batch held back come back
/// without the domains of every suggestion the receipt no longer carries, since
/// a suggestion nobody was shown stays owed.
pub(crate) fn own_batch_receipt(mut body: Value, spends: Vec<String>) -> (Value, Vec<String>) {
    let earlier = body
        .as_object_mut()
        .and_then(|core| core.remove(LEFT_OUT_KEY));
    let held: Vec<&'static str> = SUGGESTIONS
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| body.get(*key).is_some())
        .collect();
    let mut served = compose_receipt(body, &[]).served;
    let dropped: Vec<&'static str> = held
        .into_iter()
        .filter(|key| served.get(*key).is_none())
        .collect();
    // What the items left out stays named unless the receipt serves it after all.
    // **An account is read by the layers it names, in any of its three forms**,
    // and written back in the form the composer chose for this receipt.
    let made = served.get(LEFT_OUT_KEY).cloned();
    let mut named = made.as_ref().map(layers_named).unwrap_or_default();
    for layer in earlier.as_ref().map(layers_named).unwrap_or_default() {
        if served.get(&layer).is_none() && !named.contains(&layer) {
            named.push(layer);
        }
    }
    let omitted: Vec<LeftOut> = named
        .iter()
        .filter_map(|layer| SUGGESTIONS.iter().find(|(key, _)| key == layer))
        .map(|(key, _)| LeftOut {
            layer: key,
            call: RETURNED_BY,
        })
        .collect();
    if let Some(object) = served.as_object_mut()
        && !omitted.is_empty()
    {
        let form = match &made {
            Some(Value::Object(_)) => Form::Floor,
            Some(Value::Array(entries))
                if entries.iter().any(|entry| entry.get("layers").is_some()) =>
            {
                Form::Grouped
            }
            _ => Form::Full,
        };
        object.insert(LEFT_OUT_KEY.to_string(), account(&omitted, form));
    }
    let kept = spends
        .into_iter()
        .filter(|domain| suggestion_of(domain).is_none_or(|key| !dropped.contains(&key)))
        .collect();
    (served, kept)
}

/// The call that returns a left-out suggestion's ground: the thing it is about.
/// **Ground, not the suggestion itself**: `recall` does not return a hint or an
/// unlocks block, it returns the thing and its claims that the suggestion is
/// about, which is what a bot reads to judge the suggestion for itself.
const RETURNED_BY: &str = "recall";

/// **The finished receipt, composed**: its core whole, the one top-ranked
/// suggestion and the teaching notes in ranked order. Every other suggestion is
/// dropped without trace. The bot's setting picks the budget and the suggestion
/// is what it bounds: a teaching note is said once a session, its ledger bounds
/// it, and it ships whatever the budget says. Teaching already in the core ships first and unranked, as every
/// other verb's does.
fn compose_receipt(body: Value, notes: &[TeachingNote]) -> Composed {
    let Value::Object(mut core) = body else {
        return Composed::of(body);
    };
    let held = core.remove(TEACHING_SLOT);
    let mut layers: Vec<Layer> = SUGGESTIONS
        .iter()
        .filter_map(|(key, tier)| {
            let held = core.remove(*key)?;
            Some(Layer::new(key, Kind::Suggestion, *tier, RETURNED_BY, held))
        })
        .collect();
    layers.extend(notes.iter().map(|note| {
        Layer::new(
            note.name,
            Kind::Note,
            note.tier,
            RETURNED_BY,
            Value::String(note.text.clone()),
        )
        .taught_once(note.domain.clone())
        .in_slot(TEACHING_SLOT)
        .outside_the_budget()
    }));
    let mut composed = if layers.is_empty() {
        Composed::of(Value::Object(core))
    } else {
        compose_within(
            &Answer::new(Value::Object(core), layers),
            Budget::of_setting(crate::write_receipts::saving()).limit(),
            &BTreeSet::new(),
        )
    };
    if let (Some(Value::Array(held)), Value::Object(served)) = (held, &mut composed.served) {
        let shipped = served
            .remove(TEACHING_SLOT)
            .and_then(|notes| notes.as_array().cloned())
            .unwrap_or_default();
        served.insert(
            TEACHING_SLOT.to_string(),
            Value::Array(held.into_iter().chain(shipped).collect()),
        );
    }
    composed
}

#[cfg(test)]
mod tests {
    use super::TeachingNote;
    use crate::CaptureArgs;
    use crate::answer::compose::{LEAN_LIMIT, Tier};
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::memory::unlocks::OwedUnlock;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::{Value, json};

    fn the_caller() -> crate::caller::Caller {
        crate::caller::Caller {
            sid: crate::sid::Sid(TEST_RUN.into()),
            bot: jojobot_domain::memory::EntityId("bot:gamma".into()),
            card: None,
            zone: None,
            day: None,
        }
    }

    /// A receipt that carries an `unlocks` block of the steps given, by sentence.
    fn a_receipt_with_steps(sentences: &[&str]) -> Value {
        let steps: Vec<Value> = sentences
            .iter()
            .map(
                |sentence| json!({"type": "journey", "add": ["departs_from"], "unlocks": sentence}),
            )
            .collect();
        json!({"address": "thing:delta#f1", "unlocks": {"holds": {"keys": 1, "records": 1}, "steps": steps}})
    }

    fn owed(key: &str, sentence: &str) -> OwedUnlock {
        OwedUnlock {
            key: key.into(),
            sentence: sentence.into(),
        }
    }

    /// A batch's merged receipt: a note and a type step, with the marks its
    /// items left, and a core padded by `pad` characters.
    fn a_merged_batch_receipt(marks: Value, pad: usize) -> Value {
        let mut receipt = a_receipt_with_steps(&["Set returns_on in sets"]);
        receipt["possible_subject_note"] = json!("The words also name thing:delta.");
        receipt["filler"] = json!("x".repeat(pad));
        if !marks.is_null() {
            receipt["left_out"] = marks;
        }
        receipt
    }

    /// The layers an account names, in the order it names them.
    fn named_in(receipt: &Value) -> Vec<String> {
        super::layers_named(&receipt["left_out"])
    }

    /// **A batch names what its items left out unless the receipt serves it
    /// after all.** The note outranks the step, so the step is dropped and named;
    /// the item's mark for the hint stays; and an item's mark for the note, which
    /// the receipt serves, is not kept.
    #[test]
    fn a_batch_names_what_it_left_out_and_not_what_it_serves() {
        let marks = json!([
            {"layer": "hint", "call": "recall"},
            {"layer": "possible_subject_note", "call": "recall"},
        ]);
        let (served, _) = super::own_batch_receipt(a_merged_batch_receipt(marks, 0), Vec::new());
        assert!(served["possible_subject_note"].is_string(), "{served}");
        assert!(served.get("unlocks").is_none(), "{served}");
        let mut named = named_in(&served);
        named.sort();
        assert_eq!(named, ["hint", "unlocks"], "{served}");
    }

    /// **A mark an item left for a layer the batch serves is removed**, and with
    /// no layer left out the account goes.
    #[test]
    fn a_mark_for_a_layer_the_batch_serves_is_removed() {
        let mut receipt = a_receipt_with_steps(&["Set returns_on in sets"]);
        receipt["left_out"] = json!([{"layer": "unlocks", "call": "recall"}]);
        let (served, _) = super::own_batch_receipt(receipt, Vec::new());
        assert!(served["unlocks"].is_object(), "{served}");
        assert!(served.get("left_out").is_none(), "{served}");
    }

    /// **An item's compact account joins the batch's**: its layers are read
    /// whichever form it is written in.
    #[test]
    fn an_items_compact_account_joins_the_batchs_account() {
        for marks in [
            json!([{"call": "recall", "layers": ["hint"]}]),
            json!({"layers": ["hint"], "calls": "recall"}),
        ] {
            let (served, _) =
                super::own_batch_receipt(a_merged_batch_receipt(marks.clone(), 0), Vec::new());
            let mut named = named_in(&served);
            named.sort();
            assert_eq!(named, ["hint", "unlocks"], "{marks}: {served}");
        }
    }

    /// **A compact account the composer made stays compact, and keeps its
    /// marks beside the items'.** A core near the limit makes the composer write
    /// the names alone.
    #[test]
    fn a_compact_account_of_the_composer_keeps_its_marks_and_the_items() {
        let marks = json!([{"layer": "hint", "call": "recall"}]);
        let compact = (0..LEAN_LIMIT)
            .map(|pad| {
                super::own_batch_receipt(a_merged_batch_receipt(marks.clone(), pad), Vec::new()).0
            })
            .find(|served| served["left_out"].is_object())
            .expect("some core size makes the composer write a compact account");
        // The core is near the limit, so the composer may leave the note out too.
        let named = named_in(&compact);
        for layer in ["hint", "unlocks"] {
            assert!(named.iter().any(|name| name == layer), "{layer}: {compact}");
        }
        assert!(
            named.iter().all(|name| compact.get(name).is_none()),
            "a layer named as left out is not served: {compact}"
        );
    }

    /// **A step whose key another receipt spent first is taken out of the
    /// block**, and the steps whose keys are still free ship and are spent. A
    /// block left with no step goes with them.
    #[tokio::test]
    async fn a_step_another_receipt_spent_first_is_not_served() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact("type-unlocks:taken", Some(&caller))
                .await,
            "the other receipt spends the key first"
        );
        let both = jojobot
            .own_capture_receipt(
                a_receipt_with_steps(&["taken step", "free step"]),
                &caller,
                false,
                &[
                    owed("type-unlocks:taken", "taken step"),
                    owed("type-unlocks:free", "free step"),
                ],
                None,
                Vec::new(),
            )
            .await;
        let steps = both["unlocks"]["steps"]
            .as_array()
            .expect("a block with a step");
        assert_eq!(steps.len(), 1, "only the free step ships: {both}");
        assert_eq!(steps[0]["unlocks"], "free step", "{both}");
        assert!(
            !jojobot
                .first_contact("type-unlocks:free", Some(&caller))
                .await,
            "the step that shipped is spent"
        );

        let none = jojobot
            .own_capture_receipt(
                a_receipt_with_steps(&["taken step"]),
                &caller,
                false,
                &[owed("type-unlocks:taken", "taken step")],
                None,
                Vec::new(),
            )
            .await;
        assert!(
            none.get("unlocks").is_none(),
            "a block with no step left is not served: {none}"
        );
    }

    /// **A carry hint that is lost to another receipt lets the block ship, and
    /// the block then spends its keys.** The hint outranks the block, so the
    /// block is dropped while the hint stands; when the hint's spend fails the
    /// receipt is chosen again without it.
    #[tokio::test]
    async fn a_lost_carry_hint_lets_the_block_ship_and_spend() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact(crate::memory::list_carry::PLAN_CARRY_DOMAIN, Some(&caller))
                .await,
            "another receipt spent the carry teaching first"
        );
        let mut body = a_receipt_with_steps(&["step"]);
        body["hint"] = json!("carry the open entries");
        let served = jojobot
            .own_capture_receipt(
                body,
                &caller,
                true,
                &[owed("type-unlocks:step", "step")],
                None,
                Vec::new(),
            )
            .await;
        assert!(
            served.get("hint").is_none(),
            "the lost hint is not served: {served}"
        );
        assert_eq!(
            served["unlocks"]["steps"].as_array().map(Vec::len),
            Some(1),
            "the block ships once the hint is gone: {served}"
        );
        assert!(
            !jojobot
                .first_contact("type-unlocks:step", Some(&caller))
                .await,
            "the step that shipped is spent"
        );
    }

    /// **Two suggestions on one write: only the top-ranked ships, and the other
    /// is named in `left_out` with the call that returns it.** A heading written onto a document gets the
    /// "this is a claim about the document" hint, and its words name a known
    /// thing, which gets the "possible subject" note. A wrong subject outranks a
    /// feature hint. The keys are the wire's own and nothing outside this
    /// process declares them, so the literals are pinned here.
    #[tokio::test]
    async fn two_suggestions_on_one_write_ship_the_top_ranked_and_name_the_other() {
        let jojobot = handler();
        ensure(&jojobot, "thing:the-furnace").await;
        ensure(&jojobot, "document:alpha").await;
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        after_the_first_contact_teaching(&jojobot, &sid).await;
        let receipt = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(sid),
                    ..capture_args("document:alpha", "# The furnace's service is due in March")
                }))
                .await
                .expect("capture ok"),
        );
        assert_ne!(receipt["status"], "blocked", "the write lands: {receipt}");
        assert!(
            receipt["possible_subject_note"].is_string(),
            "the top-ranked suggestion ships: {receipt}"
        );
        assert!(
            receipt.get("hint").is_none(),
            "the lower suggestion does not: {receipt}"
        );
        assert_eq!(
            receipt["left_out"],
            json!([{"layer": "hint", "call": "recall"}]),
            "the lower suggestion is named with the call that returns it: {receipt}"
        );
    }

    /// **A fact about how the write stands ships beside a suggestion.** A status
    /// outside the vocabulary lands and counts as not finished, and the receipt
    /// says so in `status_word_note`. The words also name a known thing, which
    /// earns the suggestion. Both are served: the fact is core, never ranked.
    #[tokio::test]
    async fn a_status_word_note_ships_beside_a_suggestion() {
        let jojobot = handler();
        ensure(&jojobot, "thing:the-furnace").await;
        ensure(&jojobot, "work:atlas-root").await;
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;
        after_the_first_contact_teaching(&jojobot, &sid).await;
        let receipt = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    sid: Some(sid),
                    fields: Some([("status".to_string(), "nonsense".to_string())].into()),
                    ..capture_args("work:atlas-root", "The furnace's service is due in March")
                }))
                .await
                .expect("capture ok"),
        );
        assert!(
            receipt["status_word_note"].is_string(),
            "the fact about the status ships: {receipt}"
        );
        assert!(
            receipt["possible_subject_note"].is_string(),
            "the suggestion ships beside it: {receipt}"
        );
    }

    /// A session's first-contact teaching is outside the budget and crowds a
    /// suggestion out of its first receipts; an earlier capture says it, so the
    /// write under test is left with the suggestion and the facts.
    async fn after_the_first_contact_teaching(jojobot: &crate::Jojobot, sid: &str) {
        jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(sid.to_string()),
                ..capture_args("thing:the-furnace", "keeps the house warm")
            }))
            .await
            .expect("the first capture answers");
    }

    fn note(name: &'static str, domain: &'static str, text: &str, tier: Tier) -> TeachingNote {
        TeachingNote::new(name, domain, text, tier)
    }

    /// **A teaching note ships in the one `teaching` array, ranked by tier, and
    /// its domain is spent by the receipt that served it**: the next receipt
    /// finds it taught.
    #[tokio::test]
    async fn a_teaching_note_ships_ranked_and_is_spent_by_the_receipt_that_served_it() {
        let jojobot = handler();
        let caller = the_caller();
        let served = jojobot
            .own_capture_receipt(
                json!({"address": "thing:delta#f1"}),
                &caller,
                false,
                &[],
                None,
                vec![
                    note("teaching:low", "owner-test:low", "low note", Tier::Low),
                    note("teaching:high", "owner-test:high", "high note", Tier::High),
                ],
            )
            .await;
        assert_eq!(
            served["teaching"],
            json!(["high note", "low note"]),
            "{served}"
        );
        for domain in ["owner-test:low", "owner-test:high"] {
            assert!(
                !jojobot.untaught(domain, &caller).await,
                "{domain} is spent by the receipt that served it"
            );
        }
    }

    /// **A note whose domain another receipt spent first is not served**, and
    /// the note beside it still is, and is spent.
    #[tokio::test]
    async fn a_note_another_receipt_spent_first_is_not_served() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact("owner-test:taken", Some(&caller))
                .await,
            "the other receipt spends the domain first"
        );
        let served = jojobot
            .own_capture_receipt(
                json!({"address": "thing:delta#f1"}),
                &caller,
                false,
                &[],
                None,
                vec![
                    note(
                        "teaching:taken",
                        "owner-test:taken",
                        "taken note",
                        Tier::High,
                    ),
                    note("teaching:free", "owner-test:free", "free note", Tier::Low),
                ],
            )
            .await;
        assert_eq!(served["teaching"], json!(["free note"]), "{served}");
        assert!(
            !jojobot.untaught("owner-test:free", &caller).await,
            "the note that shipped is spent"
        );
    }

    /// **Teaching another part of the receipt already carries ships first**,
    /// ahead of the ranked notes, and is not ranked itself.
    #[tokio::test]
    async fn teaching_already_in_the_receipt_ships_ahead_of_the_ranked_notes() {
        let jojobot = handler();
        let served = jojobot
            .own_capture_receipt(
                json!({"address": "thing:delta#f1", "teaching": ["already there"]}),
                &the_caller(),
                false,
                &[],
                None,
                vec![note(
                    "teaching:one",
                    "owner-test:one",
                    "ranked note",
                    Tier::High,
                )],
            )
            .await;
        assert_eq!(
            served["teaching"],
            json!(["already there", "ranked note"]),
            "{served}"
        );
    }

    fn naming(things: &[(&str, &str)]) -> (super::PossibleSubject, serde_json::Value) {
        let possible = super::PossibleSubject::of("work:atlas-root", "work:atlas-root#f1", things);
        let body = json!({
            "address": "work:atlas-root#f1",
            "possible_subject_note": possible.note(),
            "hint": "a hint",
        });
        (possible, body)
    }

    /// **A possible-subject note spends the row of each thing it names when it
    /// ships**, and not before: the next receipt finds the rows taught.
    #[tokio::test]
    async fn a_possible_subject_note_that_ships_spends_its_rows() {
        let jojobot = handler();
        let caller = the_caller();
        let (possible, body) = naming(&[
            ("thing:the-furnace", "possible-subject:one"),
            ("thing:the-air-filter", "possible-subject:two"),
        ]);
        for key in ["possible-subject:one", "possible-subject:two"] {
            assert!(jojobot.untaught(key, &caller).await, "{key} is fresh");
        }
        let served = jojobot
            .own_capture_receipt(body, &caller, false, &[], Some(possible), Vec::new())
            .await;
        let note = served["possible_subject_note"].as_str().expect("the note");
        assert!(note.contains("thing:the-furnace") && note.contains("thing:the-air-filter"));
        for key in ["possible-subject:one", "possible-subject:two"] {
            assert!(!jojobot.untaught(key, &caller).await, "{key} is spent");
        }
    }

    /// **A row another receipt spent first takes its thing out of the note**,
    /// and the thing whose row was free is still named and spent.
    #[tokio::test]
    async fn a_thing_whose_row_another_receipt_spent_first_is_not_named() {
        let jojobot = handler();
        let caller = the_caller();
        let (possible, body) = naming(&[
            ("thing:the-furnace", "possible-subject:taken"),
            ("thing:the-air-filter", "possible-subject:free"),
        ]);
        assert!(
            jojobot
                .first_contact("possible-subject:taken", Some(&caller))
                .await
        );
        let served = jojobot
            .own_capture_receipt(body, &caller, false, &[], Some(possible), Vec::new())
            .await;
        let note = served["possible_subject_note"].as_str().expect("the note");
        assert!(note.contains("thing:the-air-filter"), "{note}");
        assert!(!note.contains("thing:the-furnace"), "{note}");
        assert!(
            !jojobot.untaught("possible-subject:free", &caller).await,
            "the row that shipped is spent"
        );
    }

    /// **A note left naming nothing goes, and the hint it outranked ships in
    /// its place.**
    #[tokio::test]
    async fn a_note_left_naming_nothing_gives_way_to_the_next_suggestion() {
        let jojobot = handler();
        let caller = the_caller();
        let (possible, body) = naming(&[("thing:the-furnace", "possible-subject:taken")]);
        assert!(
            jojobot
                .first_contact("possible-subject:taken", Some(&caller))
                .await
        );
        let served = jojobot
            .own_capture_receipt(body, &caller, false, &[], Some(possible), Vec::new())
            .await;
        assert!(served.get("possible_subject_note").is_none(), "{served}");
        assert_eq!(served["hint"], "a hint", "{served}");
    }

    /// **A first capture whose core and teaching pass the budget keeps its
    /// teaching and carries no suggestion**: the budget bounds the suggestion,
    /// which drops and is named, and the teaching ships and is spent.
    #[tokio::test]
    async fn a_receipt_over_the_budget_keeps_its_teaching_and_drops_the_suggestion() {
        let jojobot = handler();
        let caller = the_caller();
        let served = jojobot
            .own_capture_receipt(
                json!({
                    "address": "thing:delta#f1",
                    "padding": "x".repeat(LEAN_LIMIT - 200),
                    "hint": "a hint"
                }),
                &caller,
                false,
                &[],
                None,
                vec![note(
                    "teaching:one",
                    "owner-test:one",
                    &"taught ".repeat(30),
                    Tier::High,
                )],
            )
            .await;
        assert_eq!(
            served["teaching"],
            json!(["taught ".repeat(30)]),
            "the teaching ships over the budget: {served}"
        );
        assert!(
            served.get("hint").is_none(),
            "the suggestion is what the budget drops: {served}"
        );
        assert!(
            served["left_out"].to_string().contains("hint"),
            "and it is named: {served}"
        );
        assert!(
            served.get("over_budget").is_none(),
            "the core and the account fit: {served}"
        );
        assert!(
            !jojobot.untaught("owner-test:one", &caller).await,
            "the teaching that shipped is spent"
        );
    }

    /// **A token-saving bot's receipt is composed at the strict limit.** Single
    /// writes are compacted afterwards, but a batch is not, so the owner's own
    /// answer is what the bot reads: the suggestion drops and is named, and a
    /// core over the limit says so with `over_budget`, the size of what is left.
    #[tokio::test]
    async fn a_token_saving_receipt_is_composed_at_the_strict_limit() {
        let jojobot = handler();
        let served = crate::write_receipts::TOKEN_SAVING
            .scope(
                Some(crate::settings::TokenSaving::on()),
                jojobot.own_capture_receipt(
                    json!({
                        "address": "thing:delta#f1",
                        "padding": "x".repeat(300),
                        "hint": "a hint"
                    }),
                    &the_caller(),
                    false,
                    &[],
                    None,
                    Vec::new(),
                ),
            )
            .await;
        assert!(served.get("hint").is_none(), "{served}");
        assert!(served["left_out"].to_string().contains("hint"), "{served}");
        assert!(served["over_budget"].as_u64().is_some(), "{served}");
    }

    /// **Steps come out by key, not by sentence.** Two keys share one sentence and
    /// only one is lost: one step still ships, and its key is spent.
    #[tokio::test]
    async fn a_shared_sentence_loses_one_step_for_one_lost_key() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact("type-unlocks:one", Some(&caller))
                .await
        );
        let served = jojobot
            .own_capture_receipt(
                a_receipt_with_steps(&["same step", "same step"]),
                &caller,
                false,
                &[
                    owed("type-unlocks:one", "same step"),
                    owed("type-unlocks:two", "same step"),
                ],
                None,
                Vec::new(),
            )
            .await;
        assert_eq!(
            served["unlocks"]["steps"].as_array().map(Vec::len),
            Some(1),
            "{served}"
        );
        assert!(!jojobot.untaught("type-unlocks:two", &caller).await);
    }

    /// **A note that spent stays spent when the note after it is lost**: the
    /// receipt is composed again and the first note is not spent a second time,
    /// which would read as lost.
    #[tokio::test]
    async fn a_note_ahead_of_a_lost_note_stays_served_and_spent() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact("owner-test:taken", Some(&caller))
                .await
        );
        let served = jojobot
            .own_capture_receipt(
                json!({"address": "thing:delta#f1"}),
                &caller,
                false,
                &[],
                None,
                vec![
                    note("teaching:free", "owner-test:free", "free note", Tier::High),
                    note(
                        "teaching:taken",
                        "owner-test:taken",
                        "taken note",
                        Tier::Low,
                    ),
                ],
            )
            .await;
        assert_eq!(served["teaching"], json!(["free note"]), "{served}");
        assert!(!jojobot.untaught("owner-test:free", &caller).await);
    }

    /// **A part spent in one pass stays spent when another part is lost in the
    /// next**: a free possible-subject note ships and is spent, a teaching note
    /// another receipt took comes out, and the note is still served.
    #[tokio::test]
    async fn a_part_spent_before_a_loss_stays_served() {
        let jojobot = handler();
        let caller = the_caller();
        assert!(
            jojobot
                .first_contact("owner-test:taken", Some(&caller))
                .await
        );
        let (possible, body) = naming(&[("thing:the-furnace", "possible-subject:free")]);
        let served = jojobot
            .own_capture_receipt(
                body,
                &caller,
                false,
                &[],
                Some(possible),
                vec![note(
                    "teaching:taken",
                    "owner-test:taken",
                    "taken note",
                    Tier::High,
                )],
            )
            .await;
        assert!(served.get("possible_subject_note").is_some(), "{served}");
        assert!(served.get("teaching").is_none(), "{served}");
        assert!(!jojobot.untaught("possible-subject:free", &caller).await);
    }

    /// **The small form teaches nothing, so nothing that needs a spend ships.**
    /// A ledger that cannot spend answers the same way. The core is whole and
    /// no teaching, possible-subject note or carry hint is on it.
    #[tokio::test]
    async fn a_receipt_that_cannot_spend_serves_its_core_and_no_teaching() {
        let jojobot = handler();
        let (possible, body) = naming(&[("thing:the-furnace", "possible-subject:one")]);
        let served = crate::write_receipts::TOKEN_SAVING
            .scope(
                Some(crate::settings::TokenSaving::on()),
                jojobot.own_capture_receipt(
                    body,
                    &the_caller(),
                    true,
                    &[],
                    Some(possible),
                    vec![note("teaching:one", "owner-test:one", "a note", Tier::High)],
                ),
            )
            .await;
        for key in ["teaching", "possible_subject_note", "hint"] {
            assert!(served.get(key).is_none(), "{key} shipped: {served}");
        }
        assert_eq!(served["address"], "work:atlas-root#f1", "{served}");
    }
}
