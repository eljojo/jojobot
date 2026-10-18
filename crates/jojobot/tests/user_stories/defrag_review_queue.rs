//! "What has changed since defrag last looked?"
//!
//! Agents write as they always did. jojobot keeps track of what defrag has not
//! reviewed, so defrag's next session starts on it. When defrag writes to a
//! thing, or marks it reviewed, the thing leaves the queue; when anyone else
//! writes to it again, it comes back. The stamp that says so is jojobot's own and
//! is never in an ordinary read.
//!
//! **The negatives make the positives mean something.** A caller cannot write the
//! stamp, another bot cannot mark a thing reviewed, and the stamp's own write
//! does not put a thing back in the queue.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The handles an answer lists, in order.
fn listed(answer: &Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

/// What defrag's queue holds now.
async fn queue(defrag: &Session) -> Vec<String> {
    listed(&defrag.call("recall", json!({"unreviewed": true})).await)
}

/// What defrag's own boot fronts: the queue block of its identity.
async fn defrags_boot_queue(story: &Story) -> serde_json::Value {
    let (booted, _) = story
        .call(
            "start_here",
            json!({"bot": "defrag", "brief": true, "resume": "new"}),
        )
        .await;
    booted.json()["identity"]["review_queue"].clone()
}

/// The handles on the first page of a boot's queue block, in order.
fn first_page(fronted: &serde_json::Value) -> Vec<String> {
    fronted["first"]
        .as_array()
        .expect("the first page")
        .iter()
        .map(|thing| thing["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_thing_another_bot_writes_to_waits_for_defrag_until_defrag_writes_or_marks_it() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("person:homer", "Homer").await;
    dev.add("person:bart", "Bart").await;
    let defrag = story.as_bot("bot:defrag").await;

    // ── a thing another bot writes appears in the queue ─────────────────────
    dev.call(
        "capture",
        json!({"subject": "person:homer", "content": "likes donuts", "provenance": "testimony"}),
    )
    .await;
    let waiting = queue(&defrag).await;
    assert!(waiting.contains(&"person:homer".to_string()), "{waiting:?}");
    assert!(waiting.contains(&"person:bart".to_string()), "{waiting:?}");

    // ── defrag writes to a thing and it leaves ──────────────────────────────
    defrag
        .call(
            "capture",
            json!({"subject": "person:homer", "content": "tidied", "provenance": "inference"}),
        )
        .await;
    let after_write = queue(&defrag).await;
    assert!(
        !after_write.contains(&"person:homer".to_string()),
        "the stamp's own write does not put the thing back: {after_write:?}"
    );
    assert!(
        after_write.contains(&"person:bart".to_string()),
        "{after_write:?}"
    );

    // ── defrag marks another reviewed, changing nothing else, and it leaves ─
    defrag
        .call(
            "update_entity",
            json!({"handle": "person:bart", "reviewed": true}),
        )
        .await;
    let after_mark = queue(&defrag).await;
    assert!(
        !after_mark.contains(&"person:bart".to_string()),
        "{after_mark:?}"
    );

    // ── anyone else writes again and it returns ─────────────────────────────
    dev.call(
        "capture",
        json!({"subject": "person:homer", "content": "likes beer too", "provenance": "testimony"}),
    )
    .await;
    let back = queue(&defrag).await;
    assert!(back.contains(&"person:homer".to_string()), "{back:?}");
    assert!(!back.contains(&"person:bart".to_string()), "{back:?}");

    // ── the shipped view and defrag's boot name the same queue ──────────────
    let by_view = listed(&defrag.call("recall", json!({"view": "unreviewed"})).await);
    assert_eq!(by_view, back, "the view asks the same question");
    // The premise the page cut rests on: the queue is longer than one page (the
    // shipped views wait for defrag too), so ten are shown and the rest named.
    assert!(
        back.len() > 10,
        "the queue is longer than a page, or the cut below is never reached: {back:?}"
    );
    let fronted = defrags_boot_queue(&story).await;
    assert_eq!(
        fronted["count"].as_u64(),
        Some(back.len() as u64),
        "defrag's boot names how many wait: {fronted}"
    );
    assert_eq!(
        first_page(&fronted),
        back[..10].to_vec(),
        "…and the first ten of them, in the queue's order"
    );
    assert!(
        fronted["more"].is_string(),
        "a longer queue names the call that reads the rest: {fronted}"
    );

    // ── the stamp is jojobot's: nobody writes it, and only defrag marks ─────
    let written = dev
        .refused(
            "capture",
            json!({"subject": "person:homer", "content": "x", "provenance": "testimony",
                   "sets": {"defragged_at": "2026-01-01T00:00:00Z"}}),
        )
        .await;
    written.says("defragged_at");
    let by_defrag = defrag
        .refused(
            "capture",
            json!({"subject": "person:homer", "content": "x", "provenance": "inference",
                   "sets": {"defragged_at": "2026-01-01T00:00:00Z"}}),
        )
        .await;
    by_defrag.says("defragged_at");
    let created = dev
        .refused(
            "add_entity",
            json!({"kind": "person", "handle": "milhouse", "name": "Milhouse", "source": "user-named",
                   "sets": {"defragged_at": "2026-01-01T00:00:00Z"}}),
        )
        .await;
    created.says("defragged_at").says("\"wrote\":false");
    let edited = dev
        .refused(
            "update_fact",
            json!({"address": "person:homer#f1", "sets": {"defragged_at": "2026-01-01T00:00:00Z"}}),
        )
        .await;
    edited.says("defragged_at").says("\"wrote\":false");
    let set = dev
        .refused(
            "update_entity",
            json!({"handle": "person:homer", "sets": {"defragged_at": "2026-01-01T00:00:00Z"}}),
        )
        .await;
    set.says("defragged_at").says("\"wrote\":false");
    let marked = dev
        .refused(
            "update_entity",
            json!({"handle": "person:homer", "reviewed": true}),
        )
        .await;
    marked.holds_at("/fix_by", "change").says("\"wrote\":false");

    // ── the stamp stays out of ordinary reads and answers when asked for ────
    let plain = dev
        .call("recall", json!({"subject": "person:bart"}))
        .await
        .json();
    assert!(
        plain["objects"][0]["fields"].get("defragged_at").is_none(),
        "{plain}"
    );
    let asked = dev
        .call(
            "recall",
            json!({"subject": "person:bart", "keys": ["defragged_at"]}),
        )
        .await
        .json();
    assert!(
        asked["objects"][0]["fields"]["defragged_at"].is_string(),
        "asked for by name it is there: {asked}"
    );

    story.finish().await;
}

/// What `recall` lists as the live records of a thing, by their words, the
/// review mark among them.
async fn marks_of(reader: &Session, handle: &str) -> usize {
    let answer = reader
        .call(
            "recall",
            json!({"subject": handle, "facts": true, "review_marks": true}),
        )
        .await
        .json();
    answer["objects"][0]["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .filter(|fact| is_a_mark(fact))
        .count()
}

#[tokio::test]
async fn defrag_marks_a_full_thread_reviewed_with_one_standing_record_that_takes_no_slot() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("thread:milhouse-moves", "Milhouse moves").await;
    let defrag = story.as_bot("bot:defrag").await;
    for note in ["first", "second", "third"] {
        dev.call(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": note, "provenance": "testimony"}),
        )
        .await;
    }
    // The premise: the thread holds its three live records, so one more is
    // refused. Without it the mark below proves nothing about the ceiling.
    let full = dev
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "fourth", "provenance": "testimony"}),
        )
        .await;
    assert_eq!(full.json()["live"].as_u64(), Some(3), "{}", full.raw());
    assert!(
        queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string())
    );

    // ── the mark lands on the full thread and takes it out of the queue ─────
    defrag
        .call(
            "update_entity",
            json!({"handle": "thread:milhouse-moves", "reviewed": true}),
        )
        .await;
    assert!(
        !queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string()),
        "a full thread leaves the queue when defrag marks it"
    );
    // …and the mark took no slot: the count reads three and a fourth ordinary
    // capture is still refused.
    let still_full = dev
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "fourth", "provenance": "testimony"}),
        )
        .await;
    assert_eq!(
        still_full.json()["live"].as_u64(),
        Some(3),
        "{}",
        still_full.raw()
    );

    // ── another bot rewrites a record, the thing returns, and a second mark
    //    rewrites the same record rather than adding one ────────────────────
    dev.call(
        "update_fact",
        json!({"address": "thread:milhouse-moves#f1", "content": "first, corrected"}),
    )
    .await;
    assert!(
        queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string())
    );
    assert_eq!(marks_of(&defrag, "thread:milhouse-moves").await, 1);
    defrag
        .call(
            "update_entity",
            json!({"handle": "thread:milhouse-moves", "reviewed": true}),
        )
        .await;
    assert!(
        !queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string())
    );
    assert_eq!(
        marks_of(&defrag, "thread:milhouse-moves").await,
        1,
        "a second mark adds no record"
    );

    // ── the exemption is the mark's alone: defrag's ordinary write counts ───
    let ordinary = defrag
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "tidied", "provenance": "inference"}),
        )
        .await;
    assert_eq!(
        ordinary.json()["live"].as_u64(),
        Some(3),
        "{}",
        ordinary.raw()
    );
    // …and a caller cannot forge the mark to dodge the ceiling.
    let forged = dev
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "fifth", "provenance": "testimony",
                   "fields": {"review_mark": "true"}}),
        )
        .await;
    forged.says("review_mark");
    story.finish().await;
}

