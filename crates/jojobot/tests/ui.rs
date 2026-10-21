//! Integration tests for the browser listing: the gate, the login round trip
//! through an issuer, and the index of the roots.
//!
//! The issuer here is a real HTTP server that speaks the authorization-code
//! flow — it holds the PKCE challenge it was sent and refuses to exchange a
//! code against the wrong verifier. A login that never left this process would
//! prove the handlers and not the flow.

use std::net::SocketAddr;
use std::sync::Arc;

use jiff::civil::Date;
use jojobot::auth::IssuerEndpoints;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
use jojobot_domain::memory::mention;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{Edge, EdgeShape, EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};
use tokio_util::sync::CancellationToken;

#[path = "support/served_tools.rs"]
mod served_tools;
mod support;
use support::{browser, location, log_in, log_in_raw, query_of, session_pair, spawn_idp};

use support::ui_board::*;

// --- the tests -------------------------------------------------------------

#[tokio::test]
async fn a_browser_with_no_session_is_sent_to_the_login() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[], &idp).await;

    let response = browser()
        .get(format!("http://{addr}/"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_redirection());
    let to = location(&response);
    assert!(
        to.starts_with("/ui/login"),
        "the gate must send a person to the login, not refuse them: {to}"
    );
    assert!(
        to.contains("next="),
        "the login must be told where the browser was going: {to}"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_login_sends_the_browser_to_the_issuer_with_a_pkce_challenge() {
    let idp = support::TestIdp::new();
    let (idp_addr, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[], &idp).await;

    let response = browser()
        .get(format!("http://{addr}/ui/login?next=%2F"))
        .send()
        .await
        .unwrap();

    let to = location(&response);
    assert!(
        to.starts_with(&format!("http://{idp_addr}/authorize?")),
        "the login must go to the issuer's authorization endpoint: {to}"
    );
    let query = query_of(&to);
    assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
    assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(
        query.get("redirect_uri").map(String::as_str),
        Some(format!("http://{addr}/ui/callback").as_str())
    );
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256"),
        "a login without PKCE is a code anybody who intercepts it can redeem"
    );
    assert!(query.contains_key("code_challenge"));
    assert!(query.contains_key("state"));
    ct.cancel();
}

#[tokio::test]
async fn a_logged_in_browser_reads_the_index_of_the_roots() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let cookie = log_in(&client, addr, "/").await;

    let page = client
        .get(format!("http://{addr}/"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let body = page.text().await.unwrap();

    assert!(
        body.contains("href=\"/person:alpha/\""),
        "a root must be listed and be a link into its own index: {body}"
    );
    assert!(
        body.contains("href=\"/place:shelbyville/\""),
        "every root is listed, not just the first: {body}"
    );
    // The pairing that makes the two assertions above mean something: the page
    // is the index of the ROOTS, so a child is not on it.
    assert!(
        !element(&body, "roots", "table").contains("topic:widgets"),
        "a child belongs under its parent, not at the top: {body}"
    );
    // Nothing here is damaged, so the damaged section is not here. A standing
    // empty one would teach a reader to skip it.
    assert!(
        !body.contains("id=\"unreachable\""),
        "a healthy store shows no damage report: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_subject_off_the_allowlist_is_refused_and_gets_no_session() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for("sub-somebody-else", CLIENT_ID)).await;
    // The allowlist names the reader; the issuer will vouch for somebody else.
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let turned_away = client.get(format!("http://{addr}/")).send().await.unwrap();
    let sent_out = client
        .get(format!("http://{addr}{}", location(&turned_away)))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let refused = client.get(location(&back)).send().await.unwrap();

    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    assert!(
        refused.headers().get(reqwest::header::SET_COOKIE).is_none(),
        "a refused login must not open a session"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_callback_this_server_never_started_is_refused() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;

    let response = browser()
        .get(format!(
            "http://{addr}/ui/callback?code=made-up&state=made-up"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(
        response
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .is_none()
    );
    ct.cancel();
}

#[tokio::test]
async fn a_login_is_spent_once() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    // Walk the flow by hand so the callback URL is still in hand to replay.
    let turned_away = client.get(format!("http://{addr}/")).send().await.unwrap();
    let sent_out = client
        .get(format!("http://{addr}{}", location(&turned_away)))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let callback = location(&back);

    let first = client.get(&callback).send().await.unwrap();
    assert!(first.status().is_redirection(), "the first use logs in");

    let replayed = client.get(&callback).send().await.unwrap();
    assert_eq!(
        replayed.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "a callback replayed with a spent state must not open a second session"
    );
    ct.cancel();
}

/// **The flags are the session's whole defence, so they are asserted, not
/// discarded.** The cookie value alone says nothing about what reaches it:
/// `HttpOnly` is what keeps a script on the page from reading it, and
/// `SameSite` is what stops another site spending it on the reader's behalf.
///
/// `Secure` is the one that is conditional — it is taken from the **configured
/// origin**, so both branches are walked here. An instance served over https
/// must never hand its cookie to a plain-http request; an instance configured
/// over http would never see the cookie come back at all if it did.
#[tokio::test]
async fn the_session_cookie_is_closed_to_script_and_to_another_site() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let set_cookie = log_in_raw(&client, addr, "/").await;

    // The positive the negatives lean on: this is a real session cookie, not an
    // empty header a `contains` would be asked about in vain.
    assert!(
        session_pair(&set_cookie).contains('=')
            && session_pair(&set_cookie).split('=').nth(1) != Some(""),
        "a completed login must set a session cookie with a value: {set_cookie}"
    );
    assert!(
        set_cookie.contains("HttpOnly"),
        "a cookie a script can read is a cookie an XSS takes: {set_cookie}"
    );
    assert!(
        set_cookie.contains("SameSite=Lax"),
        "without SameSite another site spends the session on the reader's behalf: {set_cookie}"
    );
    assert!(
        !set_cookie.contains("; Secure"),
        "an origin configured over plain http would never get a Secure cookie back: {set_cookie}"
    );
    ct.cancel();

    // The other branch of the one conditional flag. The process still listens on
    // plain TCP — there is no TLS in this test — so the issuer's callback comes
    // back addressed `https` and the scheme is swapped to reach the same
    // handler. What is under test is the cookie that handler writes.
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_at(endpoints, &[READER], &idp, seeded_board().await, "https").await;
    let client = browser();

    let turned_away = client.get(format!("http://{addr}/")).send().await.unwrap();
    let sent_out = client
        .get(format!("http://{addr}{}", location(&turned_away)))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let callback = location(&back);
    assert!(
        callback.starts_with(&format!("https://{addr}/ui/callback")),
        "the configured origin is what the issuer sends the browser back to: {callback}"
    );
    let opened = client
        .get(callback.replace("https://", "http://"))
        .send()
        .await
        .unwrap();
    let secure = opened
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("a completed login sets a session cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        secure.contains("; Secure"),
        "an origin configured over https must never hand its cookie to a plain-http request: \
         {secure}"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_login_will_not_land_a_browser_off_this_server() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    let cookie = log_in(&client, addr, "/").await;
    let _ = cookie;

    // A `next` pointing off-site is what turns a real login into an open
    // redirector, so it is dropped rather than honoured.
    let sent_out = client
        .get(format!(
            "http://{addr}/ui/login?next=https%3A%2F%2Felsewhere.example%2F"
        ))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let landed = client.get(location(&back)).send().await.unwrap();

    assert_eq!(location(&landed), "/");
    ct.cancel();
}

// --- descending -------------------------------------------------------------

/// One table of a page, from its id to the end of that table.
///
/// **The sections are told apart by id rather than by the words around them.**
/// A heading is prose and will be improved; an id is a name, and renaming one
/// is a real change to what the page offers.
fn section<'a>(body: &'a str, id: &str) -> &'a str {
    let anchor = format!("id=\"{id}\"");
    let start = body
        .find(&anchor)
        .unwrap_or_else(|| panic!("no section {id} on this page: {body}"));
    let rest = &body[start..];
    match rest.find("</table>") {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// The text of the one folded block in this chunk of a page.
fn summary_of(chunk: &str) -> &str {
    let opened = chunk
        .split_once("<summary>")
        .unwrap_or_else(|| panic!("nothing is folded here: {chunk}"))
        .1;
    opened
        .split_once("</summary>")
        .unwrap_or_else(|| panic!("a summary that never closes: {chunk}"))
        .0
}

#[tokio::test]
async fn a_node_page_lists_what_is_below_it() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/person:alpha/", &cookie).await;
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    let body = page.text().await.unwrap();

    assert!(
        body.contains("href=\"/person:alpha/topic:widgets/\""),
        "a child must be a link to its own place in the tree: {body}"
    );
    // The pairing: a sibling of this node is not below it, so a page that
    // listed everything would pass the assertion above and fail this one.
    assert!(
        !body.contains("href=\"/person:alpha/place:shelbyville/\""),
        "a root is not a child of another root: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_node_page_shows_the_facts_held_there_and_who_backs_them() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("The widget stall runs on Thursdays"),
        "the claim itself must be on the page: {body}"
    );
    assert!(
        body.contains("testimony"),
        "a claim is read differently depending on who backs it, so the page says: {body}"
    );
    assert!(
        body.contains("2026-03-04"),
        "a fact carries its date: {body}"
    );
    ct.cancel();
}

/// **A thing's page lists what recall lists, and recall leaves the review mark
/// out.** The mark is the one record defrag leaves on a thing it has read; it
/// holds no content a reader chose to keep. The pairing: the ordinary record on
/// the same page is still listed, so a page that listed no records at all would
/// not pass.
#[tokio::test]
async fn a_node_page_leaves_a_review_mark_out_of_its_records() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let mut mark = NewFact::about(
        EntityId("topic:widgets".into()),
        "Perused whole by the tidier.",
        Date::constant(2026, 3, 10),
    );
    mark.fields = [(
        jojobot_domain::memory::review::REVIEW_MARK.to_string(),
        "2026-03-10".to_string(),
    )]
    .into_iter()
    .collect();
    board
        .memory
        .capture(mark)
        .await
        .expect("the review mark is written");
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("The widget stall runs on Thursdays"),
        "the ordinary record is still on the page: {body}"
    );
    assert!(
        !body.contains("Perused"),
        "the review mark is bookkeeping and is not listed as a record: {body}"
    );
    ct.cancel();
}

/// **A mention in served text becomes a link to that thing's own page.**
///
/// Four plays in one read, because a linkifier that wrapped every `@`-shaped
/// run in an anchor would pass on the genuine case alone: a real mention
/// becomes a followable link; text shaped like a handle that never named
/// anything renders distinctly and is never a link; the unrelated claim
/// already on this page (`The widget stall runs on Thursdays`, which mentions
/// nothing) is unchanged in the same body — a page with no mentions on it is
/// not something a passing linkifier gets to touch; and a mention in the
/// entity's own PROSE follows the same rule as one in a record's content —
/// the page has one linker, not two, and the map it needs (`by_id`) is
/// already read for the fact table above it.
///
/// **The rename case is here too.** `rename_entity` moves a thing's badge
/// while a stored mention keeps pointing at the same row — proven below by
/// renaming `thing:handcart` after the mention above is captured and reading
/// the same page again: the link follows the new handle, and the anchor the
/// old handle rendered is gone. `merge` is a different move and stays
/// distinct: it folds a losing entity's facts into a survivor and marks it
/// `merged_into`, without touching either side's badge. Read at the source in
/// `jojobot-domain/src/memory/testing.rs`'s `merge`.
#[tokio::test]
async fn a_mention_in_served_text_becomes_a_followable_link() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;

    let written = board
        .memory
        .add_entity(NewEntity::new(
            EntityId("thing:handcart".into()),
            "Handcart",
            "the fixture roster",
        ))
        .await
        .expect("the mentioned thing is written");
    assert!(
        matches!(written, jojobot_domain::memory::Guarded::Written(_)),
        "thing:handcart was blocked rather than written: {written:?}",
    );
    board
        .memory
        .capture(NewFact::about(
            EntityId("topic:widgets".into()),
            "@person:alpha rode @thing:handcart",
            Date::constant(2026, 3, 9),
        ))
        .await
        .expect("the mention-bearing claim is written");
    board
        .memory
        .set_prose(
            &EntityId("topic:widgets".into()),
            "handcart's twin is @thing:handcart-lost, and it is kept near @place:shelbyville",
            None,
        )
        .await
        .expect("the unresolvable mention is written");

    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("<a href=\"/person:alpha/\">@person:alpha</a>"),
        "a mention of a root becomes a link to its own page: {body}"
    );
    assert!(
        body.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "a mention of a root thing becomes a link to its own page: {body}"
    );
    assert!(
        body.contains("@thing:handcart-lost (no such handle)")
            && !body.contains("<a href=\"/thing:handcart-lost/\">"),
        "text shaped like a handle that names nothing renders distinctly and is never a link: {body}"
    );
    assert!(
        body.contains("The widget stall runs on Thursdays"),
        "a claim that mentions nothing is unchanged on the same page: {body}"
    );
    assert!(
        body.contains("<a href=\"/place:shelbyville/\">@place:shelbyville</a>"),
        "a mention in the entity's own prose becomes a link exactly as one in a record's \
         content does: {body}"
    );

    // **The rename case.** Nothing about the claim above is rewritten — what
    // changes is what a read of its stored mention renders back.
    board
        .memory
        .rename_entity(
            &EntityId("thing:handcart".into()),
            &EntityId("thing:handcart-2".into()),
            None,
            Date::constant(2026, 3, 10),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename is written");

    let after = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        after.contains("<a href=\"/thing:handcart-2/\">@thing:handcart-2</a>"),
        "the mention did not follow the rename to the thing's new page: {after}"
    );
    assert!(
        !after.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "the mention still renders under the handle it wore before the rename: {after}"
    );
    ct.cancel();
}

/// **A handle inside a fence that opens on a claim's content is not a link on the
/// page.** The content is a fence line and the details hold the code and the
/// closing fence, so neither field alone holds a block; the page reads them as
/// one text, and the handle in the code is text. **Paired**: a handle in a
/// sentence of another claim on the same page is still an anchor.
#[tokio::test]
async fn a_handle_inside_a_fence_that_opens_on_the_content_is_not_a_link_on_the_page() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;

    for (handle, name) in [
        ("person:milhouse", "Milhouse"),
        ("thing:handcart", "Handcart"),
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
            matches!(written, jojobot_domain::memory::Guarded::Written(_)),
            "{handle} was blocked rather than written: {written:?}",
        );
    }
    board
        .memory
        .capture(NewFact {
            details: Some("run @person:milhouse here\n```".into()),
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

    assert!(
        body.contains("<a href=\"/thing:handcart/\">@thing:handcart</a>"),
        "a handle in a sentence is an anchor: {body}"
    );
    assert!(
        body.contains("run @person:milhouse here")
            && !body.contains("<a href=\"/person:milhouse/\">"),
        "a handle inside the fence is text and not an anchor: {body}"
    );
    ct.cancel();
}

/// **A thing's page shows what the thing HOLDS — its fields, folded.**
///
/// A thing's fields are what it conforms to and what that unlocks, and they
/// arrive one record at a time. The page rendered seven columns of a record's
/// qualifiers and never the fields themselves, so the one surface the operator
/// reads directly could not show the thing the model is about.
#[tokio::test]
async fn a_node_page_shows_the_things_folded_fields_and_what_they_conform_to() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // **Both keys, from two different records.** Either one alone would pass on
    // a page that rendered a single record's fields and called it the thing's.
    assert!(
        body.contains("opens") && body.contains("2026-03-05"),
        "a key one record carries is on the page: {body}"
    );
    assert!(
        body.contains("pitch") && body.contains("14"),
        "…and so is the key the OTHER record carries, which is the fold: {body}"
    );
    // What the thing is, said in the type's own words — the payoff of the fold
    // and the thing a reader of this page cannot work out unaided.
    assert!(
        body.contains("stall"),
        "the type the thing answers is named: {body}"
    );
    ct.cancel();
}

/// **A key on the page opens to the writes behind it.**
///
/// The fold says what the thing holds NOW, which is one value and the answer
/// most of the time. The writes behind that value are the other question the
/// same data answers — every time the key was written, and when — and the page
/// showed no way to ask it.
///
/// Both halves in one case: a plain page carries no history at all, and the
/// page for one key carries that key's writes, oldest first.
#[tokio::test]
async fn a_key_on_a_node_page_opens_to_the_writes_behind_it() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let plain = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        plain.contains("?history=takings"),
        "the key is a link to its own writes — a page nobody can ask is a \
         capability nobody finds: {plain}"
    );
    assert!(
        !plain.contains("id=\"history\""),
        "…and a page nobody asked carries no history: {plain}"
    );

    let opened = read(
        &client,
        addr,
        "/person:alpha/topic:widgets/?history=takings",
        &cookie,
    )
    .await
    .text()
    .await
    .unwrap();
    let table = opened
        .split_once("id=\"history\"")
        .expect("the opened page carries the history table")
        .1;
    let table = table.split_once("</table>").expect("…and it closes").0;
    // **Scoped to the table**, because the current value appears in the fold
    // above it: a substring over the whole page would find "45" there and call
    // it a history.
    let first = table.find(">40<").expect("the write that came first");
    let second = table.find(">45<").expect("…and the one that replaced it");
    assert!(
        first < second,
        "the writes read oldest first, as a history does: {table}"
    );
    assert!(
        table.contains("2026-03-07") && table.contains("2026-03-08"),
        "each write says when it happened: {table}"
    );
    ct.cancel();
}

/// **One word, one axis — across both tables on the page.**
///
/// A record's status (active · archived) and a claim's standing
/// (settled · open) are two different questions, and the facts table asks both
/// of them in columns of their own. The writes table carries the first and no
/// second, so whichever word it heads that column with is read against the
/// facts table one screen below: heading the status column "Standing" tells a
/// reader either that a write can be settled or that a standing can be
/// retracted.
///
/// Asserted on the page that serves both, because the collision is between
/// them: either header read on its own is defensible.
#[tokio::test]
async fn the_two_tables_head_one_axis_with_one_word() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(
        &client,
        addr,
        "/person:alpha/topic:widgets/?history=takings",
        &cookie,
    )
    .await
    .text()
    .await
    .unwrap();

    let facts = page
        .split_once("<h2>Facts</h2>")
        .expect("the page carries the facts table")
        .1;
    assert!(
        facts.contains("<th>Standing</th>") && facts.contains("<th>State</th>"),
        "the facts table asks both questions, in columns of their own — which is what makes \
         either word ambiguous when the other table reuses it: {facts}"
    );

    let writes = page
        .split_once("id=\"history\"")
        .expect("the page carries the writes table")
        .1
        .split_once("</table>")
        .expect("…and it closes")
        .0;
    assert!(
        writes.contains("active"),
        "the writes table carries each write's status, which is the column at issue: {writes}"
    );
    assert!(
        writes.contains("<th>State</th>"),
        "…and heads it with the word the facts table gives that same axis: {writes}"
    );
    assert!(
        !writes.contains("<th>Standing</th>"),
        "…never with the word the facts table spends on the other one: {writes}"
    );
    ct.cancel();
}

/// **A thing nobody has recorded a field on says so**, rather than carrying an
/// empty table. Most things have no fields, and a page that grew a blank
/// scaffold on every one of them teaches a reader to skip the section that
/// matters on the few that do.
#[tokio::test]
async fn a_node_page_with_no_fields_says_so_rather_than_showing_an_empty_table() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/place:shelbyville/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("Fields"),
        "the section is there, so a reader learns the difference between \
         nothing recorded and nothing shown: {body}"
    );
    assert!(
        !body.contains("id=\"fields\""),
        "…and there is no table to read: {body}"
    );
    // A thing with no fields conforms to nothing, and a heading saying so on
    // every page would be a judgement nobody asked for.
    assert!(
        !body.contains("Conforms"),
        "a thing answering no type carries no conformance section: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_relation_is_a_link_to_the_entity_on_the_other_end() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        body.contains("location"),
        "the relation's shape is what it means, so it is named: {body}"
    );
    assert!(
        body.contains("href=\"/place:shelbyville/\""),
        "a relation must be followable to where that entity actually lives: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_path_that_is_not_where_an_entity_lives_lands_at_the_one_that_is() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    // The handle is identity and the path is position. A handle typed at the
    // top is the same entity, so it is sent to where that entity sits rather
    // than served at a second URL of its own.
    let response = read(&client, addr, "/topic:widgets/", &cookie).await;
    assert!(
        response.status().is_redirection(),
        "got {}",
        response.status()
    );
    assert_eq!(location(&response), "/person:alpha/topic:widgets/");
    ct.cancel();
}

#[tokio::test]
async fn a_handle_nobody_has_is_not_found() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for missing in ["/person:contract-ghost/", "/not-a-handle/"] {
        let response = read(&client, addr, missing, &cookie).await;
        assert_eq!(
            response.status(),
            reqwest::StatusCode::NOT_FOUND,
            "{missing} names nothing, so it is missing rather than empty"
        );
    }
    ct.cancel();
}

#[tokio::test]
async fn a_node_page_is_closed_to_a_browser_with_no_session() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;

    let response = browser()
        .get(format!("http://{addr}/person:alpha/"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_redirection());
    let to = location(&response);
    assert!(to.starts_with("/ui/login"), "{to}");
    assert!(
        to.contains("person%3Aalpha"),
        "the login must carry the page the browser wanted: {to}"
    );
    ct.cancel();
}

