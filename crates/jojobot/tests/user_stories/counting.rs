//! "Every time one of them eats a donut I write it down. How many has each
//! had?"
//!
//! **The arithmetic is jojobot's, not the session's.** Every key folded one way
//! before this: the newest write wins, so three sittings each recording one
//! donut read back as one donut. To get a total the session had to fetch the
//! history, add it up and write the answer back — which puts the sum in
//! whichever session last touched the key, and lets two of them disagree.
//!
//! **A key declares that it is a counter, and the declaration is what buys the
//! total.** It sits on the key rather than on the write, so nobody can record
//! one donut as a replacement while somebody else records it as an addition.
//!
//! **And the substrate is untouched.** The projection adds the writes up; the
//! writes are all still there, so *how many donuts* and *which occasions* are
//! two reads of one body of data.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_counter_adds_its_writes_up_and_still_lists_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:homer", "Homer").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:bart", "Bart").await;

    // ── the key is declared a counter, before anything is written ───────────
    //
    // The declaration says `sum` and says nothing about a value type: a total
    // of anything but a number is not a total, so the key holds a number and
    // the caller does not have to say so twice.
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "snacking",
                "fields": [
                    { "key": "donuts", "folds": "sum" },
                    { "key": "mood" },
                ],
            }),
        )
        .await;
    declared.says("\"folds\":\"sum\"");
    declared.says("\"holds\":\"number\"");
    // The other key says what every key said before there was a choice, so the
    // answer never leaves a reader to infer which keys are counters. **A key
    // declaring neither a fold nor a kind reads back as it always did** — text,
    // newest — which is what says neither half moved the default.
    declared.says("\"folds\":\"newest\"");
    declared.says("\"holds\":\"text\"");

    // A counter that holds anything but a number is a declaration that is
    // wrong about itself, and it is refused rather than stored and worked
    // around later. Nothing else on this path refuses anything: a record is
    // never checked against a declaration.
    s.refused(
        "declare_type",
        json!({
            "name": "snacking",
            "fields": [{ "key": "donuts", "holds": "date", "folds": "sum" }],
        }),
    )
    .await
    .says("donuts");

    // **The same rule reaches the key that names the kind it points at.** A
    // reference narrowed to a kind is still a handle, and a total of handles is
    // not a total — so a caller who asks for both halves in one breath is
    // refused, and the refusal names the key to fix and the half that is wrong,
    // spelled the way it was sent.
    s.refused(
        "declare_type",
        json!({
            "name": "snacking",
            "fields": [{ "key": "donuts", "holds": "reference:place", "folds": "sum" }],
        }),
    )
    .await
    .says("donuts")
    .says("reference:place");

    // The positive both refusals rest on: each half on its own is an ordinary
    // declaration, and it goes through the same door. Without it, both beats
    // above pass on a build that refuses every declaration.
    s.call(
        "declare_type",
        json!({
            "name": "stay",
            "fields": [{ "key": "venue", "holds": "reference:place" }],
        }),
    )
    .await
    .says("\"holds\":\"reference:place\"");

    // ── three sittings, one donut each ──────────────────────────────────────
    //
    // Nobody adds anything up here. Each record says what happened that day,
    // which is the only thing the session at the table actually knows.
    for eaten in ["at the plant", "on the drive home", "in front of the tv"] {
        s.event_with(
            "person:homer",
            &format!("a donut {eaten}"),
            json!({ "donuts": "1", "mood": "content" }),
            &[],
        )
        .await;
    }

    // One sitting, three donuts, written by somebody who counted at the time.
    // **It is the same total by a different route**, and both must land on the
    // same number or the count means two things.
    s.event_with(
        "person:milhouse",
        "three at once, regretted immediately",
        json!({ "donuts": "3", "mood": "queasy" }),
        &[],
    )
    .await;

    // The negative every total below rests on: one donut is one donut. Without
    // it, "the counter added them up" cannot be told from "the counter reports
    // three for everybody".
    s.event_with(
        "person:bart",
        "took one and ran",
        json!({ "donuts": "1", "mood": "smug" }),
        &[],
    )
    .await;

    // ── so what has each of them had ────────────────────────────────────────
    let homer = s
        .shape(
            "how many donuts Homer has had",
            json!({
                "subject": "person:homer",
            }),
        )
        .await;
    homer.says("\"donuts\":\"3\"");
    // **The key that is not a counter still takes the newest write**, in the
    // same answer that just proved the counter adds. One row, two folds, and
    // the declaration is the only thing telling them apart.
    homer.says("\"mood\":\"content\"");

    let milhouse = s
        .shape(
            "how many donuts Milhouse has had",
            json!({ "subject": "person:milhouse" }),
        )
        .await;
    milhouse.says("\"donuts\":\"3\"");

    let bart = s
        .shape(
            "how many donuts Bart has had",
            json!({ "subject": "person:bart" }),
        )
        .await;
    bart.says("\"donuts\":\"1\"");

    // ── and the occasions are still there ───────────────────────────────────
    //
    // **The projection changed and the substrate did not.** The three writes
    // behind Homer's total are each still their own write, with the record and
    // the date they arrived on — so *how many* and *which times* are one body
    // of data read two ways.
    let occasions = s
        .shape(
            "the times Homer ate a donut",
            json!({ "subject": "person:homer", "history": "donuts" }),
        )
        .await;
    occasions.says("\"count\":3");
    // **Three writes, each naming the record it arrived in.** The count alone
    // would pass on a build that stored one write and said three; the addresses
    // are what say the occasions are separate things somebody can go and read.
    occasions.says("person:homer#f1");
    occasions.says("person:homer#f2");
    occasions.says("person:homer#f3");

    // ── so who has had three or more ────────────────────────────────────────
    //
    // **A counter nobody can ask about is a feature that is not there.** The
    // filter is asked of what each thing HOLDS, which is the total — so Homer
    // is in the answer on three writes of one, and no record of his says three
    // at all.
    let three_or_more = s
        .shape(
            "who has had three or more donuts",
            json!({
                "kind": "person",
                "fields": [{ "key": "donuts", "compare": "greater", "value": "2" }],
            }),
        )
        .await;
    three_or_more.says("person:homer");
    // **The positive the negative rests on.** Milhouse got there in one sitting
    // and Homer over three, and both are the same answer to the same question —
    // an answer holding only Milhouse would be today's build, and one holding
    // everybody would be no filter at all.
    three_or_more.says("person:milhouse");
    three_or_more.never_says("person:bart");

    // ── and the other question, which is not this one ───────────────────────
    //
    // **"Who has eaten three" and "which sitting was three" are two questions,
    // and they are named apart.** A filter scoped to a record asks about the
    // occasions rather than about what anybody holds now, so it finds the one
    // sitting where three donuts went at once and does not find Homer, whose
    // three arrived one at a time.
    let three_at_once = s
        .shape(
            "the sittings where three donuts went at once",
            json!({
                "kind": "person",
                "facts": true,
                "fields": [{
                    "key": "donuts", "compare": "greater", "value": "2", "scope": "record",
                }],
            }),
        )
        .await;
    three_at_once.says("person:milhouse");
    three_at_once.says("three at once, regretted immediately");
    three_at_once.never_says("person:homer");
    three_at_once.never_says("person:bart");

    s.wrap("counted the donuts").await;
    story.finish().await;
}
