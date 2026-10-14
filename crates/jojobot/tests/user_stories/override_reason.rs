//! "I know the guard has a point, and I am making it anyway."
//!
//! A new name that resembles something already there is refused at first. A
//! resemblance in the NAME alone (the same name under another kind, or a name
//! that holds or sits inside another) is often two real things, so the caller
//! may insist. **Insisting means saying why, and the why is kept** on the new
//! thing as `told_apart_by`, shown beside its name wherever it is listed.
//!
//! A resemblance of the SPELLING (a typo's distance) keeps the token alone: a
//! reason there would be a ritual.

use super::dsl::Story;
use serde_json::json;

fn creation(kind: &str, handle: &str, name: &str) -> serde_json::Value {
    json!({"kind": kind, "handle": handle, "name": name, "source": "user-named"})
}

fn with(mut call: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let (call, extra) = (call.as_object_mut().unwrap(), extra.as_object().unwrap());
    for (key, value) in extra {
        call.insert(key.clone(), value.clone());
    }
    serde_json::Value::Object(call.clone())
}

#[tokio::test]
async fn the_same_name_under_another_kind_lands_only_with_a_reason_and_the_reason_is_kept() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:moes-tavern", "Moe's Tavern").await;

    let refusal = s
        .refused(
            "add_entity",
            creation("place", "moes-tavern", "Moe's Tavern"),
        )
        .await;
    refusal.says("org:moes-tavern");
    let token = refusal.advised("override_token");

    // The token alone is refused again, and the refusal asks for a reason by
    // the argument's name and says what it resembled.
    let again = s
        .refused(
            "add_entity",
            with(
                creation("place", "moes-tavern", "Moe's Tavern"),
                json!({"override_token": &token}),
            ),
        )
        .await;
    again.says("override_reason").says("org:moes-tavern");
    s.recall("place:moes-tavern")
        .await
        .never_says("Moe's Tavern");

    // Token and reason: it lands, and the reason reads back on the thing.
    s.call(
        "add_entity",
        with(
            creation("place", "moes-tavern", "Moe's Tavern"),
            json!({
                "override_token": &token,
                "override_reason": "the building on the corner, not the business inside it",
            }),
        ),
    )
    .await
    .says("place:moes-tavern");
    // The reason is a key on the thing, under the spelling a model reads it by.
    s.recall("place:moes-tavern")
        .await
        .says("told_apart_by")
        .says("corner");

    s.wrap("made the place beside the organisation, with the why")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn a_name_that_contains_another_lands_only_with_a_reason() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("place:moes", "Moe's").await;

    let refusal = s
        .refused(
            "add_entity",
            creation("place", "moes-tavern", "Moe's Tavern Annex"),
        )
        .await;
    refusal.says("place:moes");
    let token = refusal.advised("override_token");

    s.refused(
        "add_entity",
        with(
            creation("place", "moes-tavern", "Moe's Tavern Annex"),
            json!({"override_token": &token}),
        ),
    )
    .await
    .says("override_reason");

    s.call(
        "add_entity",
        with(
            creation("place", "moes-tavern", "Moe's Tavern Annex"),
            json!({
                "override_token": &token,
                "override_reason": "a second room across the alley",
            }),
        ),
    )
    .await
    .says("place:moes-tavern");
    s.recall("place:moes-tavern").await.says("alley");

    s.wrap("made the annex with the why").await;
    story.finish().await;
}

#[tokio::test]
async fn a_near_slug_keeps_the_token_alone() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:contract-nearslug", "Contract Nearslug").await;

    let refusal = s
        .refused(
            "add_entity",
            creation("person", "contract-nearslugg", "Contract Nearslugg"),
        )
        .await;
    refusal.says("person:contract-nearslug");
    s.call(
        "add_entity",
        with(
            creation("person", "contract-nearslugg", "Contract Nearslugg"),
            json!({"override_token": refusal.advised("override_token")}),
        ),
    )
    .await
    .says("person:contract-nearslugg");

    s.wrap("a typo's distance needs no reason").await;
    story.finish().await;
}

#[tokio::test]
async fn a_rename_onto_a_name_that_resembles_another_lands_only_with_a_reason_and_keeps_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:moes-tavern", "Moes Tavern").await;
    s.add("place:moes", "Moe's").await;
    s.add("place:gorge-spot", "The back room").await;

    let call = json!({"handle": "place:gorge-spot", "to": "place:moes-tavern"});
    let refusal = s.refused("rename_entity", call.clone()).await;
    refusal.says("org:moes-tavern");
    let token = refusal.advised("override_token");

    s.refused(
        "rename_entity",
        with(call.clone(), json!({"override_token": &token})),
    )
    .await
    .says("override_reason");
    s.recall("place:gorge-spot").await.never_says("moes-tavern");

    s.call(
        "rename_entity",
        with(
            call,
            json!({
                "override_token": &token,
                "override_reason": "the back room became its own premises",
            }),
        ),
    )
    .await
    .says("place:moes-tavern");
    s.recall("place:moes-tavern").await.says("premises");

    s.wrap("renamed with the why").await;
    story.finish().await;
}

