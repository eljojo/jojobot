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
use jojobot::config::UiConfig;
use jojobot::ui::Ui;
use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
use jojobot_domain::memory::search::Search;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    Boot, Edge, EdgeShape, Entity, EntityId, EntityKind, Memory, NewEntity, NewFact, Provenance,
};
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};
use tokio_util::sync::CancellationToken;

mod support;
use support::{browser, location, log_in, log_in_raw, query_of, session_pair, spawn_idp};

const CLIENT_ID: &str = "jojobot-ui";
const READER: &str = "sub-reader";

/// A body past the budget the digest strategy declares, with a distinct opening
/// and a distinct last line — so a page can be asserted to carry both the
/// summary and the whole of it.
const LONG_BODY: &str = "The survey needs a second pair of eyes on the north section, because the \
counts taken there disagree with the ones from last season and nobody has said which of them is \
right. This closing sentence is the tail of the long body.";

/// The same, for a chronology beat.
const LONG_BEAT: &str = "Set out to reconcile the two counts and found that the disagreement is in \
the north section alone, which narrows it to one afternoon of records rather than the whole \
season. This closing sentence is the tail of the long beat.";

/// A fixed instant, so nothing here reads a clock.
const FIXED_INSTANT: jiff::Timestamp = jiff::Timestamp::constant(1_780_000_000, 0);

/// Markup somebody typed, in a value a page renders.
///
/// **One payload per field, named after the field**, so an assertion covers the
/// call site that renders *that* value: a corpus carrying a single shared
/// payload lets one escaped field stand in for every raw one beside it. `<b>`
/// is not in this listing's own vocabulary, so a `<b>` on a served page is the
/// writer's.
fn typed(field: &str) -> String {
    format!("<b>{field} & \"quoted\"</b>")
}

