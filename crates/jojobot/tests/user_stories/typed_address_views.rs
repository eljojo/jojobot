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
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "twelfth-rule-ledger", "name": "Twelfth rule ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "where": format!("ruled_by = {RULES}/12")}}),
    )
    .await;
    let saved = s
        .call("recall", json!({"view": "twelfth-rule-ledger"}))
        .await;
    assert_eq!(ids(&saved.json()), ["thing:omega"], "{}", saved.raw());
    let refused = s.refused(
        "add_entity",
        json!({"kind": "view", "handle": "ninety-ninth-rule-ledger", "name": "Ninety-ninth ledger",
               "source": "user-named",
               "sets": {"selects": "thing", "where": format!("ruled_by = {RULES}/99")}}),
    )
    .await.json();
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["attempted"], format!("{RULES}/99"), "{refused}");
    s.refused(
        "recall",
        json!({"subject": "view:ninety-ninth-rule-ledger"}),
    )
    .await;
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
    assert_eq!(
        archived["changed"],
        json!([{"on": "decision:springfield-12", "what": "status",
                "from": "active", "to": "archived"}]),
        "{archived}"
    );
    let stored = s
        .call("recall", json!({"subject": "decision:springfield-12"}))
        .await
        .json();
    assert_eq!(
        stored["objects"][0]["archived"]["reason"], "repealed",
        "{stored}"
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

/// `count` aliases of about 130 characters, none shared with any other.
fn clauses(seed: usize, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            format!("clause{:05} ", seed * 1_000 + i)
                .repeat(12)
                .trim()
                .to_string()
        })
        .collect()
}

/// **An edit that makes the row too big to read back is refused whichever way
/// the member is named**, with nothing written; a small one lands on each
/// spelling. The size guard measures the member and not the address.
#[tokio::test]
async fn a_relabelling_too_big_to_read_is_refused_by_number_as_by_handle() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_ledger_under_the_twelfth_rule(&s).await;
    let number = format!("{RULES}/12");
    let handle = "decision:springfield-12";
    for spelling in [number.as_str(), handle] {
        let refusal = s
            .refused(
                "update_entity",
                json!({"handle": spelling, "aliases": clauses(1, 400)}),
            )
            .await;
        assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
        refusal.says("aliases");
    }
    let untouched = s.call("recall", json!({"subject": handle})).await;
    assert_eq!(
        untouched.json()["objects"][0]["alternateName"],
        json!([]),
        "the refusals wrote nothing: {}",
        untouched.raw()
    );
    for (spelling, seed) in [(number.as_str(), 2), (handle, 3)] {
        s.call(
            "update_entity",
            json!({"handle": spelling, "aliases": clauses(seed, 3)}),
        )
        .await;
    }
    let kept = s.call("recall", json!({"subject": handle})).await;
    assert_eq!(
        kept.json()["objects"][0]["alternateName"]
            .as_array()
            .map(Vec::len),
        Some(3),
        "{}",
        kept.raw()
    );
    story.finish().await;
}

/// **A collection that was renamed still answers for its numbers by its old
/// handle** in a document read and write and in a `where`, live and saved, as
/// `recall` does. The old handle as a WHOLE reference stays refused.
#[tokio::test]
async fn a_number_through_a_renamed_collections_old_handle_names_its_member() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("topic:rules-old", "Old rules").await;
    s.call(
        "capture",
        json!({"subject": "topic:rules-old", "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    nest(&s, "document", "homer", "topic:rules-old", "20").await;
    s.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "# Minutes\n\nThe rule stands.\n"}),
    )
    .await;
    s.add("thing:omega", "Omega").await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "is kept in the minutes",
               "provenance": "testimony", "sets": {"kept_in": "document:homer"}}),
    )
    .await;
    s.call(
        "rename_entity",
        json!({"handle": "topic:rules-old", "to": "topic:rules-new"}),
    )
    .await;
    let address = "topic:rules-old/20";

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
    nest(&s, "document", "marge", "topic:rules-new", "21").await;
    s.call(
        "put_document",
        json!({"document": "topic:rules-old/21", "markdown": "# Notes\n\nA second record.\n"}),
    )
    .await;
    let written = s
        .call("read_document", json!({"document": "document:marge"}))
        .await;
    assert_eq!(
        written.json()["markdown"],
        "# Notes\n\nA second record.\n",
        "{}",
        written.raw()
    );

    let line = format!("kept_in = {address}");
    let live = s
        .call("recall", json!({"kind": "thing", "where": line}))
        .await;
    assert_eq!(ids(&live.json()), ["thing:omega"], "{}", live.raw());
    s.call(
        "add_entity",
        json!({"kind": "view", "handle": "old-minutes-ledger", "name": "Old minutes ledger",
               "source": "user-named", "sets": {"selects": "thing", "where": line}}),
    )
    .await;
    let saved = s
        .call("recall", json!({"view": "old-minutes-ledger"}))
        .await;
    assert_eq!(ids(&saved.json()), ["thing:omega"], "{}", saved.raw());

    // The old handle as the whole target is still refused.
    s.refused(
        "capture",
        json!({"subject": "thing:omega", "content": "points at the old handle",
               "provenance": "testimony", "object": "topic:rules-old",
               "shape": "connection"}),
    )
    .await
    .says("\"wrote\":false");
    story.finish().await;
}
