//! **This file is a contract, not a story.** It pins how much a standard boot and
//! a standard records read may weigh. Nothing here is a use case in a person's
//! words: each case builds an instance the way a real one is, reads it the way a
//! session does by default, and measures the characters served.

use serde_json::json;

use super::boot::slug;
use super::dsl::Story;

/// How many other bots the instance has.
const COLLEAGUES: usize = 12;

/// How long a record's details are: a paragraph, the way a record's reasons
/// are written.
fn paragraph(n: usize) -> String {
    format!("reason {n}: ")
        + &"the quote was higher than the last one and the contractor said the price would not hold. "
            .repeat(4)
}

async fn a_working_instance(story: &Story) {
    let s = story.session().await;
    for i in 0..COLLEAGUES {
        s.add(&format!("bot:{}", slug(i)), &slug(i + 10_000)).await;
    }
    s.add("topic:milhouse-moves", "Milhouse moves").await;
    for n in 1..=6 {
        s.call(
            "capture",
            json!({
                "subject": "topic:milhouse-moves",
                "content": format!("the quote number {n} came in"),
                "details": paragraph(n),
                "provenance": "testimony",
            }),
        )
        .await;
    }
    s.wrap("set the instance up").await;
}

/// The characters a standard records read of the instance's topic serves.
async fn records_read(s: &super::dsl::Session, details: bool) -> usize {
    let mut args = json!({"subject": "topic:milhouse-moves", "facts": true});
    if details {
        args["details"] = true.into();
    }
    s.answer_size("recall", args).await
}

/// **A standard records read leaves the details out, and weighs what the claims
/// weigh.** Six claims, each with a paragraph of reasons: the read that asks for
/// the paragraphs is heavier by about what they weigh, and the standard one stays
/// under a fixed size, so a change that serves them unasked again fails here and
/// not only in a story about their absence.
#[tokio::test]
async fn a_standard_records_read_weighs_what_the_claims_weigh() {
    let story = Story::begin("bot:otto").await;
    a_working_instance(&story).await;
    let (_, s) = story.full_boot().await;
    // The first read of a session carries a teaching once; both reads below are
    // the second and third, so they are measured on equal terms.
    records_read(&s, false).await;
    let (standard_answer, standard) = s
        .answer_and_size(
            "recall",
            json!({"subject": "topic:milhouse-moves", "facts": true}),
        )
        .await;
    let asked = records_read(&s, true).await;
    let paragraphs: usize = (1..=6).map(|n| paragraph(n).chars().count()).sum();
    assert!(
        standard <= 4_200,
        "a standard read of six claims is {standard} characters"
    );
    // The standard read carries the line that says how many details it left out,
    // with the addresses it names and the ways to ask for them, which the other
    // read does not. It is weighed as served, so a longer way to ask moves this
    // allowance with it.
    let line = format!(
        "\"details_left_out\":{},",
        standard_answer["details_left_out"]
    )
    .chars()
    .count();
    assert!(
        line > 0 && standard_answer["details_left_out"].is_object(),
        "the standard read says what it left out: {standard_answer}"
    );
    assert!(
        asked + line >= standard + paragraphs,
        "the read that asks for details ({asked}) is heavier than the standard one \
         ({standard}) by about the {paragraphs} characters of reasons, less its \
         {line}-character line"
    );
    story.finish().await;
}

/// **A named boot does not grow with the roster.** The booting bot is listed and
/// the others are counted, so thirty more colleagues move the boot by the digits
/// of two counts and nothing else, and the boot stays under a fixed size.
#[tokio::test]
async fn a_named_boot_does_not_grow_with_the_roster() {
    let story = Story::begin("bot:otto").await;
    a_working_instance(&story).await;
    let (small, _) = story.full_boot().await;
    let small = small.to_string().chars().count();
    // The ceiling carries the name of the `usage` kind, which the boot lists twice
    // (the essay's kind line and the snapshot's vocabulary).
    assert!(small <= 14_550, "a standard boot is {small} characters");

    let s = story.session().await;
    for i in 0..30 {
        s.add(&format!("bot:{}", slug(i + 200)), &slug(i + 20_000))
            .await;
    }
    s.wrap("added thirty colleagues").await;
    let (large, _) = story.full_boot().await;
    let large = large.to_string().chars().count();
    assert!(
        large < small + 400,
        "thirty colleagues moved the boot from {small} to {large} characters"
    );
    story.finish().await;
}
