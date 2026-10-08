//! "The operator said the handcart is red. A later run wrote that it is blue."
//!
//! **Words the operator said are theirs.** A run that restated the word
//! `testimony` could rewrite a claim of that kind in place, and the record went
//! on naming the operator as the source of words they never said. A claim a run
//! wrote itself is the run's to rewrite, and only another run's testimony is
//! protected: the later run archives the original, so it stays readable, and
//! writes the corrected claim beside it, derived from it.
//!
//! **The negatives are what make the boundary mean something.** A later run
//! still rewrites its own inference in place, and a run still rewrites its own
//! testimony without restating what it is.

use serde_json::json;

use super::dsl::Story;

fn said(subject: &str, content: &str, provenance: &str) -> serde_json::Value {
    json!({"subject": subject, "content": content, "provenance": provenance})
}

/// **A later run cannot rewrite the operator's words in place, and the way it
/// can is open.** The refusal names the route and says the original stays
/// readable. Following the route archives the original, which a history read
/// still shows, and the corrected claim is derived from it.
#[tokio::test]
async fn a_later_run_cannot_rewrite_the_operators_words_in_place_and_the_route_works() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let red = first.fact("thing:handcart", "the handcart is red").await;
    first.wrap("wrote down what the operator said").await;

    let later = story.session_in("UTC", Some("new")).await;
    let refused = later
        .refused(
            "update_fact",
            json!({"address": red, "content": "the handcart is blue", "provenance": "testimony"}),
        )
        .await;
    refused
        .says("blocked")
        .says("derived_from")
        .says("archived");
    // Nothing was rewritten: the words are the operator's still.
    later
        .recall("thing:handcart")
        .await
        .claim(&red)
        .says("the handcart is red")
        .never_says("the handcart is blue");

    // **The route.** Archive the original with the reason, then write the
    // corrected claim naming it.
    later
        .call(
            "update_fact",
            json!({"address": red, "status": "archived", "details": "the operator corrected it"}),
        )
        .await;
    let blue = later
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "the handcart is blue",
                "provenance": "testimony", "derived_from": red,
            }),
        )
        .await
        .json();
    let blue = blue["address"]
        .as_str()
        .or_else(|| blue["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    // **The ordinary read after the replace** carries both, each as what it is
    // now: the original, archived, with the reason; the corrected claim, active,
    // derived from it. A read that dropped the archived one, or served it as
    // active beside the new one, would leave two live claims about one colour.
    let now = later.recall("thing:handcart").await;
    now.claim(&red)
        .says("\"status\":\"archived\"")
        .says("the handcart is red")
        .says("the operator corrected it")
        .never_says("\"status\":\"active\"");
    now.claim(&blue)
        .says("\"status\":\"active\"")
        .says("the handcart is blue")
        .says(&format!("\"derived_from\":\"{red}\""));
    // The archived original is readable, with what it said and that it is archived.
    later
        .shape(
            "the archived original",
            json!({"subject": "thing:handcart", "history_record": red}),
        )
        .await
        .says("the handcart is red")
        .says("archived");
    // And the new claim stands on it.
    later
        .shape("what was built on the original", json!({"built_on": red}))
        .await
        .says(&blue);
    story.finish().await;
}

/// **A run rewrites its own testimony without saying what it is.** The
/// provenance it has is kept, and the receipt says it is still testimony.
#[tokio::test]
async fn a_run_rewrites_its_own_words_and_keeps_their_provenance() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let said_by_operator = s.fact("thing:handcart", "the handcart is red").await;
    let guessed = s
        .call(
            "capture",
            said(
                "thing:handcart",
                "the handcart has a squeaky wheel",
                "inference",
            ),
        )
        .await
        .json();
    let guessed = guessed["address"]
        .as_str()
        .or_else(|| guessed["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    let kept = s
        .call(
            "update_fact",
            json!({"address": said_by_operator, "content": "the handcart is dark red"}),
        )
        .await
        .json();
    assert_eq!(kept["provenance"], "testimony", "kept, not demoted: {kept}");
    let guess_kept = s
        .call(
            "update_fact",
            json!({"address": guessed, "content": "the handcart has a loud wheel"}),
        )
        .await
        .json();
    assert_eq!(
        guess_kept["provenance"], "inference",
        "an inference stays an inference: {guess_kept}"
    );
    s.recall("thing:handcart")
        .await
        .claim(&said_by_operator)
        .says("the handcart is dark red");
    story.finish().await;
}

/// **A run still cannot make its own guess the operator's words.** Moving a
/// claim to testimony needs the operator's confirmation, in the run that wrote
/// it as in any other.
#[tokio::test]
async fn a_run_still_needs_the_operators_word_to_make_its_own_guess_testimony() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let guess = s
        .call(
            "capture",
            said("thing:handcart", "the handcart is green", "inference"),
        )
        .await
        .json();
    let guess = guess["address"]
        .as_str()
        .or_else(|| guess["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    s.refused(
        "update_fact",
        json!({"address": guess, "content": "the handcart is red", "provenance": "testimony"}),
    )
    .await
    .says("blocked")
    .says("confirmed_by_user");
    s.recall("thing:handcart")
        .await
        .claim(&guess)
        .says("the handcart is green");

    let promoted = s
        .call(
            "update_fact",
            json!({
                "address": guess, "content": "the handcart is red",
                "provenance": "testimony", "confirmed_by_user": true,
            }),
        )
        .await
        .json();
    assert_eq!(promoted["provenance"], "testimony", "{promoted}");
    story.finish().await;
}

/// **Only testimony is protected.** A later run rewrites an earlier run's
/// inference in place, as it always has, and still has to say how it knows.
#[tokio::test]
async fn a_later_run_still_rewrites_an_earlier_runs_inference_in_place() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let guess = first
        .call(
            "capture",
            said("thing:handcart", "the handcart is green", "inference"),
        )
        .await
        .json();
    let guess = guess["address"]
        .as_str()
        .or_else(|| guess["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    first.wrap("guessed a colour").await;

    let later = story.session_in("UTC", Some("new")).await;
    // Without a provenance it is refused as it is today: the later run did not
    // write the claim, so it cannot keep what it did not say.
    later
        .refused(
            "update_fact",
            json!({"address": guess, "content": "the handcart is yellow"}),
        )
        .await
        .says("blocked")
        .says("provenance");
    let landed = later
        .call(
            "update_fact",
            json!({
                "address": guess, "content": "the handcart is yellow",
                "provenance": "inference",
            }),
        )
        .await
        .json();
    assert_eq!(landed["provenance"], "inference", "{landed}");
    later
        .recall("thing:handcart")
        .await
        .claim(&guess)
        .says("the handcart is yellow");
    story.finish().await;
}

/// **The operator's words include the fields their claim carries.** A later run
/// cannot set, change or clear a value field on a testimony claim in place, in
/// the claim's own fields or in its setting bag, and the refusal names the same
/// route as for the words: archive it, then capture the correction derived from
/// it. The run that wrote the claim changes its fields as it always did.
///
/// **What describes the record says nothing the operator said**, so a later run
/// still sets a describing key, and archiving, which is the route itself, still
/// lands. Each refusal sits beside the same edit made where it is allowed.
#[tokio::test]
async fn a_later_run_cannot_set_change_or_clear_a_value_field_on_the_operators_claim() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let red = first
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "the handcart is red",
                "provenance": "testimony", "fields": {"colour": "red"},
            }),
        )
        .await
        .field("address");
    let heavy = first
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "the handcart weighs ninety",
                "provenance": "testimony", "sets": {"weight": "90"},
            }),
        )
        .await
        .field("address");
    let spare = first
        .call(
            "capture",
            said("thing:handcart", "the handcart has two wheels", "testimony"),
        )
        .await
        .field("address");

    // ── the run that wrote the claim changes its fields ─────────────────────
    first
        .call(
            "update_fact",
            json!({"address": red, "fields": {"colour": "crimson"}}),
        )
        .await;
    first.wrap("wrote down what the operator said").await;

    // ── a later run cannot set, change or clear a value field in place ──────
    let later = story.session_in("UTC", Some("new")).await;
    for edit in [
        json!({"address": red, "fields": {"colour": "blue"}}),
        json!({"address": red, "fields": {"wheels": "two"}}),
        json!({"address": red, "clear_fields": ["colour"]}),
        json!({"address": heavy, "sets": {"weight": "95"}}),
    ] {
        later
            .refused("update_fact", edit)
            .await
            .says("blocked")
            .says("derived_from")
            .says("archived");
    }
    // Nothing moved: the claim still carries what its own run left.
    later
        .recall("thing:handcart")
        .await
        .says("\"colour\":\"crimson\"")
        .says("\"weight\":\"90\"")
        .never_says("\"colour\":\"blue\"");

    // ── what describes the record is anybody's to set ───────────────────────
    later
        .call(
            "update_fact",
            json!({"address": red, "fields": {"purpose": "a note on the colour"}}),
        )
        .await;
    later
        .recall("thing:handcart")
        .await
        .says("a note on the colour");

    // ── and archiving, which is the route itself, lands ─────────────────────
    later
        .call(
            "update_fact",
            json!({"address": spare, "status": "archived", "details": "no longer true"}),
        )
        .await;
    story.finish().await;
}

