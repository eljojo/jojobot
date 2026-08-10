//! "Nobody told me I had mail."
//!
//! Two agents corresponding, each posting, each holding a reply it has not
//! seen, neither of them blocked and neither of them wrong to carry on. The
//! failure is invisible from inside: a session that never polls looks exactly
//! like a session with an empty box.
//!
//! So every answer carries what the caller should know and did not ask about.
//! **It is on the answer of a verb that has nothing to do with mail**, because
//! the session that needs telling is precisely the one not thinking about its
//! box — and it is **absent when there is nothing to say**, so the block never
//! becomes noise a reader learns to skip.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn an_answer_says_there_is_mail_waiting_even_when_nothing_asked() {
    let story = Story::begin("bot:otto").await;

    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;

    // ── the quiet case first, and everything below rests on it ──────────────
    //
    // With no mail waiting there is no block at all. Asserted BEFORE any is
    // sent, against the same verb, so "absent when empty" cannot be satisfied
    // by a build that never emits it.
    let quiet = s.call("list_entities", json!({ "kind": "bot" })).await;
    quiet.never_says("status_bar");

    // ── a colleague leaves work, and says nothing to anybody ────────────────
    let gamma = story.as_bot("bot:gamma").await;
    gamma
        .post("otto", "the damper question", "your turn on the damper")
        .await;

    // ── the same verb, and now it says so ───────────────────────────────────
    //
    // `list_entities` has nothing to do with mail. That is the point: the
    // session being told is the one that was not going to ask.
    let told = s.call("list_entities", json!({ "kind": "bot" })).await;
    told.says("status_bar");
    told.says("\"mail_waiting\":1");

    // …and it is still the answer to the question that was asked.
    told.says("bot:gamma");

    // ── a refusal carries it too ────────────────────────────────────────────
    //
    // "Every answer" has to mean the blocked ones as well. A caller that just
    // got turned down is one about to work out what to do next, and what is
    // waiting for it is part of that.
    let turned_down = s
        .refused("recall", json!({ "subject": "person:zzz-nobody" }))
        .await;
    turned_down.says("\"mail_waiting\":1");

    // ── including the refusal that never reaches a verb ──────────────────────
    //
    // The argument gate answers before dispatch, so it is the one answer that
    // can miss the block by sitting in front of where the block is attached. A
    // caller whose call was turned back at the door is as much a caller about
    // to decide what to do next as any other.
    let at_the_door = s
        .refused(
            "list_entities",
            json!({ "kind": "bot", "parent": "bot:gamma" }),
        )
        .await;
    at_the_door.says("does not implement parent");
    at_the_door.says("\"mail_waiting\":1");

    // ── taking delivery ends it ─────────────────────────────────────────────
    //
    // The block reports what is waiting, so a box nobody is owed anything from
    // stops carrying one. Without this the bar could be a constant that
    // happens to read 1.
    s.drain().await;
    let after = s.call("list_entities", json!({ "kind": "bot" })).await;
    after.never_says("mail_waiting");

    s.wrap("was told about the damper without going looking for it")
        .await;
    story.finish().await;
}
