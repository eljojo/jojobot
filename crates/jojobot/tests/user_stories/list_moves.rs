//! "I put Homer first on the rotation, and the list kept him there."
//!
//! **A deliberate move of an entry holds.** `update_fact` with `after` on an
//! entry of a list puts it first, straight after another entry, or at the end,
//! and the list reads that way until the entry is next ticked. Then the
//! rotation, or the checklist, goes on as its setting says. The server mints the
//! entry's place; a caller never sends one.
//!
//! The roster names are the cast: a camping trip, a packing list, three people.

use serde_json::{Value, json};

use super::dsl::{Answer, Session, Story};
use super::lists::{a_list_on_the_trip, add_entry, read_back};

/// One tick, as a caller sends it.
fn ticking(list: &str, entry: &str, act: &str) -> Value {
    json!({"subject": list, "content": format!("{act} it"), "provenance": "testimony",
           "tick": {"entry": entry, "act": act}})
}

/// A move of an existing entry: where it goes is `first`, `last`, or the address
/// of the entry it should follow.
async fn moving(s: &Session, entry: &str, after: &str) -> Answer {
    s.call("update_fact", json!({"address": entry, "after": after}))
        .await
}

/// A rotation of three, Homer then Bart then Lisa, after Homer took his turn:
/// it reads Bart, Lisa, Homer. The addresses are Homer's, Bart's and Lisa's.
async fn a_rotation_after_a_turn(s: &Session) -> (String, String, String) {
    a_list_on_the_trip(s, "packing", json!({"tick": "back"})).await;
    let homer = add_entry(s, "list:packing", "Homer", None).await;
    let bart = add_entry(s, "list:packing", "Bart", None).await;
    let lisa = add_entry(s, "list:packing", "Lisa", None).await;
    s.call("capture", ticking("list:packing", &homer, "done"))
        .await;
    assert_eq!(
        read_back(s, "list:packing").await,
        ["Bart", "Lisa", "Homer"],
        "the setup: Homer took his turn and went to the end"
    );
    (homer, bart, lisa)
}

/// **An entry moved to the front reads first until it is ticked; then the
/// rotation resumes.** The negative is the read before the move, and the turns
/// that follow go on from where they were.
#[tokio::test]
async fn a_back_list_reads_a_moved_entry_first_until_it_is_ticked_and_then_resumes() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (homer, bart, _lisa) = a_rotation_after_a_turn(&s).await;

    moving(&s, &homer, "first").await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Homer", "Bart", "Lisa"]
    );

    s.call("capture", ticking("list:packing", &homer, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Bart", "Lisa", "Homer"],
        "the rotation resumes when Homer is next ticked"
    );
    s.call("capture", ticking("list:packing", &bart, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Lisa", "Homer", "Bart"]
    );
    story.finish().await;
}

/// **An entry put after another reads right behind it**, even when the other has
/// taken a turn, and the hold lasts through other entries' turns.
#[tokio::test]
async fn a_back_list_reads_an_entry_put_after_another_behind_it_until_it_is_ticked() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (homer, bart, _lisa) = a_rotation_after_a_turn(&s).await;

    moving(&s, &homer, &bart).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Bart", "Homer", "Lisa"]
    );

    s.call("capture", ticking("list:packing", &bart, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Homer", "Lisa", "Bart"],
        "Bart's turn does not undo the hold on Homer"
    );
    s.call("capture", ticking("list:packing", &homer, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Lisa", "Bart", "Homer"],
        "Homer's own turn ends the hold: the rotation goes by turns again"
    );
    story.finish().await;
}

/// **An entry sent to the end reads last, behind the entries that took a turn.**
#[tokio::test]
async fn a_back_list_reads_an_entry_sent_to_the_end_last_until_it_is_ticked() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let (_homer, bart, _lisa) = a_rotation_after_a_turn(&s).await;

    moving(&s, &bart, "last").await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Lisa", "Homer", "Bart"]
    );

    s.call("capture", ticking("list:packing", &bart, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["Lisa", "Homer", "Bart"],
        "Bart's turn puts him at the end, which is where the move left him"
    );
    story.finish().await;
}