/// **A resend of what a testimony claim already says is not a rewrite.** A later
/// run that sends the claim's own words, with the values its fields already hold,
/// beside a status move or a key that only describes the record, changes nothing
/// the operator said, and it lands. A word or a value that differs is still
/// refused with the route, in the same read: a build that let every such call
/// through would not pass.
#[tokio::test]
async fn a_resend_of_the_operators_words_and_fields_is_not_a_rewrite() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("thing:handcart", "The Handcart").await;
    let red = first
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "the handcart is red",
                "provenance": "testimony", "fields": {"colour": "red"},
            }),
        )
        .await
        .field("address");
    let spare = first
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "the handcart has two wheels",
                "provenance": "testimony", "fields": {"wheels": "two"},
            }),
        )
        .await
        .field("address");
    first.wrap("wrote down what the operator said").await;

    let later = story.session_in("UTC", Some("new")).await;

    // ── the words and the value, resent, with a key that only describes ─────
    later
        .call(
            "update_fact",
            json!({
                "address": red, "content": "the handcart is red",
                "fields": {"colour": "red", "purpose": "a note on the colour"},
            }),
        )
        .await;
    later
        .recall("thing:handcart")
        .await
        .says("a note on the colour")
        .says("the handcart is red");

    // ── the words resent beside a status move: the route itself ─────────────
    later
        .call(
            "update_fact",
            json!({
                "address": spare, "content": "the handcart has two wheels",
                "status": "archived", "details": "no longer true",
            }),
        )
        .await;

    // ── a word or a value that differs is still the operator's ──────────────
    for edit in [
        json!({"address": red, "content": "the handcart is blue", "provenance": "testimony"}),
        json!({"address": red, "content": "the handcart is red", "fields": {"colour": "blue"}}),
    ] {
        later
            .refused("update_fact", edit)
            .await
            .says("blocked")
            .says("derived_from")
            .says("archived");
    }
    story.finish().await;
}