/// **What a bot holds does not open a page.** A bot reaches this server with a
/// bearer token that `/mcp` accepts. The pages take a session cookie that only
/// a completed login mints, so the token is not a way in, however it is
/// presented: as the bearer it came as, as the value of the session cookie, or
/// as an ID token minted for the UI's own client. Every one of those is sent to
/// the login and shown no page. Paired with the same token at `/mcp`, which
/// lets it in (so it IS a real credential), and with a real login, which opens
/// the page (so the refusals are about the credential and not about the page).
#[tokio::test]
async fn what_a_bot_holds_does_not_open_a_page_and_a_real_session_does() {
    const BOT: &str = "sub-a-bot";
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let mcp = Arc::new(idp.validator(&[BOT]));
    let (addr, ct, _) = spawn_jojobot_guarding(
        endpoints,
        &[READER],
        &idp,
        seeded_board().await,
        "http",
        Some(mcp),
    )
    .await;
    let bot_token = idp.token(BOT);
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"a-bot","version":"0"}}}"#;
    let at_mcp = |token: Option<String>| {
        let mut request = browser()
            .post(format!("http://{addr}/mcp"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(initialize);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send()
    };

    // ── the token is a real credential at /mcp, and nothing is not ──────────
    assert_eq!(
        at_mcp(None).await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "/mcp must refuse a call with no token, or the positive below proves nothing"
    );
    let let_in = at_mcp(Some(bot_token.clone())).await.unwrap().status();
    assert!(
        let_in.is_success(),
        "the bot's token must open /mcp, or it is no credential to refuse elsewhere: {let_in}"
    );

    // ── and it opens no page, in any of the shapes a bot could try ──────────
    let real_session = log_in(&browser(), addr, "/").await;
    let cookie_name = real_session
        .split_once('=')
        .map(|(name, _)| name.to_string())
        .expect("a session pair is name=value");
    let ui_id_token = idp.token_for(BOT, CLIENT_ID);
    for (how, request) in [
        (
            "as the bearer it came as",
            browser()
                .get(format!("http://{addr}/person:alpha/"))
                .bearer_auth(&bot_token),
        ),
        (
            "as the value of the session cookie",
            browser()
                .get(format!("http://{addr}/person:alpha/"))
                .header("cookie", format!("{cookie_name}={bot_token}")),
        ),
        (
            "as an ID token minted for the UI's client",
            browser()
                .get(format!("http://{addr}/person:alpha/"))
                .bearer_auth(&ui_id_token),
        ),
        (
            "as the bearer, at the index",
            browser()
                .get(format!("http://{addr}/"))
                .bearer_auth(&bot_token),
        ),
    ] {
        let response = request.send().await.unwrap();
        assert!(
            response.status().is_redirection(),
            "a bot's token {how} must be sent to the login, got {}",
            response.status()
        );
        let to = location(&response);
        assert!(to.starts_with("/ui/login"), "{how}: {to}");
        assert!(
            response.text().await.unwrap().is_empty(),
            "a bot's token {how} was shown a page body, not only redirected"
        );
    }

    // ── a real login does open the page ──────────────────────────────────────
    let opened = browser()
        .get(format!("http://{addr}/person:alpha/"))
        .header("cookie", &real_session)
        .send()
        .await
        .unwrap();
    assert_eq!(opened.status(), reqwest::StatusCode::OK);
    assert!(
        opened.text().await.unwrap().contains("person:alpha"),
        "the real session must be shown the page"
    );

    // ── and the session is no credential at /mcp ────────────────────────────
    let cookie_value = real_session
        .split_once('=')
        .map(|(_, value)| value.to_string())
        .expect("a session pair is name=value");
    assert_eq!(
        at_mcp(Some(cookie_value)).await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "the UI's session cookie must not be a bearer token for /mcp"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_listing_does_not_take_over_what_was_already_served() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();

    // The listing is mounted on a catch-all, so the paths that were already
    // there have to keep winning — and none of them may start answering the
    // login redirect the listing answers with.
    let health = client
        .get(format!("http://{addr}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    assert_eq!(health.text().await.unwrap(), "ok");

    let metadata = client
        .get(format!(
            "http://{addr}/.well-known/oauth-protected-resource"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(metadata.status(), reqwest::StatusCode::OK);
    assert!(
        metadata
            .text()
            .await
            .unwrap()
            .contains("authorization_servers"),
        "the protected-resource metadata must still be the metadata"
    );

    // The route the whole agent surface lives on. A catch-all in front of it
    // would not fail loudly: `/mcp` is not a handle path, so the listing would
    // report it missing, and every client would read that as a server that does
    // not speak MCP.
    let mcp = client
        .get(format!("http://{addr}/mcp"))
        .send()
        .await
        .unwrap();
    assert!(
        !mcp.status().is_redirection(),
        "/mcp answers a program, not a person, so it is never sent to a login: {} to {}",
        mcp.status(),
        mcp.headers()
            .get(reqwest::header::LOCATION)
            .map_or("nowhere", |to| to.to_str().unwrap_or("nowhere")),
    );
    assert_ne!(
        mcp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "the listing's catch-all must not swallow the transport"
    );
    // And it is the transport itself answering, not merely something that is
    // not the listing: the words are the streamable-HTTP transport's own.
    let refusal = mcp.text().await.unwrap();
    assert!(
        refusal.contains("text/event-stream"),
        "/mcp must still be the MCP transport, stating its own terms: {refusal}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_path_that_could_never_be_a_handle_is_not_found_rather_than_a_login() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;

    // The listing is mounted on a catch-all, so every path this server does not
    // implement now reaches it. A path that is not a handle path can never name
    // an entity, so it is missing — telling a client probing for an endpoint to
    // go and log in names the wrong problem, and it is the one a client
    // discovering this server actually hits.
    for probe in [
        "/.well-known/oauth-authorization-server",
        "/robots.txt",
        "/nope/nope",
    ] {
        let response = browser()
            .get(format!("http://{addr}{probe}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::NOT_FOUND,
            "{probe} is not a listing path, so it is missing"
        );
    }

    // The pairing: a path that COULD name an entity still asks for a login,
    // because whether it does is not something an anonymous caller may learn.
    let gated = browser()
        .get(format!("http://{addr}/person:contract-ghost/"))
        .send()
        .await
        .unwrap();
    assert!(
        gated.status().is_redirection(),
        "a handle path is behind the login whether or not anything is filed at it"
    );
    ct.cancel();
}

#[tokio::test]
async fn an_entity_whose_parent_is_missing_is_shown_as_damaged_rather_than_hidden() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) = spawn_jojobot_over(
        endpoints,
        &[READER],
        &idp,
        seeded_board_over(memory_with_an_orphan().await).await,
    )
    .await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // It sits under a handle nobody has, so it is nobody's child — and it is
    // not a root either. Without a place of its own it is on no page at all.
    assert!(
        section(&body, "unreachable").contains("href=\"/thing:sigma/\""),
        "an entity whose parent is missing must still be reachable: {body}"
    );
    // Promoting it to a root would show a damaged record as a normal one.
    assert!(
        !section(&body, "roots").contains("thing:sigma"),
        "a record with a dangling parent is not a root: {body}"
    );
    // The pairing: the roots list still works in the same read, so the
    // assertion above cannot pass against a page that files everything as
    // damaged.
    assert!(
        section(&body, "roots").contains("href=\"/person:alpha/\""),
        "a genuine root is still a root: {body}"
    );
    ct.cancel();
}

#[tokio::test]
async fn an_archived_entity_is_excluded_from_every_browse_but_still_serves_its_own_page() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) = spawn_jojobot_over(
        endpoints,
        &[READER],
        &idp,
        seeded_board_over(memory_with_an_archived_entity().await).await,
    )
    .await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let roots = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !section(&roots, "roots").contains("shelbyville"),
        "an archived root must not be on the default browse: {roots}"
    );
    // The pairing: a live root is still listed in the same read.
    assert!(
        section(&roots, "roots").contains("href=\"/person:alpha/\""),
        "a live root is still a root: {roots}"
    );

    let parent = read(&client, addr, "/person:alpha/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !parent.contains("topic:widgets"),
        "an archived child must not be listed below its parent: {parent}"
    );

    let direct = read(&client, addr, "/place:shelbyville/", &cookie).await;
    assert_eq!(
        direct.status(),
        reqwest::StatusCode::OK,
        "a direct handle still serves an archived entity whole"
    );
    ct.cancel();
}

// --- the other record types -------------------------------------------------

#[tokio::test]
async fn a_bot_page_shows_the_mailbox_it_owns_and_the_mail_in_it() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let mail = section(&body, "mailbox");
    assert!(
        mail.contains("A second pair of eyes"),
        "a message's subject is what a reader scans: {body}"
    );
    assert!(
        mail.contains("bot:gamma"),
        "who sent it is half of what a message is: {body}"
    );
    assert!(
        mail.contains("new"),
        "the state is what says whether anybody has taken it: {body}"
    );
    ct.cancel();
}

// --- a page holds the newest of what it lists, and says what it left out ------

/// A page of the browser as a logged-in reader sees it, over a board a case
/// built, and nothing else.
async fn page_over(board: Board, path: &str) -> String {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let body = read(&client, addr, path, &cookie)
        .await
        .text()
        .await
        .unwrap();
    ct.cancel();
    body
}

/// The count a page says it left out under `id`, or `None` when it says nothing.
/// Read off the element's own attribute: the sentence around it is the page's
/// wording and not what a case holds it to.
fn left_out(body: &str, id: &str) -> Option<usize> {
    let anchor = format!("id=\"{id}\" data-left-out=\"");
    let rest = body.split_once(&anchor)?.1;
    Some(
        rest.split_once('"')
            .expect("the attribute closes")
            .0
            .parse()
            .expect("a count"),
    )
}

/// `n` moments one minute apart, the first at the fixed instant.
fn minute(n: usize) -> jiff::Timestamp {
    FIXED_INSTANT + jiff::SignedDuration::from_secs(60 * n as i64)
}

/// Otto and the box it owns, with no mail in it and no run.
async fn otto_with_nothing_yet() -> Board {
    let memory = seeded_memory().await;
    let bot = EntityId("bot:otto".into());
    memory
        .add_entity(NewEntity::new(bot.clone(), "Otto", "the fixture roster"))
        .await
        .expect("the bot is written");
    let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    mailboxes
        .create_mailbox(&MailboxName("otto".to_string()), &bot, None)
        .await
        .expect("the box opens");
    Board {
        memory,
        mailboxes,
        sessions: Arc::new(InMemorySessions::new()),
    }
}

/// The page of a bot holding `n` messages, the first sent earliest.
async fn otto_page_with_mail(n: usize) -> String {
    let board = otto_with_nothing_yet().await;
    for i in 1..=n {
        board
            .mailboxes
            .post_message(NewMessage {
                mailbox: MailboxName("otto".to_string()),
                body: format!("body {i}"),
                subject: Some(format!("mail-{i:02}")),
                sender: "bot:gamma".to_string(),
                sent_at: minute(i),
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("the message is posted");
    }
    page_over(board, "/bot:otto/").await
}

/// **A bot's page holds the newest twenty messages, newest first, and says how
/// many it left out.** Over the limit: the newest twenty are there and the five
/// before them are not, and the count is five. At the limit: all twenty and no
/// line, so a page that always said something would fail here, and a page that
/// always cut would fail above.
#[tokio::test]
async fn a_bot_page_holds_the_newest_twenty_messages_and_counts_the_rest() {
    let over = otto_page_with_mail(25).await;
    for kept in 6..=25 {
        assert!(
            over.contains(&format!("mail-{kept:02}")),
            "mail-{kept:02}: {over}"
        );
    }
    for dropped in 1..=5 {
        assert!(
            !over.contains(&format!("mail-{dropped:02}")),
            "mail-{dropped:02} is older than the twenty: {over}"
        );
    }
    assert!(
        place(&over, "mail-25") < place(&over, "mail-24"),
        "the newest comes first: {over}"
    );
    assert_eq!(left_out(&over, "mailbox-left-out"), Some(5), "{over}");
    assert!(
        over.contains("25 new"),
        "the counts above the table still count the whole box: {over}"
    );

    let at = otto_page_with_mail(20).await;
    for kept in 1..=20 {
        assert!(
            at.contains(&format!("mail-{kept:02}")),
            "mail-{kept:02}: {at}"
        );
    }
    assert_eq!(left_out(&at, "mailbox-left-out"), None, "{at}");
}

/// The page of a bot holding `n` runs, each with one beat, the first earliest.
async fn otto_page_with_runs(n: usize) -> String {
    let board = otto_with_nothing_yet().await;
    for i in 1..=n {
        let run = board
            .sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:otto".into()),
                sid: Sid(format!("r{i:03}")),
                focus: format!("focus-{i:02}"),
                started_at: minute(i),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the run begins");
        board
            .sessions
            .append(
                &run.id,
                NewEntry::manual(format!("beat-{i:02}"), minute(i), None),
            )
            .await
            .expect("the beat is recorded");
    }
    page_over(board, "/bot:otto/").await
}

/// **A bot's page lists every run and writes out the chronologies of the newest
/// twenty**, then says how many it left out. The table is one line a run, so it
/// is the index of what exists; the chronologies are what grows.
#[tokio::test]
async fn a_bot_page_writes_out_the_newest_twenty_chronologies_and_counts_the_rest() {
    let over = otto_page_with_runs(25).await;
    let runs = section(&over, "sessions");
    for every in 1..=25 {
        assert!(
            runs.contains(&format!("focus-{every:02}")),
            "the table keeps a line for run {every}: {over}"
        );
    }
    for kept in 6..=25 {
        assert!(
            over.contains(&format!("beat-{kept:02}")),
            "beat-{kept:02}: {over}"
        );
    }
    for dropped in 1..=5 {
        assert!(
            !over.contains(&format!("beat-{dropped:02}")),
            "beat-{dropped:02} is in a run older than the twenty: {over}"
        );
    }
    assert!(
        place(&over, "Chronology of r025") < place(&over, "Chronology of r024"),
        "the newest run comes first: {over}"
    );
    assert_eq!(left_out(&over, "sessions-left-out"), Some(5), "{over}");

    let at = otto_page_with_runs(20).await;
    for kept in 1..=20 {
        assert!(
            at.contains(&format!("beat-{kept:02}")),
            "beat-{kept:02}: {at}"
        );
    }
    assert_eq!(left_out(&at, "sessions-left-out"), None, "{at}");
}

/// The page of `person:alpha` holding `n` keys, written one day apart, the
/// first earliest.
async fn alpha_page_with_keys(n: usize) -> String {
    let raw = InMemoryMemory::booted();
    let clock = raw.clock_handle();
    let memory: Arc<dyn Memory> = Arc::new(mention::Mentioning::new(Arc::new(raw)));
    seed(memory.as_ref()).await;
    for i in 1..=n {
        clock.stating(Date::constant(2026, 4, 1) + jiff::Span::new().days(i as i64));
        let mut piece = NewFact::about(
            EntityId("person:alpha".into()),
            format!("wrote key {i}"),
            Date::constant(2026, 4, 1),
        );
        piece.fields = [(format!("key-{i:02}"), format!("value-{i:02}"))]
            .into_iter()
            .collect();
        piece.sets = piece.fields.keys().cloned().collect();
        memory.capture(piece).await.expect("the field is written");
    }
    page_over(seeded_board_over(memory).await, "/person:alpha/").await
}

/// **A page shows the forty keys written most recently, newest first, and says
/// how many it left out.** The age is the moment of the write holding the value
/// now, so a key written again moves up.
#[tokio::test]
async fn a_node_page_shows_the_forty_newest_keys_and_counts_the_rest() {
    let over = alpha_page_with_keys(45).await;
    let fields = section(&over, "fields");
    for kept in 6..=45 {
        assert!(
            fields.contains(&format!("key-{kept:02}")),
            "key-{kept:02}: {over}"
        );
    }
    for dropped in 1..=5 {
        assert!(
            !fields.contains(&format!("key-{dropped:02}")),
            "key-{dropped:02} is older than the forty: {over}"
        );
    }
    assert!(
        place(fields, "key-45") < place(fields, "key-44"),
        "the newest comes first: {over}"
    );
    assert_eq!(left_out(&over, "fields-left-out"), Some(5), "{over}");

    let at = alpha_page_with_keys(40).await;
    let fields = section(&at, "fields");
    for kept in 1..=40 {
        assert!(
            fields.contains(&format!("key-{kept:02}")),
            "key-{kept:02}: {at}"
        );
    }
    assert_eq!(left_out(&at, "fields-left-out"), None, "{at}");
}

/// The text of the first table row of this class on a page, or nothing.
fn row_of<'a>(page: &'a str, class: &str) -> Option<&'a str> {
    let anchor = format!("<tr class=\"{class}\">");
    let start = page.find(&anchor)?;
    let rest = &page[start..];
    Some(&rest[..rest.find("</tr>")?])
}

/// 🚨 **A record shows what it set on its thing apart from its own fields**, the
/// way every served read does. A claim with an own `note` and a set `status`
/// showed both on one line of "fields", so a reader took the label for a property
/// of the thing. Two lines now: the record's own fields, then its settings.
///
/// **Both halves**: each line carries its keys and not the other's, and a claim
/// that set nothing has no settings line.
#[tokio::test]
async fn a_record_shows_its_own_fields_and_its_settings_on_separate_lines() {
    let memory = seeded_memory().await;
    let mut both = NewFact::about(
        EntityId("person:alpha".into()),
        "a claim with one of each",
        Date::constant(2026, 4, 1),
    );
    both.fields = [
        ("note".to_string(), "kept on the claim".to_string()),
        ("status".to_string(), "now".to_string()),
    ]
    .into_iter()
    .collect();
    both.sets = ["status".to_string()].into_iter().collect();
    memory.capture(both).await.expect("the claim is written");

    let page = page_over(seeded_board_over(memory.clone()).await, "/person:alpha/").await;
    let own = row_of(&page, "fields").unwrap_or_else(|| panic!("no fields line: {page}"));
    let set = row_of(&page, "sets").unwrap_or_else(|| panic!("no sets line: {page}"));
    assert!(own.contains("note = kept on the claim"), "{own}");
    assert!(
        !own.contains("status"),
        "a setting is not an own field: {own}"
    );
    assert!(set.contains("status = now"), "{set}");
    assert!(
        !set.contains("note"),
        "an own field is not a setting: {set}"
    );

    // A claim that set nothing has no settings line.
    let memory = seeded_memory().await;
    let mut label = NewFact::about(
        EntityId("person:alpha".into()),
        "a claim that only labels itself",
        Date::constant(2026, 4, 2),
    );
    label.fields = [("note".to_string(), "only a label".to_string())]
        .into_iter()
        .collect();
    memory.capture(label).await.expect("the claim is written");
    let page = page_over(seeded_board_over(memory).await, "/person:alpha/").await;
    assert!(row_of(&page, "fields").is_some(), "{page}");
    assert!(row_of(&page, "sets").is_none(), "{page}");
}

// --- the operator's own mail ------------------------------------------------

/// A mark-processed POST the way the page's own form sends it: from this
/// origin, with the session cookie.
async fn mark_processed_in_the_ui(
    addr: SocketAddr,
    cookie: &str,
    id: &str,
    note: &str,
    origin: Option<&str>,
) -> reqwest::Response {
    let mut request = browser()
        .post(format!("http://{addr}/ui/mail/processed"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("id", id), ("note", note)]);
    if let Some(origin) = origin {
        request = request.header(reqwest::header::ORIGIN, origin);
    }
    request.send().await.unwrap()
}

/// Whether the page carries the mark-processed form for this message id.
fn mark_form_for(page: &str, id: &str) -> bool {
    page.contains(&format!(
        "action=\"/ui/mail/processed\"><input type=\"hidden\" name=\"id\" value=\"{id}\">"
    ))
}

/// The state a message is in, read from the store rather than the page.
async fn state_in_the_store(
    mailboxes: &InMemoryMailboxes,
    id: &jojobot_domain::mailbox::MessageId,
) -> String {
    mailboxes
        .message_by_id(id)
        .await
        .expect("reads")
        .expect("the message is there")
        .state
        .as_token()
        .to_string()
}

/// **The operator's box is on the operator's page, to the operator, and on no
/// other page.** A non-operator person's page, the instance's own page and a
/// bot's page leave the message out. The bot's own box on its own page is the
/// positive: the window does show mail where it shows any.
#[tokio::test]
async fn the_operators_mail_is_on_the_operators_page_and_no_other() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let operator_page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let box_section = section(&operator_page, "mailbox");
    assert!(
        box_section.contains(OPERATOR_SUBJECT),
        "the operator's page shows their box: {operator_page}"
    );
    assert!(
        !operator_page.contains(OPERATOR_BODY),
        "a new message's body is not shown by a GET: {operator_page}"
    );
    assert!(
        box_section.contains(mail.to_the_operator.as_str()),
        "each message is shown by its id, which is what the action names: {operator_page}"
    );

    for other in [
        "/person:milhouse/",
        "/topic:instance/",
        "/bot:otto/",
        "/person:alpha/",
    ] {
        let page = read(&client, addr, other, &cookie)
            .await
            .text()
            .await
            .unwrap();
        assert!(
            !page.contains(OPERATOR_BODY) && !page.contains(OPERATOR_SUBJECT),
            "{other} carries the operator's mail: {page}"
        );
    }
    let bot_page = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        bot_page.contains("A note for a bot"),
        "a bot's own box is still on its own page: {bot_page}"
    );
    assert!(
        !operator_page.contains("A note for a bot"),
        "a bot's mail is not the operator's: {operator_page}"
    );
    ct.cancel();
}

/// **A message the sender replaced shows as replaced on the operator's page, and
/// names its replacement.** The mistake keeps its row, its state and its two
/// buttons; the correction beside it carries no such mark. The box counts one new
/// message, not two.
#[tokio::test]
async fn a_replaced_message_shows_as_replaced_on_the_operators_page_and_names_its_replacement() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let replacement = board
        .mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName::named_for(&EntityId("person:lisa".into())),
            body: "the corrected figure".to_string(),
            subject: Some("A correction for the operator".to_string()),
            sender: "bot:otto".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: Some(mail.to_the_operator.clone()),
        })
        .await
        .expect("posted")
        .written()
        .expect("the replacement landed")
        .id;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let row_for = |id: &str| -> String {
        let anchor = format!("<tr><td>{id}</td>");
        let start = page
            .find(&anchor)
            .unwrap_or_else(|| panic!("no row for {id}: {page}"));
        let rest = &page[start..];
        rest[..rest.find("</tr>").expect("the row ends")].to_string()
    };
    let mistake = row_for(mail.to_the_operator.as_str());
    let correction = row_for(replacement.as_str());
    assert!(
        mistake.contains(&format!("data-replaced-by=\"{replacement}\"")),
        "the replaced row names its replacement: {mistake}"
    );
    assert!(
        !correction.contains("data-replaced-by"),
        "the replacement is not marked as replaced: {correction}"
    );
    assert!(
        mark_form_for(&page, mail.to_the_operator.as_str()),
        "the replaced message keeps its Mark processed button: {page}"
    );
    assert!(
        page.contains("<p>1 new,"),
        "the box counts one new message, not two: {page}"
    );
    ct.cancel();
}

/// **A GET of the operator's page moves no mail**, so a prefetch of it cannot
/// mark anything seen. The message stays `new` after the operator's page, and
/// after another person's, is opened; the page shows its row and an Open
/// button, and none of its body.
#[tokio::test]
async fn opening_the_operators_page_leaves_new_mail_new() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "new"
    );

    read(&client, addr, "/person:milhouse/", &cookie).await;
    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    read(&client, addr, "/person:lisa/", &cookie).await;

    assert!(
        page.contains("action=\"/ui/mail/open\"") && page.contains(mail.to_the_operator.as_str()),
        "a new message carries the button that opens it: {page}"
    );
    assert!(
        !page.contains(OPERATOR_BODY),
        "and its body is not on a GET: {page}"
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "new",
        "a GET of the operator's page took delivery"
    );
    assert_eq!(state_in_the_store(&mailboxes, &mail.to_a_bot).await, "new");
    ct.cancel();
}

/// **The Open button takes delivery of that one message and no other.** The
/// operator's box holds a second new message; opening the first moves only the
/// first to `read`, lands the operator back on their page, and the page now
/// shows the body it held back. A bot's message is not touched.
#[tokio::test]
async fn the_open_button_takes_delivery_of_one_message_only() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let second = mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName::named_for(&EntityId("person:lisa".into())),
            body: "the second report is about the kiln".to_string(),
            subject: Some("A second report".to_string()),
            sender: "bot:otto".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("posted")
        .written()
        .expect("the post landed")
        .id;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let origin = format!("http://{addr}");

    let opened = open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
    assert!(
        opened.status().is_redirection(),
        "the button lands the operator back on the page: {}",
        opened.status()
    );
    assert_eq!(location(&opened), "/person:lisa/");
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "read"
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &second).await,
        "new",
        "opening one message took delivery of another"
    );
    assert_eq!(state_in_the_store(&mailboxes, &mail.to_a_bot).await, "new");

    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(OPERATOR_BODY),
        "an opened message shows its body: {page}"
    );
    assert!(
        mark_form_for(&page, mail.to_the_operator.as_str()),
        "an opened message still carries the mark-processed form: {page}"
    );
    assert!(
        !page.contains("the second report is about the kiln"),
        "a message still new keeps its body back: {page}"
    );
    ct.cancel();
}

/// **The Open action checks where it came from and whose box the message is
/// in**, exactly as the mark-processed action does. Another origin, no origin,
/// no session and a bot's message each move nothing; the same POST from this
/// origin lands in the case above.
#[tokio::test]
async fn the_open_action_refuses_another_origin_no_session_and_a_bots_message() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let origin = format!("http://{addr}");
    let id = mail.to_the_operator.as_str();

    for (what, origin) in [
        ("from another origin", Some("https://elsewhere.example")),
        ("with no origin at all", None),
    ] {
        let refused = open_in_the_ui(addr, &cookie, id, origin).await;
        assert_eq!(
            refused.status(),
            reqwest::StatusCode::FORBIDDEN,
            "an Open {what} with a valid cookie was not refused"
        );
        assert_eq!(
            state_in_the_store(&mailboxes, &mail.to_the_operator).await,
            "new",
            "an Open {what} moved the message"
        );
    }

    let no_session = browser()
        .post(format!("http://{addr}/ui/mail/open"))
        .header(reqwest::header::ORIGIN, &origin)
        .form(&[("id", id)])
        .send()
        .await
        .unwrap();
    assert!(
        !no_session.status().is_success() && !no_session.status().is_redirection(),
        "an Open with no session was accepted: {}",
        no_session.status()
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "new"
    );

    let bots = open_in_the_ui(addr, &cookie, mail.to_a_bot.as_str(), Some(&origin)).await;
    assert!(
        !bots.status().is_success() && !bots.status().is_redirection(),
        "a bot's message was opened from the operator's action: {}",
        bots.status()
    );
    assert_eq!(state_in_the_store(&mailboxes, &mail.to_a_bot).await, "new");
    ct.cancel();
}

