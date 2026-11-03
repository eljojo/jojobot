//! "I deleted Nelson straight out of the database. Does jojobot still show him?"
//!
//! A person running SQL by hand, or an admin tool, can erase a thing under the
//! store. The server holds the store's content in memory, so it keeps serving
//! the thing until it reads the store again, as a restart does. After that read
//! neither `recall` nor `search` may find it, and a thing nobody erased must
//! still be found by the same search.

use super::dsl::Story;

#[tokio::test]
async fn a_thing_erased_under_the_store_is_no_longer_served_after_the_store_is_read_again() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let s = story.session().await;
    s.add("person:nelson", "Nelson").await;
    s.fact("person:nelson", "laughs at everything").await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact("person:milhouse", "laughs at nothing").await;

    // ── before · both are served ────────────────────────────────────────────
    s.recall("person:nelson").await.says("laughs at everything");
    s.find("laughs").await.says("person:nelson");

    // ── the erasure · under the store, then the server reads it again ───────
    store.deletes_the_thing("person:nelson");
    story.the_store_changed_underneath().await;

    // ── after · the erased thing is gone from both doors, and the miss is
    // named. The thing nobody erased is in the same search, so an empty
    // answer cannot pass for a search wired to nothing.
    let recalled = s.recall("person:nelson").await;
    recalled.says("blocked");
    recalled.never_says("laughs at everything");
    let found = s.find("laughs").await;
    found.says("person:milhouse");
    found.never_says("person:nelson");
    story.finish().await;
}
