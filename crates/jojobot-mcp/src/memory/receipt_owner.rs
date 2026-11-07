//! **The one owner of what a capture receipt says beyond the facts of the
//! write.** The receipt's facts (what landed, what was not stored, what changed)
//! are its core and always ship. Every advisory part a capture adds is a named
//! layer here, and the composer decides which ship: at most one suggestion,
//! ranked, the rest dropped.

use std::collections::BTreeSet;

use serde_json::Value;

use super::unlocks::OwedUnlock;
use crate::answer::compose::{Answer, Kind, Layer, Tier, compose_within};
use crate::{Jojobot, caller::Caller};

impl Jojobot {
    /// **What a capture receipt serves, and the teachings it spends for it.** A
    /// dropped suggestion spends nothing: a carry hint spends its domain, and a
    /// type step its ledger key, only if the receipt still carries them once the
    /// composer has chosen. If another receipt claimed the teaching first, or its
    /// ledger write fails, choose again without it.
    pub(crate) async fn own_capture_receipt(
        &self,
        mut body: Value,
        caller: &Caller,
        carry_hint: bool,
        owed: &[OwedUnlock],
    ) -> Value {
        let mut served = own(body.clone());
        if carry_hint
            && served.get("hint").is_some()
            && !self
                .first_contact(super::list_carry::PLAN_CARRY_DOMAIN, Some(caller))
                .await
        {
            body.as_object_mut()
                .expect("a capture receipt is an object")
                .remove("hint");
            served = own(body.clone());
        }
        if served.get("unlocks").is_none() || owed.is_empty() {
            return served;
        }
        let mut lost = Vec::new();
        for step in owed {
            if !self.first_contact(&step.key, Some(caller)).await {
                lost.push(step.sentence.as_str());
            }
        }
        if lost.is_empty() {
            return served;
        }
        let steps = body
            .get_mut("unlocks")
            .and_then(|block| block.get_mut("steps"))
            .and_then(Value::as_array_mut)
            .expect("the served block came from the body");
        steps.retain(|step| {
            !step["unlocks"]
                .as_str()
                .is_some_and(|sentence| lost.contains(&sentence))
        });
        if steps.is_empty() {
            body.as_object_mut()
                .expect("a capture receipt is an object")
                .remove("unlocks");
        }
        own(body)
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

/// The call that returns a left-out suggestion's ground: the thing it is about.
const RETURNED_BY: &str = "recall";

/// Takes the finished receipt and returns what is served: its core whole, and
/// the one top-ranked suggestion. Every other suggestion is dropped without a
/// trace. No ceiling applies yet, so only the one-suggestion rule acts.
pub(crate) fn own(body: Value) -> Value {
    let Value::Object(mut core) = body else {
        return body;
    };
    let layers: Vec<Layer> = SUGGESTIONS
        .iter()
        .filter_map(|(key, tier)| {
            let held = core.remove(*key)?;
            Some(Layer::new(key, Kind::Suggestion, *tier, RETURNED_BY, held))
        })
        .collect();
    if layers.is_empty() {
        return Value::Object(core);
    }
    compose_within(
        &Answer::new(Value::Object(core), layers),
        usize::MAX,
        &BTreeSet::new(),
    )
    .served
}

#[cfg(test)]
mod tests {
    use crate::CaptureArgs;
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
            .own_capture_receipt(body, &caller, true, &[owed("type-unlocks:step", "step")])
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
    /// is dropped.** A heading written onto a document gets the
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
        assert!(
            receipt.get("left_out").is_none(),
            "the dropped one is not named, since it consumed no budget: {receipt}"
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
}
