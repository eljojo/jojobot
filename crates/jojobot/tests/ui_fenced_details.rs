//! The node page links a mention in a claim, and reads the claim's content and
//! details as one text for what is code.

mod support;

use jiff::civil::Date;
use jojobot_domain::memory::{EntityId, Guarded, NewEntity, NewFact};
use support::ui_board::{CLIENT_ID, READER, read, seeded_board, spawn_jojobot_over};
use support::{browser, log_in, spawn_idp};

/// **A handle inside a fence that opens on a claim's content and closes in its
/// details is not a link on the page.** Neither field alone holds a block, so the
/// page has to read them as one text. **Paired**: a handle in a sentence of
/// another claim, and one in the details after the fence closes, are still
/// anchors. The second keeps the case red when the details are not read at all.
#[tokio::test]
async fn a_fence_across_content_and_details_is_not_a_link_on_the_page() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;

    for (handle, name) in [
        ("person:milhouse", "Milhouse"),
        ("thing:handcart", "Handcart"),
        ("person:marge", "Marge"),
    ] {
        let written = board
            .memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the mentioned thing is written");
        assert!(
            matches!(written, Guarded::Written(_)),
            "{handle} was blocked rather than written: {written:?}",
        );
    }
    board
        .memory
        .capture(NewFact {
            details: Some("run @person:milhouse here\n```\nthen @person:marge".into()),
            ..NewFact::about(
                EntityId("topic:widgets".into()),
                "```text",
                Date::constant(2026, 3, 9),
            )
        })
        .await
        .expect("the fenced claim is written");
    board
        .memory
        .capture(NewFact::about(
            EntityId("topic:widgets".into()),
            "@person:alpha rode @thing:handcart",
            Date::constant(2026, 3, 9),
        ))
        .await
        .expect("the sentence claim is written");

    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    ct.cancel();

    assert!(
        body.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "a handle in a sentence is an anchor: {body}"
    );
    assert!(
        body.contains("<a href=\"/person:marge/\">@person:marge</a>"),
        "a handle in the details after the fence closes is an anchor: {body}"
    );
    assert!(
        body.contains("run @person:milhouse here")
            && !body.contains("<a href=\"/person:milhouse/\">"),
        "a handle inside the fence is text and not an anchor: {body}"
    );
}
