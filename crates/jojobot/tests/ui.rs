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
use jojobot_domain::memory::mention;
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
///
/// **Wrapped in `Mentioning`, exactly as production wires it** (rule 205): the
/// server resolves mentions above the store and below the index before either
/// the MCP surface or the browser listing ever sees a record, and a test
/// store that skipped this layer would carry no mention at all — unreachable
/// before it was ever unwritten.
/// **A fact whose fields are the state it sets on its thing**, as a verb's
/// `sets` would make them: every key is also named in the setting bag, which
/// reaches the thing whether or not own fields do.
fn set_on(mut fact: NewFact) -> NewFact {
    fact.sets = fact.fields.keys().cloned().collect();
    fact
}

async fn seeded_memory() -> Arc<dyn Memory> {
    let store: Arc<dyn Memory> =
        Arc::new(mention::Mentioning::new(Arc::new(InMemoryMemory::booted())));
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
        folded_names: Vec::new(),
        id: EntityId("thing:sigma".into()),
        kind: EntityKind::THING,
        name: "Sigma".to_string(),
        aliases: Vec::new(),
        source: "the fixture roster".to_string(),
        crm: None,
        parent: Some(EntityId("person:contract-ghost".into())),
        boot: Boot::default(),
        merged_into: None,
        badge: None,
        archived: None,
    });
    store
}

/// The seeded roster with one root and one child archived, through the real
/// verb rather than a hand-built record — archiving is a write, not a shape
/// this store would otherwise produce.
async fn memory_with_an_archived_entity() -> Arc<dyn Memory> {
    let store = Arc::new(InMemoryMemory::booted());
    seed(store.as_ref()).await;
    store
        .archive_entity(
            &EntityId("place:shelbyville".into()),
            "a mistaken write",
            None,
        )
        .await
        .expect("archive_entity ok");
    store
        .archive_entity(&EntityId("topic:widgets".into()), "a mistaken write", None)
        .await
        .expect("archive_entity ok");
    store
}