#[tokio::test]
async fn defrag_marks_a_thread_that_borrowed_over_its_ceiling_and_the_mark_takes_no_slot() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("thread:milhouse-moves", "Milhouse moves").await;
    let defrag = story.as_bot("bot:defrag").await;
    for note in ["first", "second", "third"] {
        dev.call(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": note, "provenance": "testimony"}),
        )
        .await;
    }
    // The premise: the reserve puts a fourth live record on the thread, so it
    // holds more than its ceiling. A second borrow is refused at four.
    dev.call(
        "capture",
        json!({"subject": "thread:milhouse-moves", "content": "borrowed",
               "provenance": "testimony", "borrow": true}),
    )
    .await;
    let over = dev
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "fifth",
                   "provenance": "testimony", "borrow": true}),
        )
        .await;
    assert_eq!(over.json()["live"].as_u64(), Some(4), "{}", over.raw());
    assert!(
        queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string())
    );

    // ── the first mark lands on the thread that is over, and it leaves ──────
    defrag
        .call(
            "update_entity",
            json!({"handle": "thread:milhouse-moves", "reviewed": true}),
        )
        .await;
    assert!(
        !queue(&defrag)
            .await
            .contains(&"thread:milhouse-moves".to_string()),
        "a thread over its ceiling leaves the queue when defrag marks it"
    );
    // …and the mark took no slot: the count still reads four.
    let still_over = dev
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "sixth", "provenance": "testimony"}),
        )
        .await;
    assert_eq!(
        still_over.json()["live"].as_u64(),
        Some(4),
        "{}",
        still_over.raw()
    );
    story.finish().await;
}

