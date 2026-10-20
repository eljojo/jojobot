//! **The server the browser listing is tested against, and the data it is
//! tested over**, shared by every integration binary that drives the UI.
//!
//! Moved out of `ui.rs` verbatim. Each binary compiles this separately and
//! reaches for a different part of it.

#![allow(dead_code)]

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

use super as support;
use super::browser;

pub const CLIENT_ID: &str = "jojobot-ui";
pub const READER: &str = "sub-reader";

/// A body past the budget the digest strategy declares, with a distinct opening
/// and a distinct last line — so a page can be asserted to carry both the
/// summary and the whole of it.
pub const LONG_BODY: &str = "The survey needs a second pair of eyes on the north section, because the \
counts taken there disagree with the ones from last season and nobody has said which of them is \
right. This closing sentence is the tail of the long body.";

/// The same, for a chronology beat.
pub const LONG_BEAT: &str = "Set out to reconcile the two counts and found that the disagreement is in \
the north section alone, which narrows it to one afternoon of records rather than the whole \
season. This closing sentence is the tail of the long beat.";

/// A fixed instant, so nothing here reads a clock.
pub const FIXED_INSTANT: jiff::Timestamp = jiff::Timestamp::constant(1_780_000_000, 0);

/// Markup somebody typed, in a value a page renders.
///
/// **One payload per field, named after the field**, so an assertion covers the
/// call site that renders *that* value: a corpus carrying a single shared
/// payload lets one escaped field stand in for every raw one beside it. `<b>`
/// is not in this listing's own vocabulary, so a `<b>` on a served page is the
/// writer's.
pub fn typed(field: &str) -> String {
    format!("<b>{field} & \"quoted\"</b>")
}

/// The same text as a page has to carry it: markup a person typed is **text**,
/// so it arrives as character references or it arrives as document. Spelled out
/// here from the encoding rather than borrowed from the renderer — an
/// expectation that called the code under test would agree with it by
/// construction.
pub fn as_text(raw: &str) -> String {
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
pub fn set_on(mut fact: NewFact) -> NewFact {
    fact.sets = fact.fields.keys().cloned().collect();
    fact
}

pub async fn seeded_memory() -> Arc<dyn Memory> {
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
pub async fn memory_with_an_orphan() -> Arc<dyn Memory> {
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
pub async fn memory_with_an_archived_entity() -> Arc<dyn Memory> {
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

pub async fn seed(store: &dyn Memory) {
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
pub struct Board {
    pub memory: Arc<dyn Memory>,
    pub mailboxes: Arc<InMemoryMailboxes>,
    pub sessions: Arc<InMemorySessions>,
}

/// A bot with the three record types hanging off it: the mailbox it owns, one
/// message waiting in `new`, one run of it, and a beat in that run's
/// chronology.
pub async fn seeded_board_over(memory: Arc<dyn Memory>) -> Board {
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

pub async fn seeded_board() -> Board {
    seeded_board_over(seeded_memory().await).await
}

pub async fn spawn_jojobot(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
) -> (SocketAddr, CancellationToken) {
    let (addr, ct, _) = spawn_jojobot_over(endpoints, allowed, idp, seeded_board().await).await;
    (addr, ct)
}

pub async fn spawn_jojobot_over(
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
pub async fn spawn_jojobot_at(
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
pub async fn spawn_jojobot_guarding(
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
pub async fn spawn_jojobot_serving(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
    mcp: Option<Arc<jojobot::auth::Validator>>,
    base_path: &str,
) -> (SocketAddr, CancellationToken, Board) {
    spawn_jojobot_serving_over(endpoints, allowed, idp, board, scheme, mcp, base_path, None).await
}

/// **A search that answers as the real one does and reports its memory index as
/// behind the store.** The state a server is in until a read has filled the index.
struct BehindSearch {
    inner: Arc<dyn Search>,
    behind: jojobot_domain::memory::search::Behind,
}

#[async_trait::async_trait]
impl Search for BehindSearch {
    async fn search(
        &self,
        query: &jojobot_domain::memory::search::SearchQuery,
    ) -> Result<Vec<jojobot_domain::memory::search::Hit>, jojobot_domain::memory::MemoryError> {
        self.inner.search(query).await
    }
    fn mail_coverage(&self) -> jojobot_domain::memory::search::Coverage {
        self.inner.mail_coverage()
    }
    fn memory_coverage(&self) -> jojobot_domain::memory::search::Coverage {
        jojobot_domain::memory::search::Coverage::Partial(self.behind)
    }
    fn session_coverage(&self) -> jojobot_domain::memory::search::Coverage {
        self.inner.session_coverage()
    }
}

/// The same, with **the search's memory index reported as behind the store** when
/// `behind` names how.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_jojobot_serving_over(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
    mcp: Option<Arc<jojobot::auth::Validator>>,
    base_path: &str,
    behind: Option<jojobot_domain::memory::search::Behind>,
) -> (SocketAddr, CancellationToken, Board) {
    let clock = jojobot_domain::clock::Clock::default();
    spawn_jojobot_clocked(
        endpoints, allowed, idp, board, scheme, mcp, base_path, behind, clock,
    )
    .await
}

/// The same server **on a day the case states**, so a page that reads today does
/// not read the machine's.
pub async fn spawn_jojobot_on(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    day: Date,
) -> (SocketAddr, CancellationToken, Board) {
    let clock = jojobot_domain::clock::Clock::stating(day);
    spawn_jojobot_clocked(
        endpoints, allowed, idp, board, "http", None, "", None, clock,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_jojobot_clocked(
    endpoints: IssuerEndpoints,
    allowed: &[&str],
    idp: &support::TestIdp,
    board: Board,
    scheme: &str,
    mcp: Option<Arc<jojobot::auth::Validator>>,
    base_path: &str,
    behind: Option<jojobot_domain::memory::search::Behind>,
    clock: jojobot_domain::clock::Clock,
) -> (SocketAddr, CancellationToken, Board) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let indexed =
        Arc::new(IndexedMemory::new(board.memory.clone()).expect("the search index opens"));
    let real: Arc<dyn Search> = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let search: Arc<dyn Search> = match behind {
        Some(behind) => Arc::new(BehindSearch {
            inner: real,
            behind,
        }),
        None => real,
    };

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
        apps: Default::default(),
        metrics: None,
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
        clock,
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

/// Fetch a page as a logged-in browser.
pub async fn read(
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

/// What the operator cases read back: the id of the one message left in the
/// operator's box, and the id of one left in an ordinary bot's.
pub struct OperatorMail {
    pub to_the_operator: jojobot_domain::mailbox::MessageId,
    pub to_a_bot: jojobot_domain::mailbox::MessageId,
}

pub const OPERATOR_BODY: &str = "the operator-only figure is 4242";
pub const OPERATOR_SUBJECT: &str = "A report for the operator";

/// The seeded board plus an operator: a person the instance's record names, a
/// second person who is not, and the operator's box with one message waiting in
/// `new`. The bot otto's box holds a message too, which is the control: the
/// operator's page must not carry it, and a bot's mail is not the operator's to
/// process.
pub async fn with_an_operators_mail(board: &Board) -> OperatorMail {
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

/// The Open button's POST the way the page's own form sends it.
pub async fn open_in_the_ui(
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
