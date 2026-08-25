//! "Don't take my word for it — read it."
//!
//! A session tells a colleague what one of its operating rules says. The
//! colleague does not have to decide whether to believe it: the claim was
//! handed over **by its address**, so it reads the rule at the source and gets
//! the same words, with who backs them and the day they were said.
//!
//! **That is what makes a rule a fact rather than an assertion.** A rule
//! written into prose can only be quoted, and a quote is worth exactly as much
//! as the trust in whoever quoted it. A rule that is a dated claim at an
//! address can be CHECKED, and the reader never has to weigh the speaker.
//!
//! ⚠️ **The verifiable part is the point.** A citation that returned the words
//! alone would be a lookup — this asserts the provenance and the date come back
//! WITH the claim, because those are what let a reader say *the operator said
//! this, on that day* rather than *somebody typed this*.
//!
//! **Paired:** an address that names nothing comes back refused, with the
//! addresses that do exist. A citation surface that answered an empty page to a
//! wrong address would make a claim that was never written and a claim the
//! reader mistyped the same answer — and the reader would conclude the first.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_claim_is_settled_by_reading_it_at_its_address() {
    let story = Story::begin("bot:otto").await;

    // ── the rule is written as a dated claim on the bot, by the operator ────
    let s = story.session().await;
    let rule = s
        .fact(
            "bot:otto",
            "never posts to a box before the work it reports on is done",
        )
        .await;
    // The address is what makes it citable, and it comes back from the write.
    assert!(
        rule.starts_with("bot:otto#"),
        "a rule is a claim on the bot and carries the bot's own address: {rule}",
    );
    // The colleague, stood up from inside a session: the door mints no
    // identity, so somebody has to create one before it can be booted.
    s.add("bot:delta", "Delta").await;
    s.wrap("wrote down the rule the operator gave").await;

    // ── a colleague is told the rule, and told where to read it ─────────────
    //
    // The claim rides as an ADDRESS rather than as a quotation. What the
    // colleague does with it is the whole story.
    let dev = story.as_bot("bot:delta").await;
    let read = dev
        .shape(
            "the rule at the address I was given",
            json!({"subject": "bot:otto", "facts": true}),
        )
        .await;
    read.claim(&rule)
        .says("never posts to a box")
        // ⭐ **The two fields that make this a citation and not a lookup.**
        // Who backs it, and the day it was said: a reader can weigh the claim
        // without weighing whoever passed it on.
        .says("\"provenance\":\"testimony\"")
        .says("\"recorded_at\":");

    // ── and the words themselves are checkable against what was rewritten ───
    //
    // A rule that was corrected still says where it came from: every write of
    // it is kept, so a reader who suspects the wording moved can see whether
    // it did.
    dev.shape(
        "every version of that rule",
        json!({"subject": "bot:otto", "history_record": rule}),
    )
    .await
    // **The count structurally**, because `"count":1` is a prefix of
    // `"count":10` and a text needle would read as *exactly one*.
    .number("/objects/0/record_history/count", 1)
    .says("never posts to a box");

    // ── an address naming nothing is refused, not answered empty ────────────
    //
    // ⚠️ Without this the reader cannot tell a claim that was never written
    // from one whose address they got wrong, and both read as "there is
    // nothing there".
    let missed = dev
        .refused(
            "recall",
            json!({"subject": "bot:otto", "history_record": "bot:otto#f404"}),
        )
        .await;
    missed.says("blocked").says("bot:otto#f404").says(&rule);

    dev.wrap("checked the rule at its address instead of taking it on trust")
        .await;

    story.finish().await;
}
