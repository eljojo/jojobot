//! "What did I actually say?" — a bot that quotes the operator quotes a record.
//!
//! **A bot writing to the operator cites their words by address, and the address
//! is checked.** `@kind:slug#fN` in a post to the operator must name a claim that
//! exists, that the writer can read, and that the operator said (testimony). An
//! inference is jojobot's own or a bot's guess and cannot be handed back as the
//! operator's word. Accepted, the claim's words are written into the message once,
//! with the citation beside them, so a later correction of the source does not
//! change what an old message says was sent.
//!
//! **Plain quotation marks are never refused**: punctuation is not attribution.
//! Every refusal sits beside the post that lands, in the same story.

use serde_json::json;

use super::dsl::{Session, Story};

const WORDS: &str = "wants the harbor mural painted in cobalt";

/// An instance with an operator, a writer, and one testimony claim to quote.
async fn with_an_operator() -> (Story, Session, String) {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    // On the instance's record and not on the operator's own page, which would
    // show the claim whether or not a message quoted it.
    let said = s.fact("topic:instance", WORDS).await;
    (story, s, said)
}

/// Post to the operator and hand back the message's id.
async fn post_to_the_operator(s: &Session, body: &str) -> String {
    s.call(
        "post_message",
        json!({"to": "person:lisa", "subject": "a question", "body": body}),
    )
    .await
    .json()["id"]
        .as_str()
        .expect("a post hands back its id")
        .to_string()
}

#[tokio::test]
async fn a_quote_of_a_testimony_record_lands_with_the_words_and_the_citation() {
    let (story, s, said) = with_an_operator().await;

    let id = post_to_the_operator(&s, &format!("As you said, @{said}. Is that still right?")).await;
    story.mark_processed_on_the_page("person:lisa", &id).await;
    story
        .page("person:lisa")
        .await
        .says(WORDS)
        .says(&said)
        .says("Is that still right?");

    // ── the pair: plain quotation marks are never refused ───────────────────
    let plain = post_to_the_operator(&s, "The sign reads \"closed for the winter\".").await;
    assert_ne!(plain, id, "a post with plain quotation marks landed");

    story.finish().await;
}

#[tokio::test]
async fn a_quote_of_an_inference_is_refused_naming_the_address_and_the_same_post_without_it_lands()
{
    let (story, s, _said) = with_an_operator().await;
    let guessed = s
        .guess("topic:instance", "probably prefers the east wall")
        .await;

    let held = story.box_counts_on_the_page("person:lisa").await;
    s.refused(
        "post_message",
        json!({"to": "person:lisa", "body": format!("You said @{guessed}.")}),
    )
    .await
    .says(&guessed)
    .says("testimony");
    assert_eq!(
        story.box_counts_on_the_page("person:lisa").await,
        held,
        "the refused post wrote nothing"
    );

    // An address that names nothing is refused as well, with the same form.
    s.refused(
        "post_message",
        json!({"to": "person:lisa", "body": "You said @topic:instance#f9999."}),
    )
    .await
    .says("topic:instance#f9999")
    .says("testimony");

    // ── the pair: the same post without the address lands ───────────────────
    post_to_the_operator(&s, "You may prefer the east wall.").await;
    assert_ne!(story.box_counts_on_the_page("person:lisa").await, held);

    story.finish().await;
}

