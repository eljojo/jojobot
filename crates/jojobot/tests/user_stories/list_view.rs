//! "Which of my lists are moving, and how far along is each?"
//!
//! **One saved question lists every list.** Each answers with its name, what it
//! hangs on, how it is ticked, how far along it is by its own ticks and the day
//! it was last ticked. A list hung on one thing is asked for by naming that thing.
//!
//! The roster names are the cast: a camping trip, a packing list, a rota for Homer.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// A list under `parent`, with this tick setting.
async fn a_list(s: &Session, handle: &str, parent: &str, tick: &str) {
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": handle, "name": handle,
               "source": "user-named", "parent": parent, "sets": {"tick": tick}}),
    )
    .await;
}

/// The addresses a batch of entries landed at, in order.
async fn entries(s: &Session, list: &str, texts: &[&str]) -> Vec<String> {
    let receipt = s
        .call(
            "capture",
            json!({"subject": list, "provenance": "testimony", "entries": texts}),
        )
        .await;
    receipt.json()["entries"]
        .as_array()
        .expect("the receipt lists the entries")
        .iter()
        .map(|entry| entry["address"].as_str().expect("an address").to_string())
        .collect()
}

/// One tick act on each of these entries, recorded on `day`.
async fn ticks(s: &Session, list: &str, at: &[String], act: &str, day: &str) {
    let items: Vec<Value> = at
        .iter()
        .map(|entry| json!({"entry": entry, "act": act}))
        .collect();
    s.call(
        "capture",
        json!({"subject": list, "provenance": "testimony", "recorded_at": day,
               "ticks": items}),
    )
    .await;
}

/// The cast: three lists on three kinds of parent, each ticked on its own days.
async fn three_lists(s: &Session) {
    s.add("thing:springfield-camping", "Springfield camping")
        .await;
    s.add("person:homer", "Homer").await;
    a_list(s, "list:packing", "thing:springfield-camping", "sink").await;
    a_list(s, "list:rotation", "person:homer", "back").await;
    a_list(s, "list:errands", "bot:gamma", "leave").await;

    let packing = entries(s, "list:packing", &["tent", "stove", "lantern", "kettle"]).await;
    ticks(s, "list:packing", &packing[..1], "done", "2026-10-10").await;

    let shifts = entries(s, "list:rotation", &["monday", "tuesday", "wednesday"]).await;
    ticks(s, "list:rotation", &shifts[..1], "rotated", "2026-10-12").await;
    ticks(s, "list:rotation", &shifts[1..2], "rotated", "2026-10-13").await;
    ticks(s, "list:rotation", &shifts[2..], "rotated", "2026-10-14").await;

    let reading = entries(
        s,
        "list:errands",
        &["one", "two", "three", "four", "five", "six", "seven"],
    )
    .await;
    ticks(s, "list:errands", &reading[..1], "done", "2026-10-11").await;
    ticks(s, "list:errands", &reading[1..2], "done", "2026-10-13").await;
}

/// The object in a view's answer for this list.
fn the<'a>(answer: &'a Value, handle: &str) -> &'a Value {
    answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a view answers with objects: {answer}"))
        .iter()
        .find(|object| object["id"] == handle)
        .unwrap_or_else(|| panic!("{handle} is in the answer: {answer}"))
}

