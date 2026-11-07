//! "I kept editing the old handle after the two things were put together."
//!
//! **A thing that was folded into another only forwards to it.** An edit sent to
//! the folded handle used to write the forwarding row and change nothing a reader
//! sees. It is refused now, and the refusal names the survivor, so the caller
//! sends the edit there. Roster names throughout.

use serde_json::json;

use super::dsl::Story;

const SURVIVOR: &str = "thing:kettle";
const FOLDED: &str = "thing:handcart";

/// **The folded handle takes no edit and says where the thing went.** The edit
/// to the survivor lands and reads back. Paired with the refusal, so the edit
/// is judged on the row and not on the verb.
#[tokio::test]
async fn an_edit_sent_to_a_folded_handle_names_the_survivor_and_the_survivor_takes_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(SURVIVOR, "Kettle").await;
    s.add(FOLDED, "Handcart").await;
    s.call(
        "merge_entities",
        json!({"duplicate": FOLDED, "survivor": SURVIVOR, "reason": "one thing, two names"}),
    )
    .await;

    // ── the folded handle takes no edit, and the refusal names the survivor ──
    s.refused(
        "update_entity",
        json!({"handle": FOLDED, "name": "Handcart Mk Two"}),
    )
    .await
    .says(SURVIVOR);

    // ── the survivor takes the same edit ─────────────────────────────────────
    s.call(
        "update_entity",
        json!({"handle": SURVIVOR, "name": "Kettle Mk Two"}),
    )
    .await;
    s.call("recall", json!({"subject": SURVIVOR}))
        .await
        .says("Kettle Mk Two");
    story.finish().await;
}
