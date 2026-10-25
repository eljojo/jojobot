//! "The view of what the twelfth rule governs, saved by its number."
//!
//! **A saved view's `where` may name a decision by its number**, the way a live
//! read of the same line does. The value is bound to the decision it names, so a
//! rename of the decision changes nothing the view selects, and a number nobody
//! holds is refused where the view is saved.

use serde_json::json;

use super::dsl::{Session, Story};

const RULES: &str = "topic:rules-springfield";

/// Springfield's numbered rules with its twelfth rule, and a ledger that records
/// which rule governs it.
async fn the_ledger_under_the_twelfth_rule(s: &Session) {
    s.add(RULES, "Springfield rules").await;
    s.call(
        "capture",
        json!({"subject": RULES, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "springfield-12", "name": "Leads",
               "source": "user-named", "parent": RULES, "sets": {"number": "12"}}),
    )
    .await;
    s.add("thing:omega", "Omega").await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "governed by the twelfth rule",
               "provenance": "testimony",
               "sets": {"ruled_by": "decision:springfield-12"}}),
    )
    .await;
}

fn ids(read: &serde_json::Value) -> Vec<String> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer has objects: {read}"))
        .iter()
        .map(|object| object["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// **The live read, the saved view and a rename agree.**
#[tokio::test]
async fn a_saved_view_names_a_decision_by_its_number_and_follows_its_rename() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    let line = format!("ruled_by = {RULES}/12");

    // The control: the decision named by its own handle selects the ledger.
    let direct = s
        .call(
            "recall",
            json!({"kind": "thing", "where": "ruled_by = decision:springfield-12"}),
        )
        .await;
    assert_eq!(ids(&direct.json()), ["thing:omega"], "{}", direct.raw());
    let live = s
        .call("recall", json!({"kind": "thing", "where": line}))
        .await;
    assert_eq!(ids(&live.json()), ["thing:omega"], "{}", live.raw());

    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "twelfth-rule-ledger", "name": "Twelfth rule ledger",
               "source": "user-named", "sets": {"selects": "thing", "where": line}}),
    )
    .await;
    let saved = s
        .call("recall", json!({"view": "twelfth-rule-ledger"}))
        .await;
    assert_eq!(ids(&saved.json()), ["thing:omega"], "{}", saved.raw());

    s.call(
        "rename_entity",
        json!({"handle": "decision:springfield-12", "to": "decision:springfield-13"}),
    )
    .await;
    let after = s
        .call("recall", json!({"view": "twelfth-rule-ledger"}))
        .await;
    assert_eq!(ids(&after.json()), ["thing:omega"], "{}", after.raw());
    story.finish().await;
}

/// **A number nobody holds is refused where the view is saved**, naming what it
/// found instead, and nothing is saved.
#[tokio::test]
async fn a_saved_view_naming_a_number_nobody_holds_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.refused(
        "add_entity",
        json!({"kind": "view", "handle": "ninety-ninth-rule-ledger", "name": "Ninety-ninth ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "where": format!("ruled_by = {RULES}/99")}}),
    )
    .await
    .says("\"wrote\":false");
    story.finish().await;
}

/// **A view that takes a decision as an input takes it by its number**, and the
/// kind it must be is the kind of the decision, not of the collection it is
/// addressed through. A number nobody holds is refused when the view is asked.
#[tokio::test]
async fn a_view_input_of_kind_decision_takes_a_decision_by_its_number() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "rulings-ledger", "name": "Rulings ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "inputs": "ruling: kind decision",
                        "where": "ruled_by = $ruling"}}),
    )
    .await;

    let by_handle = s
        .call(
            "recall",
            json!({"view": "rulings-ledger",
                   "inputs": {"ruling": "decision:springfield-12"}}),
        )
        .await;
    assert_eq!(
        ids(&by_handle.json()),
        ["thing:omega"],
        "{}",
        by_handle.raw()
    );
    let by_number = s
        .call(
            "recall",
            json!({"view": "rulings-ledger",
                   "inputs": {"ruling": format!("{RULES}/12")}}),
        )
        .await;
    assert_eq!(
        ids(&by_number.json()),
        ["thing:omega"],
        "{}",
        by_number.raw()
    );
    s.refused(
        "recall",
        json!({"view": "rulings-ledger", "inputs": {"ruling": format!("{RULES}/99")}}),
    )
    .await
    .says("names nothing");
    story.finish().await;
}

/// **A document that is a member of a numbered collection is read by its
/// number**, as `recall` reads it. A number nobody holds is an unknown thing.
#[tokio::test]
async fn a_document_that_is_a_numbered_member_is_read_by_its_number() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "document", "handle": "homer", "name": "Homer's minutes",
               "source": "user-named", "parent": RULES, "sets": {"number": "20"}}),
    )
    .await;
    s.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "# Minutes\n\nThe leads rule stands.\n"}),
    )
    .await;

    let direct = s
        .call("read_document", json!({"document": "document:homer"}))
        .await;
    assert_eq!(
        direct.json()["markdown"],
        "# Minutes\n\nThe leads rule stands.\n",
        "{}",
        direct.raw()
    );
    let by_number = s
        .call("read_document", json!({"document": format!("{RULES}/20")}))
        .await;
    assert_eq!(
        by_number.json()["markdown"],
        "# Minutes\n\nThe leads rule stands.\n",
        "{}",
        by_number.raw()
    );
    s.refused("read_document", json!({"document": format!("{RULES}/99")}))
        .await;
    story.finish().await;
}