async fn seed(store: &dyn Memory) {
    for new in [
        NewEntity::new(
            EntityId("person:alpha".into()),
            "Alpha",
            "the fixture roster",
        ),
        NewEntity::new(
            EntityId("place:shelbyville".into()),
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
        EntityId("topic:widgets".into()),
        "Widgets",
        "the fixture roster",
    );
    child.parent = Some(EntityId("person:alpha".into()));
    store.add_entity(child).await.expect("the child is written");

    // A fact held at the child, drawing a relation out of the subtree — so a
    // page can be asserted to follow a relation somewhere its own branch does
    // not reach.
    let mut fact = NewFact::about(
        EntityId("topic:widgets".into()),
        "The widget stall runs on Thursdays",
        Date::constant(2026, 3, 4),
    );
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(Edge::new(
        EdgeShape::Location,
        EntityId("place:shelbyville".into()),
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
            EntityId("topic:widgets".into()),
            said,
            Date::constant(2026, 3, 6),
        );
        piece.fields = [(key.to_string(), value.to_string())].into_iter().collect();
        piece.sets = [key.to_string()].into_iter().collect();
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
            EntityId("topic:widgets".into()),
            said,
            Date::constant(2026, 3, day),
        );
        takings.fields = [("takings".to_string(), value.to_string())]
            .into_iter()
            .collect();
        takings.sets = ["takings".to_string()].into_iter().collect();
        store.capture(takings).await.expect("the field is written");
    }
    // A type the fold makes the stall answer whole, which no one of its records
    // answers alone.
    store
        .declare_type(
            DeclaredType::new(
                "stall",
                vec![
                    Field::new("opens", ValueType::Date),
                    Field::new("pitch", ValueType::Number),
                ],
            ),
            None,
        )
        .await
        .expect("the type is declared");

    // A claim and a portrait are free text too, and they land in different
    // parts of a node page — the fact table and the prose block.
    store
        .capture(NewFact::about(
            EntityId("topic:widgets".into()),
            typed("fact-content"),
            Date::constant(2026, 3, 5),
        ))
        .await
        .expect("the markup-bearing fact is written");
    store
        .set_prose(
            &EntityId("topic:widgets".into()),
            &format!("What the stall is for {}", typed("prose")),
            None,
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
    let bot = EntityId("bot:otto".into());
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
            body: "The gorge inspection needs a second pair of eyes.".to_string(),
            subject: Some("A second pair of eyes".to_string()),
            sender: "bot:gamma".to_string(),
            sent_at: FIXED_INSTANT,
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
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
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
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
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("the markup-bearing message is posted");

    let sessions = Arc::new(InMemorySessions::new());
    let session = sessions
        .begin(NewSession {
            timezone: None,
            bot: bot.clone(),
            sid: Sid("pt1x".to_string()),
            focus: "Reading the survey".to_string(),
            started_at: FIXED_INSTANT,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("the run begins");
    sessions
        .append(
            &session.id,
            NewEntry::manual(
                "Set out to read the survey end to end.",
                FIXED_INSTANT,
                None,
            ),
        )
        .await
        .expect("the beat is recorded");
    sessions
        .append(
            &session.id,
            NewEntry::manual(LONG_BEAT, FIXED_INSTANT, None),
        )
        .await
        .expect("the long beat is recorded");
    sessions
        .append(
            &session.id,
            NewEntry::manual(typed("chronology-beat"), FIXED_INSTANT, None),
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
    spawn_jojobot_guarding(endpoints, allowed, idp, board, scheme, None).await
}

/// The same, with **`/mcp` guarded by a bearer validator** as well, which is how
/// a real instance runs: the UI and the MCP surface behind one issuer, each
/// checking its own credential. `None` leaves `/mcp` open, as every other case
/// in this file has it.
async fn spawn_jojobot_guarding(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
    mcp: Option<Arc<jojobot::auth::Validator>>,
) -> (SocketAddr, CancellationToken, Board) {
    spawn_jojobot_serving(endpoints, allowed, idp, board, scheme, mcp, "").await
}

/// The same, with **the path the configured base URL carries** as a knob. A
/// real instance is configured with the URL its people reach it at, which may
/// have a path, and a reverse proxy takes the path off before the request gets
/// here; this process serves at its root either way.
async fn spawn_jojobot_serving(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
    mcp: Option<Arc<jojobot::auth::Validator>>,
    base_path: &str,
) -> (SocketAddr, CancellationToken, Board) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let indexed =
        Arc::new(IndexedMemory::new(board.memory.clone()).expect("the search index opens"));
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));

    let ui_cfg = UiConfig {
        client_id: CLIENT_ID.to_string(),
        base_url: format!("{scheme}://{addr}{base_path}"),
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
        validator: mcp,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search,
        mailboxes: board.mailboxes.clone() as Arc<dyn Mailboxes>,
        sessions: board.sessions.clone() as Arc<dyn Sessions>,
        teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        requests: None,
        registry: Arc::new(jojobot_mcp::sid::SessionRegistry::new()),
        ui: Some(Arc::new(ui)),
        clock: jojobot_domain::clock::Clock::default(),
        push: None,
        quiet_after: None,
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
        over.find("mail-25") < over.find("mail-24"),
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
        over.find("Chronology of r025") < over.find("Chronology of r024"),
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
        fields.find("key-45") < fields.find("key-44"),
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

/// What the operator cases read back: the id of the one message left in the
/// operator's box, and the id of one left in an ordinary bot's.
struct OperatorMail {
    to_the_operator: jojobot_domain::mailbox::MessageId,
    to_a_bot: jojobot_domain::mailbox::MessageId,
}

const OPERATOR_BODY: &str = "the operator-only figure is 4242";
const OPERATOR_SUBJECT: &str = "A report for the operator";

/// The seeded board plus an operator: a person the instance's record names, a
/// second person who is not, and the operator's box with one message waiting in
/// `new`. The bot otto's box holds a message too, which is the control: the
/// operator's page must not carry it, and a bot's mail is not the operator's to
/// process.
async fn with_an_operators_mail(board: &Board) -> OperatorMail {
    let operator = EntityId("person:lisa".into());
    for new in [
        NewEntity::new(operator.clone(), "Lisa", "the fixture roster"),
        NewEntity::new(
            EntityId("person:milhouse".into()),
            "Milhouse",
            "the fixture roster",
        ),
        NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the fixture roster",
        ),
    ] {
        board.memory.add_entity(new).await.expect("written");
    }
    let mut naming = NewFact::about(
        EntityId("topic:instance".into()),
        "who the operator is",
        Date::constant(2026, 3, 4),
    );
    naming.provenance = Provenance::Testimony;
    naming
        .fields
        .insert("operator".to_string(), "person:lisa".to_string());
    naming.sets.insert("operator".to_string());
    board
        .memory
        .capture(naming)
        .await
        .expect("the operator is named");

    let box_name = MailboxName::named_for(&operator);
    board
        .mailboxes
        .create_mailbox(&box_name, &operator, None)
        .await
        .expect("the operator's box opens");
    let post = |mailbox: MailboxName, subject: &str, body: &str| NewMessage {
        mailbox,
        body: body.to_string(),
        subject: Some(subject.to_string()),
        sender: "bot:otto".to_string(),
        sent_at: FIXED_INSTANT,
        in_reply_to: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
        supersedes: None,
    };
    let to_the_operator = board
        .mailboxes
        .post_message(post(box_name, OPERATOR_SUBJECT, OPERATOR_BODY))
        .await
        .expect("posted")
        .written()
        .expect("the post landed")
        .id;
    let to_a_bot = board
        .mailboxes
        .post_message(post(
            MailboxName("otto".to_string()),
            "A note for a bot",
            "the kiln is relined",
        ))
        .await
        .expect("posted")
        .written()
        .expect("the post landed")
        .id;
    OperatorMail {
        to_the_operator,
        to_a_bot,
    }
}

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

/// The Open button's POST the way the page's own form sends it.
async fn open_in_the_ui(
    addr: SocketAddr,
    cookie: &str,
    id: &str,
    origin: Option<&str>,
) -> reqwest::Response {
    let mut request = browser()
        .post(format!("http://{addr}/ui/mail/open"))
        .header(reqwest::header::COOKIE, cookie)
        .form(&[("id", id)]);
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
        assert!(
            !page.contains(OPERATOR_SUBJECT)
                && !page.contains(mail.to_the_operator.as_str())
                && !page.contains("/ui/mail/"),
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

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let board = seeded_board_over(memory).await;
    let (addr, ct, _) = spawn_jojobot_over(endpoints, &[READER], &idp, board).await;
    let client = browser();
    let cookie = log_in(&client, addr, "/").await;

    for view in ["ready", "colleagues", "marked-things"] {
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