/// The same text as a page has to carry it: markup a person typed is **text**,
/// so it arrives as character references or it arrives as document. Spelled out
/// here from the encoding rather than borrowed from the renderer — an
/// expectation that called the code under test would agree with it by
/// construction.
fn as_text(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// --- the server under test -------------------------------------------------

/// Two roots and one child under the first, so a page can be wrong in a way a
/// single entity would hide.
async fn seeded_memory() -> Arc<dyn Memory> {
    let store = Arc::new(InMemoryMemory::booted());
    seed(store.as_ref()).await;
    store
}

/// The seeded roster plus one entity whose parent names nothing.
///
/// **It is staged past the guard because that is the only way this state
/// arises.** `add_entity` refuses a parent that is not there, in the fake and
/// in the real store alike, so a record holding one came from a hand edit
/// outside jojobot rather than from a verb.
async fn memory_with_an_orphan() -> Arc<dyn Memory> {
    let store = Arc::new(InMemoryMemory::booted());
    seed(store.as_ref()).await;
    store.past_the_guard(Entity {
        id: EntityId::new(EntityKind::THING, "sigma"),
        kind: EntityKind::THING,
        name: "Sigma".to_string(),
        aliases: Vec::new(),
        source: "the fixture roster".to_string(),
        crm: None,
        parent: Some(EntityId::new(EntityKind::PERSON, "ghost")),
        boot: Boot::default(),
    });
    store
}

async fn seed(store: &dyn Memory) {
    for new in [
        NewEntity::new(
            EntityId::new(EntityKind::PERSON, "alpha"),
            "Alpha",
            "the fixture roster",
        ),
        NewEntity::new(
            EntityId::new(EntityKind::PLACE, "shelbyville"),
            // A name is free text a person wrote, and this one bites: it is
            // rendered in a root's row on the index and in the record table on
            // its own page.
            format!("Shelbyville {}", typed("place-name")),
            "the fixture roster",
        ),
    ] {
        store.add_entity(new).await.expect("the roster is written");
    }
    let mut child = NewEntity::new(
        EntityId::new(EntityKind::TOPIC, "widgets"),
        "Widgets",
        "the fixture roster",
    );
    child.parent = Some(EntityId::new(EntityKind::PERSON, "alpha"));
    store.add_entity(child).await.expect("the child is written");

    // A fact held at the child, drawing a relation out of the subtree — so a
    // page can be asserted to follow a relation somewhere its own branch does
    // not reach.
    let mut fact = NewFact::about(
        EntityId::new(EntityKind::TOPIC, "widgets"),
        "The widget stall runs on Thursdays",
        Date::constant(2026, 3, 4),
    );
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(Edge::new(
        EdgeShape::Location,
        EntityId::new(EntityKind::PLACE, "shelbyville"),
    ));
    store.capture(fact).await.expect("the fact is written");

    // **Two records carrying one key each**, so the page has to fold them to
    // show what the thing is — which is the whole reason a thing's fields are
    // not the same as a record's.
    for (key, value, said) in [
        ("opens", "2026-03-05", "somebody wrote down when it opens"),
        ("pitch", "14", "and somebody else counted the pitches"),
    ] {
        let mut piece = NewFact::about(
            EntityId::new(EntityKind::TOPIC, "widgets"),
            said,
            Date::constant(2026, 3, 6),
        );
        piece.fields = [(key.to_string(), value.to_string())].into_iter().collect();
        store.capture(piece).await.expect("the field is written");
    }
    // **One key written twice, on two different days.** The fold shows what it
    // holds now; the writes behind it are what the history half of the page is
    // for, and a key written once cannot tell the two apart.
    for (value, said, day) in [
        ("40", "the stall took forty on the first day", 7),
        ("45", "and forty-five on the second", 8),
    ] {
        let mut takings = NewFact::about(
            EntityId::new(EntityKind::TOPIC, "widgets"),
            said,
            Date::constant(2026, 3, day),
        );
        takings.fields = [("takings".to_string(), value.to_string())]
            .into_iter()
            .collect();
        store.capture(takings).await.expect("the field is written");
    }
    // A type the fold makes the stall answer whole, which no one of its records
    // answers alone.
    store
        .declare_type(DeclaredType::new(
            "stall",
            vec![
                Field::new("opens", ValueType::Date),
                Field::new("pitch", ValueType::Number),
            ],
        ))
        .await
        .expect("the type is declared");

    // A claim and a portrait are free text too, and they land in different
    // parts of a node page — the fact table and the prose block.
    store
        .capture(NewFact::about(
            EntityId::new(EntityKind::TOPIC, "widgets"),
            typed("fact-content"),
            Date::constant(2026, 3, 5),
        ))
        .await
        .expect("the markup-bearing fact is written");
    store
        .set_prose(
            &EntityId::new(EntityKind::TOPIC, "widgets"),
            &format!("What the stall is for {}", typed("prose")),
        )
        .await
        .expect("the prose is written");
}

/// Every store the server is spawned over, kept so a test can read the records
/// back **after** a page has been served. Asserting that a page changed nothing
/// needs the store, not the page.
struct Board {
    memory: Arc<dyn Memory>,
    mailboxes: Arc<InMemoryMailboxes>,
    sessions: Arc<InMemorySessions>,
}

/// A bot with the three record types hanging off it: the mailbox it owns, one
/// message waiting in `new`, one run of it, and a beat in that run's
/// chronology.
async fn seeded_board_over(memory: Arc<dyn Memory>) -> Board {
    let bot = EntityId::new(EntityKind::BOT, "otto");
    memory
        .add_entity(NewEntity::new(bot.clone(), "Otto", "the fixture roster"))
        .await
        .expect("the bot is written");

    let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let box_name = MailboxName("otto".to_string());
    mailboxes
        .create_mailbox(&box_name, &bot, None)
        .await
        .expect("the box opens");
    mailboxes
        .post_message(NewMessage {
            mailbox: box_name,
            body: "The trail survey needs a second pair of eyes.".to_string(),
            subject: Some("A second pair of eyes".to_string()),
            sender: "bot:gamma".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
        })
        .await
        .expect("the message is posted");

    mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName("otto".to_string()),
            body: LONG_BODY.to_string(),
            subject: Some("The north section counts".to_string()),
            sender: "bot:gamma".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
        })
        .await
        .expect("the long message is posted");

    // Short, so the fold stays the long body's alone — this one is here for the
    // two free-text fields a message carries.
    mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName("otto".to_string()),
            body: typed("message-body"),
            subject: Some(typed("message-subject")),
            sender: "bot:gamma".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
        })
        .await
        .expect("the markup-bearing message is posted");

    let sessions = Arc::new(InMemorySessions::new());
    let session = sessions
        .begin(NewSession {
            timezone: None,
            bot: bot.clone(),
            sid: Sid("ot1x".to_string()),
            focus: "Reading the survey".to_string(),
            started_at: FIXED_INSTANT,
        })
        .await
        .expect("the run begins");
    sessions
        .append(
            &session.id,
            NewEntry::manual("Set out to read the survey end to end.", FIXED_INSTANT),
        )
        .await
        .expect("the beat is recorded");
    sessions
        .append(&session.id, NewEntry::manual(LONG_BEAT, FIXED_INSTANT))
        .await
        .expect("the long beat is recorded");
    sessions
        .append(
            &session.id,
            NewEntry::manual(typed("chronology-beat"), FIXED_INSTANT),
        )
        .await
        .expect("the markup-bearing beat is recorded");

    Board {
        memory,
        mailboxes,
        sessions,
    }
}

