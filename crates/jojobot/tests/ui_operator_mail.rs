//! **The operator's mail page asks the one owner who the operator is and which
//! box is theirs**, so a record that names something that is not a person, or a
//! person who owns two boxes, is refused there exactly as it is over `/mcp`.
//!
//! Each refusal is paired, in the same case, with the healthy instance: the same
//! POSTs on the same kind of message land.

use std::net::SocketAddr;
use std::sync::Arc;

use jiff::civil::Date;
use jojobot::config::UiConfig;
use jojobot::ui::Ui;
use jojobot::{AppState, build_app};
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::mailbox::{MailboxName, Mailboxes, MessageId, NewMessage};
use jojobot_domain::memory::search::Search;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::session::Sessions;
use jojobot_domain::session::testing::InMemorySessions;
use tokio_util::sync::CancellationToken;

mod support;
use support::{browser, log_in, spawn_idp};

const CLIENT_ID: &str = "jojobot-ui";
const READER: &str = "sub-reader";
const FIXED_INSTANT: jiff::Timestamp = jiff::Timestamp::constant(1_780_000_000, 0);

/// A running server and the stores it was spawned over.
struct Served {
    addr: SocketAddr,
    ct: CancellationToken,
    mailboxes: Arc<InMemoryMailboxes>,
    cookie: String,
    /// The message in the operator's box, and the one in the bot's.
    to_homer: MessageId,
    to_gamma: MessageId,
}