#[tokio::test]
async fn a_new_name_that_resembles_another_lands_only_with_a_reason_and_keeps_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:moes-tavern", "Moe's Tavern").await;
    s.add("place:gorge-spot", "The back room").await;

    let call = json!({"handle": "place:gorge-spot", "name": "Moe's Tavern"});
    let refusal = s.refused("update_entity", call.clone()).await;
    refusal.says("org:moes-tavern");
    let token = refusal.advised("override_token");

    s.refused(
        "update_entity",
        with(call.clone(), json!({"override_token": &token})),
    )
    .await
    .says("override_reason");
    s.recall("place:gorge-spot")
        .await
        .never_says("Moe's Tavern");

    s.call(
        "update_entity",
        with(
            call,
            json!({
                "override_token": &token,
                "override_reason": "it trades under the same sign next door",
            }),
        ),
    )
    .await
    .says("Moe's Tavern");
    s.recall("place:gorge-spot").await.says("sign");

    s.wrap("renamed the display name with the why").await;
    story.finish().await;
}

#[tokio::test]
async fn a_reason_sent_where_none_was_needed_is_kept() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:contract-nearslug", "Contract Nearslug").await;

    let refusal = s
        .refused(
            "add_entity",
            creation("person", "contract-nearslugg", "Contract Nearslugg"),
        )
        .await;
    s.call(
        "add_entity",
        with(
            creation("person", "contract-nearslugg", "Contract Nearslugg"),
            json!({
                "override_token": refusal.advised("override_token"),
                "override_reason": "a cousin with a longer name",
            }),
        ),
    )
    .await
    .says("person:contract-nearslugg");
    s.recall("person:contract-nearslugg").await.says("cousin");

    s.wrap("a reason nobody asked for is still kept").await;
    story.finish().await;
}

#[tokio::test]
async fn a_later_refusal_and_a_search_show_what_tells_a_kept_apart_thing_from_the_one_it_resembles()
{
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("place:moes", "Moe's").await;

    // The second Moe, made with the reason that tells it apart.
    let refusal = s
        .refused(
            "add_entity",
            creation("place", "moes-tavern", "Moe's Tavern"),
        )
        .await;
    s.call(
        "add_entity",
        with(
            creation("place", "moes-tavern", "Moe's Tavern"),
            json!({
                "override_token": refusal.advised("override_token"),
                "override_reason": "the one with the back room",
            }),
        ),
    )
    .await
    .says("place:moes-tavern");

    // A third that resembles both: the list shows the reason beside the one that
    // has it, and nothing extra beside the one that has none.
    let third = s
        .refused(
            "add_entity",
            creation("place", "contract-moes-tavern-annex", "Moe's Tavern Annex"),
        )
        .await
        .json();
    let candidates = third["candidates"].as_array().expect("candidates").clone();
    let told = |handle: &str| {
        candidates
            .iter()
            .find(|candidate| candidate["handle"] == handle)
            .unwrap_or_else(|| panic!("{handle} is a candidate: {third}"))
            .clone()
    };
    assert!(
        told("place:moes-tavern")["told_apart_by"]
            .as_str()
            .is_some_and(|reason| reason.contains("back room")),
        "{third}"
    );
    assert!(told("place:moes").get("told_apart_by").is_none(), "{third}");

    // A search shows it beside the name too.
    let found = s
        .call("search", json!({"query": "Tavern", "kind": "place"}))
        .await
        .json();
    let hit = found["results"]
        .as_array()
        .and_then(|hits| hits.iter().find(|hit| hit["id"] == "place:moes-tavern"))
        .unwrap_or_else(|| panic!("the search finds the tavern: {found}"));
    assert!(
        hit["told_apart_by"]
            .as_str()
            .is_some_and(|reason| reason.contains("back room")),
        "{found}"
    );

    s.wrap("the reason tells the two apart in a list").await;
    story.finish().await;
}

#[tokio::test]
async fn a_move_that_keeps_its_name_is_never_screened_and_a_new_handle_is() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:moes-tavern", "Tavern Holdings").await;
    s.add("place:moes-tavern", "The back room").await;
    // Made beside it, with a reason: the pair that the screen would refuse.
    let refusal = s
        .refused("add_entity", creation("place", "moes", "Moe's"))
        .await;
    s.call(
        "add_entity",
        with(
            creation("place", "moes", "Moe's"),
            json!({
                "override_token": refusal.advised("override_token"),
                "override_reason": "the street, not the room",
            }),
        ),
    )
    .await
    .says("place:moes");

    // Reparenting only: the handle and the names stay, so nothing resembles.
    s.call(
        "rename_entity",
        json!({
            "handle": "place:moes-tavern", "to": "place:moes-tavern",
            "parent": "org:moes-tavern",
        }),
    )
    .await
    .says("org:moes-tavern");

    // Paired: a call that changes the handle is screened as before.
    s.refused(
        "rename_entity",
        json!({"handle": "place:moes-tavern", "to": "place:contract-moes-tavern-annex"}),
    )
    .await
    .says("place:moes");

    s.wrap("moved without a refusal and renamed with one").await;
    story.finish().await;
}
