//! "I have filed the same person twice, and neither page knows about the
//! other."
//!
//! A duplicate that gets past the write guard splits one thing across two
//! handles. Every read of either handle answers with half the file and reports
//! it as whole, and the split is invisible from each side, because each looks
//! like a complete record of itself. **With a repair the guard only has to be
//! good. Without one it has to be perfect, and nothing is.**
//!
//! `// NOTE —` marks something a SESSION did not do: jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to.

use super::dsl::Story;
use serde_json::json;

#[tokio::test]
async fn one_thing_filed_twice_can_be_put_back_together() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the same person, filed twice ────────────────────────────
    let s = story.session().await;

    s.add("person:nelson", "Nelson").await;
    s.fact("person:nelson", "plays the bass").await;
    s.wrap("filed him once").await;

    // ⭐ **A later session, not knowing, files him again — and the guard is
    // GOOD: it catches the near-slug and refuses.** So the duplicate only
    // exists because that session read the candidates and insisted the two were
    // different people. **That is how a duplicate gets past a guard that is
    // only good, and it is why the repair has to exist.**
    let s = story.session().await;
    let refusal = s
        .refused(
            "add_entity",
            json!({
                "kind": "person", "handle": "nelson-2", "name": "Nelson M",
                "source": "user-named",
            }),
        )
        .await;
    refusal.says("person:nelson");
    s.call(
        "add_entity",
        json!({
            "kind": "person", "handle": "nelson-2", "name": "Nelson M",
            "source": "user-named",
            "override_token": refusal.advised("override_token"),
        }),
    )
    .await
    .says("person:nelson-2");
    let second = s.fact("person:nelson-2", "reads the sleeve notes").await;

    // 🚨 **The fault, asserted before the repair.** Each handle answers with
    // its own half and neither says the other half exists. Without this the
    // whole answer below proves nothing.
    s.recall("person:nelson")
        .await
        .says("plays the bass")
        .never_says("reads the sleeve notes");
    s.recall("person:nelson-2")
        .await
        .says("reads the sleeve notes")
        .never_says("plays the bass");

    s.wrap("and again, under a second handle").await;

    // ── session 2 · somebody notices and repairs it ─────────────────────────
    let s = story.session().await;

    let done = s
        .merge_entities(
            "person:nelson-2",
            "person:nelson",
            "one person, filed twice",
        )
        .await;

    // ⚠️ **The answer says the claims changed address**, which is the thing a
    // caller holding one of those addresses has to know.
    assert_eq!(
        done["claims_moved"], 1,
        "the answer did not say what moved: {done}",
    );
    assert_eq!(done["addresses_changed"], true, "{done}");
    assert_eq!(
        done["now_resolves_to"], "person:nelson",
        "the answer did not say where the spare handle now sends a reader: {done}",
    );

    // ⭐ **Whole from the survivor, in one read.** This is the whole point.
    s.recall("person:nelson")
        .await
        .says("plays the bass")
        .says("reads the sleeve notes")
        // And the account of the repair is readable from the thing it happened
        // to, long after anybody remembers doing it.
        .says("one person, filed twice");

    // **The address a caller was holding is stale**, exactly as the answer
    // warned: the claim moved and took a new one.
    assert!(
        second.starts_with("person:nelson-2#"),
        "the fixture did not capture the second claim under the spare handle: {second}",
    );

    // NOTE — the spare handle still resolves in the store, and no read verb
    // takes a handle and reports where it now points. A session that only has
    // the old handle finds out by searching for it, not by being forwarded.
    //   s.recall("person:nelson-2").await.says("now_resolves_to");

    s.wrap("one person again, and the repair is on the record")
        .await;

    story.finish().await;
}