/// **The operator's mail is served only while the allowlist admits exactly one
/// person.** A UI session carries no subject, so with two people admitted, or
/// with the allowlist open (which admits every subject), the operator's page
/// withholds the box and both actions refuse, and no message moves. One person
/// is the positive: the same page shows the box and the same actions land.
#[tokio::test]
async fn the_operators_mail_is_withheld_unless_the_allowlist_is_one_person() {
    for (what, allowed) in [
        ("two people", vec![READER, "sub-of-a-second-person"]),
        ("an open allowlist", vec![]),
    ] {
        let idp = support::TestIdp::new();
        let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
        let board = seeded_board().await;
        let mail = with_an_operators_mail(&board).await;
        let mailboxes = board.mailboxes.clone();
        let (addr, ct, _board) = spawn_jojobot_over(endpoints, &allowed, &idp, board).await;
        let client = browser();
        let cookie = log_in(&client, addr, "/").await;
        let origin = format!("http://{addr}");

        let page = read(&client, addr, "/person:lisa/", &cookie)
            .await
            .text()
            .await
            .unwrap();
        assert!(
            page.contains("id=\"mailbox-withheld\""),
            "with {what} the page says the mail is not shown: {page}"
        );
        // Everything but the stylesheet, the header and its breadcrumbs
        // included: a message id is a short number here, and the stylesheet is
        // full of digits that are not mail.
        let (before_style, rest) = page
            .split_once("<style>")
            .unwrap_or_else(|| panic!("a page with a stylesheet: {page}"));
        let after_style = rest
            .split_once("</style>")
            .unwrap_or_else(|| panic!("a stylesheet that closes: {page}"))
            .1;
        let unstyled = format!("{before_style}{after_style}");
        assert!(
            unstyled.contains("<header class=\"bar\">"),
            "the header is among what is checked: {page}"
        );
        assert!(
            !unstyled.contains(OPERATOR_SUBJECT)
                && !unstyled.contains(mail.to_the_operator.as_str())
                && !unstyled.contains("/ui/mail/"),
            "with {what} the page carries the box: {page}"
        );

        let opened =
            open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
        assert_eq!(
            opened.status(),
            reqwest::StatusCode::FORBIDDEN,
            "with {what} the Open action was not refused"
        );
        let marked = mark_processed_in_the_ui(
            addr,
            &cookie,
            mail.to_the_operator.as_str(),
            "x",
            Some(&origin),
        )
        .await;
        assert_eq!(
            marked.status(),
            reqwest::StatusCode::FORBIDDEN,
            "with {what} the mark-processed action was not refused"
        );
        assert_eq!(
            state_in_the_store(&mailboxes, &mail.to_the_operator).await,
            "new",
            "with {what} an action moved the message"
        );
        ct.cancel();
    }
}

/// **A base URL that carries a path still matches the browser's own origin.**
/// An instance is configured with the URL its people reach it at, which may
/// have a path, and a browser's `Origin` header is the scheme and the authority
/// and never the path. Both actions on the operator's mail are POSTs that check
/// it, so a path in the configured URL must not make every one of them a 403.
/// The proxy that serves the path takes it off before the request arrives, so
/// the case logs in the way that proxy would deliver the callback.
///
/// Paired with the same actions from another origin, and from an `Origin` that
/// carries the path (which no browser sends), both refused.
#[tokio::test]
async fn a_base_url_with_a_path_still_accepts_the_browsers_origin() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) =
        spawn_jojobot_serving(endpoints, &[READER], &idp, board, "http", None, "/jojobot").await;
    let client = browser();
    let cookie = log_in_behind_a_path(&client, addr, "/jojobot").await;
    let origin = format!("http://{addr}");
    let id = mail.to_the_operator.as_str();

    for (what, sent) in [
        (
            "from another origin",
            "https://elsewhere.example".to_string(),
        ),
        (
            "with the path in the origin",
            format!("http://{addr}/jojobot"),
        ),
    ] {
        let refused = open_in_the_ui(addr, &cookie, id, Some(&sent)).await;
        assert_eq!(
            refused.status(),
            reqwest::StatusCode::FORBIDDEN,
            "an Open {what} was not refused"
        );
    }
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "new",
        "a refused Open moved the message"
    );

    let opened = open_in_the_ui(addr, &cookie, id, Some(&origin)).await;
    assert!(
        opened.status().is_redirection(),
        "an Open from the browser's own origin was refused behind a base path: {}",
        opened.status()
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "read"
    );
    let marked = mark_processed_in_the_ui(addr, &cookie, id, "done", Some(&origin)).await;
    assert!(
        marked.status().is_redirection(),
        "a mark-processed from the browser's own origin was refused behind a base path: {}",
        marked.status()
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "processed"
    );
    ct.cancel();
}

/// Walk the login for an instance whose base URL has a path: the issuer sends
/// the browser back to the callback under that path, and the proxy in front
/// takes the path off, so the callback is delivered at the root.
async fn log_in_behind_a_path(client: &reqwest::Client, jojobot: SocketAddr, path: &str) -> String {
    let turned_away = client
        .get(format!("http://{jojobot}/"))
        .send()
        .await
        .unwrap();
    let to_login = location(&turned_away);
    let sent_out = client
        .get(format!("http://{jojobot}{to_login}"))
        .send()
        .await
        .unwrap();
    let back = client.get(location(&sent_out)).send().await.unwrap();
    let callback = location(&back).replacen(&format!("{path}/ui/callback"), "/ui/callback", 1);
    let opened = client.get(&callback).send().await.unwrap();
    assert!(
        opened.status().is_redirection(),
        "a completed login lands the browser back, got {}",
        opened.status()
    );
    session_pair(
        opened
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .expect("a completed login sets a session cookie")
            .to_str()
            .unwrap(),
    )
}

/// **One person listed twice is still one person.** The allowlist is a set of
/// subjects, and a subject named twice (by a deployment that lists the same id
/// under two names it knows it by, or by a stray repeat) admits the same one
/// person. The operator's mail is served only while the allowlist admits exactly
/// one person, so counting list entries would withhold the page from the one
/// person it is for.
///
/// Paired with two different subjects, which still withhold it, in the case
/// above.
#[tokio::test]
async fn one_person_listed_twice_still_gets_the_operators_mail() {
    for (what, allowed) in [
        ("listed twice", vec![READER, READER]),
        (
            "listed twice, once with a blank beside it",
            vec![READER, " sub-reader ", ""],
        ),
    ] {
        let idp = support::TestIdp::new();
        let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
        let board = seeded_board().await;
        let mail = with_an_operators_mail(&board).await;
        let mailboxes = board.mailboxes.clone();
        let (addr, ct, _board) = spawn_jojobot_over(endpoints, &allowed, &idp, board).await;
        let client = browser();
        let cookie = log_in(&client, addr, "/").await;
        let origin = format!("http://{addr}");

        let page = read(&client, addr, "/person:lisa/", &cookie)
            .await
            .text()
            .await
            .unwrap();
        assert!(
            !page.contains("id=\"mailbox-withheld\"") && page.contains(OPERATOR_SUBJECT),
            "with one person {what} the page withheld the box: {page}"
        );
        let opened =
            open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
        assert!(
            opened.status().is_redirection(),
            "with one person {what} the Open action was refused: {}",
            opened.status()
        );
        assert_eq!(
            state_in_the_store(&mailboxes, &mail.to_the_operator).await,
            "read",
            "with one person {what} the Open did not land"
        );
        ct.cancel();
    }
}

/// **Mark-processed in the UI moves the message to processed with its note**,
/// read back from the store and from the page. A bot's message is not the
/// operator's to process here.
#[tokio::test]
async fn the_operator_marks_a_message_processed_with_a_note() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let origin = format!("http://{addr}");

    // **A message still new carries the mark-processed form**, beside its Open
    // button: the operator may finish it without opening it, and nothing but
    // Open sets `read`.
    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        mark_form_for(&page, mail.to_the_operator.as_str()) && page.contains("name=\"note\""),
        "a new message carries a mark-processed form with a note: {page}"
    );
    assert_eq!(
        state_in_the_store(&mailboxes, &mail.to_the_operator).await,
        "new",
        "the page showed the form without opening the message"
    );

    let done = mark_processed_in_the_ui(
        addr,
        &cookie,
        mail.to_the_operator.as_str(),
        "paid on the second",
        Some(&origin),
    )
    .await;
    assert!(
        done.status().is_redirection(),
        "the action lands the operator back on the page: {}",
        done.status()
    );
    assert_eq!(location(&done), "/person:lisa/");
    let held = mailboxes
        .message_by_id(&mail.to_the_operator)
        .await
        .expect("reads")
        .expect("the message is there");
    assert_eq!(held.state.as_token(), "processed");
    assert_eq!(held.notes.as_deref(), Some("paid on the second"));
    let after = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        after.contains("paid on the second"),
        "the note is on the page: {after}"
    );

    // A message in a bot's box is not the operator's to process here.
    let refused = mark_processed_in_the_ui(
        addr,
        &cookie,
        mail.to_a_bot.as_str(),
        "not mine",
        Some(&origin),
    )
    .await;
    assert!(
        !refused.status().is_success() && !refused.status().is_redirection(),
        "a bot's message was processed from the operator's action: {}",
        refused.status()
    );
    let bots = mailboxes
        .message_by_id(&mail.to_a_bot)
        .await
        .expect("reads")
        .expect("there");
    assert_ne!(bots.state.as_token(), "processed");
    ct.cancel();
}

/// **The action is a POST that checks where it came from.** A cookie alone is
/// not enough: a request from another origin, or from none, is refused and moves
/// nothing. Paired with the same POST from this origin in the case above, which
/// lands. A request with no session is refused as well.
#[tokio::test]
async fn the_action_refuses_a_post_from_another_origin_and_one_with_no_session() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let id = mail.to_the_operator.as_str();

    for (what, origin) in [
        ("from another origin", Some("https://elsewhere.example")),
        ("with an origin that only starts like ours", {
            // Held outside the call: the string must outlive the borrow.
            None
        }),
        ("with no origin at all", None),
    ] {
        let origin_owned = format!("http://{addr}.elsewhere.example");
        let origin = if what.starts_with("with an origin") {
            Some(origin_owned.as_str())
        } else {
            origin
        };
        let refused = mark_processed_in_the_ui(addr, &cookie, id, "x", origin).await;
        assert_eq!(
            refused.status(),
            reqwest::StatusCode::FORBIDDEN,
            "a POST {what} with a valid cookie was not refused"
        );
        let held = mailboxes
            .message_by_id(&mail.to_the_operator)
            .await
            .expect("reads")
            .expect("there");
        assert_ne!(
            held.state.as_token(),
            "processed",
            "a POST {what} moved the message"
        );
    }

    let no_session = browser()
        .post(format!("http://{addr}/ui/mail/processed"))
        .header(reqwest::header::ORIGIN, format!("http://{addr}"))
        .form(&[("id", id), ("note", "x")])
        .send()
        .await
        .unwrap();
    assert!(
        !no_session.status().is_success(),
        "a POST with no session was accepted: {}",
        no_session.status()
    );
    let held = mailboxes
        .message_by_id(&mail.to_the_operator)
        .await
        .expect("reads")
        .expect("there");
    assert_ne!(held.state.as_token(), "processed");
    ct.cancel();
}

/// **With no `Origin`, the `Referer` stands in, and it has to sit UNDER this
/// origin.** A Referer that only starts like the origin, on a longer port or a
/// longer host, is another origin and is refused; one under ours lands. The
/// refusals come first so the positive is the one that moves the message.
#[tokio::test]
async fn the_action_takes_a_referer_under_our_origin_when_there_is_no_origin() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board().await;
    let mail = with_an_operators_mail(&board).await;
    let mailboxes = board.mailboxes.clone();
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let id = mail.to_the_operator.as_str();
    let post_with_referer = |referer: String| {
        let cookie = cookie.clone();
        async move {
            browser()
                .post(format!("http://{addr}/ui/mail/processed"))
                .header(reqwest::header::COOKIE, cookie)
                .header(reqwest::header::REFERER, referer)
                .form(&[("id", id), ("note", "x")])
                .send()
                .await
                .unwrap()
        }
    };

    for (what, referer) in [
        ("a longer port", format!("http://{addr}5/person:lisa/")),
        (
            "a longer host",
            format!("http://{addr}.elsewhere.example/person:lisa/"),
        ),
        ("another site", "https://elsewhere.example/".to_string()),
    ] {
        let refused = post_with_referer(referer).await;
        assert_eq!(
            refused.status(),
            reqwest::StatusCode::FORBIDDEN,
            "a Referer with {what} was not refused"
        );
        let held = mailboxes
            .message_by_id(&mail.to_the_operator)
            .await
            .expect("reads")
            .expect("there");
        assert_ne!(
            held.state.as_token(),
            "processed",
            "a Referer with {what} moved the message"
        );
    }

    let landed = post_with_referer(format!("http://{addr}/person:lisa/")).await;
    assert!(
        landed.status().is_redirection(),
        "a same-origin Referer with no Origin was refused: {}",
        landed.status()
    );
    let held = mailboxes
        .message_by_id(&mail.to_the_operator)
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(held.state.as_token(), "processed");
    ct.cancel();
}

#[tokio::test]
async fn a_bot_page_shows_its_runs_and_what_each_one_recorded() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let runs = section(&body, "sessions");
    assert!(
        runs.contains("pt1x"),
        "a run is told from another by its sid: {body}"
    );
    assert!(
        runs.contains("Reading the survey"),
        "the focus says what the run is for: {body}"
    );
    assert!(
        runs.contains("active"),
        "whether a run is still going is on it: {body}"
    );
    assert!(
        body.contains("Set out to read the survey end to end."),
        "the chronology is the record of the run, so it is on the page: {body}"
    );
    ct.cancel();
}

/// Every rail a page can reach, read the way the next reader would read it:
/// `scan` is the memory rail whole — every doc, its entity, its prose and the
/// facts in its table — beside the mail on the board and the runs on record.
///
/// Rendered to strings so a difference prints as one, and sorted so a store's
/// ordering is never mistaken for a change.
struct Rails {
    memory: Vec<String>,
    mail: Vec<String>,
    runs: Vec<String>,
}

async fn rails(board: &Board) -> Rails {
    let mut memory: Vec<String> = board
        .memory
        .scan()
        .await
        .expect("the store is readable")
        .iter()
        .map(|doc| format!("{doc:?}"))
        .collect();
    let mut mail: Vec<String> = board
        .mailboxes
        .scan_messages()
        .await
        .expect("the mail rail is readable")
        .iter()
        .map(|message| format!("{message:?}"))
        .collect();
    let mut runs: Vec<String> = board
        .sessions
        .sessions_of(&EntityId("bot:otto".into()))
        .await
        .expect("the runs are readable")
        .iter()
        .map(|run| format!("{run:?}"))
        .collect();
    memory.sort();
    mail.sort();
    runs.sort();
    Rails { memory, mail, runs }
}

/// **The listing is a window, and this is the pane.**
///
/// Looking through it must not change what is on the other side: a page that
/// took delivery of a message, began a run, or touched a doc would alter the
/// thing it was built to observe, and the bot on the other end would pay for it
/// — a message it never saw, gone from its next delivery.
///
/// **Every page kind, and every rail.** Each handler reaches a different set of
/// verbs — the index lists, a node page also recalls and scans, a bot page adds
/// the mail and the runs — and a rail nobody reads back is a rail a page is free
/// to write to.
#[tokio::test]
async fn looking_through_the_listing_writes_to_no_rail() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let before = rails(&board).await;
    assert!(
        !before.memory.is_empty() && !before.mail.is_empty() && !before.runs.is_empty(),
        "two empty rails compare equal and prove nothing: {:?}",
        (&before.memory, &before.mail, &before.runs)
    );

    let index = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let node = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let bot = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // The positive half, one per page: each really did serve the records it is
    // about to be checked against. Without it a handler that returned nothing
    // would pass every assertion below.
    assert!(
        index.contains("href=\"/person:alpha/\""),
        "the index must render the roots it is being checked for: {index}"
    );
    assert!(
        node.contains("The widget stall runs on Thursdays"),
        "the node page must render the facts held there: {node}"
    );
    assert!(
        section(&bot, "mailbox").contains("A second pair of eyes"),
        "the bot page must render the mail: {bot}"
    );
    assert!(
        section(&bot, "sessions").contains("pt1x"),
        "the bot page must render the runs: {bot}"
    );

    let after = rails(&board).await;
    assert_eq!(
        before.memory, after.memory,
        "serving a page rewrote the entity store"
    );
    assert_eq!(
        before.mail, after.mail,
        "serving a page took delivery, or moved mail the bot has never seen"
    );
    assert_eq!(
        before.runs, after.runs,
        "serving a page began, ended or swept a run — the page is not a boot"
    );
    ct.cancel();
}

/// **Escaping is a property of the served page, not of a function.**
///
/// Every value below is free text somebody wrote, and each reaches the browser
/// through a different call site. A pure-function test proves the escaper
/// transforms a string; it says nothing about whether the twenty places that
/// render one call it, and against a corpus with no markup in it the escaper is
/// the identity — so the corpus carries the markup and the assertion reads the
/// page.
#[tokio::test]
async fn markup_a_writer_typed_arrives_as_text_and_not_as_document() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for (path, fields) in [
        ("/", &["place-name"][..]),
        ("/place:shelbyville/", &["place-name"][..]),
        (
            "/person:alpha/topic:widgets/",
            &["fact-content", "prose"][..],
        ),
        (
            "/bot:otto/",
            &["message-subject", "message-body", "chronology-beat"][..],
        ),
    ] {
        let page = read(&client, addr, path, &cookie)
            .await
            .text()
            .await
            .unwrap();
        for field in fields {
            let raw = typed(field);
            // The pair, in the same read: the value is on the page, and it is on
            // it as text. Either alone passes against a page that dropped the
            // value entirely.
            assert!(
                page.contains(&as_text(&raw)),
                "{path} must carry the {field} a writer typed, escaped: {page}"
            );
            assert!(
                !page.contains(&raw),
                "{path} handed a writer's markup to the browser as document, at the {field}: {page}"
            );
        }
    }
    ct.cancel();
}

#[tokio::test]
async fn a_long_body_is_collapsed_to_its_opening_and_still_whole_on_the_page() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let mail = section(&body, "mailbox");

    // Collapsed: the ellipsis is what the digest strategy adds when it cuts, so
    // a summary carrying one is a body shown by its opening.
    let summary = summary_of(mail);
    assert!(
        summary.contains('…'),
        "a long body is folded to the opening the digest strategy renders: {mail}"
    );
    assert!(
        summary.contains("bytes"),
        "a folded block states how much of it there is: {mail}"
    );
    // And whole: the full text is on this page, not a fetch away. Without this
    // the assertion above passes against a page that threw the body out.
    assert!(
        mail.contains("This closing sentence is the tail of the long body."),
        "the whole body stays on the page: {mail}"
    );
    // The pairing that makes both mean something: the short message is NOT
    // collapsed, so a page that folded everything would fail here.
    assert_eq!(
        mail.matches("<details>").count(),
        1,
        "only the body that outgrew the budget is collapsed: {mail}"
    );
    ct.cancel();
}

#[tokio::test]
async fn a_long_beat_is_collapsed_the_same_way_as_a_long_body() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _board) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board().await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let body = read(&client, addr, "/bot:otto/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // A chronology entry is somebody's writing at the same lengths a message
    // body reaches, so it is folded by the same rule rather than a second one.
    let chronology = body
        .split_once("<h3>")
        .expect("the chronology is on the page")
        .1;
    assert!(
        summary_of(chronology).contains('…'),
        "a long beat is folded to its opening: {chronology}"
    );
    assert!(
        chronology.contains("This closing sentence is the tail of the long beat."),
        "the whole beat stays on the page: {chronology}"
    );
    assert_eq!(
        chronology.matches("<details>").count(),
        1,
        "only the beat that outgrew the budget is collapsed: {chronology}"
    );
    ct.cancel();
}

// --- a view's page runs the view -------------------------------------------