/// "I am writing down the form for the next agent." An address shown in code is
/// literal here exactly as a mention is: it is neither checked nor expanded. The
/// same address outside code in the same kind of post is a quote, so the pair
/// holds in both directions.
#[tokio::test]
async fn an_address_shown_in_code_is_posted_as_written_and_the_same_address_outside_it_is_quoted() {
    let (story, s, said) = with_an_operator().await;

    // ── inside code: an address naming nothing lands, where outside it is refused
    let held = story.box_counts_on_the_page("person:lisa").await;
    s.refused(
        "post_message",
        json!({"to": "person:lisa", "body": "You said @topic:instance#f9999."}),
    )
    .await
    .says("topic:instance#f9999");
    post_to_the_operator(&s, "Write the form as `@topic:instance#f9999`.").await;
    assert_ne!(
        story.box_counts_on_the_page("person:lisa").await,
        held,
        "the post that shows the form landed"
    );

    // ── inside a fence: a real address stays as written and its words are not
    // spliced in
    let fenced =
        post_to_the_operator(&s, &format!("The form:\n```\n@{said}\n```\nIs that clear?")).await;
    story
        .mark_processed_on_the_page("person:lisa", &fenced)
        .await;
    story
        .page("person:lisa")
        .await
        .says(&format!("@{said}"))
        .says("Is that clear?")
        .never_says(WORDS);

    // ── the pair: the same address outside code is quoted ────────────────────
    let quoted = post_to_the_operator(&s, &format!("As you said, @{said}.")).await;
    story
        .mark_processed_on_the_page("person:lisa", &quoted)
        .await;
    story.page("person:lisa").await.says(WORDS);

    story.finish().await;
}

#[tokio::test]
async fn a_source_corrected_after_the_post_leaves_the_message_as_it_was() {
    let (story, s, said) = with_an_operator().await;

    let id = post_to_the_operator(&s, &format!("As you said, @{said}.")).await;
    s.call(
        "update_fact",
        json!({"address": said, "content": "wants the harbor mural painted in saffron"}),
    )
    .await
    .says("saffron");
    story.mark_processed_on_the_page("person:lisa", &id).await;
    story
        .page("person:lisa")
        .await
        .says("cobalt")
        .never_says("saffron");

    story.finish().await;
}

#[tokio::test]
async fn a_body_under_the_ceiling_before_expansion_and_over_it_after_is_refused() {
    let (story, s, _said) = with_an_operator().await;
    let long = "mural ".repeat(2_000);
    let big = s.fact("topic:instance", &long).await;

    // ── the pair: one expansion fits and lands ──────────────────────────────
    post_to_the_operator(&s, &format!("@{big}")).await;

    // Three expansions of it do not fit, though the body is a few dozen characters.
    let held = story.box_counts_on_the_page("person:lisa").await;
    let body = format!("@{big} @{big} @{big}");
    assert!(body.len() < 200, "the body is short before expansion");
    let refusal = s
        .refused("post_message", json!({"to": "person:lisa", "body": body}))
        .await
        .json();
    assert_eq!(refusal["wrote"], false, "nothing was posted: {refusal}");
    assert_eq!(
        refusal["fix_by"], "change",
        "the call must change: {refusal}"
    );
    assert_eq!(story.box_counts_on_the_page("person:lisa").await, held);

    story.finish().await;
}

#[tokio::test]
async fn an_archived_source_keeps_its_archived_mark_in_the_quote() {
    let (story, s, said) = with_an_operator().await;
    s.call(
        "update_fact",
        json!({"address": said, "status": "archived", "details": "no longer true"}),
    )
    .await;

    let id = post_to_the_operator(&s, &format!("You once said @{said}.")).await;
    story.mark_processed_on_the_page("person:lisa", &id).await;
    story.page("person:lisa").await.says(WORDS).says("archived");

    story.finish().await;
}

#[tokio::test]
async fn the_convention_is_taught_once_on_the_first_post_to_the_operator() {
    let (story, s, _said) = with_an_operator().await;

    // ── the pair, first: a post to a bot's box does not carry it, and does not
    //    spend it either ─────────────────────────────────────────────────────
    s.add("bot:sigma", "Sigma").await;
    s.call(
        "post_message",
        json!({"to": "bot:sigma", "body": "An ordinary note."}),
    )
    .await
    .never_says("teaching");

    let first = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "body": "A first note."}),
        )
        .await;
    first.says("teaching").says("#f");
    let second = s
        .call(
            "post_message",
            json!({"to": "person:lisa", "body": "A second note."}),
        )
        .await;
    second.never_says("teaching");

    story.finish().await;
}