#[tokio::test]
async fn a_thing_defrag_creates_is_reviewed_and_a_thing_another_bot_creates_waits() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    let defrag = story.as_bot("bot:defrag").await;

    dev.add("person:bart", "Bart").await;
    defrag.add("person:lisa", "Lisa").await;

    let waiting = queue(&defrag).await;
    assert!(
        waiting.contains(&"person:bart".to_string()),
        "a thing another bot creates waits: {waiting:?}"
    );
    assert!(
        !waiting.contains(&"person:lisa".to_string()),
        "a thing defrag creates is stamped in the same act: {waiting:?}"
    );

    // …and another bot's write to it brings it back, as to any reviewed thing.
    dev.call(
        "capture",
        json!({"subject": "person:lisa", "content": "plays the saxophone", "provenance": "testimony"}),
    )
    .await;
    assert!(queue(&defrag).await.contains(&"person:lisa".to_string()));
    story.finish().await;
}

/// The facts `recall` lists for a thing, whole, and the object that carries them.
async fn recalled(reader: &Session, handle: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut args = json!({"subject": handle, "facts": true});
    for (key, value) in extra.as_object().expect("an object") {
        args[key] = value.clone();
    }
    reader.call("recall", args).await.json()["objects"][0].clone()
}

/// Whether a listed record is a thing's review mark, by the label it carries.
fn is_a_mark(fact: &serde_json::Value) -> bool {
    fact["fields"]["review_mark"].is_string()
}

