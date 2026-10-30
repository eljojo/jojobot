use super::*;
use jojobot_domain::memory::kinds::NotAKind;

/// The text of each paragraph of a page. A page puts a view's refusal in one, and
/// the form that echoes the address is not one, so a needle found here was said as
/// the refusal and not echoed.
fn paragraphs(page: &str) -> impl Iterator<Item = &str> {
    page.split("<p>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</p>").map(|(text, _)| text))
}

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
        !refused.contains("<table id=\"answer\">"),
        "the page refuses and lists nothing: {refused}"
    );
    // The reason is the kind resolver's own refusal for the blank value, as the
    // verb gives it, and it sits in a paragraph, not elsewhere on the page. The
    // expected text comes from the resolver's error, so the case follows the
    // verb's words and quotes none of the page's.
    let resolver = NotAKind::NotDeclared {
        token: String::new(),
        known: Vec::new(),
    }
    .to_string();
    let reason = jojobot::ui::pages::escape(
        resolver
            .split_once(". ")
            .map_or(resolver.as_str(), |(head, _)| head),
    );
    assert!(
        paragraphs(&refused).any(|paragraph| paragraph.contains(&reason)),
        "the page gives the verb's reason in a paragraph: {refused}"
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
        !refused.contains("<table id=\"answer\">"),
        "the page refuses and lists nothing: {refused}"
    );
    assert!(
        paragraphs(&refused).any(|paragraph| paragraph.contains("2026-13-45")),
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
