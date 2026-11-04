//! **The one owner of what a capture receipt says beyond the facts of the
//! write.** The receipt's facts (what landed, what was not stored, what changed)
//! are its core and always ship. Every advisory part a capture adds is a named
//! layer here, and the composer decides which ship: at most one suggestion,
//! ranked, the rest named as left out.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::answer::compose::{Answer, Kind, Layer, Tier, compose_within};

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
/// the one top-ranked suggestion. Every other suggestion is named under
/// `left_out`. No ceiling applies yet, so only the one-suggestion rule acts.
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
    use rmcp::handler::server::wrapper::Parameters;

    /// **Two suggestions on one write: only the top-ranked ships, and the other
    /// is named as left out.** A heading written onto a document gets the
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
        assert_eq!(
            receipt["left_out"][0]["layer"], "hint",
            "the dropped one is named: {receipt}"
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