async fn seeded_board() -> Board {
    seeded_board_over(seeded_memory().await).await
}

async fn spawn_jojobot(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
) -> (SocketAddr, CancellationToken) {
    let (addr, ct, _) = spawn_jojobot_over(endpoints, allowed, idp, seeded_board().await).await;
    (addr, ct)
}

async fn spawn_jojobot_over(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
) -> (SocketAddr, CancellationToken, Board) {
    spawn_jojobot_at(endpoints, allowed, idp, board, "http").await
}

/// The same, with the **configured origin's scheme** as a knob. It decides one
/// thing this suite cares about — whether the session cookie is marked `Secure`
/// — and a real instance behind a tunnel is configured `https` while the process
/// itself still listens on plain TCP.
async fn spawn_jojobot_at(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
) -> (SocketAddr, CancellationToken, Board) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let indexed =
        Arc::new(IndexedMemory::new(board.memory.clone()).expect("the search index opens"));
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));

    let ui_cfg = UiConfig {
        client_id: CLIENT_ID.to_string(),
        base_url: format!("{scheme}://{addr}"),
    };
    let ui = Ui::new(
        &ui_cfg,
        endpoints,
        idp.validator_for(CLIENT_ID, allowed),
        reqwest::Client::new(),
    );

    let state = AppState {
        resource: format!("http://{addr}/mcp"),
        issuer: Some(support::ISS.to_string()),
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search,
        mailboxes: board.mailboxes.clone() as Arc<dyn Mailboxes>,
        sessions: board.sessions.clone() as Arc<dyn Sessions>,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: Some(Arc::new(ui)),
    };

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
            .unwrap();
    });
    (addr, ct, board)
}

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
        !body.contains("topic:widgets"),
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

/// Fetch a page as a logged-in browser.
async fn read(
    client: &reqwest::Client,
    jojobot: SocketAddr,
    path: &str,
    cookie: &str,
) -> reqwest::Response {
    client
        .get(format!("http://{jojobot}{path}"))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
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
/// A record's status (active · superseded · retracted) and a claim's standing
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

    for missing in ["/person:ghost/", "/not-a-handle/"] {
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
        .get(format!("http://{addr}/person:ghost/"))
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
        runs.contains("ot1x"),
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
        .sessions_of(&EntityId::new(EntityKind::BOT, "otto"))
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
        section(&bot, "sessions").contains("ot1x"),
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