/// **On a checklist and on a queue a move puts the entry where it was asked,
/// and a tick does what the list is for.**
#[tokio::test]
async fn a_move_holds_on_sink_and_leave_lists_and_a_tick_does_what_the_list_is_for() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;
    let lantern = add_entry(&s, "list:packing", "lantern", None).await;

    moving(&s, &lantern, "first").await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["lantern", "tent", "stove"]
    );
    moving(&s, &tent, "last").await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["lantern", "stove", "tent"]
    );
    moving(&s, &stove, &tent).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["lantern", "tent", "stove"]
    );
    s.call("capture", ticking("list:packing", &lantern, "done"))
        .await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["tent", "stove", "lantern"],
        "a done entry sinks"
    );

    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "errands", "name": "Errands",
               "source": "user-named", "parent": "thing:springfield-camping",
               "sets": {"tick": "leave"}}),
    )
    .await;
    let post = add_entry(&s, "list:errands", "post", None).await;
    add_entry(&s, "list:errands", "bank", None).await;
    let shop = add_entry(&s, "list:errands", "shop", None).await;
    moving(&s, &shop, &post).await;
    assert_eq!(
        read_back(&s, "list:errands").await,
        ["post", "shop", "bank"]
    );
    s.call("capture", ticking("list:errands", &shop, "done"))
        .await;
    assert_eq!(read_back(&s, "list:errands").await, ["post", "bank"]);
    story.finish().await;
}

/// **A move is refused, naming the way forward, when it cannot be honoured, and
/// nothing is written.** The positive is a move that lands on the same list.
#[tokio::test]
async fn a_move_that_cannot_be_honoured_is_refused_and_writes_nothing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;
    let lantern = add_entry(&s, "list:packing", "lantern", None).await;
    s.call("capture", ticking("list:packing", &tent, "done"))
        .await;
    let before = read_back(&s, "list:packing").await;
    assert_eq!(before, ["stove", "lantern", "tent"]);

    // A stranger is named with the entries there are.
    let stranger = s
        .refused(
            "update_fact",
            json!({"address": stove, "after": "list:packing#f99"}),
        )
        .await;
    stranger.says("list:packing#f99").says("\"wrote\":false");
    // An entry cannot follow itself.
    s.refused("update_fact", json!({"address": stove, "after": stove}))
        .await
        .says("itself");
    // A done entry has no place to move to, and none to follow.
    s.refused("update_fact", json!({"address": tent, "after": "first"}))
        .await
        .says("done");
    s.refused("update_fact", json!({"address": stove, "after": tent}))
        .await
        .says("done");
    // The server's own key is never sent by a caller.
    s.refused(
        "update_fact",
        json!({"address": stove, "fields": {"order": "5"}}),
    )
    .await
    .says("after");
    assert_eq!(
        read_back(&s, "list:packing").await,
        before,
        "a refused move wrote nothing"
    );

    // The same call on a real place lands.
    moving(&s, &lantern, "first").await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["lantern", "stove", "tent"]
    );
    story.finish().await;
}

/// **A move needs no say-so from the session that wrote the entry.** The entry's
/// words are the operator's testimony and stay as they were; where it sits is the
/// server's to mint. A different session moves it, and an edit of its words by
/// that session is still refused.
#[tokio::test]
async fn another_session_moves_an_entry_without_touching_its_words() {
    let story = Story::begin("bot:gamma").await;
    let writer = story.session().await;
    a_list_on_the_trip(&writer, "packing", json!({"tick": "back"})).await;
    let homer = add_entry(&writer, "list:packing", "Homer", None).await;
    add_entry(&writer, "list:packing", "Bart", None).await;
    writer
        .call("capture", ticking("list:packing", &homer, "done"))
        .await;

    let mover = story.session_in("UTC", Some("new")).await;
    moving(&mover, &homer, "first").await;
    assert_eq!(read_back(&mover, "list:packing").await, ["Homer", "Bart"]);

    mover
        .refused(
            "update_fact",
            json!({"address": homer, "content": "Homer, rewritten"}),
        )
        .await
        .says("\"wrote\":false");
    assert_eq!(read_back(&mover, "list:packing").await, ["Homer", "Bart"]);
    story.finish().await;
}