/// **A bot told it may not write its own ceiling is told so before it is told to
/// archive and capture.** A seat count the operator's session wrote is edited by
/// the bot it binds: the refusal is the ceiling's, which says a different identity
/// has to raise it, and not the testimony rule's route, which would end at the same
/// ceiling. A value key on the same claim is the testimony rule's refusal, in the
/// same read.
#[tokio::test]
async fn a_bot_editing_its_own_ceiling_on_the_operators_claim_meets_the_ceiling_first() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let claim = s
        .call(
            "capture",
            json!({
                "subject": "bot:gamma", "content": "gamma has eight seats",
                "provenance": "testimony",
                "fields": {"rule_seats": "8", "colour": "green"},
            }),
        )
        .await
        .field("address");
    s.wrap("wrote down what the operator said").await;

    let gamma = story.as_bot("bot:gamma").await;
    gamma
        .refused(
            "update_fact",
            json!({"address": claim, "fields": {"rule_seats": "20"}}),
        )
        .await
        .says("rule_seats")
        .says("\"wrote\":false")
        .never_says("derived_from");
    gamma
        .refused(
            "update_fact",
            json!({"address": claim, "fields": {"colour": "blue"}}),
        )
        .await
        .says("derived_from")
        .says("archived");
    story.finish().await;
}
