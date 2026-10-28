use super::*;

/// **A view whose `selects` is blank is refused on its page, as the verb refuses
/// it.** The page used to drop a blank `selects` and list every kind that fell
/// due. Paired in the same read: a view that selects a kind lists that kind.
#[tokio::test]
async fn a_view_with_a_blank_selects_is_refused_and_one_that_selects_a_kind_lists_it() {
    let memory = memory_with_views().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("view:bare-listing".into()),
            "Bare Listing",
            "the operator",
        ))
        .await
        .expect("the view is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), String::new()),
                ("asks".to_string(), "overdue".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:bare-listing".into()),
                "what has gone quiet, of no kind",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let refused = read(&client, addr, "/view:bare-listing/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        refused.contains("cannot be run") && !refused.contains("<table id=\"answer\">"),
        "the page refuses and lists nothing: {refused}"
    );
    // The reason names the key that is blank, as the verb's refusal does.
    assert!(
        refused
            .split_once("cannot be run")
            .and_then(|(_, rest)| rest.split_once("</p>"))
            .is_some_and(|(reason, _)| reason.contains("selects")),
        "the page gives the verb's reason: {refused}"
    );

    let kept = read(&client, addr, "/view:my-week/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        kept.contains("<table id=\"answer\">") && kept.contains("rhythm:descale"),
        "a view that selects a kind lists it: {kept}"
    );
    ct.cancel();
}

/// **A view whose day is no day is refused on its page, as the verb refuses it,
/// and the page no longer runs the list as of today.** Paired in the same read: a
/// view whose day reads draws its answer.
#[tokio::test]
async fn a_view_with_a_day_that_is_no_day_is_refused_on_its_page() {
    let memory = memory_with_views().await;
    for (handle, name) in [("view:my-projects", "My Projects")] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the operator",
            ))
            .await
            .expect("the view is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: [
                    ("selects".to_string(), "rhythm".to_string()),
                    ("asks".to_string(), "overdue".to_string()),
                    ("inputs".to_string(), "by: date?".to_string()),
                    ("as_of".to_string(), "$by".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "what is owed on a day",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the question is written");
    }
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let refused = read(&client, addr, "/view:my-projects/?by=2026-13-45", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        refused.contains("cannot be run") && !refused.contains("<table id=\"answer\">"),
        "the page refuses and lists nothing: {refused}"
    );
    assert!(
        refused
            .split_once("cannot be run")
            .and_then(|(_, rest)| rest.split_once("</p>"))
            .is_some_and(|(reason, _)| reason.contains("2026-13-45")),
        "the refusal names the day it could not read, and not only the address: {refused}"
    );
    let kept = read(&client, addr, "/view:my-projects/?by=2026-03-10", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        kept.contains("<table id=\"answer\">")
            && listed_handles(&kept)
                .iter()
                .any(|id| id == "rhythm:descale")
            && !listed_handles(&kept)
                .iter()
                .any(|id| id == "rhythm:worming"),
        "the valid day renders the owed loops and leaves the future loop out: {kept}"
    );
    ct.cancel();
}
