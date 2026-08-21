//! "I upgraded jojobot. The old build had a kind this one dropped — is it
//! still there?"
//!
//! **No. What the binary owns is reconciled at every boot, never upserted**
//! (rule 234). A build writes the rows it ships, and it also takes back the
//! rows it owns and no longer ships — so an instance serves the vocabulary the
//! software has now, rather than the union of every vocabulary it ever had.
//!
//! Nothing here is a verb. A session cannot ask for this and cannot see it
//! happen; what a session sees is the answer to the next handle it writes. That
//! is why the story asks the question from outside: the retired kind was
//! parseable in the process that came before, and after the boot a handle
//! carrying it is refused with the kinds this build does have.
//!
//! **Both halves, in one story.** The refusal alone passes on a build that has
//! lost every kind it ever had; the write that lands alone passes on a build
//! that reconciles nothing.
//!
//! ⚠️ **The set a process parses against is process-wide, and every story in
//! this binary loads it at its own boot.** So this story reads a set another
//! story can replace between its boot and its first call. The contract case is
//! what pins the mechanism against a store; what this one proves is that a
//! session meets the result through the served surface. A green run here on a
//! build that reconciles nothing is possible and rare — a flake to read as this
//! caveat rather than as an intermittent bug.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_kind_the_software_stopped_shipping_is_gone_after_the_upgrade() {
    // An instance an older build left: it holds `zeta` as a kind the software
    // shipped, and this build's set does not name it.
    let story = Story::begin_on_an_instance_an_older_build_left("bot:gamma", "zeta").await;
    let s = story.session().await;

    // ── the retired kind is no longer a kind ────────────────────────────────
    let refused = s
        .refused(
            "add_entity",
            json!({
                "kind": "zeta",
                "handle": "brass-lamp",
                "name": "The Brass Lamp",
                "source": "user-named",
            }),
        )
        .await;
    // The refusal carries a way forward rather than only a no: what this build
    // does have. Pinned on a kind's own token, which a rename would be a real
    // change to.
    refused.says("thing");

    // ── …and the kinds this build ships still are ───────────────────────────
    //
    // The positive the verdict rests on. Without it the case passes on a boot
    // that reconciled the whole set away, and the story would be reporting a
    // broken instance as a working one.
    s.add("thing:jukebox", "The Jukebox").await;
    s.recall("thing:jukebox").await.says("The Jukebox");

    story.finish().await;
}