/// **The lists view answers every list with its progress.** A checklist says done
/// out of all, a queue says done and open, a rotation says its turns, and each
/// says the day it was last ticked.
#[tokio::test]
async fn the_lists_view_answers_every_list_with_how_far_along_it_is() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;

    let answer = s.call("recall", json!({"view": "lists"})).await.json();
    assert_eq!(
        answer["objects"].as_array().map(Vec::len),
        Some(3),
        "{answer}"
    );

    let packing = the(&answer, "list:packing");
    assert_eq!(packing["ticked"]["done"], 1, "{packing}");
    assert_eq!(packing["ticked"]["of"], 4, "{packing}");
    assert_eq!(packing["last_ticked"], "2026-10-10", "{packing}");
    assert_eq!(packing["fields"]["tick"], "sink", "{packing}");
    assert_eq!(packing["parent"], "thing:springfield-camping", "{packing}");
    assert_eq!(packing["name"], "list:packing", "{packing}");

    let shifts = the(&answer, "list:rotation");
    assert_eq!(shifts["ticked"]["turns"], 3, "{shifts}");
    assert_eq!(shifts["last_ticked"], "2026-10-14", "{shifts}");
    assert_eq!(shifts["fields"]["tick"], "back", "{shifts}");

    let reading = the(&answer, "list:errands");
    assert_eq!(reading["ticked"]["done"], 2, "{reading}");
    assert_eq!(reading["ticked"]["open"], 5, "{reading}");
    assert_eq!(reading["last_ticked"], "2026-10-13", "{reading}");
    assert_eq!(reading["fields"]["tick"], "leave", "{reading}");
    story.finish().await;
}

/// **`under` narrows the view to the lists hung on one thing.** Homer's rota
/// alone, and the paired negative: the other two lists are not in it.
#[tokio::test]
async fn the_lists_view_under_a_parent_answers_only_the_lists_hung_on_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;

    let answer = s
        .call(
            "recall",
            json!({"view": "lists", "inputs": {"under": "person:homer"}}),
        )
        .await;
    let json = answer.json();
    assert_eq!(json["objects"].as_array().map(Vec::len), Some(1), "{json}");
    the(&json, "list:rotation");
    answer.never_says("list:packing").never_says("list:errands");
    story.finish().await;
}

/// **An archived list is left out and counted.** The count names what it left out,
/// and the lists that stand still answer.
#[tokio::test]
async fn an_archived_list_is_left_out_of_the_lists_view_and_counted() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;
    s.call(
        "archive_entity",
        json!({"handle": "list:errands", "reason": "finished the books"}),
    )
    .await;

    let answer = s.call("recall", json!({"view": "lists"})).await;
    let json = answer.json();
    assert_eq!(json["objects"].as_array().map(Vec::len), Some(2), "{json}");
    answer.never_says("list:errands");
    assert_eq!(json["archived_excluded"], 1, "{json}");
    story.finish().await;
}

/// **A read that names keys gets a list's blocks only when it names them.** A
/// plain read of a list is unchanged; keys naming `tick` and `ticked` bring the
/// setting and the count and no entries, and say what they left out and how to
/// read it; keys naming `entries` bring the entries and nothing else of the
/// blocks.
#[tokio::test]
async fn a_list_read_that_names_keys_gets_only_the_blocks_it_names() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;

    // The regression: no keys, the whole list as it always was.
    let plain = s
        .call("recall", json!({"subject": "list:rotation"}))
        .await
        .json();
    let whole = &plain["objects"][0];
    assert_eq!(
        whole["entries"].as_array().map(Vec::len),
        Some(3),
        "{plain}"
    );
    assert_eq!(whole["ticked"]["turns"], 3, "{plain}");
    assert_eq!(whole["last_ticked"], "2026-10-14", "{plain}");

    // Keys naming the setting and the count: those, and no entries.
    let counted = s
        .call(
            "recall",
            json!({"subject": "list:rotation", "keys": ["tick", "ticked"]}),
        )
        .await
        .json();
    let object = &counted["objects"][0];
    assert_eq!(object["fields"]["tick"], "back", "{counted}");
    assert_eq!(object["ticked"]["turns"], 3, "{counted}");
    assert!(
        object.get("entries").is_none(),
        "entries came back: {counted}"
    );
    assert!(object.get("last_ticked").is_none(), "{counted}");
    let left_out = counted["narrowed"]["left_out_keys"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer names what it left out: {counted}"));
    assert!(left_out.contains(&json!("entries")), "{counted}");
    assert!(left_out.contains(&json!("last_ticked")), "{counted}");
    // **The advice says a list's blocks are keys**: "ask again without keys" alone
    // sent a caller back to a full read, and never said `entries` could be named.
    let advice = counted["narrowed"]["fields"]
        .as_str()
        .unwrap_or_else(|| panic!("the narrowed note carries its advice: {counted}"));
    assert!(advice.contains("entries"), "{advice}");

    // Keys naming the entries: the entries, and not the count.
    let listed = s
        .call(
            "recall",
            json!({"subject": "list:rotation", "keys": ["entries"]}),
        )
        .await
        .json();
    let object = &listed["objects"][0];
    assert_eq!(
        object["entries"].as_array().map(Vec::len),
        Some(3),
        "{listed}"
    );
    assert!(object.get("ticked").is_none(), "{listed}");
    story.finish().await;
}