/// The seeded roster, plus the views this build supplies, over a store that
/// resolves them.
///
/// **The suite's ordinary board has no `Provisioned` layer**, so no shipped
/// view exists on any page it serves and a view page cannot be exercised at
/// all. The wiring here is what the binary wires: one set of provisions, read
/// by the store's guard and by the layer that resolves them into answers.
///
/// Two loops sit under a root, one long past due and one not due for
/// centuries. **The dates are far from any day this runs on**, because the
/// page reads the server's clock and a case whose verdict moved with the
/// calendar would be a case nobody can trust.
async fn memory_with_views() -> Arc<dyn Memory> {
    let supplied = jojobot_mcp::provisions();
    let store = InMemoryMemory::booted().knowing(supplied.clone());
    let memory: Arc<dyn Memory> = Arc::new(jojobot_adapters::provisioned::Provisioned::new(
        store, supplied,
    ));
    seed(memory.as_ref()).await;

    for (handle, name) in [
        ("rhythm:descale", "Descale the machine"),
        ("rhythm:worming", "Worm the dog"),
        // Alphabetically ahead of "descale", and overdue less long than it —
        // the disagreement between handle order and staleness order is what
        // makes an ordering case mean something. See
        // `a_view_that_asks_for_what_is_overdue_renders_oldest_first`.
        ("rhythm:abseiling", "Check the abseiling gear"),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = Some(EntityId("person:alpha".into()));
        memory
            .add_entity(new)
            .await
            .expect("the loop is written")
            .written()
            .expect("nothing on this board collides with it");
    }
    // The loop that has gone quiet, one that has not, and one that went
    // quiet more recently than the first — three points, so an ordering case
    // can tell "sorted" from "sorted by something else".
    for (handle, counts_from) in [
        ("rhythm:descale", "2020-01-06"),
        ("rhythm:worming", "2400-01-06"),
        ("rhythm:abseiling", "2024-01-06"),
    ] {
        memory
            .capture(set_on(NewFact {
                fields: [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "due_date".to_string()),
                    ("counts_from".to_string(), counts_from.to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "the loop was set up",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the schedule is written");
    }

    // The operator's own, beside the two the build ships: one that narrows the
    // same kind to what is late, and one nobody has finished writing.
    memory
        .add_entity(NewEntity::new(
            EntityId("view:my-week".into()),
            "My Week",
            "the operator",
        ))
        .await
        .expect("the operator's view is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "rhythm".to_string()),
                ("asks".to_string(), "overdue".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:my-week".into()),
                "what has gone quiet",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    // A question whose selection finds a loop that is not due, so its answer is
    // empty and the page has something to say about why.
    memory
        .add_entity(NewEntity::new(
            EntityId("view:nothing-owed".into()),
            "Nothing Owed",
            "the operator",
        ))
        .await
        .expect("the view is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "rhythm".to_string()),
                ("asks".to_string(), "overdue".to_string()),
                ("where".to_string(), "counts_from = 2400-01-06".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:nothing-owed".into()),
                "the loops counted from a far day",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    memory
        .add_entity(NewEntity::new(
            EntityId("view:matches-nothing".into()),
            "The Empty Shelf",
            "the operator",
        ))
        .await
        .expect("the view that finds none is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("selects".to_string(), "pet".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("view:matches-nothing".into()),
                "which pets are here",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    memory
        .add_entity(NewEntity::new(
            EntityId("view:names-no-kind".into()),
            "The Sandwich Board",
            "the operator",
        ))
        .await
        .expect("the view naming no kind is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("selects".to_string(), "sandwich".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("view:names-no-kind".into()),
                "which sandwiches are here",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    memory
        .add_entity(NewEntity::new(
            EntityId("view:my-people".into()),
            "My People",
            "the operator",
        ))
        .await
        .expect("the half-written view is created")
        .written()
        .expect("nothing on this board collides with it");

    // **A view that narrows by a key filter, not only by `selects`.** The
    // three loops above share a kind and a cadence but differ in
    // `counts_from`, so a filter on that key keeps exactly one.
    memory
        .add_entity(NewEntity::new(
            EntityId("view:the-old-loop".into()),
            "The Old Loop",
            "the operator",
        ))
        .await
        .expect("the filtered view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("selects".to_string(), "rhythm".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("view:the-old-loop".into()),
                "which loop is the old one",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("key".to_string(), "counts_from".to_string()),
                ("value".to_string(), "2020-01-06".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:the-old-loop".into()),
                "filters to counts_from 2020-01-06",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the filter record is written");

    // The rhythm kind says `counts_from` holds a date, which is what licenses
    // an ordering on it. The fake store declares no kind's keys on its own.
    memory
        .declare_type(
            DeclaredType::shipped("rhythm", vec![Field::new("counts_from", ValueType::Date)]),
            None,
        )
        .await
        .expect("the rhythm kind's date key is declared");
    // **A view that narrows by an expression and orders by a key**, which is
    // what the same page has to run beside a filter record. `counts_from` is a
    // date the rhythm kind declares, so the comparison is licensed; the order
    // runs against handle order, so a page that ignored it would list the other
    // loop first.
    memory
        .add_entity(NewEntity::new(
            EntityId("view:latest-first".into()),
            "Latest First",
            "the operator",
        ))
        .await
        .expect("the expression view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "rhythm".to_string()),
                ("where".to_string(), "counts_from > 2021-01-01".to_string()),
                ("order".to_string(), "counts_from descending".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:latest-first".into()),
                "the loops counted from after 2021, latest first",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the expression is written");

    // **Work in three conditions, for the ready view**: one that can start, one
    // that is finished, and one that waits on work that is not. The fourth is
    // what the third waits on, and it can start too.
    for (handle, name, fields) in [
        ("work:delta", "Wash the car", vec![("status", "next")]),
        ("work:epsilon", "File the taxes", vec![("status", "done")]),
        ("work:phi", "Buy the paint", vec![("status", "next")]),
        (
            "work:gamma",
            "Paint the fence",
            vec![("status", "next"), ("depends_on", "work:phi")],
        ),
    ] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the work is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(set_on(NewFact {
                fields: fields
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "where the work stands",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the work's state is written");
    }
    // **Two colleagues, one reporting to the other**, for the directory view.
    for (handle, name) in [("bot:lisa", "Lisa"), ("bot:marge", "Marge")] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the colleague is written")
            .written()
            .expect("nothing on this board collides with it");
    }
    memory
        .capture(set_on(NewFact {
            fields: [("reports_to".to_string(), "bot:lisa".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("bot:marge".into()),
                "who marge reports to",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the reporting line is written");

    memory
}

async fn spawn_over_views(
    idp: &support::TestIdp,
    endpoints: IssuerEndpoints,
) -> (SocketAddr, CancellationToken) {
    let board = seeded_board_over(memory_with_views().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], idp, board).await;
    (addr, ct)
}

/// The refusal sentence alone — everything the page says about why the view
/// could not be run, and nothing else it happens to carry.
fn refusal_on(page: &str) -> &str {
    let why = page
        .split_once("This view cannot be run:")
        .expect("the page says the view cannot be run")
        .1
        .split_once("</p>")
        .expect("…and the sentence closes")
        .0;
    // **A sentence whose whole purpose is to be read is asserted on its
    // shape.** A wrapped literal whose continuation collapses serves a run of
    // spaces mid-sentence and passes every check that only asks whether the
    // words are there.
    assert!(
        !why.contains("  "),
        "the refusal reads as one line: {why:?}"
    );
    why
}

/// **A view's page runs the view.**
///
/// A view IS a node, so its page rendered what the node holds — the keys of its
/// own definition — and never the answer. A reader who went looking for a view
/// found a page that reads as empty.
///
/// **Both halves in one read**: the definition stays, because a reader has to
/// see what question they just asked, and the answer arrives beside it with
/// each result linking onward.
#[tokio::test]
async fn a_views_page_runs_the_view_and_shows_the_result() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:loops/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;
    assert!(
        answer.contains("rhythm:descale") && answer.contains("rhythm:worming"),
        "every loop the view selects is in the answer: {answer}"
    );
    assert!(
        answer.contains("href=\"/person:alpha/rhythm:descale/\""),
        "each result links onward to where it lives: {answer}"
    );
    // **The definition stays.** A page that swapped the keys for the answer
    // would leave a reader unable to see which question they asked.
    assert!(
        page.contains("id=\"fields\"") && page.contains("selects"),
        "the view's own definition is still on the page: {page}"
    );
    ct.cancel();
}

/// **A view's page runs the view's own filter, not only its `selects`.**
/// A view narrowed to one loop by a key filter must answer with that loop
/// alone — the record the filter excludes must not be on the page, in the
/// same read as the record it keeps.
#[tokio::test]
async fn a_views_page_runs_the_views_own_filter() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:the-old-loop/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;
    assert!(
        answer.contains("rhythm:descale"),
        "the loop the filter keeps is in the answer: {answer}"
    );
    assert!(
        !answer.contains("rhythm:worming") && !answer.contains("rhythm:abseiling"),
        "the loops the filter excludes must not be in the answer: {answer}"
    );
    ct.cancel();
}

/// **A view's page runs the view's expression and its order.** The page reads
/// `where` and `order` where the verb reads them, so the loop counted from
/// before the cut is not on the page and the other two come latest first, against
/// the handle order they would otherwise take.
#[tokio::test]
async fn a_views_page_runs_the_views_expression_and_order() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:latest-first/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let answer = page
        .split_once("id=\"answer\"")
        .unwrap_or_else(|| panic!("the page carries the view's answer: {page}"))
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;
    assert!(
        !answer.contains("rhythm:descale"),
        "the loop counted from before the cut is not on the page: {answer}"
    );
    let (worming, abseiling) = (
        answer.find("rhythm:worming"),
        answer.find("rhythm:abseiling"),
    );
    assert!(
        worming.is_some() && abseiling.is_some() && worming < abseiling,
        "both later loops are there, latest first: {answer}"
    );
    ct.cancel();
}

/// **Where `needle` stands in `text`, and the page must hold it.** An order
/// asserted between two `find` results compares two `Option`s, and `None` is less
/// than `Some`, so a needle the page does not hold made the order hold. This
/// panics instead, naming the needle.
fn place(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the text: {text}"))
}

#[test]
fn a_needle_the_page_does_not_hold_fails_an_order_check_and_does_not_pass_it() {
    let page = "alpha beta";
    // The comparison `place` replaces: with the first needle absent, `None` is
    // less than `Some`, and the order held.
    assert!(page.find("gamma") < page.find("beta"));
    assert!(
        std::panic::catch_unwind(|| place(page, "gamma")).is_err(),
        "an absent needle panics"
    );
    // Paired: with both present the order is the real one, both ways.
    assert!(place(page, "alpha") < place(page, "beta"));
    assert!(place(page, "beta") > place(page, "alpha"));
}

/// **The rows of the page's answer table**, one string each.
fn answer_rows(page: &str) -> Vec<&str> {
    let answer = page
        .split_once("id=\"answer\"")
        .unwrap_or_else(|| panic!("the page carries the view's answer: {page}"))
        .1;
    answer
        .split_once("</table>")
        .expect("…and it closes")
        .0
        .split("<tr>")
        .skip(1)
        .collect()
}

/// **A view's page answers the question `recall` answers.** The ready view asks
/// which work can start: the page lists the work that can, and not the work that
/// is finished or waits on work that is not. Paired in one read, because a page
/// that listed every piece of work would pass the positives alone.
#[tokio::test]
async fn the_ready_views_page_lists_the_work_that_can_start_and_no_other() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:ready/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let rows = answer_rows(&page);
    let listed = |handle: &str| rows.iter().any(|row| row.contains(&format!("{handle}/")));
    assert!(
        listed("work:delta") && listed("work:phi"),
        "work that can start is listed: {rows:?}"
    );
    assert!(
        !listed("work:epsilon"),
        "finished work is not listed: {rows:?}"
    );
    assert!(
        !listed("work:gamma"),
        "work waiting on unfinished work is not listed: {rows:?}"
    );
    ct.cancel();
}

/// **A view's page shows the keys the view says it shows.** The directory view
/// shows each colleague's one-liner and who they report to; the page carries a
/// column for each and the value in the colleague's own row, not in another's.
#[tokio::test]
async fn the_colleagues_views_page_shows_who_each_bot_reports_to() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:colleagues/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let rows = answer_rows(&page);
    let row_of = |handle: &str| {
        rows.iter()
            .find(|row| row.contains(&format!("{handle}/")))
            .unwrap_or_else(|| panic!("{handle} is a colleague: {rows:?}"))
    };
    assert!(
        rows[0].contains("reports_to"),
        "the header names the key: {rows:?}"
    );
    assert!(
        row_of("bot:marge").contains("bot:lisa"),
        "the row of the bot that reports to another says so: {rows:?}"
    );
    assert!(
        !row_of("bot:lisa").contains("bot:marge"),
        "…and the other row does not: {rows:?}"
    );
    ct.cancel();
}

/// **A view whose question legitimately matches nothing says so**, and says it
/// differently from a view that could not be run at all.
#[tokio::test]
async fn a_view_that_matches_nothing_says_so() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:matches-nothing/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let said = page
        .split_once("This view matches nothing")
        .expect("a question with no answer says so")
        .1
        .split_once("</p>")
        .expect("…and the sentence closes")
        .0;
    assert!(!said.contains("  "), "…and it reads as one line: {said:?}");
    assert!(
        !page.contains("cannot be run"),
        "…and it is not reported as a view that could not be run: {page}"
    );
    ct.cancel();
}

/// **A view short of a key it needs says which key**, rather than rendering as
/// a view whose answer is empty.
///
/// The two are different states with different repairs: one is a question
/// nobody can ask, the other is a question with no answer today.
#[tokio::test]
async fn a_view_short_of_a_key_names_the_key_rather_than_reading_as_empty() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:my-people/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let why = refusal_on(&page);
    assert!(
        why.contains("selects"),
        "the refusal names the key the view lacks: {why}"
    );
    assert!(
        !page.contains("matches nothing"),
        "…and it does not read as a question that simply found none: {page}"
    );
    ct.cancel();
}

/// **A view whose `selects` names no kind cannot be run either**, and the page
/// says what it holds rather than reporting an empty answer.
///
/// It is the same repair as a view short of the key — the value is on the view
/// — and a different sentence, because *there is no such key* and *that key
/// holds something that is no kind* send a reader to different places.
#[tokio::test]
async fn a_view_selecting_something_that_is_no_kind_says_what_it_holds() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:names-no-kind/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // **Scoped to the sentence**, because the view's own definition is on the
    // page too: a substring over the whole page finds `sandwich` in the fields
    // table and calls it an explanation.
    let why = refusal_on(&page);
    assert!(
        why.contains("sandwich"),
        "the refusal names the value the view holds: {why}"
    );
    assert!(
        !page.contains("id=\"answer\""),
        "…and answers nothing: {page}"
    );
    ct.cancel();
}

/// **A view narrows by what it ASKS as well as by what it selects.**
///
/// `asks: overdue` is the one question a selection cannot express, and a page
/// that ran the selection alone would answer with more than the view asked
/// for. Asserted against the view one key away from it: the same kind, and a
/// different answer.
#[tokio::test]
async fn a_view_that_asks_for_what_is_overdue_leaves_out_what_is_not() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:my-week/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;

    assert!(
        answer.contains("rhythm:descale"),
        "the loop that has gone quiet is in the answer: {answer}"
    );
    assert!(
        !answer.contains("rhythm:worming"),
        "…and the loop that is not due is left out, which is what `asks` narrows: {answer}"
    );
    ct.cancel();
}

/// **The overdue view renders oldest due first, not in whatever order the
/// selection happened to find things.**
///
/// `rhythm:abseiling` sorts ahead of `rhythm:descale` alphabetically and is
/// overdue since 2024; `rhythm:descale` has been overdue since 2020. A view
/// that rendered in handle order — or in whatever order the store's own
/// selection returns — would put abseiling first. Ordered by staleness, the
/// more overdue one comes first. The disagreement between the two orderings
/// is what makes finding descale before abseiling mean the sort ran, rather
/// than passing on a page that was never reordered at all.
#[tokio::test]
async fn a_view_that_asks_for_what_is_overdue_renders_oldest_first() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:my-week/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;

    let descale_at = answer
        .find("rhythm:descale")
        .expect("the loop overdue since 2020 is in the answer");
    let abseiling_at = answer
        .find("rhythm:abseiling")
        .expect("the loop overdue since 2024 is in the answer");
    assert!(
        descale_at < abseiling_at,
        "the loop overdue longer must render first, even though its handle sorts after the \
         other's alphabetically: {answer}"
    );
    ct.cancel();
}

/// **The overdue page reads today in the instance's zone, the same fallback the
/// verbs use.** A browser states no timezone, so the page falls back to the
/// instance's, then UTC. Reading UTC always puts the page a day out of step
/// with every verb for the hours the instance's zone is on another day.
///
/// One of the two widest zones is on another day than UTC at any instant. A loop
/// that falls due on the later of the two days is owed in exactly the frame
/// standing on it; a loop that fell due on the earlier day is owed in both, so
/// the page is shown to answer at all. The instance's record holds the zone.
#[tokio::test]
async fn the_overdue_page_reads_today_in_the_instances_zone() {
    let utc = jojobot_domain::clock::Clock::default().today_in(&jiff::tz::TimeZone::UTC);
    let (zone, there) = ["Etc/GMT+12", "Pacific/Kiritimati"]
        .into_iter()
        .map(|zone| {
            let tz = jiff::tz::TimeZone::get(zone).expect("a zone");
            (zone, jojobot_domain::clock::Clock::default().today_in(&tz))
        })
        .find(|(_, there)| *there != utc)
        .expect("one of the widest zones is on another day than UTC");
    let (earlier, later) = (utc.min(there), utc.max(there));
    let a_week_before = |day: Date| {
        day.checked_sub(jiff::Span::new().days(7))
            .expect("a week before a day is a day")
    };

    let memory = memory_with_views().await;
    for (handle, due) in [("rhythm:polish", later), ("rhythm:swim", earlier)] {
        let mut new = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
        new.parent = Some(EntityId("person:alpha".into()));
        memory
            .add_entity(new)
            .await
            .expect("the loop is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(set_on(NewFact {
                fields: [
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "due_date".to_string()),
                    ("counts_from".to_string(), a_week_before(due).to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "the loop was set up",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the schedule is written");
    }
    memory
        .add_entity(NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the operator",
        ))
        .await
        .expect("the instance's record is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("timezone".to_string(), zone.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("topic:instance".into()),
                "where the operator lives",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the zone is written");

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/view:my-week/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;

    assert!(
        answer.contains("rhythm:swim"),
        "a loop due on the earlier day is owed in either frame: {answer}"
    );
    assert_eq!(
        answer.contains("rhythm:polish"),
        there > utc,
        "a loop due on the later day is owed exactly when the instance's day is the later \
         one: the zone is on {there}, UTC on {utc}: {answer}"
    );
    ct.cancel();
}

/// **The overdue page leaves out a project whose status word the instance maps
/// to `done`, and keeps one whose word nobody mapped.** Two projects hold the
/// same past decide-by day. The instance's record maps `wrapped` to `done`, and
/// says nothing about `adrift`, so the first is finished and the second is not.
/// The page reads the instance's record the way the verbs do.
#[tokio::test]
async fn the_overdue_page_reads_the_words_the_instance_maps() {
    let memory = memory_with_views().await;
    for (handle, status) in [("project:atlas", "wrapped"), ("project:visa", "adrift")] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                handle,
                "the fixture roster",
            ))
            .await
            .expect("the project is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: [
                    ("decide_by".to_string(), "2020-01-01".to_string()),
                    ("status".to_string(), status.to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "where it stands",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the status is written");
    }
    memory
        .add_entity(NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the operator",
        ))
        .await
        .expect("the instance's record is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [("status_word/wrapped".to_string(), "done".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("topic:instance".into()),
                "what the operator's words mean",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the mapping is written");
    memory
        .add_entity(NewEntity::new(
            EntityId("view:my-projects".into()),
            "My Projects",
            "the operator",
        ))
        .await
        .expect("the view is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [
                ("selects".to_string(), "project".to_string()),
                ("asks".to_string(), "overdue".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:my-projects".into()),
                "which projects are late",
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

    let page = read(&client, addr, "/view:my-projects/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let answer = page
        .split_once("id=\"answer\"")
        .expect("the page carries the view's answer")
        .1;
    let answer = answer.split_once("</table>").expect("…and it closes").0;

    assert!(
        answer.contains("project:visa"),
        "a word nobody mapped is not finished, so the project is late: {answer}"
    );
    assert!(
        !answer.contains("project:atlas"),
        "a word mapped to done finishes the project: {answer}"
    );
    ct.cancel();
}

/// **A handle that is no view is unaffected.**
#[tokio::test]
async fn a_page_that_is_no_view_carries_no_answer() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/person:alpha/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    assert!(
        page.contains("<h1>Index of /person:alpha/</h1>"),
        "the page renders as it does today: {page}"
    );
    assert!(
        !page.contains("id=\"answer\"") && !page.contains("cannot be run"),
        "…and carries nothing a view page carries: {page}"
    );
    ct.cancel();
}

// --- one engine answers a view for the page and for recall --------------------

/// **The handles an answer table lists, in the order it lists them.** Read off
/// the link text of each row, which is the handle the row stands for.
fn listed_handles(page: &str) -> Vec<String> {
    answer_rows(page)
        .into_iter()
        .skip(1)
        .map(|row| {
            row.split_once("\">")
                .and_then(|(_, rest)| rest.split_once("/</a>"))
                .unwrap_or_else(|| panic!("a row names its handle: {row}"))
                .0
                .to_string()
        })
        .collect()
}

/// **The served `recall` of a view, as a client with no identity makes it**, and
/// the handles it answers with in the order it answers them. Over the server's
/// own port, so the call travels the path a client's does.
async fn recall_of_view(addr: SocketAddr, view: &str) -> Vec<String> {
    let body = recall_body_of_view(addr, view).await;
    body["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("recall answers with objects: {body}"))
        .iter()
        .map(|object| {
            object["id"]
                .as_str()
                .expect("an object has an id")
                .to_string()
        })
        .collect()
}

/// **The whole body the served `recall` of a view answers with**, a refusal
/// included.
async fn recall_body_of_view(addr: SocketAddr, view: &str) -> serde_json::Value {
    recall_body(addr, serde_json::json!({ "view": view })).await
}

/// **The whole body the served `recall` answers with** for these arguments.
async fn recall_body(addr: SocketAddr, arguments: serde_json::Value) -> serde_json::Value {
    let http = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");
    let (session, _) = support::open_with(&http, &url, "2025-06-18").await;
    let mut asking = http
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "recall", "arguments": arguments },
            })
            .to_string(),
        );
    if let Some(session) = session {
        asking = asking.header("mcp-session-id", session);
    }
    let answered = asking.send().await.unwrap().text().await.unwrap();
    support::tool_body(&support::event_stream_result(&answered))
}

/// **The page of a view and a `recall` of it select the same things.** The
/// views are the ones a build ships (`ready`, `colleagues`) and one an operator
/// wrote that holds an expression, an order and a declared type. Each is read
/// from one store, by the page and by the served verb, and the handles must
/// match one for one and in one order.
///
/// **The operator's view names a type that one loop does not answer.** A page
/// that built its own selection and dropped the type lists that loop; the verb
/// does not.
#[tokio::test]
async fn a_views_page_and_a_recall_of_it_select_the_same_things() {
    let memory = memory_with_views().await;
    // A type the loops answer by carrying `flagged`, so the type narrows what
    // `where` alone leaves: the loop below counts from after the cut and
    // carries no such key.
    memory
        .declare_type(
            DeclaredType::new("marked", vec![Field::new("flagged", ValueType::Text)]),
            None,
        )
        .await
        .expect("the type is declared");
    for handle in ["rhythm:worming", "rhythm:abseiling"] {
        memory
            .capture(set_on(NewFact {
                fields: [("flagged".to_string(), "yes".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "set aside",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the key is written");
    }
    let mut unmarked = NewEntity::new(
        EntityId("rhythm:polish".into()),
        "Polish the brass",
        "the fixture roster",
    );
    unmarked.parent = Some(EntityId("person:alpha".into()));
    memory
        .add_entity(unmarked)
        .await
        .expect("the loop is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("counts_from".to_string(), "2300-01-06".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("rhythm:polish".into()),
                "counted from after the cut",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the date is written");
    memory
        .add_entity(NewEntity::new(
            EntityId("view:marked-things".into()),
            "Marked Things",
            "the operator",
        ))
        .await
        .expect("the view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "rhythm".to_string()),
                ("answers_type".to_string(), "marked".to_string()),
                ("where".to_string(), "counts_from > 2021-01-01".to_string()),
                ("order".to_string(), "counts_from descending".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:marked-things".into()),
                "the marked loops counted from after 2021, latest first",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the question is written");

    // Two lists on two parents, for the lists view.
    for (handle, parent) in [
        ("list:packing", "person:alpha"),
        ("list:rotation", "place:shelbyville"),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
        new.parent = Some(EntityId(parent.into()));
        let added = memory.add_entity(new).await.expect("the list is written");
        let said = format!("{added:?}");
        added
            .written()
            .unwrap_or_else(|| panic!("{handle} under {parent} was blocked: {said}"));
    }

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for view in ["ready", "colleagues", "marked-things", "lists"] {
        let page = read(&client, addr, &format!("/view:{view}/"), &cookie)
            .await
            .text()
            .await
            .unwrap();
        let on_the_page = listed_handles(&page);
        let from_recall = recall_of_view(addr, view).await;
        assert!(
            !from_recall.is_empty(),
            "the control: recall of {view} selects something, so agreeing is not both being empty"
        );
        assert_eq!(
            on_the_page, from_recall,
            "the page of {view} lists what recall of it selects"
        );
    }
    // **The lists view takes its parent from the URL as the verb takes it**, and
    // the page and the verb agree on the narrowed answer too.
    let narrowed = read(&client, addr, "/view:lists/?under=person:alpha", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let from_recall = recall_body(
        addr,
        serde_json::json!({"view": "lists", "inputs": {"under": "person:alpha"}}),
    )
    .await["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(from_recall, ["list:packing"]);
    assert_eq!(listed_handles(&narrowed), from_recall, "{narrowed}");
    // The operator's view, spelled out: the expression, the order and the type
    // each took something away, so the three answers above are not one answer.
    assert_eq!(
        recall_of_view(addr, "marked-things").await,
        ["rhythm:worming", "rhythm:abseiling"],
        "the type drops the loop that is not marked, the expression the loop counted \
         from before the cut, and the order puts the later date first"
    );
    ct.cancel();
}

/// **A view that takes inputs renders them as a typed form on its page, prefilled
/// from the URL, and answers with what the form holds** as the served `recall`
/// does: the page and the verb fill a view through one function. **Paired**: the
/// same page with nothing asked lists every piece of ready work, an input of the
/// wrong kind or one nobody declared is refused on the page with the same words,
/// and a left-out optional input is the same as none.
#[tokio::test]
async fn a_views_page_renders_its_inputs_as_a_form_and_answers_with_them() {
    let memory = memory_with_views().await;
    for (handle, name) in [("project:atlas", "Atlas"), ("project:the-move", "The Move")] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the project is written")
            .written()
            .expect("nothing on this board collides with it");
    }
    for (handle, parent) in [
        ("work:omega", "project:atlas"),
        ("work:kappa", "project:atlas"),
        ("work:lambda", "project:the-move"),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
        new.parent = Some(EntityId(parent.into()));
        memory
            .add_entity(new)
            .await
            .expect("the work is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(set_on(NewFact {
                fields: [("status".to_string(), "next".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(EntityId(handle.into()), "to do", Date::constant(2026, 3, 5))
            }))
            .await
            .expect("the status is written");
    }
    memory
        .add_entity(NewEntity::new(
            EntityId("view:my-projects".into()),
            "My projects",
            "the operator",
        ))
        .await
        .expect("the view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "work".to_string()),
                (
                    "inputs".to_string(),
                    "project: kind project, before: date?, size: number?, note: text?".to_string(),
                ),
                ("parent".to_string(), "$project".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:my-projects".into()),
                "the work under one project",
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
    let page = |path: String| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, &path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // ── the ready view: a form with its one input, and the whole answer ─────
    let everything = page("/view:ready/".into()).await;
    assert!(
        everything.contains("<form id=\"inputs\"")
            && everything.contains("name=\"project\" list=\"inputs-project\" value=\"\"")
            && everything.contains("<option value=\"project:atlas\">"),
        "the page offers the ready view's input as a form: {everything}"
    );
    let all_ready = listed_handles(&everything);
    assert_eq!(
        all_ready,
        [
            "work:delta",
            "work:kappa",
            "work:lambda",
            "work:omega",
            "work:phi"
        ],
        "{everything}"
    );

    // ── filled from the URL, it answers what the verb answers ───────────────
    let narrowed = page("/view:ready/?project=project:atlas".into()).await;
    let from_the_page = listed_handles(&narrowed);
    let from_recall = recall_body(
        addr,
        serde_json::json!({"view": "ready", "inputs": {"project": "project:atlas"}}),
    )
    .await["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(from_the_page, ["work:kappa", "work:omega"], "{narrowed}");
    assert_eq!(from_the_page, from_recall, "the page and the verb agree");
    assert!(
        narrowed.contains("name=\"project\" list=\"inputs-project\" value=\"project:atlas\""),
        "the form comes back prefilled from the URL: {narrowed}"
    );

    // …and a left-out optional input is the same as none.
    let blank = page("/view:ready/?project=".into()).await;
    assert_eq!(listed_handles(&blank), all_ready, "{blank}");

    // ── refused on the page in the verb's words, with the form still there ──
    for (path, named) in [("/view:ready/?project=person:alpha", "person:alpha")] {
        let refused = page(path.into()).await;
        assert!(
            refused.contains("This view cannot be run")
                && refused.contains(named)
                && refused.contains("project: kind project?")
                && refused.contains("<form id=\"inputs\""),
            "{path} is refused on the page naming what the view takes: {refused}"
        );
        assert!(
            !refused.contains("<table id=\"answer\""),
            "a refused page lists nothing: {refused}"
        );
    }

    // ── a key the view does not declare is not the view's: it is ignored ─────
    // (a tracking parameter on a link must not turn a page into a refusal)
    for path in [
        "/view:ready/?utm_source=newsletter",
        "/view:ready/?projekt=project:atlas",
    ] {
        let ignored = page(path.into()).await;
        assert_eq!(
            listed_handles(&ignored),
            all_ready,
            "{path} answers as the page does with nothing asked: {ignored}"
        );
        assert!(
            !ignored.contains("This view cannot be run"),
            "{path} is not a refusal: {ignored}"
        );
    }

    // ── each declared type is its own kind of field ─────────────────────────
    let typed = page("/view:my-projects/".into()).await;
    for expected in [
        "name=\"project\" required",
        "name=\"before\"",
        "type=\"date\"",
        "name=\"size\"",
        "type=\"number\"",
        "name=\"note\"",
        "type=\"text\"",
    ] {
        assert!(typed.contains(expected), "{expected} is missing: {typed}");
    }
    assert!(
        typed.contains("Fill in the inputs") && !typed.contains("<table id=\"answer\""),
        "a view with a required input asks for it before it answers: {typed}"
    );
    ct.cancel();
}

/// **A view whose `answers_type` is an input asks for the type the URL names**, and
/// answers what the verb answers for the same type. **Paired**: the loop that does
/// not carry the type's key is left out, a name no type answers to is refused with
/// the form still on the page, and the page asks for the type before it answers.
#[tokio::test]
async fn a_views_page_asks_for_the_type_its_input_names() {
    let memory = memory_with_views().await;
    memory
        .declare_type(
            DeclaredType::new("marked", vec![Field::new("flagged", ValueType::Text)]),
            None,
        )
        .await
        .expect("the type is declared");
    for handle in ["rhythm:worming", "rhythm:abseiling"] {
        memory
            .capture(set_on(NewFact {
                fields: [("flagged".to_string(), "yes".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "set aside",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the key is written");
    }
    memory
        .add_entity(NewEntity::new(
            EntityId("view:my-things-of".into()),
            "My things of a type",
            "the operator",
        ))
        .await
        .expect("the view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "rhythm".to_string()),
                ("inputs".to_string(), "type: text".to_string()),
                ("answers_type".to_string(), "$type".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:my-things-of".into()),
                "the loops that answer a type",
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
    let page = |path: String| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, &path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    let asked = page("/view:my-things-of/".into()).await;
    assert!(
        asked.contains("Fill in the inputs") && !asked.contains("<table id=\"answer\""),
        "the page asks for the type before it answers: {asked}"
    );

    let marked = page("/view:my-things-of/?type=marked".into()).await;
    let mut from_the_page = listed_handles(&marked);
    from_the_page.sort();
    let mut from_recall = recall_body(
        addr,
        serde_json::json!({"view": "my-things-of", "inputs": {"type": "marked"}}),
    )
    .await["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect::<Vec<_>>();
    from_recall.sort();
    assert_eq!(
        from_the_page,
        ["rhythm:abseiling", "rhythm:worming"],
        "{marked}"
    );
    assert_eq!(from_the_page, from_recall, "the page and the verb agree");

    let unknown = page("/view:my-things-of/?type=no-such-type".into()).await;
    let why = refusal_on(&unknown);
    assert!(why.contains("no-such-type"), "{why}");
    assert!(
        unknown.contains("<form id=\"inputs\"") && !unknown.contains("<table id=\"answer\""),
        "a refused page keeps the form and lists nothing: {unknown}"
    );
    ct.cancel();
}

/// **A view runs on its page whenever recall runs it with the same inputs.** An
/// input that is not optional but carries a default is not demanded: the page
/// answers a first visit exactly as the verb answers a call that sends nothing,
/// and the default is used. **Paired**: a required input with no default still
/// asks for a value before it answers, and a value in the URL beats the default.
#[tokio::test]
async fn a_views_page_runs_a_required_input_that_carries_a_default() {
    let memory = memory_with_views().await;
    memory
        .declare_type(
            DeclaredType::new("marked", vec![Field::new("flagged", ValueType::Text)]),
            None,
        )
        .await
        .expect("the type is declared");
    for handle in ["rhythm:worming", "rhythm:abseiling"] {
        memory
            .capture(set_on(NewFact {
                fields: [("flagged".to_string(), "yes".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "set aside",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the key is written");
    }
    for (view, name, inputs) in [
        (
            "view:marked-by-default",
            "Marked by default",
            "type: text = marked",
        ),
        ("view:marked-by-asking", "Marked by asking", "type: text"),
    ] {
        memory
            .add_entity(NewEntity::new(EntityId(view.into()), name, "the operator"))
            .await
            .expect("the view is created")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(set_on(NewFact {
                fields: [
                    ("selects".to_string(), "rhythm".to_string()),
                    ("inputs".to_string(), inputs.to_string()),
                    ("answers_type".to_string(), "$type".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(view.into()),
                    "the loops that answer a type",
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
    let page = |path: String| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, &path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // The verb, sent nothing, uses the default.
    let from_recall = recall_body(
        addr,
        serde_json::json!({"view": "marked-by-default", "inputs": {}}),
    )
    .await["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect::<Vec<_>>();
    let mut from_recall = from_recall;
    from_recall.sort();
    assert_eq!(from_recall, ["rhythm:abseiling", "rhythm:worming"]);

    // The page, visited with nothing in the URL, answers the same.
    let visited = page("/view:marked-by-default/".into()).await;
    let mut from_the_page = listed_handles(&visited);
    from_the_page.sort();
    assert_eq!(
        from_the_page, from_recall,
        "the page and the verb agree: {visited}"
    );
    assert!(
        !visited.contains("Fill in the inputs"),
        "an input with a default is not asked for: {visited}"
    );

    // A value in the URL beats the default.
    let other = page("/view:marked-by-default/?type=no-such-type".into()).await;
    assert!(
        refusal_on(&other).contains("no-such-type"),
        "the URL's value is the one used: {other}"
    );

    // The pair: without a default the page still asks first.
    let asked = page("/view:marked-by-asking/".into()).await;
    assert!(
        asked.contains("Fill in the inputs") && !asked.contains("<table id=\"answer\""),
        "a required input with no default is still asked for: {asked}"
    );
    ct.cancel();
}

/// **A ready view over something that is not work cannot be run on the page**,
/// and the page says why: readiness is a question about work.
#[tokio::test]
async fn a_ready_view_over_something_that_is_not_work_cannot_be_run() {
    // The half-written view the board already holds, given a question.
    let memory = memory_with_views().await;
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "person".to_string()),
                ("asks".to_string(), "ready".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:my-people".into()),
                "who is ready",
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

    let page = read(&client, addr, "/view:my-people/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    let why = refusal_on(&page);
    assert!(
        why.contains("ready") && why.contains("work"),
        "the refusal names the ask and the kind that answers it: {why}"
    );
    assert!(
        !page.contains("id=\"answer\""),
        "…and answers nothing: {page}"
    );
    ct.cancel();
}

/// **A view that holds an EMPTY `answers_type` cannot be run, by the page or by
/// the verb.** An empty value is a stored value and not a removed key: recall
/// resolves it as a type name, finds none and refuses, so the page says the
/// same and lists nothing, where it once listed the whole kind.
#[tokio::test]
async fn a_view_holding_an_empty_type_cannot_be_run_by_the_page_or_the_verb() {
    let memory = memory_with_views().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("view:unscoped".into()),
            "Unscoped",
            "the operator",
        ))
        .await
        .expect("the view is created")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [
                ("selects".to_string(), "work".to_string()),
                ("answers_type".to_string(), String::new()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("view:unscoped".into()),
                "all work, with a type that was left empty",
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

    let refused = recall_body_of_view(addr, "unscoped").await;
    assert_eq!(
        refused["status"], "blocked",
        "the verb refuses a view whose type is empty: {refused}"
    );

    let page = read(&client, addr, "/view:unscoped/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    refusal_on(&page);
    assert!(
        !page.contains("id=\"answer\""),
        "…and the page lists nothing, as the verb does: {page}"
    );
    ct.cancel();
}

/// **The count a page line carries**, taken from its attribute: the structure a
/// reader of the markup and a test both take the number from.
fn left_out_on(page: &str, id: &str) -> Option<u64> {
    let tag = page
        .split_once(&format!("<p id=\"{id}\" data-left-out=\""))?
        .1;
    tag.split_once('"')?.0.parse().ok()
}

/// **A view's page says what it left out, as the served `recall` of the view
/// says it.** The overdue view and the ready view each leave things out. The
/// page carries each count and the call that lists them, taken from the notes
/// `recall` serves, and the two agree: the same count, the same reasons and the
/// same reading instruction.
#[tokio::test]
async fn a_views_page_says_what_it_left_out_as_recall_does() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_over_views(&idp, endpoints).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let week = read(&client, addr, "/view:my-week/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let served = recall_body_of_view(addr, "my-week").await;
    let because = &served["overdue_excluded_because"];
    assert!(
        because["reasons"].is_object(),
        "the fixture leaves things out: {served}"
    );
    assert_eq!(
        left_out_on(&week, "left-out-owed"),
        served["overdue_excluded"].as_u64(),
        "the page carries the count recall serves: {week}"
    );
    let how_to_read = because["how_to_read"].as_str().expect("the call to read");
    assert!(
        week.contains(how_to_read),
        "the page serves recall's reading instruction: {week}"
    );
    for reason in because["reasons"].as_object().expect("reasons").keys() {
        assert!(
            week.contains(reason),
            "{reason} is named on the page: {week}"
        );
    }

    let ready = read(&client, addr, "/view:ready/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let served = recall_body_of_view(addr, "ready").await;
    assert!(
        served["ready_left_out"]["count"].as_u64() > Some(0),
        "the fixture leaves work out: {served}"
    );
    assert_eq!(
        left_out_on(&ready, "left-out-ready"),
        served["ready_left_out"]["count"].as_u64(),
        "the page carries the count recall serves: {ready}"
    );
    let how_to_read = served["ready_left_out"]["how_to_read"]
        .as_str()
        .expect("the call to read");
    assert!(
        ready.contains(how_to_read),
        "the page serves recall's reading instruction: {ready}"
    );

    // An answer with nothing in it still says what the view left out.
    let empty = read(&client, addr, "/view:nothing-owed/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        empty.contains("This view matches nothing") && !empty.contains("cannot be run"),
        "the view answers empty: {empty}"
    );
    assert_eq!(left_out_on(&empty, "left-out-owed"), Some(1), "{empty}");

    // The positive beside the negative: a view that left nothing out says
    // nothing about it.
    let colleagues = read(&client, addr, "/view:colleagues/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !colleagues.contains("data-left-out"),
        "a view that left nothing out has no such line: {colleagues}"
    );
    ct.cancel();
}

// --- the one page style -----------------------------------------------------

/// The pages a logged-in browser can reach, each of a different kind: the index
/// of the roots, a bot's page with its box and runs, a nested node, and the page
/// a missing handle answers with.
const PAGES: [&str; 4] = [
    "/",
    "/bot:otto/",
    "/person:alpha/topic:widgets/",
    "/person:contract-ghost/",
];

#[tokio::test]
async fn every_page_wears_the_one_stylesheet_and_the_shell() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for path in PAGES {
        let page = read(&client, addr, path, &cookie)
            .await
            .text()
            .await
            .unwrap();
        // The positive: the page carries the mockup's palette, once.
        assert_eq!(
            page.matches("<style>").count(),
            1,
            "{path} carries one stylesheet: {page}"
        );
        for colour in ["--ai: #2e4a7a", "--shu: #c8553d"] {
            assert!(page.contains(colour), "{path} lacks {colour}: {page}");
        }
        // The shell: a header bar with the mark, and the content in a main.
        assert!(
            page.contains("<header class=\"bar\"><span class=\"mark\">jojobot</span>"),
            "{path} has no header bar: {page}"
        );
        assert!(page.contains("<main>\n"), "{path} has no main: {page}");
        // The negative the positive depends on: the old page's own style is gone.
        assert!(
            !page.contains("max-width:60rem"),
            "{path} still carries the old inline style: {page}"
        );
    }
    ct.cancel();
}

#[tokio::test]
async fn the_header_walks_the_handle_path_and_the_page_content_is_unchanged_inside_main() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let header = page
        .split_once("<header class=\"bar\">")
        .and_then(|(_, rest)| rest.split_once("</header>"))
        .unwrap_or_else(|| panic!("a header on the page: {page}"))
        .0;
    // Every step up is a link; the page you are on is not.
    assert!(header.contains("<a href=\"/\">/</a>"), "{header}");
    assert!(
        header.contains("<a href=\"/person:alpha/\">person:alpha</a>"),
        "{header}"
    );
    assert!(
        !header.contains("<a href=\"/person:alpha/topic:widgets/\">"),
        "the current page is not a link to itself: {header}"
    );
    assert!(header.contains("topic:widgets"), "{header}");
    // The content is what the page showed before the shell, byte for byte at
    // its edges: its heading opens the main and its last table closes it.
    let main = page
        .split_once("<main>\n")
        .and_then(|(_, rest)| rest.split_once("</main>"))
        .unwrap_or_else(|| panic!("a main on the page: {page}"))
        .0;
    assert!(
        main.starts_with("<h1>Index of /person:alpha/topic:widgets/</h1>\n"),
        "{main}"
    );
    ct.cancel();
}

/// **The keys an answer table shows and the cells under them**, by the handle each
/// row stands for. Read off the header's key columns (after Name and Called) and
/// the cells of each row, so a page that drops a field leaves a cell empty here.
fn answer_cells(page: &str) -> (Vec<String>, Vec<(String, Vec<String>)>) {
    let answer = page
        .split_once("id=\"answer\"")
        .unwrap_or_else(|| panic!("the page carries the view's answer: {page}"))
        .1
        .split_once("</table>")
        .expect("…and it closes")
        .0;
    let cells = |row: &str, open: &str, close: &str| -> Vec<String> {
        row.split(open)
            .skip(1)
            .map(|cell| {
                cell.split_once(close)
                    .map_or(cell, |(text, _)| text)
                    .to_string()
            })
            .collect()
    };
    let mut rows = answer.split("<tr>").skip(1);
    let header = cells(rows.next().expect("a header row"), "<th>", "</th>");
    let keys = header[2..].to_vec();
    let body = rows
        .map(|row| {
            let cells = cells(row, "<td>", "</td>");
            let handle = cells[0]
                .split_once("\">")
                .and_then(|(_, rest)| rest.split_once("/</a>"))
                .unwrap_or_else(|| panic!("a row names its handle: {row}"))
                .0
                .to_string();
            (handle, cells[2..].to_vec())
        })
        .collect();
    (keys, body)
}

/// **A view's page shows the fields the view shows, and `recall` of the view
/// serves the same values.** The board view shows `status`, `depends_on` and
/// `owner`: each row of the page holds, under each of those, what the verb answers
/// for that thing, and an empty cell where it holds none. **Paired in one read**:
/// the page is not empty, a row with a value in every column is among them, and a
/// row with none is too.
#[tokio::test]
async fn a_views_page_shows_the_fields_the_view_shows_as_the_verb_serves_them() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_board().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = read(
        &client,
        addr,
        "/view:board/?project=project%3Aatlas",
        &cookie,
    )
    .await
    .text()
    .await
    .unwrap();
    let (keys, rows) = answer_cells(&page);
    assert_eq!(keys, ["status", "depends_on", "owner"], "{page}");
    assert!(
        rows.len() >= 3,
        "the board lists the project's work: {page}"
    );

    let served = recall_body(
        addr,
        serde_json::json!({"view": "board", "inputs": {"project": "project:atlas"}}),
    )
    .await;
    let objects = served["objects"].as_array().expect("objects");
    assert_eq!(rows.len(), objects.len(), "the same things: {page}");
    for (handle, cells) in &rows {
        let object = objects
            .iter()
            .find(|object| object["id"].as_str() == Some(handle))
            .unwrap_or_else(|| panic!("{handle} is in the verb's answer: {served}"));
        for (key, cell) in keys.iter().zip(cells) {
            let held = object["fields"][key].as_str().unwrap_or("");
            assert_eq!(
                cell, held,
                "{handle} under {key}: the page holds {cell:?} and the verb {held:?}"
            );
        }
    }
    let full = rows
        .iter()
        .filter(|(_, cells)| cells.iter().filter(|cell| !cell.is_empty()).count() >= 2)
        .count();
    let bare = rows
        .iter()
        .filter(|(_, cells)| cells.iter().skip(1).all(String::is_empty))
        .count();
    assert!(full >= 1 && bare >= 1, "rows of both shapes: {rows:?}");
    ct.cancel();
}

/// **A project's page links to the ready view narrowed to that project**, and
/// the page that link opens lists what `recall` of the view with that input
/// lists. **Paired**: the other project's link opens its own work and not this
/// one's, and a page that is not a project's carries no such link.
#[tokio::test]
async fn a_projects_page_links_to_the_ready_work_of_that_project() {
    let memory = memory_with_views().await;
    for (handle, name) in [("project:atlas", "Atlas"), ("project:the-move", "The Move")] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the project is written")
            .written()
            .expect("nothing on this board collides with it");
    }
    for (handle, parent) in [
        ("work:omega", "project:atlas"),
        ("work:kappa", "project:atlas"),
        ("work:lambda", "project:the-move"),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
        new.parent = Some(EntityId(parent.into()));
        memory
            .add_entity(new)
            .await
            .expect("the work is written")
            .written()
            .expect("nothing on this board collides with it");
        memory
            .capture(set_on(NewFact {
                fields: [("status".to_string(), "next".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(EntityId(handle.into()), "to do", Date::constant(2026, 3, 5))
            }))
            .await
            .expect("the status is written");
    }
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: String| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, &path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // ── the project's page carries the link, and the link opens its work ────
    let atlas = page("/project:atlas/".into()).await;
    let link = "/view:ready/?project=project%3Aatlas";
    assert!(
        atlas.contains(&format!("<a href=\"{link}\">")),
        "the project's page links to the ready view narrowed to it: {atlas}"
    );
    let opened = page(link.into()).await;
    let from_the_page = listed_handles(&opened);
    let from_recall = recall_body(
        addr,
        serde_json::json!({"view": "ready", "inputs": {"project": "project:atlas"}}),
    )
    .await["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(from_the_page, ["work:kappa", "work:omega"], "{opened}");
    assert_eq!(from_the_page, from_recall, "the page and the verb agree");

    // ── the other project's link is its own ──────────────────────────────────
    let the_move = page("/project:the-move/".into()).await;
    assert!(
        the_move.contains("<a href=\"/view:ready/?project=project%3Athe-move\">")
            && !the_move.contains(link),
        "each project links to itself: {the_move}"
    );
    assert_eq!(
        listed_handles(&page("/view:ready/?project=project:the-move".into()).await),
        ["work:lambda"]
    );

    // ── a page that is not a project's has no such link ──────────────────────
    let work = page("/project:atlas/work:omega/".into()).await;
    assert!(
        !work.contains("/view:ready/"),
        "a piece of work is not a project: {work}"
    );
    ct.cancel();
}

// --- a bot's place in the org chart ----------------------------------------

/// The board with the reporting lines the org cases read, all built here: a line
/// three deep (upsilon reports to psi, which reports to rho), a line beside it
/// (nelson reports to smithers), and a bot nobody reports to or through (moe).
async fn memory_with_a_reporting_line() -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    for (handle, name, boss) in [
        ("bot:rho", "The chief", None),
        ("bot:psi", "The deputy", Some("bot:rho")),
        ("bot:upsilon", "The intern", Some("bot:psi")),
        ("bot:smithers", "The aide", None),
        ("bot:nelson", "The messenger", Some("bot:smithers")),
        ("bot:moe", "The loner", None),
    ] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the bot is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        if let Some(boss) = boss {
            memory
                .capture(set_on(NewFact {
                    fields: [("reports_to".to_string(), boss.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(
                        EntityId(handle.into()),
                        "who this bot reports to",
                        Date::constant(2026, 3, 5),
                    )
                }))
                .await
                .expect("the reporting line is written");
        }
    }
    memory
}

/// The part of a page between the element with this id and its end tag.
fn element<'a>(page: &'a str, id: &str, tag: &str) -> &'a str {
    let anchor = format!("id=\"{id}\"");
    let start = page
        .find(&anchor)
        .unwrap_or_else(|| panic!("no element {id} on this page: {page}"));
    let rest = &page[start..];
    rest.split_once(&format!("</{tag}>"))
        .unwrap_or_else(|| panic!("element {id} never closes: {page}"))
        .0
}

#[tokio::test]
async fn a_bots_page_shows_the_chain_it_reports_through_and_who_reports_to_it() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_reporting_line().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: &'static str| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // ── upsilon: the chain from the head down to it, each step a link ─────────
    let upsilon = page("/bot:upsilon/").await;
    let chain = element(&upsilon, "chain", "p");
    let at = |handle: &str| {
        chain
            .find(&format!(">{handle}</a>"))
            .unwrap_or_else(|| panic!("{handle} is a step of upsilon's chain: {chain}"))
    };
    assert!(
        at("bot:rho") < at("bot:psi"),
        "the head comes first: {chain}"
    );
    assert!(
        chain.contains("href=\"/bot:rho/\"") && chain.contains("href=\"/bot:psi/\""),
        "{chain}"
    );
    // The page's own bot closes the chain and is not a link to itself.
    assert!(
        chain.trim_end().ends_with("bot:upsilon"),
        "the chain ends on the bot: {chain}"
    );
    assert!(
        !chain.contains("bot:nelson") && !chain.contains("bot:smithers"),
        "another line of reports is not upsilon's chain: {chain}"
    );

    // ── psi: upsilon is its report, and its own manager is not ──────────────
    let psi = page("/bot:psi/").await;
    let reports = element(&psi, "reports", "ul");
    assert!(
        reports.contains("href=\"/bot:upsilon/\"") && !reports.contains("bot:rho"),
        "{reports}"
    );

    // ── rho has no manager: a root, not an error ───────────────────────────
    let rho = page("/bot:rho/").await;
    let chain = element(&rho, "chain", "p");
    assert!(
        chain.trim_end().ends_with("bot:rho") && !chain.contains("href=\"/bot:"),
        "a bot with no manager is the whole of its own chain: {chain}"
    );
    assert!(
        !rho.contains("cannot be run") && element(&rho, "reports", "ul").contains("bot:psi"),
        "{rho}"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_org_chart_page_folds_every_reporting_line_into_one_tree() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_reporting_line().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();

    // ── closed to a browser with no session, like every page ─────────────────
    let closed = client
        .get(format!("http://{addr}/ui/org/"))
        .send()
        .await
        .unwrap();
    assert!(closed.status().is_redirection(), "{}", closed.status());

    let cookie = log_in(&client, addr, "/").await;
    let chart = read(&client, addr, "/ui/org/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    // Each bot is one line, deeper for each manager above it.
    let depth_of = |handle: &str| -> usize {
        let line = chart
            .lines()
            .find(|line| line.contains(&format!("href=\"/{handle}/\"")))
            .unwrap_or_else(|| panic!("{handle} is on the chart: {chart}"));
        let marker = "data-depth=\"";
        let from = line.find(marker).expect("a line carries its depth") + marker.len();
        line[from..]
            .split('"')
            .next()
            .and_then(|depth| depth.parse().ok())
            .expect("a depth is a number")
    };
    assert_eq!(depth_of("bot:rho"), 0, "{chart}");
    assert_eq!(depth_of("bot:psi"), 1, "{chart}");
    assert_eq!(depth_of("bot:upsilon"), 2, "{chart}");
    // The other line of reports sits beside it, and a bot nobody reports to or
    // through is a root as well. All of them are built by this case's fixture.
    assert_eq!(depth_of("bot:smithers"), 0, "{chart}");
    assert_eq!(depth_of("bot:nelson"), 1, "{chart}");
    assert_eq!(depth_of("bot:moe"), 0, "{chart}");
    // A child follows its manager and not another's: upsilon is after psi and
    // before the next root.
    let order = |handle: &str| chart.find(&format!("href=\"/{handle}/\"")).unwrap();
    assert!(order("bot:psi") < order("bot:upsilon"), "{chart}");
    // The chart is in a box of the grid, and a bot's page links to it.
    assert!(
        chart.contains("<section class=\"box\"><h2>Org chart"),
        "{chart}"
    );
    let upsilon = read(&client, addr, "/bot:upsilon/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(upsilon.contains("<a href=\"/ui/org/\">"), "{upsilon}");
    // A path no entity answers to is still missing, chart or no chart.
    let missing = read(&client, addr, "/ui/nothing/", &cookie).await;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    ct.cancel();
}

// --- the homepage ----------------------------------------------------------

/// The board for the homepage: topics written one after another so their last
/// writes have an order, a topic archived after the newest write, two roots
/// written last in a known order, and the rhythms and views the build supplies.
async fn memory_for_the_homepage() -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    let day = Date::constant(2026, 3, 5);
    for (handle, name) in [
        ("topic:stunt-rehearsals", "Rehearsals"),
        ("topic:stunt-schedule", "Schedule"),
        ("topic:the-five-words", "Five words"),
        ("topic:all-rules", "All of the rules"),
        ("topic:upgrade-fixture-ordinances", "Ordinances"),
    ] {
        memory
            .add_entity(NewEntity::new(
                EntityId(handle.into()),
                name,
                "the fixture roster",
            ))
            .await
            .expect("the topic is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        memory
            .capture(NewFact::about(EntityId(handle.into()), "a note", day))
            .await
            .expect("the note is written");
    }
    // `topic:stunt-rehearsals`, the topic written FIRST, is written once more
    // after the rest: a thing's last write is its newest claim, not its first.
    memory
        .capture(NewFact::about(
            EntityId("topic:stunt-rehearsals".into()),
            "a second note",
            day,
        ))
        .await
        .expect("the second note is written");
    memory
        .archive_entity(
            &EntityId("topic:upgrade-fixture-ordinances".into()),
            "a mistaken write",
            None,
        )
        .await
        .expect("archive_entity ok");
    // Two roots, written last: the place after the person.
    for handle in ["person:alpha", "place:shelbyville"] {
        memory
            .capture(NewFact::about(EntityId(handle.into()), "a late note", day))
            .await
            .expect("the note is written");
    }
    memory
}

#[tokio::test]
async fn the_homepage_has_a_box_for_each_kind_that_has_members_and_counts_them_truly() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_for_the_homepage().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let home = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // ── topics: five stand (widgets and four more), the archived one does not ─
    let topics = element(&home, "kind-topic", "section");
    assert!(topics.contains("<span class=\"n\">5</span>"), "{topics}");
    let at = |handle: &str| topics.find(&format!("href=\"/{handle}/\""));
    // The three written last, the newest first: the rehearsals topic was
    // created first and written again last, so it leads.
    let (rehearsals, rules, words) = (
        at("topic:stunt-rehearsals").expect("the topic written last is listed"),
        at("topic:all-rules").expect("the next is listed"),
        at("topic:the-five-words").expect("the third is listed"),
    );
    assert!(
        rehearsals < rules && rules < words,
        "newest first: {topics}"
    );
    assert!(
        at("topic:stunt-schedule").is_none() && !topics.contains(">Widgets</a>"),
        "only the three written last: {topics}"
    );
    assert!(
        at("topic:upgrade-fixture-ordinances").is_none()
            && !home.contains("topic:upgrade-fixture-ordinances"),
        "an archived thing is in no box: {home}"
    );
    // The link to all of them.
    assert!(topics.contains("href=\"/ui/kind/topic/\""), "{topics}");

    // ── a kind with no members has no box, and one with members has ──────────
    assert!(!home.contains("id=\"kind-pet\""), "{home}");
    assert!(home.contains("id=\"kind-person\""), "{home}");
    // ── the org chart is linked from the bots box and from no other ──────────
    let bots = element(&home, "kind-bot", "section");
    assert!(bots.contains("href=\"/ui/org/\""), "{bots}");
    assert!(!topics.contains("/ui/org/"), "{topics}");
    ct.cancel();
}

#[tokio::test]
async fn the_homepage_lists_every_top_level_thing_and_sorts_by_name_or_last_write() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let memory = memory_for_the_homepage().await;
    let children_of_alpha = memory
        .children(&EntityId("person:alpha".into()))
        .await
        .expect("children")
        .len();
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: &'static str| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };
    let by_name = page("/").await;
    let roots = element(&by_name, "roots", "table");
    // path · kind · inside · last write, one row to a top-level thing.
    for head in ["path", "kind", "inside", "last write"] {
        assert!(roots.contains(&format!("<th>{head}</th>")), "{roots}");
    }
    let row = |handle: &str| -> String {
        roots
            .split("<tr>")
            .find(|row| row.contains(&format!("href=\"/{handle}/\"")))
            .unwrap_or_else(|| panic!("{handle} is a top-level thing: {roots}"))
            .to_string()
    };
    let alpha = row("person:alpha");
    assert!(
        alpha.contains(&format!("<td class=\"n\">{children_of_alpha}</td>"))
            && alpha.contains("<td class=\"k\">person</td>"),
        "a row says its kind and how many things sit inside: {alpha}"
    );
    assert!(row("place:shelbyville").contains("<td class=\"n\">—</td>"));
    assert!(
        !roots.contains("href=\"/topic:widgets/\""),
        "a child is not top level: {roots}"
    );
    // Default order is by handle, and sorting by last write puts the thing
    // written last first: the same page, two orders.
    let position = |page: &str, handle: &str| {
        element(page, "roots", "table")
            .find(&format!("href=\"/{handle}/\""))
            .unwrap_or_else(|| panic!("{handle} is listed"))
    };
    assert!(position(&by_name, "person:alpha") < position(&by_name, "place:shelbyville"));
    let sorted_by_name = page("/?sort=name").await;
    assert!(
        position(&sorted_by_name, "person:alpha") < position(&sorted_by_name, "place:shelbyville")
    );
    let by_write = page("/?sort=written").await;
    assert!(
        position(&by_write, "place:shelbyville") < position(&by_write, "person:alpha"),
        "the thing written last comes first: {by_write}"
    );
    // The two sorts are links on the listing's heading.
    assert!(
        by_name.contains("href=\"/?sort=name\"") && by_name.contains("href=\"/?sort=written\""),
        "{by_name}"
    );
    ct.cancel();
}

#[tokio::test]
async fn the_homepage_has_a_due_box_from_the_due_view_and_a_kind_has_a_page_of_its_own() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_for_the_homepage().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let home = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let due = element(&home, "due", "section");
    // The loop that went quiet long ago is due; the one counted from 2400 is not.
    assert!(
        due.contains("href=\"/person:alpha/rhythm:descale/\""),
        "{due}"
    );
    assert!(
        due.contains(">Descale the machine</a>") && !due.contains("Worm the dog"),
        "{due}"
    );

    // The page of a kind lists every thing of it that stands, in the same shell.
    let topics = read(&client, addr, "/ui/kind/topic/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let table = element(&topics, "things", "table");
    for stands in [
        "person:alpha/topic:widgets",
        "topic:stunt-rehearsals",
        "topic:stunt-schedule",
        "topic:the-five-words",
        "topic:all-rules",
    ] {
        assert!(table.contains(&format!("href=\"/{stands}/\"")), "{table}");
    }
    assert!(
        !table.contains("topic:upgrade-fixture-ordinances"),
        "{table}"
    );
    assert!(topics.contains("<header class=\"bar\">"), "{topics}");
    // A kind nothing holds has no page, rather than an empty one.
    let none = read(&client, addr, "/ui/kind/pet/", &cookie).await;
    assert_eq!(none.status(), reqwest::StatusCode::NOT_FOUND);
    ct.cancel();
}

#[tokio::test]
async fn a_last_write_is_the_day_it_fell_on_in_the_instances_zone() {
    // The widest zones are on another day than UTC for part of every day; the one
    // that is now is the instance's, so a write made now falls on one day there
    // and another in UTC.
    let clock = jojobot_domain::clock::Clock::default();
    let utc = clock.today_in(&jiff::tz::TimeZone::UTC);
    let (zone, there) = ["Etc/GMT+12", "Pacific/Kiritimati"]
        .into_iter()
        .map(|zone| {
            let tz = jiff::tz::TimeZone::get(zone).expect("a zone");
            (zone, clock.today_in(&tz))
        })
        .find(|(_, there)| *there != utc)
        .expect("one of the widest zones is on another day than UTC");

    let memory = memory_for_the_homepage().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the operator",
        ))
        .await
        .expect("the instance's record is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [("timezone".to_string(), zone.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("topic:instance".into()),
                "where the operator lives",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the zone is written");

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let home = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // The thing written last, in the box and in the listing: the day in the
    // instance's zone, and not the day in UTC.
    let people = element(&home, "kind-person", "section");
    let alpha = element(&home, "roots", "table")
        .split("<tr>")
        .find(|row| row.contains("href=\"/person:alpha/\""))
        .expect("person:alpha is a root")
        .to_string();
    let kind_page = read(&client, addr, "/ui/kind/person/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let kind_row = element(&kind_page, "things", "table");
    for (what, cell) in [
        ("the box", people),
        ("the listing", alpha.as_str()),
        ("the page of a kind", kind_row),
    ] {
        assert!(
            cell.contains(&format!(">{there}<")),
            "{what} shows the day in the instance's zone ({zone}, {there}): {cell}"
        );
        assert!(
            !cell.contains(&format!(">{utc}<")),
            "{what} does not show the UTC day ({utc}): {cell}"
        );
    }
    ct.cancel();
}

// --- built-in kinds look like what they are ---------------------------------

/// A project whose work stands in every status word, a word its own list adds
/// and a word nobody listed.
async fn memory_with_a_board() -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("project:atlas".into()),
            "Atlas",
            "the fixture roster",
        ))
        .await
        .expect("the project is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(set_on(NewFact {
            fields: [(
                "columns".to_string(),
                "someday, next, now, waiting, done, considering, review".to_string(),
            )]
            .into_iter()
            .collect(),
            ..NewFact::about(
                EntityId("project:atlas".into()),
                "the columns this project lists",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the columns are written");
    // The piece of work moving to "now" was "next" before: two writes of one key.
    let mut states = vec![("work:omicron", vec![("status", "next")])];
    for (handle, name, fields) in [
        ("work:sigma", "Draft the plans", vec![("status", "someday")]),
        (
            "work:tau",
            "Order glazing",
            vec![("status", "next"), ("depends_on", "work:omicron")],
        ),
        (
            "work:omicron",
            "Raise the frame",
            vec![("status", "now"), ("owner", "bot:marge")],
        ),
        (
            "work:theta",
            "Await the permit",
            vec![("status", "waiting")],
        ),
        ("work:iota", "Pour footings", vec![("status", "done")]),
        (
            "work:kappa",
            "Compare builders",
            vec![("status", "considering")],
        ),
        (
            "work:frame-the-wall",
            "Park the crane",
            vec![("status", "parked")],
        ),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = Some(EntityId("project:atlas".into()));
        memory
            .add_entity(new)
            .await
            .expect("the work is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        states.push((handle, fields));
    }
    // **The state is written after every piece of work exists**, because a handle
    // in a value must name something, and a write that is blocked is not an error.
    for (handle, fields) in states {
        memory
            .capture(set_on(NewFact {
                fields: fields
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "where the work stands",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the work's state is written")
            .written()
            .unwrap_or_else(|| panic!("the state of {handle} was refused"));
    }
    memory
}

#[tokio::test]
async fn a_projects_page_is_a_board_with_a_column_for_each_status_word() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_board().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let project = read(&client, addr, "/project:atlas/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let board = element(&project, "board", "section");

    // The column of a status word: its cards, and no other column's.
    let column = |word: &str| -> &str {
        let anchor = format!("data-status=\"{word}\"");
        let start = board
            .find(&anchor)
            .unwrap_or_else(|| panic!("a column for {word}: {board}"));
        let rest = &board[start + 1..];
        rest.split("data-status=").next().unwrap_or(rest)
    };
    // The five shipped words, then the project's own, then a word nobody listed,
    // in that order.
    let order: Vec<usize> = [
        "someday",
        "next",
        "now",
        "waiting",
        "done",
        "considering",
        "review",
        "parked",
    ]
    .iter()
    .map(|word| {
        board
            .find(&format!("data-status=\"{word}\""))
            .unwrap_or_else(|| panic!("a column for {word}: {board}"))
    })
    .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{board}");
    // Each piece of work stands in its own column.
    for (word, name) in [
        ("someday", "Draft the plans"),
        ("next", "Order glazing"),
        ("now", "Raise the frame"),
        ("waiting", "Await the permit"),
        ("done", "Pour footings"),
        ("considering", "Compare builders"),
        ("parked", "Park the crane"),
    ] {
        assert!(
            column(word).contains(name),
            "{name} is under {word}: {board}"
        );
    }
    // …and not in another: the done column holds the done work only.
    assert!(!column("done").contains("Raise the frame"), "{board}");
    assert!(!column("now").contains("Pour footings"), "{board}");
    // A column the project lists and nothing stands in is still a column.
    assert!(!column("review").contains("class=\"card"), "{board}");
    // A card says what it needs and who owns it.
    let now = column("now");
    assert!(
        now.contains("Raise the frame") && now.contains("bot:marge"),
        "{now}"
    );
    let next = column("next");
    assert!(
        next.contains("href=\"/project:atlas/work:omicron/\""),
        "the card of work that needs other work links to it: {next}"
    );
    // The generic page is still below the board.
    assert!(project.contains("<h2>Below here</h2>"), "{project}");
    ct.cancel();
}

/// **A project over the pieces of work given, each holding its status word**, under an
/// instance that maps `shipped` to `done`.
async fn memory_with_a_project_of(project: &str, work: &[(&str, &str)]) -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the operator",
        ))
        .await
        .expect("the instance's record is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [("status_word/shipped".to_string(), "done".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("topic:instance".into()),
                "what the operator's words mean",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the mapping is written");
    memory
        .add_entity(NewEntity::new(
            EntityId(project.into()),
            "The plant",
            "the fixture roster",
        ))
        .await
        .expect("the project is written")
        .written()
        .expect("nothing on this board collides with it");
    for (handle, word) in work {
        let mut new = NewEntity::new(EntityId((*handle).into()), *handle, "the fixture roster");
        new.parent = Some(EntityId(project.into()));
        memory
            .add_entity(new)
            .await
            .expect("the work is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        memory
            .capture(set_on(NewFact {
                fields: [("status".to_string(), (*word).to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    EntityId((*handle).into()),
                    "where the work stands",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the work's state is written")
            .written()
            .unwrap_or_else(|| panic!("the state of {handle} was refused"));
    }
    memory
}

/// **The board counts finished work through the one reader recall uses.** Three
/// pieces of work hold `shipped`, a word the instance maps to `done`: the board
/// styles them done and its header counts three. Paired: work that holds `done`
/// still counts, and work in another status does not.
#[tokio::test]
async fn a_board_counts_work_in_a_word_the_instance_maps_to_done() {
    for (project, work, finished, label) in [
        (
            "project:atlas",
            vec![
                ("work:sigma", "shipped"),
                ("work:tau", "shipped"),
                ("work:theta", "shipped"),
                ("work:iota", "now"),
            ],
            3,
            "3 done",
        ),
        (
            "project:atlas-two",
            vec![("work:kappa", "done"), ("work:omicron", "next")],
            1,
            "1 done",
        ),
    ] {
        let idp = support::TestIdp::new();
        let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
        let board = seeded_board_over(memory_with_a_project_of(project, &work).await).await;
        let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
        let client = browser();
        let cookie = log_in(&client, addr, "/").await;
        let page = read(&client, addr, &format!("/{project}/"), &cookie)
            .await
            .text()
            .await
            .unwrap();
        let board = element(&page, "board", "section");
        assert_eq!(
            board.matches("class=\"card done\"").count(),
            finished,
            "{project}: cards styled done: {board}"
        );
        assert!(
            board.contains(label),
            "{project}: the header says {label}: {board}"
        );
        ct.cancel();
    }
}

#[tokio::test]
async fn a_pieces_of_work_page_leads_with_its_status_needs_owner_and_history() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_board().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: &'static str| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // The work that is under way, owned, and needed by other work.
    let omicron = page("/project:atlas/work:omicron/").await;
    let work = element(&omicron, "work", "section");
    assert!(work.contains("<dd>now</dd>"), "{work}");
    assert!(
        work.contains("href=\"/bot:marge/\""),
        "the owner is a link: {work}"
    );
    // Its status was "next" before it was "now": the newest write leads.
    let history = element(&omicron, "status-history", "ul");
    let (now, next) = (
        history.find(">now<").expect("the write that holds it now"),
        history.find(">next<").expect("the write before it"),
    );
    assert!(now < next, "newest first: {history}");

    // The work that needs it says so, as a link, and has no owner to name.
    let tau = page("/project:atlas/work:tau/").await;
    let work = element(&tau, "work", "section");
    assert!(
        work.contains("href=\"/project:atlas/work:omicron/\""),
        "what it needs is a link: {work}"
    );
    assert!(work.contains("<dd>next</dd>"), "{work}");
    // The generic page follows, and a page of another kind carries no such box.
    assert!(tau.contains("<h2>Below here</h2>"), "{tau}");
    assert!(!page("/project:atlas/").await.contains("id=\"work\""));
    ct.cancel();
}

/// Two loops under a person: one that fell due three days before today and one
/// that falls due long after.
async fn memory_with_two_loops() -> (Arc<dyn Memory>, Date, Date) {
    // **A day the case states, not the machine's**: the page reads today off the
    // server's clock, which the case sets to this day, so "3 days overdue" is
    // three days from a fixed day and not from whichever day the suite runs.
    let today = Date::constant(2026, 4, 20);
    let days = |n: i32| {
        today
            .checked_add(jiff::Span::new().days(n))
            .expect("a day near today")
    };
    let memory = memory_with_views().await;
    for (handle, name, counts_from) in [
        ("rhythm:polish", "Polish the silver", days(-10)),
        ("rhythm:swim", "Swim a length", days(3650)),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = Some(EntityId("person:alpha".into()));
        memory
            .add_entity(new)
            .await
            .expect("the loop is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        memory
            .capture(set_on(NewFact {
                fields: [
                    ("name".to_string(), name.to_string()),
                    ("last_check_in".to_string(), days(-20).to_string()),
                    ("cadence_days".to_string(), "7".to_string()),
                    ("advances_from".to_string(), "due_date".to_string()),
                    ("counts_from".to_string(), counts_from.to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "the loop was set up",
                    Date::constant(2026, 3, 5),
                )
            }))
            .await
            .expect("the schedule is written")
            .written()
            .unwrap_or_else(|| panic!("the schedule of {handle} was refused"));
    }
    (memory, days(-3), today)
}

#[tokio::test]
async fn a_rhythms_page_says_its_cadence_last_check_in_next_due_and_how_late() {
    let (memory, fell_due, today) = memory_with_two_loops().await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_on(endpoints, &[READER], &idp, board, today).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: &'static str| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    // The loop that fell due three days ago says so, with the day it fell due.
    let late = page("/person:alpha/rhythm:polish/").await;
    let rhythm = element(&late, "rhythm", "section");
    assert!(rhythm.contains("every 7 days"), "{rhythm}");
    assert!(
        rhythm.contains(&fell_due.to_string()),
        "the day it fell due: {rhythm}"
    );
    assert!(rhythm.contains("3 days overdue"), "{rhythm}");
    assert!(rhythm.contains("last check-in"), "{rhythm}");

    // The loop that falls due far ahead has a next due day and is not late.
    let ahead = page("/person:alpha/rhythm:swim/").await;
    let rhythm = element(&ahead, "rhythm", "section");
    assert!(rhythm.contains("next due"), "{rhythm}");
    assert!(!rhythm.contains("overdue"), "{rhythm}");
    // A page of another kind has no such box.
    assert!(!page("/person:alpha/").await.contains("id=\"rhythm\""));
    ct.cancel();
}

/// **A loop at the person's page, with the keys given.**
async fn memory_with_a_loop(handle: &str, fields: &[(&str, &str)]) -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    let mut new = NewEntity::new(EntityId(handle.into()), handle, "the fixture roster");
    new.parent = Some(EntityId("person:alpha".into()));
    memory
        .add_entity(new)
        .await
        .expect("the loop is written")
        .written()
        .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
    memory
        .capture(set_on(NewFact {
            fields: fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            ..NewFact::about(
                EntityId(handle.into()),
                "the loop was set up",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the schedule is written")
        .written()
        .unwrap_or_else(|| panic!("the schedule of {handle} was refused"));
    memory
}

/// **A rhythm's cadence is read the way the domain reads it.** A calendar rule
/// shows in words, and a loop with no cadence shows a dash. Paired with the page
/// above, where a number of days shows as every N days.
#[tokio::test]
async fn a_rhythms_page_says_a_calendar_rule_in_words_and_a_loop_with_none_in_a_dash() {
    for (handle, fields, said, not_said) in [
        (
            "rhythm:pay-rent",
            vec![
                ("name", "Pay the rent"),
                ("last_check_in", "2026-03-01"),
                ("rrule", "FREQ=MONTHLY;BYMONTHDAY=1"),
                ("counts_from", "2026-03-01"),
            ],
            "<dt>cadence</dt><dd>on day 1 of every month</dd>",
            "FREQ=",
        ),
        (
            "rhythm:sand",
            vec![("name", "Sand the table"), ("last_check_in", "2026-03-01")],
            "<dt>cadence</dt><dd>—</dd>",
            "every",
        ),
    ] {
        let idp = support::TestIdp::new();
        let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
        let board = seeded_board_over(memory_with_a_loop(handle, &fields).await).await;
        let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
        let client = browser();
        let cookie = log_in(&client, addr, "/").await;
        let page = read(&client, addr, &format!("/person:alpha/{handle}/"), &cookie)
            .await
            .text()
            .await
            .unwrap();
        let rhythm = element(&page, "rhythm", "section");
        assert!(rhythm.contains(said), "{handle}: {rhythm}");
        assert!(!rhythm.contains(not_said), "{handle}: {rhythm}");
        ct.cancel();
    }
}

/// The kitchen thread: two standing decisions, one decision replaced and
/// archived, and three findings on the thread itself, which is what a thread
/// holds.
async fn memory_with_a_kitchen_thread() -> Arc<dyn Memory> {
    let memory = memory_with_views().await;
    let day = Date::constant(2026, 3, 5);
    memory
        .add_entity(NewEntity::new(
            EntityId("thread:simpson-kitchen".into()),
            "The kitchen renovation",
            "the fixture roster",
        ))
        .await
        .expect("the thread is written")
        .written()
        .expect("nothing on this board collides with it");
    for (handle, name, said) in [
        (
            "decision:cabinets-oak",
            "Cabinets in oak",
            "The cabinets are oak, not pine",
        ),
        (
            "decision:floor-tile",
            "Floor in tile",
            "The floor is slate tile, not vinyl",
        ),
        (
            "decision:paint-cream",
            "Walls in cream",
            "The walls are cream",
        ),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = Some(EntityId("thread:simpson-kitchen".into()));
        memory
            .add_entity(new)
            .await
            .expect("the decision is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        memory
            .capture(NewFact::about(EntityId(handle.into()), said, day))
            .await
            .expect("the decision is said");
    }
    memory
        .archive_entity(
            &EntityId("decision:paint-cream".into()),
            "replaced by a later choice",
            None,
        )
        .await
        .expect("archive_entity ok");
    for finding in 1..=3 {
        memory
            .capture(NewFact::about(
                EntityId("thread:simpson-kitchen".into()),
                format!("finding {finding}: a contractor quoted a different price"),
                day,
            ))
            .await
            .expect("the finding is written");
    }
    memory
}

#[tokio::test]
async fn a_threads_page_puts_its_standing_decisions_first_and_then_its_own_claims() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory_with_a_kitchen_thread().await).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let thread = read(&client, addr, "/thread:simpson-kitchen/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // The standing decisions, each with what it says; the replaced one is not.
    let decisions = element(&thread, "decisions", "section");
    for (handle, said) in [
        ("decision:cabinets-oak", "The cabinets are oak, not pine"),
        ("decision:floor-tile", "The floor is slate tile, not vinyl"),
    ] {
        assert!(
            decisions.contains(&format!("href=\"/thread:simpson-kitchen/{handle}/\""))
                && decisions.contains(said),
            "{handle} stands: {decisions}"
        );
    }
    assert!(!decisions.contains("paint-cream"), "{decisions}");
    // They come before the thread's own claims.
    assert!(
        place(&thread, "id=\"decisions\"") < place(&thread, "<h2>Facts</h2>"),
        "{thread}"
    );
    // Its own claims follow, all of them: a thread is bounded by its own ceiling.
    let facts = &thread[thread.find("<h2>Facts</h2>").expect("the claims")..];
    for finding in 1..=3 {
        assert!(facts.contains(&format!("finding {finding}:")), "{facts}");
    }
    ct.cancel();
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
        refused.contains("cannot be run") && !refused.contains("<table id=\"answer\">"),
        "the page refuses and lists nothing: {refused}"
    );
    // The verb's own words: it refuses the same blank with "no kind is named ''".
    assert!(
        refused.contains("no kind is named"),
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

/// **A thread's decisions are listed in the order the verb lists them.** A thread
/// that numbers its members by `number` lists decisions 2 and 10 in that order, not
/// 10 first as their handles sort. Paired in the same read: a thread that numbers
/// nothing lists its decisions by handle.
#[tokio::test]
async fn a_threads_page_lists_numbered_decisions_in_number_order() {
    let memory = memory_with_views().await;
    for (thread, slug, numbered) in [
        ("thread:krusty-burger", "burger", true),
        ("thread:krusty-telethon", "telethon", false),
    ] {
        memory
            .add_entity(NewEntity::new(
                EntityId(thread.into()),
                if numbered {
                    "The burger talks"
                } else {
                    "Planning the telethon"
                },
                "the fixture roster",
            ))
            .await
            .expect("the thread is written")
            .written()
            .unwrap_or_else(|| panic!("{thread} collides with something on this board"));
        for (suffix, number) in [("bridge", "10"), ("roof", "2")] {
            let handle = format!("decision:{slug}-{suffix}");
            let mut new = NewEntity::new(
                EntityId(handle.clone()),
                format!("The {slug} {suffix}"),
                "the fixture roster",
            );
            new.parent = Some(EntityId(thread.into()));
            memory
                .add_entity(new)
                .await
                .expect("the decision is written")
                .written()
                .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
            memory
                .capture(set_on(NewFact {
                    fields: [("number".to_string(), number.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(
                        EntityId(handle),
                        format!("the {suffix} is decided"),
                        Date::constant(2026, 3, 5),
                    )
                }))
                .await
                .expect("the decision is numbered");
        }
        if numbered {
            memory
                .capture(set_on(NewFact {
                    fields: [("numbers_by".to_string(), "number".to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(
                        EntityId(thread.into()),
                        "its decisions are numbered",
                        Date::constant(2026, 3, 5),
                    )
                }))
                .await
                .expect("the thread numbers its decisions");
        }
    }
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let numbered = read(&client, addr, "/thread:krusty-burger/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let decisions = element(&numbered, "decisions", "section");
    assert!(
        place(decisions, "burger-roof") < place(decisions, "burger-bridge"),
        "decision 2 comes before decision 10: {decisions}"
    );
    let plain = read(&client, addr, "/thread:krusty-telethon/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let decisions = element(&plain, "decisions", "section");
    assert!(
        place(decisions, "telethon-bridge") < place(decisions, "telethon-roof"),
        "without numbers the decisions read by handle: {decisions}"
    );
    ct.cancel();
}

/// **A topic with a standing decision shows its decisions box first**, as a
/// thread's page does, before the topic's own claims.
#[tokio::test]
async fn a_topics_page_puts_its_standing_decisions_first() {
    let memory = memory_with_views().await;
    let mut topic = NewEntity::new(EntityId("topic:pegs".into()), "Pegs", "the fixture roster");
    topic.parent = Some(EntityId("person:alpha".into()));
    memory
        .add_entity(topic)
        .await
        .expect("the topic is written")
        .written()
        .expect("nothing on this board collides with it");
    let mut new = NewEntity::new(
        EntityId("decision:cabinets-oak".into()),
        "Cabinets in oak",
        "the fixture roster",
    );
    new.parent = Some(EntityId("topic:pegs".into()));
    memory
        .add_entity(new)
        .await
        .expect("the decision is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .capture(NewFact::about(
            EntityId("decision:cabinets-oak".into()),
            "The cabinets are oak, not pine",
            Date::constant(2026, 3, 5),
        ))
        .await
        .expect("the decision is said");
    memory
        .capture(NewFact::about(
            EntityId("topic:pegs".into()),
            "the widget stall opens at nine",
            Date::constant(2026, 3, 5),
        ))
        .await
        .expect("the topic's own claim is written");

    let page = page_over(seeded_board_over(memory).await, "/person:alpha/topic:pegs/").await;
    let decisions = element(&page, "decisions", "section");
    assert!(
        decisions.contains("The cabinets are oak, not pine"),
        "the decision stands: {decisions}"
    );
    assert!(
        place(&page, "id=\"decisions\"") < place(&page, "<h2>Facts</h2>"),
        "{page}"
    );
}

/// **A typed page keeps the generic sections behind one closed fold.** For each
/// of project, work, rhythm and thread: the fields, facts and children sections
/// are inside the one fold and not on the typed page above it. Paired in the same
/// read: a kind with no renderer keeps those sections on the page and has no fold.
#[tokio::test]
async fn a_typed_page_keeps_the_generic_sections_behind_one_closed_fold() {
    let fold = "<details><summary>Everything held</summary>";
    let board = || async { seeded_board_over(memory_with_a_board().await).await };
    let loops = || async { seeded_board_over(memory_with_two_loops().await.0).await };
    let kitchen = || async { seeded_board_over(memory_with_a_kitchen_thread().await).await };
    let pages = [
        ("project", page_over(board().await, "/project:atlas/").await),
        (
            "work",
            page_over(board().await, "/project:atlas/work:omicron/").await,
        ),
        (
            "rhythm",
            page_over(loops().await, "/person:alpha/rhythm:polish/").await,
        ),
        (
            "thread",
            page_over(kitchen().await, "/thread:simpson-kitchen/").await,
        ),
    ];
    for (kind, body) in &pages {
        let (typed, folded) = body.split_once(fold).unwrap_or_else(|| {
            panic!("the {kind} page has its generic sections in a fold: {body}")
        });
        assert_eq!(body.matches(fold).count(), 1, "one fold on the {kind} page");
        assert!(!body.contains("<details open"), "the {kind} fold is closed");
        for heading in ["<h2>Fields</h2>", "<h2>Facts</h2>", "<h2>Below here</h2>"] {
            assert!(
                !typed.contains(heading),
                "{heading} is not on the {kind} page itself: {typed}"
            );
        }
        // A value only the generic sections hold is still in the HTML, inside the fold.
        assert!(
            folded.contains("<td>source</td><td>the fixture roster</td>")
                && !typed.contains("the fixture roster"),
            "the {kind} page keeps what only the generic sections hold, in the fold: {body}"
        );
        assert!(
            folded.contains("<h2>Fields</h2>") && folded.contains("<h2>Facts</h2>"),
            "the {kind} page's fold holds them: {folded}"
        );
    }
    let person = page_over(seeded_board().await, "/person:alpha/").await;
    assert!(person.contains("<h2>Fields</h2>"), "{person}");
    assert!(!person.contains("Everything held"), "{person}");
}

/// The atlas project with an owner the operator said, a budget somebody worked
/// out, twelve dated claims and a child that is not work.
async fn memory_with_a_described_project() -> Arc<dyn Memory> {
    let memory = memory_with_a_board().await;
    for (key, value, provenance) in [
        ("owner", "person:alpha", Provenance::Testimony),
        ("budget", "40k", Provenance::Inference),
    ] {
        let mut fact = set_on(NewFact {
            fields: [(key.to_string(), value.to_string())].into_iter().collect(),
            ..NewFact::about(
                EntityId("project:atlas".into()),
                format!("the {key} of the atlas"),
                Date::constant(2026, 3, 6),
            )
        });
        fact.provenance = provenance;
        memory.capture(fact).await.expect("the key is written");
    }
    for day in 1..=12 {
        memory
            .capture(NewFact::about(
                EntityId("project:atlas".into()),
                format!("Claim number {day:02}"),
                Date::constant(2026, 4, day),
            ))
            .await
            .expect("the claim is written");
    }
    let mut bench = NewEntity::new(
        EntityId("thing:omega".into()),
        "The tool bench",
        "the fixture roster",
    );
    bench.parent = Some(EntityId("project:atlas".into()));
    memory
        .add_entity(bench)
        .await
        .expect("the bench is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
}

/// **A project says what it is, what happened lately and what sits in it.** About
/// shows what the project holds, with who wrote each value behind a ▸; recent
/// shows the newest ten claims and counts the older ones; contents lists
/// everything under it, work included, with its kind. Paired: the board's own
/// bookkeeping key is not a value of the project, and the eleventh newest claim
/// is counted and not shown.
#[tokio::test]
async fn a_projects_page_says_what_it_is_what_happened_lately_and_what_sits_in_it() {
    let body = page_over(
        seeded_board_over(memory_with_a_described_project().await).await,
        "/project:atlas/",
    )
    .await;

    let about = element(&body, "about", "section");
    assert!(about.contains("href=\"/person:alpha/\""), "{about}");
    assert!(about.contains("40k"), "{about}");
    assert!(
        !about.contains("considering"),
        "the board's columns are not a value: {about}"
    );
    let (shown, behind) = about
        .split_once("<details>")
        .unwrap_or_else(|| panic!("who wrote each value is behind a fold: {about}"));
    assert!(!shown.contains("testimony"), "{shown}");
    let owner = behind
        .split("<dt>owner</dt>")
        .nth(1)
        .expect("the owner's writer");
    assert!(owner.starts_with("<dd>testimony"), "{behind}");
    let budget = behind
        .split("<dt>budget</dt>")
        .nth(1)
        .expect("the budget's writer");
    assert!(budget.starts_with("<dd>inference"), "{behind}");

    let recent = element(&body, "recent", "section");
    assert!(
        recent.contains("Claim number 12") && recent.contains("Claim number 03"),
        "{recent}"
    );
    assert!(
        !recent.contains("Claim number 02"),
        "the eleventh newest is not shown: {recent}"
    );
    assert!(
        recent.contains("5 older"),
        "and the older ones are counted: {recent}"
    );
    assert!(
        place(recent, "Claim number 12") < place(recent, "Claim number 11"),
        "newest first: {recent}"
    );

    let contents = element(&body, "contents", "section");
    assert!(
        contents.contains("href=\"/project:atlas/thing:omega/\"")
            && contents.contains("href=\"/project:atlas/work:tau/\""),
        "everything under it, work included: {contents}"
    );
    assert!(
        contents.contains(">thing<") && contents.contains(">work<"),
        "with its kind: {contents}"
    );
}

/// A thread with two claims on different days and a third that was taken back.
async fn memory_with_a_dated_thread() -> Arc<dyn Memory> {
    let memory = seeded_memory().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("thread:simpson-kitchen".into()),
            "The kitchen",
            "the fixture roster",
        ))
        .await
        .expect("the thread is written")
        .written()
        .expect("nothing on this board collides with it");
    for (said, day) in [
        ("The supplier quoted six weeks", Date::constant(2026, 3, 8)),
        (
            "The supplier moved to four weeks",
            Date::constant(2026, 3, 12),
        ),
    ] {
        memory
            .capture(NewFact::about(
                EntityId("thread:simpson-kitchen".into()),
                said,
                day,
            ))
            .await
            .expect("the claim is written");
    }
    let withdrawn = memory
        .capture(NewFact::about(
            EntityId("thread:simpson-kitchen".into()),
            "The supplier never answered",
            Date::constant(2026, 3, 15),
        ))
        .await
        .expect("the claim is written")
        .written()
        .expect("nothing on this board collides with it");
    memory
        .update_fact(
            &withdrawn.address(),
            jojobot_domain::memory::FactPatch {
                status: Some(jojobot_domain::memory::FactStatus::Archived),
                ..Default::default()
            },
            &EntityId("person:alpha".into()),
        )
        .await
        .expect("the claim is archived");
    // Something under the thread that is not a decision, and a decision whose
    // newest wording was taken back: the older wording is what stands.
    let mut other = NewEntity::new(
        EntityId("topic:stunt-schedule".into()),
        "Stunt schedule",
        "the fixture roster",
    );
    other.parent = Some(EntityId("thread:simpson-kitchen".into()));
    memory
        .add_entity(other)
        .await
        .expect("the topic is written")
        .written()
        .expect("nothing on this board collides with it");
    let mut decision = NewEntity::new(
        EntityId("decision:cabinets-oak".into()),
        "Cabinets are oak",
        "the fixture roster",
    );
    decision.parent = Some(EntityId("thread:simpson-kitchen".into()));
    memory
        .add_entity(decision)
        .await
        .expect("the decision is written")
        .written()
        .expect("nothing on this board collides with it");
    for said in ["Oak, not maple", "Oak, for now"] {
        let wording = memory
            .capture(NewFact::about(
                EntityId("decision:cabinets-oak".into()),
                said,
                Date::constant(2026, 3, 1),
            ))
            .await
            .expect("the wording is written")
            .written()
            .expect("nothing on this board collides with it");
        if said == "Oak, for now" {
            memory
                .update_fact(
                    &wording.address(),
                    jojobot_domain::memory::FactPatch {
                        status: Some(jojobot_domain::memory::FactStatus::Archived),
                        ..Default::default()
                    },
                    &EntityId("person:alpha".into()),
                )
                .await
                .expect("the wording is archived");
        }
    }
    memory
}

/// **A thread lists its own claims newest first** in a section of their own, and a
/// claim that was taken back is not among them. Paired in the same read.
#[tokio::test]
async fn a_threads_page_lists_the_claims_that_stand_newest_first() {
    let body = page_over(
        seeded_board_over(memory_with_a_dated_thread().await).await,
        "/thread:simpson-kitchen/",
    )
    .await;
    let decisions = element(&body, "decisions", "section");
    assert!(
        decisions.contains("Oak, not maple"),
        "the decision stands: {decisions}"
    );
    assert!(
        !decisions.contains("for now"),
        "a withdrawn wording does not: {decisions}"
    );
    assert!(
        !decisions.contains("stunt-schedule"),
        "a topic is not a decision: {decisions}"
    );
    let claims = element(&body, "claims", "section");
    let newer = claims.find("four weeks").expect("the newer claim");
    let older = claims.find("six weeks").expect("the older claim");
    assert!(newer < older, "newest first: {claims}");
    assert!(
        !claims.contains("never answered"),
        "a withdrawn claim does not stand: {claims}"
    );
    assert!(
        body.split_once("<details><summary>Everything held</summary>")
            .is_some_and(|(typed, _)| typed.contains("id=\"claims\"")),
        "the claims are on the page itself, not in the fold: {body}"
    );
}

/// **A project with nothing under it has no contents box**, where the same page
/// of a project with children has one. Paired in the same read.
#[tokio::test]
async fn a_project_with_nothing_under_it_has_no_contents_box() {
    let memory = memory_with_a_board().await;
    memory
        .add_entity(NewEntity::new(
            EntityId("project:the-move".into()),
            "The Move",
            "the fixture roster",
        ))
        .await
        .expect("the project is written")
        .written()
        .expect("nothing on this board collides with it");
    let board = seeded_board_over(memory).await;
    let empty = page_over(board, "/project:the-move/").await;
    assert!(!empty.contains("id=\"contents\""), "{empty}");
    let full = page_over(
        seeded_board_over(memory_with_a_board().await).await,
        "/project:atlas/",
    )
    .await;
    assert!(full.contains("id=\"contents\""), "{full}");
}

/// **Every page carries a search box, and it asks the search page.** The box is
/// in the page's own bar, beside the path, so a reader never has to go back to
/// the root to look something up. A GET form with one field named `q`, so the
/// result is an address a person can keep.
#[tokio::test]
async fn every_page_carries_a_search_box_that_asks_the_search_page() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for path in ["/", "/person:alpha/", "/person:alpha/topic:widgets/"] {
        let body = read(&client, addr, path, &cookie)
            .await
            .text()
            .await
            .unwrap();
        assert!(
            body.contains("<form class=\"find\" action=\"/ui/search\" method=\"get\">")
                && body.contains("name=\"q\""),
            "{path} carries no search box that asks /ui/search: {body}"
        );
    }
    ct.cancel();
}

/// **A search from the box lists what the search finds, and only that.** The
/// words are asked of the same search the bots use, with no asker, so the
/// operator sees everything. The positive and the negative are read from the
/// same page shape: a word the fixture holds finds the claim's subject and the
/// thing named by it, and a word nothing holds says so and links nothing.
#[tokio::test]
async fn a_search_from_the_box_lists_what_the_search_finds_and_only_that() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let found = read(&client, addr, "/ui/search?q=widgets", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        found.contains("href=\"/person:alpha/topic:widgets/\""),
        "the thing the words name is a link to its page: {found}"
    );
    let claim = read(&client, addr, "/ui/search?q=stall", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        claim.contains("The widget stall runs on Thursdays")
            && claim.contains("href=\"/person:alpha/topic:widgets/\""),
        "a claim found by its words is shown and links to the page it is about: {claim}"
    );

    let nothing = read(&client, addr, "/ui/search?q=tractor", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        nothing.contains("id=\"nothing-found\"")
            && !nothing.contains("href=\"/person:alpha/topic:widgets/\""),
        "a word nothing holds finds nothing and links nothing: {nothing}"
    );
    ct.cancel();
}

/// **The search verb's own answer, asked over `/mcp` of the server under test**, so a case
/// about what the search page says takes its expected value from what the served verb says
/// and not from a helper the page itself calls.
async fn search_verb_answer(addr: SocketAddr, query: &str) -> serde_json::Value {
    use rmcp::ServiceExt;
    use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
    let transport =
        rmcp::transport::StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("ui-test", "0.0.1"),
    )
    .serve(transport)
    .await
    .expect("the verb's client connects");
    let result = client
        .call_tool(
            CallToolRequestParams::new("search".to_string()).with_arguments(
                serde_json::json!({ "query": query })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .expect("the search verb answers");
    let text = result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|text| text.text.clone())
        .expect("the verb answers in text");
    client.cancel().await.ok();
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("the verb answers in JSON: {text}"))
}

/// **An empty search says when the index had not caught up**, in the words the search
/// verb's own answer carries, and not a bare "Nothing found." An index that has not
/// read the store holds only what this process wrote, so "nothing" may be "not yet".
/// Paired with a loaded index, where the same words find nothing and say nothing more.
#[tokio::test]
async fn an_empty_search_says_when_the_index_has_not_caught_up() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_serving_over(
        endpoints,
        &[READER],
        &idp,
        seeded_board().await,
        "http",
        None,
        "",
        Some(jojobot_domain::memory::search::Behind::Unscanned),
    )
    .await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let behind = read(&client, addr, "/ui/search?q=tractor", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        behind.contains("id=\"nothing-found\"") && behind.contains("id=\"search-behind\""),
        "an empty answer from an index that has not caught up says so: {behind}"
    );
    // The words are the search verb's own coverage note for the same index state, asked of
    // the served verb rather than of the helper the page calls.
    let verb = search_verb_answer(addr, "tractor").await;
    let note = verb["memory"]["note"]
        .as_str()
        .unwrap_or_else(|| panic!("the verb reports the index as behind: {verb}"));
    assert!(
        behind.contains(&jojobot::ui::pages::escape(note)),
        "the page carries the verb's coverage note: {note:?} in {behind}"
    );
    ct.cancel();

    // The pair: a loaded index finds nothing and adds no such line.
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let cookie = log_in(&client, addr, "/").await;
    let loaded = read(&client, addr, "/ui/search?q=tractor", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        loaded.contains("id=\"nothing-found\"") && !loaded.contains("search-behind"),
        "a loaded index adds nothing to an empty answer: {loaded}"
    );
    ct.cancel();
}

/// **A thin answer carries the note that words are matched loosely, once, and a good
/// answer carries nothing more than it did.** The note is the search verb's own
/// `matching.note`, which the verb sends on an answer of fewer than three hits. Three
/// things named for one word are a good answer; none and one are thin.
#[tokio::test]
async fn a_thin_search_says_the_words_are_matched_loosely_once_and_a_good_one_does_not() {
    let board = seeded_board().await;
    for (handle, name) in [
        ("thing:delta", "Kiln furnace"),
        ("thing:epsilon", "Kiln brick"),
        ("thing:gamma", "Kiln glaze"),
        ("thing:omega", "Quartz mortar"),
    ] {
        board
            .memory
            .add_entity(NewEntity::new(EntityId(handle.into()), name, "fixture"))
            .await
            .expect("the thing is written")
            .written()
            .expect("nothing collides with it");
    }
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |q: &'static str| {
        let (client, cookie) = (client.clone(), cookie.clone());
        async move {
            read(&client, addr, &format!("/ui/search?q={q}"), &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };

    for thin in ["tractor", "quartz"] {
        let said = page(thin).await;
        assert_eq!(
            said.matches("id=\"search-matching\"").count(),
            1,
            "a thin answer ({thin}) carries the matching note once: {said}"
        );
    }
    let good = page("kiln").await;
    assert_eq!(
        good.matches("href=\"/thing:").count(),
        3,
        "three things are found for the word: {good}"
    );
    assert!(
        !good.contains("search-matching") && !good.contains("search-behind"),
        "a good answer from a loaded index looks as it did: {good}"
    );
    ct.cancel();
}

/// **Hits the page holds and does not list are counted, with the answer's own way to read
/// them.** The seeded board's prose is found by the word `stall`, and the page lists
/// entities and claims only. The words are the shared helper's, so the page and the verb
/// say one thing. Paired with a word that finds only a thing, where no such line shows.
#[tokio::test]
async fn hits_the_page_does_not_list_are_counted_with_the_way_to_read_them() {
    let board = seeded_board().await;
    board
        .memory
        .add_entity(NewEntity::new(
            EntityId("thing:omega".into()),
            "Quartz mortar",
            "fixture",
        ))
        .await
        .expect("the thing is written")
        .written()
        .expect("nothing collides with it");
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let unlisted = read(&client, addr, "/ui/search?q=stall", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let line = unlisted
        .split_once("id=\"not-listed\">")
        .unwrap_or_else(|| panic!("the unlisted hits are not counted: {unlisted}"))
        .1
        .split_once("</p>")
        .expect("the line closes")
        .0;
    let counted: usize = line
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("the line leads with how many: {line}"));
    // The expected count comes from the served verb: every hit it returns that is neither
    // an entity nor a claim is a hit the page cannot list.
    let verb = search_verb_answer(addr, "stall").await;
    let unlisted_by_verb = verb["results"]
        .as_array()
        .unwrap_or_else(|| panic!("the verb lists its hits: {verb}"))
        .iter()
        .filter(|hit| hit["hit"] != "entity" && hit["hit"] != "fact")
        .count();
    assert!(unlisted_by_verb >= 1, "the word finds prose: {verb}");
    assert_eq!(
        counted, unlisted_by_verb,
        "the page counts the hits the verb returns that it does not list: {line}"
    );
    // The way to read them is the verb, by the name the server serves it under:
    // of the words in the line, the ones that are served verbs are the search verb
    // alone, so a line that named another verb, or none, fails.
    let served = served_tools::names(addr).await;
    let verbs_named: Vec<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| served.iter().any(|tool| tool == word))
        .collect();
    assert_eq!(
        verbs_named,
        ["search"],
        "the line names the verb that returns them, and only it: {line}"
    );

    let listed = read(&client, addr, "/ui/search?q=quartz", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        listed.contains("href=\"/thing:") && !listed.contains("not-listed"),
        "a thing found and nothing held back says nothing about unlisted hits: {listed}"
    );
    ct.cancel();
}

/// **A page that counts hits it does not list never also says nothing was found,
/// and it names only the sources it searched.** The word `prose` is found in the
/// seeded board's prose and in no entity or claim, so the page lists no rows. It
/// says how many it counted and where, and it does not name mail, which this page
/// never searches. Paired with a word nothing holds, which still says nothing was
/// found.
#[tokio::test]
async fn a_page_with_only_unlisted_hits_does_not_say_nothing_found_or_name_mail() {
    let board = seeded_board().await;
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let unlisted = read(&client, addr, "/ui/search?q=prose", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !unlisted.contains("id=\"found\""),
        "the word must find no listed row for this case to be about the empty table: {unlisted}"
    );
    let line = unlisted
        .split_once("id=\"not-listed\">")
        .unwrap_or_else(|| panic!("the unlisted hits are not counted: {unlisted}"))
        .1
        .split_once("</p>")
        .expect("the line closes")
        .0;
    assert!(
        !unlisted.contains("id=\"nothing-found\""),
        "hits counted elsewhere are not nothing: {unlisted}"
    );
    assert!(
        line.contains("prose") && !line.contains("mail"),
        "the line names the source it searched and not the one it did not: {line}"
    );

    let nothing = read(&client, addr, "/ui/search?q=zzyzx", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        nothing.contains("id=\"nothing-found\"") && !nothing.contains("not-listed"),
        "a word nothing holds still says so: {nothing}"
    );
    ct.cancel();
}

/// **A search page needs a login like every other page**, and looking writes and
/// delivers nothing: the page is the same gate.
#[tokio::test]
async fn the_search_page_is_sent_to_the_login_without_a_session() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let response = browser()
        .get(format!("http://{addr}/ui/search?q=widgets"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_redirection(), "{}", response.status());
    assert!(location(&response).starts_with("/ui/login"));
    ct.cancel();
}

/// **A value says how it is known only behind its fold.** The fields table leads
/// with the value and keeps the rest out of sight: whether it was said, read or
/// worked out, and the day the write holding it was made. `takings` was written
/// twice, so the fold has to name the second write and not the first. Read from
/// the page: outside the fold the row says none of it, inside it says all of it.
#[tokio::test]
async fn a_value_says_how_it_is_known_only_behind_its_fold() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let fields = section(&page, "fields");
    let row = fields
        .split_once("?history=takings")
        .expect("the takings key is in the fields table")
        .1
        .split_once("</tr>")
        .expect("…and its row closes")
        .0;
    let (shown, folded) = row
        .split_once("<details>")
        .unwrap_or_else(|| panic!("the row has no fold: {row}"));
    assert!(
        shown.ends_with("<td>45"),
        "the value leads the row, outside the fold: {row}"
    );
    assert!(
        !shown.contains("worked out") && !shown.contains("2026-03-08"),
        "how it is known stays out of sight until the fold opens: {shown}"
    );
    assert!(
        folded.contains("worked out") && folded.contains("2026-03-08"),
        "the fold says how it is known and the day of the write holding it: {folded}"
    );
    assert!(
        !folded.contains("2026-03-07"),
        "the day is the newest write's, not the first one's: {folded}"
    );
    ct.cancel();
}

/// **Two claims set `price` on a thing, and the older is then rewritten in
/// place.** Alpha (testimony) sets 5, beta (inference) sets 7, then alpha is
/// rewritten to 6. Returns the memory and alpha's address. Written to the
/// store, not through a verb, so the rewrite keeps alpha's old moment of
/// first entry, which is the state a page that picks the newest-entered record
/// gets wrong.
async fn price_written_twice_then_the_older_rewritten(
    handle: &str,
    kind: &str,
) -> (
    Arc<dyn Memory>,
    jojobot_domain::memory::FactAddress,
    jojobot_domain::memory::FactAddress,
) {
    let memory = seeded_memory().await;
    let id = EntityId(handle.into());
    memory
        .add_entity(NewEntity::new(id.clone(), kind, "the fixture roster"))
        .await
        .expect("the thing is written");
    let mut both = Vec::new();
    for (value, provenance, said, day) in [
        ("5", Provenance::Testimony, "the price is five", 10),
        ("7", Provenance::Inference, "the price looks like seven", 11),
    ] {
        let mut claim = NewFact::about(id.clone(), said, Date::constant(2026, 3, day));
        claim.provenance = provenance;
        // Testimony is rewritten in place only by the session of its first write.
        claim.session = Some("alpha-run".to_string());
        claim.fields = [("price".to_string(), value.to_string())]
            .into_iter()
            .collect();
        claim.sets = ["price".to_string()].into_iter().collect();
        both.push(
            memory
                .capture(claim)
                .await
                .expect("the price is written")
                .written()
                .expect("nothing collides"),
        );
    }
    (memory, both[0].address(), both[1].address())
}

/// Rewrite the claim at `address` so it sets `price` to `value`, in place.
async fn rewrite_price(
    memory: &Arc<dyn Memory>,
    address: &jojobot_domain::memory::FactAddress,
    value: &str,
) {
    memory
        .update_fact(
            address,
            jojobot_domain::memory::FactPatch {
                fields: [("price".to_string(), value.to_string())]
                    .into_iter()
                    .collect(),
                sets: ["price".to_string()].into_iter().collect(),
                session: Some("alpha-run".to_string()),
                ..Default::default()
            },
            &EntityId("person:alpha".into()),
        )
        .await
        .expect("the claim is rewritten");
}

/// The row of the `price` key in a page's fields table.
fn price_row(page: &str) -> &str {
    section(page, "fields")
        .split_once("?history=price")
        .expect("the price key is in the fields table")
        .1
        .split_once("</tr>")
        .expect("…and its row closes")
        .0
}

/// **A page names the write the fold took a value from, and so does the verb's
/// backing.** Before the rewrite the newest write of `price` is beta's (7, worked
/// out). Alpha is then rewritten in place: that is the newest write of the key,
/// though alpha entered the record first, so the value is 6, said, backed by
/// alpha. A page that picked the newest-entered record would keep naming beta.
#[tokio::test]
async fn a_page_names_the_write_the_fold_took_the_value_from_after_an_in_place_rewrite() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (memory, alpha, beta) =
        price_written_twice_then_the_older_rewritten("work:donut-budget", "Donut budget").await;
    let board = seeded_board_over(memory.clone()).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let id = EntityId("work:donut-budget".into());

    let before = read(&client, addr, "/work:donut-budget/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let row = price_row(&before);
    assert!(
        row.contains("<td>7"),
        "the newest write of the key holds 7: {row}"
    );
    assert!(row.contains("worked out"), "and it is worked out: {row}");
    assert!(
        !row.contains("said"),
        "the older write is not the one named: {row}"
    );
    let backing = memory.backing(&id).await.expect("backing reads");
    assert_eq!(backing["price"].fact, beta.local, "the verb names beta");

    rewrite_price(&memory, &alpha, "6").await;

    let after = read(&client, addr, "/work:donut-budget/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let row = price_row(&after);
    assert!(
        row.contains("<td>6"),
        "the rewrite is the newest write of the key: {row}"
    );
    assert!(row.contains("said"), "and alpha said it: {row}");
    assert!(
        !row.contains("worked out"),
        "beta is not the one named now: {row}"
    );
    let backing = memory.backing(&id).await.expect("backing reads");
    assert_eq!(
        backing["price"].fact, alpha.local,
        "the verb names alpha, as the page does"
    );
    assert_eq!(
        backing["price"].provenance,
        Provenance::Testimony,
        "and as testimony"
    );
    ct.cancel();
}

/// **A summing key says nothing about how it is known, and an ordinary key beside
/// it still does.** The verb names no backing for a total, because no one write
/// holds it, so the page does not pick one. `donuts` is written twice and totals
/// 3; `price` is written once and says how.
#[tokio::test]
async fn a_summing_key_says_nothing_of_how_it_is_known_and_an_ordinary_key_still_does() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (memory, _, _) =
        price_written_twice_then_the_older_rewritten("work:donut-budget", "Donut budget").await;
    memory
        .declare_type(
            DeclaredType::new("donut-eaten", vec![Field::summing("donuts")]),
            None,
        )
        .await
        .expect("the counter is declared");
    for (count, day) in [("1", 12), ("2", 13)] {
        let mut eaten = NewFact::about(
            EntityId("work:donut-budget".into()),
            "some were eaten",
            Date::constant(2026, 3, day),
        );
        eaten.fields = [("donuts".to_string(), count.to_string())]
            .into_iter()
            .collect();
        eaten.sets = ["donuts".to_string()].into_iter().collect();
        memory.capture(eaten).await.expect("the count is written");
    }
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let page = read(&client, addr, "/work:donut-budget/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let fields = section(&page, "fields");
    let donuts = fields
        .split_once("?history=donuts")
        .expect("the donuts key is in the fields table")
        .1
        .split_once("</tr>")
        .expect("…and its row closes")
        .0;
    assert!(
        donuts.contains("<td>3"),
        "the total leads the row: {donuts}"
    );
    assert!(
        !donuts.contains("<details>"),
        "a total has no one write to name, so no fold: {donuts}"
    );
    assert!(
        price_row(&page).contains("<details>"),
        "an ordinary key on the same page still says how it is known"
    );
    ct.cancel();
}

/// **A project's page names the write the fold took a value from.** The same
/// pair as a thing's page, through the project's own "who wrote each value":
/// before the rewrite beta (inference) is named for `price`, after it alpha
/// (testimony) is.
#[tokio::test]
async fn a_projects_page_names_the_write_the_fold_took_the_value_from_after_an_in_place_rewrite() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (memory, alpha, _) =
        price_written_twice_then_the_older_rewritten("project:donut-fund", "Donut fund").await;
    let board = seeded_board_over(memory.clone()).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let who = |page: &str| -> String {
        let at = page
            .find("who wrote each value")
            .expect("the project says who wrote each value");
        let rest = &page[at..];
        let from = rest
            .find("<dt>price</dt>")
            .expect("price is among the values");
        rest[from..]
            .split_once("</dd>")
            .expect("…and its row closes")
            .0
            .to_string()
    };
    let before = read(&client, addr, "/project:donut-fund/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        who(&before).contains("inference"),
        "beta wrote the newest price: {}",
        who(&before)
    );

    rewrite_price(&memory, &alpha, "6").await;

    let after = read(&client, addr, "/project:donut-fund/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        who(&after).contains("testimony"),
        "alpha's rewrite is the newest: {}",
        who(&after)
    );
    assert!(!who(&after).contains("inference"), "{}", who(&after));
    ct.cancel();
}

/// The cells of the row of the table `id` whose first cell says `label`.
fn labelled_row<'a>(body: &'a str, id: &str, label: &str) -> &'a str {
    let table = section(body, id);
    table
        .split("<tr>")
        .find(|row| row.starts_with(&format!("<td>{label}</td>")))
        .unwrap_or_else(|| panic!("no row {label:?} in {id}: {table}"))
}

/// **A thing's page says what is around it.** The parent, what the thing points
/// at and what points at it, each a link to that page. The stall's claim draws a
/// location edge at the town, so the stall points at the town and the town is
/// pointed at by the stall. Paired both ways in one read: each direction's row
/// holds its own neighbour and not the other's.
#[tokio::test]
async fn a_things_page_says_its_parent_and_what_points_at_it_and_what_it_points_at() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct) = spawn_jojobot(endpoints, &[READER], &idp).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let stall = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        labelled_row(&stall, "around", "parent").contains("href=\"/person:alpha/\""),
        "the parent is a link: {stall}"
    );
    let out = labelled_row(&stall, "around", "points at");
    assert!(
        out.contains("href=\"/place:shelbyville/\""),
        "what the stall points at is a link: {out}"
    );
    assert!(
        !labelled_row(&stall, "around", "pointed at by").contains("shelbyville"),
        "the town does not point at the stall"
    );

    let town = read(&client, addr, "/place:shelbyville/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        labelled_row(&town, "around", "pointed at by")
            .contains("href=\"/person:alpha/topic:widgets/\""),
        "what points at the town is a link: {town}"
    );
    assert!(
        !labelled_row(&town, "around", "points at").contains("widgets"),
        "the town points at nothing"
    );
    ct.cancel();
}

/// **A topic that holds standing decisions says what is around it.** The
/// decisions are read in front of the topic, so a page that took the first
/// object of the read as the topic would show a decision's neighbours, which
/// are none. The widget topic points at the town through its stall claim, and
/// the town's own claim points back at it; both rows must name their neighbour.
#[tokio::test]
async fn a_topic_holding_decisions_still_says_what_is_around_it() {
    let memory = memory_with_views().await;
    for (handle, name, said) in [
        (
            "decision:cabinets-oak",
            "Cabinets in oak",
            "The cabinets are oak, not pine",
        ),
        (
            "decision:floor-tile",
            "Floor in tile",
            "The floor is slate tile, not vinyl",
        ),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = Some(EntityId("topic:widgets".into()));
        memory
            .add_entity(new)
            .await
            .expect("the decision is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
        memory
            .capture(NewFact::about(
                EntityId(handle.into()),
                said,
                Date::constant(2026, 3, 5),
            ))
            .await
            .expect("the decision is said");
    }
    let mut back = NewFact::about(
        EntityId("place:shelbyville".into()),
        "the widget stall trades here",
        Date::constant(2026, 3, 6),
    );
    back.edge = Some(Edge::new(
        EdgeShape::About,
        EntityId("topic:widgets".into()),
    ));
    memory
        .capture(back)
        .await
        .expect("the town's claim is written");

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();

    // The positive: the decisions are on this page, so the read had them in front.
    assert!(page.contains("decision:cabinets-oak"), "{page}");
    let out = labelled_row(&page, "around", "points at");
    assert!(
        out.contains("href=\"/place:shelbyville/\""),
        "what the topic points at is a link: {out}"
    );
    let inbound = labelled_row(&page, "around", "pointed at by");
    assert!(
        inbound.contains("href=\"/place:shelbyville/\""),
        "what points at the topic is a link: {inbound}"
    );
    ct.cancel();
}

/// **A listing row says how much sits under a thing, directly and in all.** One
/// number when the two are the same, and both when the subtree is deeper than
/// the first level, so a row never makes a deep branch look shallow.
#[tokio::test]
async fn a_listing_row_says_how_much_sits_under_a_thing_directly_and_in_all() {
    let memory = seeded_memory().await;
    for (handle, name, parent) in [
        ("topic:tarps", "Tarps", "topic:widgets"),
        ("topic:pegs", "Pegs", "topic:tarps"),
    ] {
        let mut deeper = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        deeper.parent = Some(EntityId(parent.into()));
        memory
            .add_entity(deeper)
            .await
            .expect("the child is written")
            .written()
            .unwrap_or_else(|| panic!("{handle} collides with something on this board"));
    }
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board_over(memory).await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let index = read(&client, addr, "/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let alpha = element(&index, "roots", "table")
        .split("<tr>")
        .find(|row| row.contains("href=\"/person:alpha/\""))
        .expect("alpha is a top-level thing")
        .to_string();
    assert!(
        alpha.contains("<td class=\"n\">1<span class=\"m\"> · 3 in all</span></td>"),
        "one thing sits directly under it and three in all: {alpha}"
    );
    let town = element(&index, "roots", "table")
        .split("<tr>")
        .find(|row| row.contains("href=\"/place:shelbyville/\""))
        .expect("the town is a top-level thing")
        .to_string();
    assert!(
        town.contains("<td class=\"n\">—</td>"),
        "a thing with nothing under it says so, with no second number: {town}"
    );
    ct.cancel();
}

/// **A key held badly is not a key held, and the page says so.** A market whose
/// `opens` day is "next spring" holds the stall's keys in name only: the page
/// names `opens` as the wrong one with the value it holds, and the stall is not
/// shown whole. Paired in one read with the stall that holds both keys well,
/// which the same page shape shows whole and with nothing wrong.
#[tokio::test]
async fn a_date_key_holding_next_spring_is_held_badly_and_the_thing_is_not_whole() {
    let memory = seeded_memory().await;
    let mut market = NewEntity::new(EntityId("topic:flea".into()), "Flea", "the fixture roster");
    market.parent = Some(EntityId("person:alpha".into()));
    memory
        .add_entity(market)
        .await
        .expect("the market is written")
        .written()
        .expect("the market collides with nothing on this board");
    let mut piece = NewFact::about(
        EntityId("topic:flea".into()),
        "it opens next spring, and takes fourteen pitches",
        Date::constant(2026, 3, 9),
    );
    piece.fields = [
        ("opens".to_string(), "next spring".to_string()),
        ("pitch".to_string(), "14".to_string()),
    ]
    .into_iter()
    .collect();
    piece.sets = ["opens".to_string(), "pitch".to_string()]
        .into_iter()
        .collect();
    memory.capture(piece).await.expect("the claim is written");
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let (addr, ct, _) =
        spawn_jojobot_over(endpoints, &[READER], &idp, seeded_board_over(memory).await).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    let flea = read(&client, addr, "/person:alpha/topic:flea/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let badly = labelled_row(&flea, "conforms", "stall");
    assert!(
        badly.contains("<td>partly</td>") && badly.contains("opens: next spring"),
        "the wrong key is named with the value it holds, and the thing is not whole: {badly}"
    );
    let stall = read(&client, addr, "/person:alpha/topic:widgets/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    let well = labelled_row(&stall, "conforms", "stall");
    assert!(
        well.contains("<td>whole</td>") && !well.contains("opens: "),
        "the same page shape shows a thing holding both keys well as whole: {well}"
    );
    ct.cancel();
}

/// **A page that maps the operator's status words says so when the record that
/// holds them cannot be read** (rule 130): the project's board and a view that
/// asks what is owed. Neither draws itself as if the instance mapped none. The
/// same pages, drawn while the record reads, show their work; and a record nobody
/// wrote is absence, so the pages draw as they always did.
#[tokio::test]
async fn a_page_that_maps_status_words_says_when_the_instance_record_cannot_be_read() {
    let inner = memory_with_views().await;
    inner
        .add_entity(NewEntity::new(
            EntityId("project:atlas".into()),
            "Atlas",
            "the fixture roster",
        ))
        .await
        .expect("the project is written")
        .written()
        .expect("nothing on this board collides with it");
    let mut work = NewEntity::new(EntityId("work:omega".into()), "omega", "the fixture roster");
    work.parent = Some(EntityId("project:atlas".into()));
    inner
        .add_entity(work)
        .await
        .expect("the work is written")
        .written()
        .expect("nothing on this board collides with it");
    inner
        .capture(set_on(NewFact {
            fields: [("status".to_string(), "next".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                EntityId("work:omega".into()),
                "to do",
                Date::constant(2026, 3, 5),
            )
        }))
        .await
        .expect("the status is written");
    let counted = Arc::new(jojobot_adapters::counted::Counted::new(inner));

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(counted.clone()).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let page = |path: &'static str| {
        let client = client.clone();
        let cookie = cookie.clone();
        async move {
            read(&client, addr, path, &cookie)
                .await
                .text()
                .await
                .unwrap()
        }
    };
    let words = "could not read the operator&#39;s status words";

    // The record reads (it is absent here, which is no failure): both draw.
    let board_page = page("/project:atlas/").await;
    assert!(board_page.contains("work:omega"), "{board_page}");
    assert!(!board_page.contains(words), "{board_page}");
    let due_page = page("/view:due/").await;
    assert!(!due_page.contains(words), "{due_page}");

    // The record cannot be read: both say so.
    counted.fail_fields_of(Some("topic:instance"));
    let board_page = page("/project:atlas/").await;
    assert!(board_page.contains(words), "{board_page}");
    let due_page = page("/view:due/").await;
    assert!(due_page.contains(words), "{due_page}");
    ct.cancel();
}

/// **An instance record that cannot be read is said on the operator's mail, and
/// never shown as an instance with no operator** (rule 130). The healthy page
/// names the operator's box and an action lands; with the record unreadable the
/// operator's page says it could not read the record, and an action is refused
/// with a retry status instead of 404 "no operator". A record that does not
/// exist is still absence.
#[tokio::test]
async fn an_unreadable_instance_record_is_said_on_the_operators_mail_page_and_actions() {
    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let counted = Arc::new(jojobot_adapters::counted::Counted::new(
        seeded_memory().await,
    ));
    let board = seeded_board_over(counted.clone()).await;
    let mail = with_an_operators_mail(&board).await;
    let (addr, ct, _board) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;
    let origin = format!("http://{addr}");
    let words = "could not read the instance&#39;s record";

    // Healthy: the page shows the box and says nothing of the record.
    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(page.contains(OPERATOR_SUBJECT), "{page}");
    assert!(!page.contains(words), "{page}");

    // Unreadable: the page says so, shows no box, and an action is a 502.
    counted.fail_fields_of(Some("topic:instance"));
    let page = read(&client, addr, "/person:lisa/", &cookie)
        .await
        .text()
        .await
        .unwrap();
    assert!(page.contains(words), "{page}");
    assert!(!page.contains(OPERATOR_SUBJECT), "{page}");
    let refused = open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
    assert_eq!(refused.status(), reqwest::StatusCode::BAD_GATEWAY);

    // Readable again: the same action lands.
    counted.fail_fields_of(None);
    let opened = open_in_the_ui(addr, &cookie, mail.to_the_operator.as_str(), Some(&origin)).await;
    assert!(opened.status().is_redirection(), "{}", opened.status());
    ct.cancel();
}

/// **A bot's page leaves out jojobot's record of the release notes the bot was
/// told, as `recall` does** (rule 498). The record is the one the start writes,
/// by the code that writes it in production, and not one made up here: the start
/// sends the bot the shipped notes and records that it was told. The bot's own
/// record is the paired positive: the page shows it.
#[tokio::test]
async fn a_bots_page_leaves_out_the_record_of_the_release_notes_it_was_told() {
    let board = otto_with_nothing_yet().await;
    let otto = EntityId("bot:otto".into());
    board
        .memory
        .capture(NewFact::about(
            otto.clone(),
            "keeps the kiln lit",
            Date::constant(2026, 3, 4),
        ))
        .await
        .expect("the claim is written");
    let mailboxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> = board.mailboxes.clone();
    jojobot_mcp::release_notes::send_unsent_notes(&board.memory, &mailboxes, &Default::default())
        .await;
    let recorded = board
        .memory
        .fields(&otto)
        .await
        .expect("otto reads")
        .contains_key(jojobot_domain::memory::RELEASE_NOTES_TOLD);
    assert!(recorded, "the start recorded what it sent otto");
    let page = page_over(board, "/bot:otto/").await;
    assert!(page.contains("keeps the kiln lit"), "{page}");
    assert!(
        !page.contains("keeps here the release notes"),
        "the ledger is left out of the page: {page}"
    );
    assert!(
        !page.contains("release_notes_told"),
        "and so is the key it keeps on the bot: {page}"
    );
}