#[tokio::test]
async fn a_read_of_a_marked_thing_leaves_the_review_mark_out_and_names_the_argument_that_returns_it()
 {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("person:bart", "Bart").await;
    dev.add("person:lisa", "Lisa").await;
    let defrag = story.as_bot("bot:defrag").await;
    for handle in ["person:bart", "person:lisa"] {
        dev.call(
            "capture",
            json!({"subject": handle, "content": "plays the saxophone", "provenance": "testimony"}),
        )
        .await;
    }
    defrag
        .call(
            "update_entity",
            json!({"handle": "person:bart", "reviewed": true}),
        )
        .await;

    // ── (1) the listing carries the ordinary record and not the mark ───────
    let plain = recalled(&dev, "person:bart", json!({})).await;
    let facts = plain["facts"].as_array().expect("facts");
    assert!(
        facts.iter().any(|f| f["content"]
            .as_str()
            .is_some_and(|c| c.contains("saxophone"))),
        "the ordinary record is listed: {plain}"
    );
    assert!(
        !facts.iter().any(is_a_mark),
        "the review mark is not listed by default: {plain}"
    );
    let left_out = &plain["review_marks_left_out"];
    assert_eq!(left_out["count"].as_u64(), Some(1), "{plain}");
    assert!(
        left_out["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("review_marks")),
        "the answer names the argument that returns the mark: {plain}"
    );

    // ── (3) with the argument, the mark is back and nothing is left out ─────
    let asked = recalled(&dev, "person:bart", json!({"review_marks": true})).await;
    assert!(
        asked["facts"]
            .as_array()
            .expect("facts")
            .iter()
            .any(is_a_mark),
        "the mark is listed when asked for: {asked}"
    );
    assert!(asked.get("review_marks_left_out").is_none(), "{asked}");

    // ── a thing nobody marked says nothing about marks ──────────────────────
    let unmarked = recalled(&dev, "person:lisa", json!({})).await;
    assert!(
        unmarked.get("review_marks_left_out").is_none(),
        "{unmarked}"
    );

    // ── (4) the queue and the mark verb are as they were ────────────────────
    assert!(
        !queue(&defrag).await.contains(&"person:bart".to_string()),
        "the marked thing is still out of the queue"
    );
    assert!(queue(&defrag).await.contains(&"person:lisa".to_string()));
    defrag
        .call(
            "update_entity",
            json!({"handle": "person:lisa", "reviewed": true}),
        )
        .await;
    assert!(!queue(&defrag).await.contains(&"person:lisa".to_string()));
    story.finish().await;
}

#[tokio::test]
async fn a_search_leaves_the_review_mark_out_names_it_in_left_out_and_returns_it_when_asked() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("person:bart", "Bart").await;
    let defrag = story.as_bot("bot:defrag").await;
    dev.call(
        "capture",
        json!({"subject": "person:bart", "content": "reviewed the donut budget", "provenance": "testimony"}),
    )
    .await;
    defrag
        .call(
            "update_entity",
            json!({"handle": "person:bart", "reviewed": true}),
        )
        .await;
    let hits = |answer: &serde_json::Value| -> Vec<serde_json::Value> {
        answer["results"].as_array().expect("results").clone()
    };

    // ── (2) the ordinary record is a hit and the mark is not ────────────────
    let plain = dev
        .call("search", json!({"query": "reviewed", "kind": "person"}))
        .await
        .json();
    let found = hits(&plain);
    assert!(
        found
            .iter()
            .any(|h| h["content"].as_str().is_some_and(|c| c.contains("donut"))),
        "the ordinary record matches: {plain}"
    );
    assert!(
        !found.iter().any(is_a_mark),
        "no mark among the hits: {plain}"
    );
    assert!(
        plain["left_out"]
            .as_array()
            .expect("left_out")
            .iter()
            .any(|line| line.as_str().is_some_and(|l| l.contains("review_marks"))),
        "left_out names the mark and the argument: {plain}"
    );

    // ── (3) with the argument, the mark is a hit and left_out stops naming it ─
    let asked = dev
        .call(
            "search",
            json!({"query": "reviewed", "kind": "person", "review_marks": true}),
        )
        .await
        .json();
    assert!(hits(&asked).iter().any(is_a_mark), "{asked}");
    assert!(
        !asked["left_out"]
            .as_array()
            .expect("left_out")
            .iter()
            .any(|line| line.as_str().is_some_and(|l| l.contains("review_marks"))),
        "{asked}"
    );
    story.finish().await;
}