/// **An edit sent a member's number reports an edit, as the same edit sent the
/// handle does.** The read before the write is of the member and not of nothing,
/// so the receipt never says the member was created.
#[tokio::test]
async fn an_edit_sent_a_members_number_reports_an_edit_and_not_a_creation() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    let by_number = s
        .call(
            "update_entity",
            json!({"handle": format!("{RULES}/12"), "name": "Leads in the square"}),
        )
        .await
        .json();
    assert_eq!(
        by_number["changed"],
        json!([{"on": "decision:springfield-12", "what": "name",
                "from": "Leads", "to": "Leads in the square"}]),
        "{by_number}"
    );
    let by_handle = s
        .call(
            "update_entity",
            json!({"handle": "decision:springfield-12", "name": "Leads on the square"}),
        )
        .await
        .json();
    assert_eq!(
        by_handle["changed"],
        json!([{"on": "decision:springfield-12", "what": "name",
                "from": "Leads in the square", "to": "Leads on the square"}]),
        "{by_handle}"
    );
    let archived = s
        .call(
            "archive_entity",
            json!({"handle": format!("{RULES}/12"), "reason": "repealed"}),
        )
        .await
        .json();
    assert!(
        !archived["changed"].to_string().contains("created"),
        "an archive of an existing member created nothing: {archived}"
    );
    story.finish().await;
}

/// **A document that is a numbered member is written by its number**, as it is
/// read.
#[tokio::test]
async fn a_document_that_is_a_numbered_member_is_written_by_its_number() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    s.call(
        "add_entity",
        json!({"kind": "document", "handle": "homer", "name": "Homer's minutes",
               "source": "user-named", "parent": RULES, "sets": {"number": "20"}}),
    )
    .await;
    s.call(
        "put_document",
        json!({"document": format!("{RULES}/20"), "markdown": "# Minutes\n\nThe rule stands.\n"}),
    )
    .await;
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await;
    assert_eq!(
        read.json()["markdown"],
        "# Minutes\n\nThe rule stands.\n",
        "{}",
        read.raw()
    );
    story.finish().await;
}

/// One member of a numbered collection: a topic also numbers its own members.
async fn nest(s: &Session, kind: &str, handle: &str, parent: &str, number: &str) {
    let mut sets = json!({"number": number});
    if kind == "topic" {
        sets["numbers_by"] = json!("number");
    }
    s.call(
        "add_entity",
        json!({"kind": kind, "handle": handle, "name": handle,
               "source": "user-named", "parent": parent, "sets": sets}),
    )
    .await;
}

/// Springfield's rules with a nested numbering: rule 2 is a book of rules that
/// numbers its own, the book's rule 5 is a chapter that numbers its own, and the
/// chapter's rule 7 is a document. The outer collection is [`RULES`].
async fn rules_in_a_book_in_a_chapter(s: &Session) {
    s.add(RULES, "Springfield rules").await;
    s.call(
        "capture",
        json!({"subject": RULES, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    nest(s, "topic", "rules-shelbyville", RULES, "2").await;
    nest(
        s,
        "topic",
        "rules-ogdenville",
        "topic:rules-shelbyville",
        "5",
    )
    .await;
    nest(s, "document", "homer", "topic:rules-ogdenville", "7").await;
    s.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "# Minutes\n\nThe rule stands.\n"}),
    )
    .await;
}

/// **An address through two collections names its member in a document read and
/// in a `where`**, as it does for `recall`.
#[tokio::test]
async fn an_address_through_two_collections_names_its_member_everywhere() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    rules_in_a_book_in_a_chapter(&s).await;
    let address = format!("{RULES}/2/5/7");

    let recalled = s.call("recall", json!({"subject": address})).await;
    assert_eq!(
        ids(&recalled.json()),
        ["document:homer"],
        "{}",
        recalled.raw()
    );
    let read = s.call("read_document", json!({"document": address})).await;
    assert_eq!(
        read.json()["markdown"],
        "# Minutes\n\nThe rule stands.\n",
        "{}",
        read.raw()
    );

    s.add("thing:omega", "Omega").await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "is kept in the minutes",
               "provenance": "testimony", "sets": {"kept_in": "document:homer"}}),
    )
    .await;
    let line = format!("kept_in = {address}");
    let live = s
        .call("recall", json!({"kind": "thing", "where": line}))
        .await;
    assert_eq!(ids(&live.json()), ["thing:omega"], "{}", live.raw());
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "minutes-ledger", "name": "Minutes ledger",
               "source": "user-named", "sets": {"selects": "thing", "where": line}}),
    )
    .await;
    let saved = s.call("recall", json!({"view": "minutes-ledger"})).await;
    assert_eq!(ids(&saved.json()), ["thing:omega"], "{}", saved.raw());
    story.finish().await;
}