/// An instance whose record names `operator`, with a person and a bot that each
/// own a box holding one message. `homer_boxes` is how many boxes the person owns.
async fn serve(operator: &str, homer_boxes: usize) -> Served {
    let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
    let homer = EntityId("person:homer".into());
    let gamma = EntityId("bot:gamma".into());
    for new in [
        NewEntity::new(homer.clone(), "Homer", "the fixture roster"),
        NewEntity::new(gamma.clone(), "Gamma", "the fixture roster"),
        NewEntity::new(
            EntityId("topic:instance".into()),
            "This instance",
            "the fixture roster",
        ),
    ] {
        memory.add_entity(new).await.expect("written");
    }
    let mut naming = NewFact::about(
        EntityId("topic:instance".into()),
        "who the operator is",
        Date::constant(2026, 3, 4),
    );
    naming.provenance = Provenance::Testimony;
    naming
        .fields
        .insert("operator".to_string(), operator.to_string());
    naming.sets.insert("operator".to_string());
    memory.capture(naming).await.expect("the operator is named");

    let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
    let homers = MailboxName::named_for(&homer);
    let gammas = MailboxName::named_for(&gamma);
    mailboxes
        .create_mailbox(&homers, &homer, None)
        .await
        .expect("the person's box opens");
    for extra in 1..homer_boxes {
        mailboxes
            .create_mailbox(&MailboxName(format!("homer-{extra}")), &homer, None)
            .await
            .expect("another box of the person's opens");
    }
    mailboxes
        .create_mailbox(&gammas, &gamma, None)
        .await
        .expect("the bot's box opens");
    let post = |mailbox: MailboxName, subject: &str| NewMessage {
        mailbox,
        body: format!("{subject}, in full"),
        subject: Some(subject.to_string()),
        sender: "bot:gamma".to_string(),
        sent_at: FIXED_INSTANT,
        in_reply_to: None,
        sender_mail_waiting_at_send: None,
        posted_by_session: None,
        supersedes: None,
    };
    let mut posted = Vec::new();
    for (mailbox, subject) in [(homers, "For the person"), (gammas, "For the bot")] {
        posted.push(
            mailboxes
                .post_message(post(mailbox, subject))
                .await
                .expect("posted")
                .written()
                .expect("the post landed")
                .id,
        );
    }
    let to_gamma = posted.pop().expect("two posts");
    let to_homer = posted.pop().expect("two posts");

    let idp = support::TestIdp::new();
    let (_, endpoints) = spawn_idp(idp.token_for(READER, CLIENT_ID)).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let indexed = Arc::new(IndexedMemory::new(memory.clone()).expect("the search index opens"));
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    let ui = Ui::new(
        &UiConfig {
            client_id: CLIENT_ID.to_string(),
            base_url: format!("http://{addr}"),
        },
        endpoints,
        idp.validator_for(CLIENT_ID, &[READER]),
        reqwest::Client::new(),
    );
    let state = AppState {
        apps: Default::default(),
        metrics: None,
        resource: format!("http://{addr}/mcp"),
        issuer: Some(support::ISS.to_string()),
        validator: None,
        metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
        memory: indexed.clone(),
        search,
        mailboxes: mailboxes.clone() as Arc<dyn Mailboxes>,
        sessions: Arc::new(InMemorySessions::new()) as Arc<dyn Sessions>,
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
    let cookie = log_in(&browser(), addr, "/").await;
    Served {
        addr,
        ct,
        mailboxes,
        cookie,
        to_homer,
        to_gamma,
    }
}

impl Served {
    /// A page, as the logged-in browser reads it.
    async fn page(&self, path: &str) -> String {
        browser()
            .get(format!("http://{}{path}", self.addr))
            .header(reqwest::header::COOKIE, &self.cookie)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    }

    /// One of the page's two POSTs, from this origin, with the session cookie.
    async fn post(&self, action: &str, id: &MessageId) -> reqwest::StatusCode {
        browser()
            .post(format!("http://{}/ui/mail/{action}", self.addr))
            .header(reqwest::header::COOKIE, &self.cookie)
            .header(reqwest::header::ORIGIN, format!("http://{}", self.addr))
            .form(&[("id", id.as_str()), ("note", "seen")])
            .send()
            .await
            .unwrap()
            .status()
    }

    /// The state a message is in, read from the store and not from a page.
    async fn state_of(&self, id: &MessageId) -> String {
        self.mailboxes
            .message_by_id(id)
            .await
            .expect("the board reads")
            .expect("the message exists")
            .state
            .to_string()
    }
}

/// **A record that names a bot as the operator opens no mail from the browser.**
/// Both POSTs on a message in the bot's box are refused and the message stays
/// new; the page of the handle the record names says it is not a person. Paired:
/// with a person named, the same two POSTs on that person's message land.
#[tokio::test]
async fn a_record_naming_a_bot_as_operator_opens_and_finishes_no_mail() {
    let bot_named = serve("bot:gamma", 1).await;
    for action in ["open", "processed"] {
        let status = bot_named.post(action, &bot_named.to_gamma).await;
        assert!(
            status.is_client_error(),
            "{action} on a bot's mail is refused when the record names the bot: {status}"
        );
        assert_eq!(bot_named.state_of(&bot_named.to_gamma).await, "new");
    }
    let page = bot_named.page("/bot:gamma/").await;
    assert!(
        page.contains("not a person"),
        "the page says why, in the MCP side's words: {page}"
    );
    let person_page = bot_named.page("/person:homer/").await;
    assert!(
        !person_page.contains("For the person"),
        "no box is shown while the operator is not a person: {person_page}"
    );
    bot_named.ct.cancel();

    let person_named = serve("person:homer", 1).await;
    let opened = person_named.post("open", &person_named.to_homer).await;
    assert!(opened.is_redirection(), "{opened}");
    assert_eq!(person_named.state_of(&person_named.to_homer).await, "read");
    let marked = person_named.post("processed", &person_named.to_homer).await;
    assert!(marked.is_redirection(), "{marked}");
    assert_eq!(
        person_named.state_of(&person_named.to_homer).await,
        "processed"
    );
    person_named.ct.cancel();
}

/// **A person who owns two boxes is damage, and the page does not pick one.** The
/// page names the damage and shows no box, and both POSTs are refused with the
/// message untouched. Paired: with one box they land.
#[tokio::test]
async fn an_operator_owning_two_boxes_is_damage_and_neither_is_picked() {
    let two = serve("person:homer", 2).await;
    let page = two.page("/person:homer/").await;
    let names: Vec<String> = two
        .mailboxes
        .list_mailboxes()
        .await
        .expect("the board reads")
        .into_iter()
        .filter(|held| held.owner.as_str() == "person:homer")
        .map(|held| held.name.as_str().to_string())
        .collect();
    assert!(
        page.contains(
            &jojobot_mcp::orientation::instance_zone::operator_owns_several_boxes(&names)
        ),
        "the page says the sentence the verbs say, naming both boxes: {page}"
    );
    assert!(
        !page.contains("For the person"),
        "the page does not pick a box: {page}"
    );
    for action in ["open", "processed"] {
        let status = two.post(action, &two.to_homer).await;
        assert!(
            !status.is_redirection() && !status.is_success(),
            "{action} is refused when the person owns two boxes: {status}"
        );
        assert_eq!(two.state_of(&two.to_homer).await, "new");
    }
    two.ct.cancel();

    let one = serve("person:homer", 1).await;
    let page = one.page("/person:homer/").await;
    assert!(page.contains("For the person"), "{page}");
    let opened = one.post("open", &one.to_homer).await;
    assert!(opened.is_redirection(), "{opened}");
    assert_eq!(one.state_of(&one.to_homer).await, "read");
    one.ct.cancel();
}