/// **The lists view answers each list without its entries**, so an overview of
/// many lists stays small: the name, what it hangs on, the tick setting, the
/// count and the last day, and nothing of the entries.
#[tokio::test]
async fn the_lists_view_carries_no_entries() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;

    let answer = s.call("recall", json!({"view": "lists"})).await.json();
    for handle in ["list:packing", "list:rotation", "list:errands"] {
        let object = the(&answer, handle);
        assert!(
            object.get("entries").is_none(),
            "{handle} carries its entries: {object}"
        );
        assert!(object["ticked"].is_object(), "{handle}: {object}");
        assert!(object["last_ticked"].is_string(), "{handle}: {object}");
        assert!(object["fields"]["tick"].is_string(), "{handle}: {object}");
        assert!(object["parent"].is_string(), "{handle}: {object}");
    }
    story.finish().await;
}

/// The cells of the row a page table holds for this handle.
fn cells_of(table: &str, handle: &str) -> Vec<String> {
    let row = table
        .split("<tr>")
        .find(|row| row.contains(&format!("{handle}/</a>")))
        .unwrap_or_else(|| panic!("the table has a row for {handle}: {table}"));
    row.split("<td>")
        .skip(1)
        .map(|cell| cell.split("</td>").next().unwrap_or_default().to_string())
        .collect()
}

/// **The words a list's ticks come to, as the page says them**, built from what
/// `recall` answered for the same list: a checklist says done of all, a queue
/// says done and open, a rotation says its turns.
fn ticked_in_words(list: &Value) -> String {
    let ticked = &list["ticked"];
    match (
        ticked["of"].as_u64(),
        ticked["open"].as_u64(),
        ticked["turns"].as_u64(),
    ) {
        (Some(of), _, _) => format!("{} of {of} done", ticked["done"]),
        (_, Some(open), _) => format!("{} done, {open} open", ticked["done"]),
        (_, _, Some(turns)) => format!("{turns} turns"),
        _ => String::new(),
    }
}

/// **The page of the lists view shows what `recall` of it answers.** Each list's
/// ticks and the day it was last ticked are in its row, taken from the values
/// the verb gave for the same lists, and a list nobody ticked has empty cells. A
/// page of a view that selects no list carries neither column.
#[tokio::test]
async fn the_lists_page_shows_how_far_along_each_list_is_as_recall_does() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    three_lists(&s).await;
    a_list(&s, "list:checklist", "person:homer", "sink").await;

    let answer = s.call("recall", json!({"view": "lists"})).await.json();
    let page = story.page("view:lists").await.section("answer");
    page.says("Ticked").says("Last ticked");

    for handle in ["list:packing", "list:rotation", "list:errands"] {
        let list = the(&answer, handle);
        let cells = cells_of(page.raw(), handle);
        let (words, day) = (ticked_in_words(list), list["last_ticked"].as_str().unwrap());
        assert!(
            cells.contains(&words) && cells.contains(&day.to_string()),
            "the row for {handle} should hold {words:?} and {day:?}: {cells:?}"
        );
    }

    // The control: a list with no entry reads no ticks, and its row says none.
    let untouched = the(&answer, "list:checklist");
    assert!(untouched.get("ticked").is_none(), "{untouched}");
    assert!(untouched.get("last_ticked").is_none(), "{untouched}");
    let cells = cells_of(page.raw(), "list:checklist");
    assert!(cells.iter().all(|cell| !cell.contains("2026")), "{cells:?}");

    // A view that selects no list has no tick columns.
    let colleagues = story.page("view:colleagues").await.section("answer");
    colleagues.never_says("Last ticked");
    story.finish().await;
}
