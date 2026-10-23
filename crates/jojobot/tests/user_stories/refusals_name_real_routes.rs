//! "The refusal told me to send `after` to a verb that takes none."
//!
//! **A refusal that names a way forward names one the surface has.** The
//! section-edge refusals tell a caller how to put a section where it
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

/// All four writes drop copied placement keys. An explicit `after` still asks
/// the server to place an entry through the verbs that publish it.
#[tokio::test]
async fn copied_places_are_dropped_and_moves_use_the_verbs_that_take_after() {
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

    let answers = [
        s.call(
            "add_entity",
            json!({"kind": "list", "handle": "list:errands", "name": "Errands",
                   "source": "user-named", "parent": "thing:springfield-camping",
                   "sets": {"order": "n"}}),
        )
        .await,
        s.call(
            "update_entity",
            json!({"handle": "list:packing", "sets": {"held_turn": "1"}}),
        )
        .await,
        s.call(
            "capture",
            json!({"subject": "list:packing", "content": "stove",
                   "provenance": "testimony", "fields": {"held_since": "1"}}),
        )
        .await,
        s.call(
            "update_fact",
            json!({"address": first, "content": "tent repacked", "fields": {"order": "n"}}),
        )
        .await,
    ];

    let taking_after = s.verbs_publishing("after").await;
    for (answer, key) in answers
        .iter()
        .zip(["order", "held_turn", "held_since", "order"])
    {
        assert_eq!(
            answer.json()["dropped_keys"],
            json!([key]),
            "{}",
            answer.raw()
        );
    }
    for route in ["capture", "update_fact"] {
        assert!(
            taking_after.iter().any(|verb| verb == route),
            "{taking_after:?}"
        );
    }

    // ── an explicit placement still lands ───────────────────────────────────
    let placed = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "lantern",
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

/// **A key a served answer carries is named in the text of the verb that
/// carries it.** `list_runs` lists a run whose row cannot be read with
/// `unreadable` and `fix_by`; the four write verbs carry `reads_left` when a read
/// failed under their receipt. A client learns each from the text the verb is
/// served with, its description or its arguments', or not at all.
#[tokio::test]
async fn the_description_of_a_verb_names_the_keys_its_answer_can_carry() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let list_runs = s.served_text("list_runs").await;
    for key in ["unreadable", "fix_by"] {
        assert!(
            list_runs.contains(key),
            "list_runs names {key}: {list_runs}"
        );
    }
    for verb in ["capture", "update_fact", "update_entity", "add_entity"] {
        let description = s.served_text(verb).await;
        assert!(
            description.contains("reads_left"),
            "{verb} names reads_left: {description}"
        );
    }
}

/// **An argument that does nothing is not published.** No refusal of `capture` or
/// `update_fact` carries a token while a type's rule is off (rule 636), so the
/// override arguments are not in their schemas, and a call that still sends one
/// is refused for it. **Paired**: the verbs whose refusals do carry a token still
/// publish it.
#[tokio::test]
async fn capture_and_update_fact_publish_no_override_while_no_refusal_of_theirs_carries_one() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    for argument in ["override_token", "override_reason"] {
        let publishing = s.verbs_publishing(argument).await;
        for verb in ["capture", "update_fact"] {
            assert!(
                !publishing.iter().any(|name| name == verb),
                "{verb} publishes {argument}: {publishing:?}"
            );
        }
        for verb in [
            "add_entity",
            "update_entity",
            "rename_entity",
            "declare_type",
        ] {
            assert!(
                publishing.iter().any(|name| name == verb),
                "{verb} still publishes {argument}: {publishing:?}"
            );
        }
    }
    s.refused(
        "capture",
        json!({"subject": "bot:gamma", "content": "a note", "provenance": "inference",
               "override_token": "t", "override_reason": "r"}),
    )
    .await;
}
