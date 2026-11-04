//! "Where does a thing go when I do not say?"
//!
//! A bot that creates something without a parent finds it under itself, is told
//! so in the receipt with the call that moves it, and asks for the root on
//! purpose with `root: true`. The loop and the promise still need a parent said
//! out loud, and the assistant, which holds the operator's life, still creates
//! at the root. Each placement is read back with `recall`, never from the receipt.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

fn thing(handle: &str, name: &str) -> Value {
    json!({"kind": "thing", "handle": handle, "name": name, "source": "user-named"})
}

/// The parent a `recall` of the handle reports, or none for a root. The object is
/// found first, so a missing key is not an empty answer.
async fn parent_of(s: &Session, handle: &str) -> Option<String> {
    let recalled = s.recall(handle).await;
    let object = &recalled.json()["objects"][0];
    assert_eq!(object["id"], handle, "{}", recalled.raw());
    object["parent"].as_str().map(str::to_string)
}

#[tokio::test]
async fn a_bots_parentless_create_lands_under_the_bot_and_the_receipt_says_so() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;

    // The plain path.
    let made = s.call("add_entity", thing("delta", "Delta")).await;
    assert_eq!(
        parent_of(&s, "thing:delta").await.as_deref(),
        Some("bot:marge")
    );

    // The receipt says it was defaulted, where it went, and the call that moves it.
    let said = &made.json()["parent_defaulted"];
    assert_eq!(said["parent"], "bot:marge", "{}", made.raw());
    assert_eq!(said["move"]["tool"], "rename_entity", "{}", made.raw());
    let arguments = &said["move"]["arguments"];
    assert_eq!(arguments["handle"], "thing:delta", "{}", made.raw());
    assert_eq!(arguments["to"], "thing:delta", "{}", made.raw());

    // The call it names moves the thing: the receipt's own arguments, with the
    // new parent filled in.
    s.add("thing:kettle", "Kettle").await;
    let mut moving = arguments.clone();
    moving["parent"] = json!("thing:kettle");
    s.call("rename_entity", moving).await;
    assert_eq!(
        parent_of(&s, "thing:delta").await.as_deref(),
        Some("thing:kettle")
    );
}

#[tokio::test]
async fn a_bots_parentless_create_with_sets_lands_under_the_bot_too() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;

    let mut args = thing("epsilon", "Epsilon");
    args["sets"] = json!({"flavour": "pink"});
    let made = s.call("add_entity", args).await;
    assert_eq!(
        made.json()["parent_defaulted"]["parent"],
        "bot:marge",
        "{}",
        made.raw()
    );
    assert!(
        made.json()["address"] == "thing:epsilon#f1",
        "the first claim was written: {}",
        made.raw()
    );
    assert_eq!(
        parent_of(&s, "thing:epsilon").await.as_deref(),
        Some("bot:marge")
    );
}

#[tokio::test]
async fn each_bot_is_the_home_of_what_it_creates() {
    let story = Story::begin("bot:marge").await;
    let marge = story.session().await;
    marge.add("bot:nelson", "Nelson").await;
    let nelson = story.as_bot("bot:nelson").await;

    marge.call("add_entity", thing("omega", "Omega")).await;
    nelson.call("add_entity", thing("tau", "Tau")).await;
    assert_eq!(
        parent_of(&marge, "thing:omega").await.as_deref(),
        Some("bot:marge")
    );
    assert_eq!(
        parent_of(&marge, "thing:tau").await.as_deref(),
        Some("bot:nelson")
    );
}

#[tokio::test]
async fn a_named_parent_is_kept_and_the_receipt_says_nothing_was_defaulted() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;

    let mut args = thing("brass-bell", "Brass Bell");
    args["parent"] = json!("thing:kettle");
    let made = s.call("add_entity", args).await;
    assert!(
        made.json().get("parent_defaulted").is_none(),
        "a parent the caller named is not defaulted: {}",
        made.raw()
    );
    assert_eq!(
        parent_of(&s, "thing:brass-bell").await.as_deref(),
        Some("thing:kettle")
    );
}

#[tokio::test]
async fn root_true_asks_for_the_root_and_is_refused_beside_a_parent() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;

    let mut args = thing("fat-bike", "Fat Bike");
    args["root"] = json!(true);
    let made = s.call("add_entity", args).await;
    assert!(made.json().get("parent").is_none(), "{}", made.raw());
    assert!(
        made.json().get("parent_defaulted").is_none(),
        "the root was asked for, not defaulted: {}",
        made.raw()
    );
    assert_eq!(parent_of(&s, "thing:fat-bike").await, None);

    s.add("thing:kettle", "Kettle").await;
    let mut both = thing("red-bike", "Red Bike");
    both["root"] = json!(true);
    both["parent"] = json!("thing:kettle");
    let refused = s.refused("add_entity", both).await;
    refused.says("root").says("parent");
    // Nothing was made by the refusal: the kettle is listed, the red-bike is not.
    let things = s.list("thing").await;
    things.says("thing:kettle");
    assert!(!things.raw().contains("thing:red-bike"), "{}", things.raw());
}

#[tokio::test]
async fn a_loop_and_a_promise_still_need_their_parent_said_out_loud() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;

    for kind in ["rhythm", "promise"] {
        let refused = s
            .refused(
                "add_entity",
                json!({"kind": kind, "handle": "tidy", "name": "Tidy", "source": "user-named"}),
            )
            .await;
        refused.says("parent");
    }
    // The positive the refusals rest on: with a parent said, the loop is made
    // under the thing named and not under the bot.
    s.add("thing:kettle", "Kettle").await;
    let mut args = json!({
        "kind": "rhythm", "handle": "descale", "name": "Descale", "source": "user-named",
    });
    args["parent"] = json!("thing:kettle");
    s.call("add_entity", args).await;
    assert_eq!(
        parent_of(&s, "rhythm:descale").await.as_deref(),
        Some("thing:kettle")
    );
}

#[tokio::test]
async fn the_assistant_that_holds_the_operators_life_still_creates_at_the_root() {
    let story = Story::begin("bot:marge").await;
    let assistant = story.as_bot("bot:assistant").await;

    let made = assistant.call("add_entity", thing("gamma", "Gamma")).await;
    assert!(made.json().get("parent").is_none(), "{}", made.raw());
    assert!(
        made.json().get("parent_defaulted").is_none(),
        "{}",
        made.raw()
    );
    assert_eq!(parent_of(&assistant, "thing:gamma").await, None);
}

#[tokio::test]
async fn numbered_things_under_one_bot_are_siblings_and_not_near_names() {
    let story = Story::begin("bot:marge").await;
    let s = story.session().await;

    for (handle, name) in [("shift-1", "Shift 1"), ("shift-2", "Shift 2")] {
        let mut args = thing(handle, name);
        args["kind"] = json!("work");
        s.call("add_entity", args).await;
    }
    assert_eq!(
        parent_of(&s, "work:shift-1").await.as_deref(),
        Some("bot:marge")
    );
    assert_eq!(
        parent_of(&s, "work:shift-2").await.as_deref(),
        Some("bot:marge")
    );
    // An exact name collision under the same bot is still refused.
    let mut again = thing("shift-3", "Shift 2");
    again["kind"] = json!("work");
    s.refused("add_entity", again).await;
}
