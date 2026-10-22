//! "The refusal told me to send `after` to a verb that takes none."
//!
//! **A refusal that names a way forward names one the surface has.** The
//! placement refusals tell a caller how to put an entry or a section where it
//! belongs, and every verb they name has to be a served verb that publishes
//! `after`. A move of a section that already exists is `update_fact`'s; a
//! `put_document` writes new text and is never offered for a move.
//!
//! The roster names are the cast: a camping trip, a packing list and a fryer
//! manual.

use serde_json::json;

use super::dsl::{Answer, Session, Story};
use super::lists::a_list_on_the_trip;

const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.";

/// **The served verbs a refusal names between backticks.** A token that is not a
/// verb is an argument or a key and is not read here.
async fn verbs_named(session: &Session, refusal: &Answer) -> Vec<String> {
    let served = session.served_verbs().await;
    let body = refusal.json().to_string();
    body.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| served.iter().any(|verb| verb == token))
        .map(str::to_string)
        .collect()
}

/// **A caller's own place on `add_entity`, `update_entity`, `capture` and
/// `update_fact` is refused, and each refusal names `capture` for a new entry
/// and `update_fact` for a move, both of which take `after`.** The paired
/// positive is the route the refusal names: a capture with `after` lands and the
/// server places it.
#[tokio::test]
async fn a_refused_place_names_the_verbs_that_take_after() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let first = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "tent poles", "provenance": "testimony"}),
        )
        .await
        .json();
    let first = first["address"]
        .as_str()
        .expect("an entry address")
        .to_string();

    let refusals = [
        s.refused(
            "add_entity",
            json!({"kind": "list", "handle": "list:errands", "name": "Errands",
                   "source": "user-named", "parent": "thing:springfield-camping",
                   "sets": {"order": "n"}}),
        )
        .await,
        s.refused(
            "update_entity",
            json!({"handle": "list:packing", "sets": {"held_turn": "1"}}),
        )
        .await,
        s.refused(
            "capture",
            json!({"subject": "list:packing", "content": "stove",
                   "provenance": "testimony", "fields": {"held_since": "1"}}),
        )
        .await,
        s.refused(
            "update_fact",
            json!({"address": first, "fields": {"order": "n"}}),
        )
        .await,
    ];

    let taking_after = s.verbs_publishing("after").await;
    for refusal in &refusals {
        let named = verbs_named(&s, refusal).await;
        for route in ["capture", "update_fact"] {
            assert!(
                named.iter().any(|verb| verb == route),
                "the refusal names {route}: {named:?} in {}",
                refusal.json()
            );
        }
        for verb in &named {
            assert!(
                taking_after.contains(verb),
                "{verb} is named as a way to place something and publishes no `after`: {taking_after:?}"
            );
        }
    }

    // ── the positive the refusals point to ───────────────────────────────────
    let placed = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "stove",
                   "provenance": "testimony", "after": "first"}),
        )
        .await
        .json();
    assert!(placed["address"].is_string(), "{placed}");
}

/// **A caller's `edges` on a section is refused, and the refusal names
/// `update_fact` for putting a section behind another, never `put_document`**,
/// which writes new text. The paired positive is a move through `update_fact`
/// with `after`, which lands.
#[tokio::test]
async fn a_refused_edges_names_update_fact_for_a_move_and_not_put_document() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    s.call(
        "put_document",
        json!({"document": "document:homer", "markdown": MANUAL, "provenance": "testimony"}),
    )
    .await;
    let held: Vec<String> = s.recall("document:homer").await.json()["objects"][0]["facts"]
        .as_array()
        .expect("sections")
        .iter()
        .map(|fact| fact["address"].as_str().expect("an address").to_string())
        .collect();

    let refused = s
        .refused(
            "update_fact",
            json!({"address": held[1], "fields": {"edges": ";;\\n\\n;"}}),
        )
        .await;
    let named = verbs_named(&s, &refused).await;
    assert!(named.iter().any(|verb| verb == "update_fact"), "{named:?}");
    assert!(
        !named.iter().any(|verb| verb == "put_document"),
        "{named:?}"
    );

    let moved = s
        .call("update_fact", json!({"address": held[0], "after": held[1]}))
        .await
        .json();
    assert!(moved.get("changed").is_some(), "{moved}");
}
