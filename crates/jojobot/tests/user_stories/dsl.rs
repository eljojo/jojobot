//! The words a story is written in.
//!
//! Every call goes over the wire to a served jojobot, and every session is its
//! own client — a story's later sessions must work for a client that was not
//! there for the earlier ones.
//!
//! A write that comes back `blocked` fails the test. In a story a refused write
//! means the use case is not reachable, which is the thing being measured.

use std::net::SocketAddr;
use std::sync::Arc;

use jojobot::AppState;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::request::testing::InMemoryRequests;
use jojobot_domain::session::testing::InMemorySessions;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[path = "dsl/bags.rs"]
mod bags;
#[path = "dsl/blindable.rs"]
mod blindable;
pub use blindable::Blindable;
#[path = "dsl/leases.rs"]
mod leases;
use bags::bags;

type Client = RunningService<rmcp::RoleClient, ClientInfo>;

pub struct Story {
    addr: SocketAddr,
    ct: CancellationToken,
    bot: String,
    /// **The world this jojobot serves**, kept so a story can serve a SECOND
    /// jojobot over it — see [`Story::restarted_pretending_it_is`]. The three
    /// stores, raw: the index, the decorators and the registry are what a
    /// restart rebuilds, and rebuilding them is the point.
    store: Arc<dyn jojobot_domain::memory::Memory>,
    mail: Arc<InMemoryMailboxes>,
    runs: Arc<InMemorySessions>,
    /// **The images of the board, of the runs and of memory this server
    /// answers from** (decision log 292). A change made to `store`, `mail` or
    /// `runs` past them is not seen until [`Story::the_store_changed_underneath`],
    /// as a restart would.
    mail_image: Arc<jojobot_adapters::fold::mail::FoldedMailboxes>,
    runs_image: Arc<jojobot_adapters::fold::sessions::FoldedSessions>,
    memory_fold: Arc<jojobot_adapters::fold::Folded>,
    pub(super) memory_index: Arc<jojobot_adapters::search::IndexedMemory>,
    /// **The task that pushes the operator's mail**, when this server was set
    /// up to push. Kept so a story can stop it, as stopping the process would.
    runner: Option<Arc<jojobot::push::Runner>>,
    /// **The search index this server answers from**, so a story can make the
    /// next write to it fail, as a disk that fills does.
    index: Arc<jojobot_adapters::search::FullTextIndex>,
}

/// The subject the operator's browser logs in as. One reader, because the
/// listing is the operator's own window and nobody else's.
const READER: &str = "sub-the-operator";

/// The client id the listing registers with its issuer.
const UI_CLIENT: &str = "jojobot-ui";

/// **The ledger each wired store spends into**, found again by the store when a
/// restart serves over it. Held by a weak reference, so a store that is gone
/// takes its entry with it, and an address a later store reuses is never matched
/// to an earlier one's ledger.
static LEDGERS: std::sync::Mutex<Vec<WiredLedger>> = std::sync::Mutex::new(Vec::new());

/// A wired store, held weakly, and the ledger it spends into.
type WiredLedger = (
    std::sync::Weak<dyn jojobot_domain::memory::Memory>,
    Arc<InMemoryRequests>,
);

/// The ledger `store` spends into. A store that was not wired here (a
/// stand-in that can go away, say) spends into none, and its server gets an
/// empty ledger of its own.
fn ledger_of(store: &Arc<dyn jojobot_domain::memory::Memory>) -> Arc<InMemoryRequests> {
    let mut table = LEDGERS.lock().expect("the ledger table is poisoned");
    table.retain(|(held, _)| held.strong_count() > 0);
    let wanted = Arc::downgrade(store);
    table
        .iter()
        .find(|(held, _)| held.ptr_eq(&wanted))
        .map(|(_, ledger)| ledger.clone())
        .unwrap_or_else(InMemoryRequests::new)
}

/// **When a served jojobot thinks it is, and how long a run must be quiet
/// before its name can pick it up.** `None` is the lease an instance really
/// runs with.
struct Timing {
    clock: jojobot_domain::clock::Clock,
    quiet_after: Option<jiff::SignedDuration>,
}

impl Story {
    /// Serve a fresh jojobot whose memory store can be taken away — the same
    /// server every other story gets, wired over a [`Blindable`]. Hands back
    /// the store beside the story, because taking it away is the story's move.
    pub async fn begin_over_a_store_that_can_go_away(bot: &str) -> (Self, Arc<Blindable>) {
        let ledger = InMemoryRequests::new();
        let store = Arc::new(Blindable::new(ledger.clone()));
        // **The server reads the ledger the store spends into**, as it does over
        // every other store a story serves (see [`Story::wired`]).
        let memory: Arc<dyn jojobot_domain::memory::Memory> = store.clone();
        LEDGERS
            .lock()
            .expect("the ledger table is poisoned")
            .push((Arc::downgrade(&memory), ledger));
        let story = Self::serve(bot, memory).await;
        (story, store)
    }

    /// Serve a fresh jojobot and stand the bot up, the way an operator would.
    ///
    /// **Wired exactly as the binary wires it**, decorator included: the store,
    /// what this build supplies over it, then the index over both. A story that
    /// stood up the bare store would serve a jojobot poorer than the deployment
    /// it stands for, and would report the fixture's limits as the software's.
    pub async fn begin(bot: &str) -> Self {
        // **The store reads which bot a run belongs to from the runs this server
        // keeps**, as the real store reads it from its session table.
        let runs = Arc::new(InMemorySessions::new());
        let memory = Self::wired(InMemoryMemory::booted().with_run_bots(runs.clone()));
        Self::serve_over_runs(bot, memory, runs).await
    }

    /// **Serve a fresh jojobot over a list of skills other than the shipped one**,
    /// for the story that watches a scope no shipped skill has. Everything else
    /// is [`begin`](Story::begin)'s: the same stores, the same decorators, the
    /// same stand-up.
    pub async fn begin_with_skills(
        bot: &str,
        skills: &'static [jojobot_mcp::orientation::skills::Skill],
    ) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let story = Self::spawn_serving(
            bot,
            Self::wired(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Timing {
                clock: jojobot_domain::clock::Clock::default(),
                quiet_after: Some(jiff::SignedDuration::ZERO),
            },
            None,
            skills,
            Default::default(),
        )
        .await;
        story.stand_up_the_bot().await;
        story
    }

    /// **Serve a jojobot with one app registered**, wired as the binary wires its
    /// apps: the app's name, its one grant and the hash of its token, which is all
    /// the server keeps. Hands back the token beside the story, because sending it
    /// is the app's move. The thread the grant names is the story's to create,
    /// with the operator's own tools, before the app posts.
    pub async fn begin_with_an_app(
        bot: &str,
        app: &str,
        thread: &str,
        keys: &[&str],
    ) -> (Self, String) {
        let bare = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let token = jojobot_domain::app::mint_token();
        let spec = jojobot_domain::app::AppSpec {
            name: app.to_string(),
            operation: jojobot_domain::app::RECORD_DEPLOY_EVENT.to_string(),
            thread: thread.to_string(),
            kind: "event".to_string(),
            keys: keys.iter().map(|key| key.to_string()).collect(),
            revoked: false,
        };
        let principal = jojobot_domain::app::AppPrincipal::from_spec(
            &spec,
            &jojobot_domain::app::hash_token(&token),
        )
        .expect("the app's grant is one the server can hold");
        let story = Self::spawn_serving(
            bare,
            Self::wired(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Timing {
                clock: jojobot_domain::clock::Clock::default(),
                quiet_after: Some(jiff::SignedDuration::ZERO),
            },
            None,
            jojobot_mcp::orientation::skills::SKILLS,
            jojobot::app_api::Apps::new(vec![principal]),
        )
        .await;
        story.stand_up_the_bot().await;
        (story, token)
    }

    /// **An app's request to record a deploy event**, with the token it holds,
    /// the way an outside program sends it: over HTTP, to the app route, and not
    /// through the tool surface. Answers the status and the body.
    pub async fn app_posts(&self, token: &str, body: Value) -> (u16, Value) {
        let answer = reqwest::Client::new()
            .post(format!(
                "http://{}{}",
                self.addr,
                jojobot::app_api::DEPLOY_EVENTS_PATH
            ))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .expect("the app route answers");
        let status = answer.status().as_u16();
        let text = answer.text().await.expect("the answer is text");
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    /// **Serve a jojobot set up to push the operator's mail to a phone**, over a
    /// mail store that writes a push for every message that lands in a person's
    /// box. `notifier` is the service the pushes go to and `policy` how long a
    /// failed one is chased: a story that waits out the real hour would wait an
    /// hour.
    pub async fn begin_pushing(
        bot: &str,
        notifier: Arc<dyn jojobot_domain::push::Notifier>,
        policy: jojobot_domain::push::Policy,
    ) -> Self {
        let mail = Arc::new(InMemoryMailboxes::knowing_any_owner().pushing());
        let runs = Arc::new(InMemorySessions::new());
        let bare = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let story = Self::spawn(
            bare,
            Self::wired(InMemoryMemory::booted()),
            mail,
            runs,
            jojobot_domain::clock::Clock::default(),
            Some((notifier, policy)),
        )
        .await;
        story.stand_up_the_bot().await;
        story
    }

    /// **Stop pushing, as stopping the process would.** A push still pending
    /// stays in the outbox for whichever server starts next over these stores.
    pub fn stop_pushing(&self) {
        self.runner
            .as_ref()
            .expect("this story was not set up to push")
            .stop();
    }

    /// **A second server over the same stores, set up to push** — a restart of an
    /// instance that has a push service. It drains whatever the first one left
    /// pending as it starts.
    pub async fn restarted_pushing(
        &self,
        notifier: Arc<dyn jojobot_domain::push::Notifier>,
        policy: jojobot_domain::push::Policy,
    ) -> Self {
        Self::spawn(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            jojobot_domain::clock::Clock::default(),
            Some((notifier, policy)),
        )
        .await
    }

    /// **A fresh jojobot nobody has written to**, served for the identity every
    /// instance arrives holding. [`begin`](Story::begin) stands its bot up by
    /// writing as that identity, which leaves a run behind in every story; this
    /// one skips the stand-up, and a boot writes no run until the first write,
    /// so the index holds no session at all.
    pub async fn begin_with_nothing_written() -> Self {
        let mail = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let runs = Arc::new(InMemorySessions::new());
        let bot = jojobot_mcp::seed::DEFAULT_BOT
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        Self::spawn(
            bot,
            Self::wired(InMemoryMemory::booted()),
            mail,
            runs,
            jojobot_domain::clock::Clock::default(),
            None,
        )
        .await
    }

    /// The store, plus what this build supplies over it — everything short of
    /// `jojobot::wiring::assemble_memory`'s own `Folded` and `Mentioning`
    /// wraps, which `spawn` calls, exactly as `main.rs` does, so the two
    /// cannot drift.
    fn wired(store: InMemoryMemory) -> Arc<dyn jojobot_domain::memory::Memory> {
        // **Both halves are told the same set**, exactly as the binary wires
        // it: the layer above resolves supplied records into answers and the
        // store below sees them when its guard asks what exists.
        let supplied = jojobot_mcp::provisions();
        // **The store spends request ids into a ledger of its own**, as the real
        // store does in its own table, and the server reads the same ledger. A
        // restart serves over the same store, so it finds the same ledger.
        let ledger = InMemoryRequests::new();
        let memory: Arc<dyn jojobot_domain::memory::Memory> = Arc::new(Provisioned::new(
            store
                .with_requests(ledger.clone())
                .knowing(supplied.clone()),
            supplied,
        ));
        LEDGERS
            .lock()
            .expect("the ledger table is poisoned")
            .push((Arc::downgrade(&memory), ledger));
        memory
    }

    /// **Serve a jojobot on an instance an older build left behind** — a store
    /// already holding a kind the software shipped once and this build does
    /// not.
    ///
    /// The row goes into the STORE rather than through a verb, because that is
    /// how this state really arises: a previous binary wrote it, and what a
    /// story asks is what the next boot does about it. The store is loaded
    /// before it serves, so the retired kind is one this process could parse
    /// until the boot reconciles it away.
    pub async fn begin_on_an_instance_an_older_build_left(bot: &str, retired: &str) -> Self {
        let store = InMemoryMemory::new();
        jojobot_domain::memory::Memory::declare_kind(
            &store,
            retired,
            jojobot_domain::memory::types::Origin::Shipped,
            Vec::new(),
        )
        .await
        .expect("an older build declared its own kinds");
        store.boot();
        Self::serve(bot, Self::wired(store)).await
    }

    /// **Serve a jojobot over a store that already holds things**, the state of an
    /// instance an operator filled before the software started. The things go
    /// into the STORE rather than through a verb, with no box for any bot among
    /// them, because that is how this state arises: what a story asks is what
    /// the next start does about it.
    pub async fn begin_over_a_prefilled_store(
        bot: &str,
        prefill: Vec<jojobot_domain::memory::NewEntity>,
    ) -> Self {
        let store = InMemoryMemory::booted();
        for entity in prefill {
            jojobot_domain::memory::Memory::add_entity(&store, entity)
                .await
                .expect("the store takes what it was prefilled with")
                .written()
                .expect("nothing in the prefill collides");
        }
        Self::serve(bot, Self::wired(store)).await
    }

    /// **Serve a jojobot that is acting out a day**, the way an operator does
    /// by setting `JOJOBOT_TODAY` — nothing else about the instance differs,
    /// and no story call passes a date because of it.
    pub async fn begin_pretending_it_is(bot: &str, day: &str) -> Self {
        let clock = jojobot_domain::clock::Clock::stating(day.parse().expect("a story's day"));
        // **The store stamps on the same clock**, exactly as the binary wires
        // it: `when jojobot took this in` is the store's own stamp, and a
        // fixture that clocked only the handler could not see it move.
        Self::serve_on_clock(
            bot,
            Self::wired(InMemoryMemory::booted().on_clock(clock)),
            clock,
        )
        .await
    }

    /// **Serve a jojobot whose STORE stamps on a day a story can move.** The
    /// hand on the clock is handed back beside the story, so a story can let
    /// weeks pass between two writes and read how old what it holds is.
    pub async fn begin_on_a_store_clock_that_moves(
        bot: &str,
        day: &str,
    ) -> (Self, jojobot_domain::memory::testing::ClockHandle) {
        let clock = jojobot_domain::clock::Clock::stating(day.parse().expect("a story's day"));
        let memory = InMemoryMemory::booted().on_clock(clock);
        let hand = memory.clock_handle();
        (Self::serve(bot, Self::wired(memory)).await, hand)
    }

    /// **Serve a jojobot that runs with the lease an instance really has**: a
    /// run can be picked up by its name only after it has been quiet for 45
    /// minutes by the server's clock. The one story of that rule serves this.
    pub async fn begin_on_the_real_lease(bot: &str) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let story = Self::spawn_with(
            bot,
            Self::wired(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Timing {
                clock: jojobot_domain::clock::Clock::default(),
                quiet_after: None,
            },
            None,
        )
        .await;
        story.stand_up_the_bot().await;
        story
    }

    /// **Serve a jojobot on the real clock with a lease of its own length**, so
    /// a story can wait out a lease of seconds rather than 45 minutes. The real
    /// clock matters where a run must be quiet and still not swept: a stated
    /// far-off day sweeps every run it meets, and the quiet rule never runs.
    pub async fn begin_on_a_lease_of(bot: &str, lease: std::time::Duration) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        let story = Self::spawn_with(
            bot,
            Self::wired(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Timing {
                clock: jojobot_domain::clock::Clock::default(),
                quiet_after: Some(
                    jiff::SignedDuration::try_from(lease).expect("a lease is a duration"),
                ),
            },
            None,
        )
        .await;
        story.stand_up_the_bot().await;
        story
    }

    /// **The ledger of request ids this story's stores spend into.**
    pub fn requests(&self) -> Arc<InMemoryRequests> {
        ledger_of(&self.store)
    }

    /// **The next commit of the part called `part` fails once and stores
    /// nothing**, as a store that stops between two parts of one write. The part
    /// before it stands.
    pub fn fails_the_next_commit_of(&self, part: &str) {
        self.requests().fail_the_next_commit_of(part);
    }

    /// **Every commit of the part called `part` is refused by a rule**, with
    /// `words`, and stores nothing: a store that answers and says no, as against
    /// one that is down. The same call sent again is refused the same way.
    pub fn refuses_every_commit_of(&self, part: &str, words: &str) {
        self.requests().refuse_every_commit_of(part, words);
    }

    /// **The sessions store under this story's server**, for the one thing a
    /// story cannot do through the surface: begin a run that has been quiet for
    /// longer than the lease, without waiting for it.
    pub fn sessions_store(&self) -> Arc<InMemorySessions> {
        self.runs.clone()
    }

    /// **The memory store under this story's server**, for a fixture too big
    /// to write through the served surface: each served write costs a commit
    /// of the index, so a world of thousands of records is seeded here and a
    /// restart indexes it in one pass.
    pub fn memory_store(&self) -> Arc<dyn jojobot_domain::memory::Memory> {
        self.store.clone()
    }

    /// **Read the stores again, as a restart would.** A story that wrote to the
    /// raw stores past the server (`memory_store`, `sessions_store`, `mail_store`) changed
    /// something this server holds in memory and did not see; this is the one
    /// way to say that happened.
    pub async fn the_store_changed_underneath(&self) {
        self.runs_image.rebuild().await.expect("the runs load");
        self.mail_image.rebuild().await.expect("the board loads");
        jojobot::wiring::fill_memory(&self.memory_fold, &self.memory_index).await;
    }

    /// The mailbox store under this story's server, for the same reason.
    pub fn mail_store(&self) -> Arc<InMemoryMailboxes> {
        self.mail.clone()
    }

    /// **A second server over the same stores on the real clock**, whose index
    /// is built from what the stores now hold. The handles of the first server
    /// address nothing here.
    pub async fn restarted(&self) -> Self {
        Self::spawn(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            jojobot_domain::clock::Clock::default(),
            None,
        )
        .await
    }

    /// **The same world, served again on another day** — a restart with a
    /// different `JOJOBOT_TODAY`, which is what two sittings of one simulation
    /// really are.
    ///
    /// The stores are the ones this story already wrote to; the index, the
    /// handle registry and the clock are new, because a restart rebuilds them.
    /// **The old server keeps running and its handles are not this one's**: a
    /// session that carried one over is told it addresses nothing, exactly as
    /// it would across a real restart.
    pub async fn restarted_pretending_it_is(&self, day: &str) -> Self {
        let clock = jojobot_domain::clock::Clock::stating(day.parse().expect("a story's day"));
        Self::spawn(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            clock,
            None,
        )
        .await
    }

    /// **The same world served again on a clock that reads `after` past midnight
    /// of `day`**, so a story can ask a question the edge of an hour decides.
    pub async fn restarted_pretending_it_is_past_midnight(
        &self,
        day: &str,
        after: jiff::SignedDuration,
    ) -> Self {
        let clock = jojobot_domain::clock::Clock::Stated {
            day: day.parse().expect("a story's day"),
            began: jiff::Timestamp::now()
                .checked_sub(after)
                .expect("a moment a story can state"),
        };
        Self::spawn(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            clock,
            None,
        )
        .await
    }

    /// **The same world served again on the lease an instance really runs
    /// with, on another day.** What a run's key and its name do after the
    /// server has been quiet toward it for longer than the lease: the day is the
    /// server's own, so every run written before it reads as quiet.
    pub async fn restarted_on_the_real_lease_pretending_it_is(&self, day: &str) -> Self {
        let clock = jojobot_domain::clock::Clock::stating(day.parse().expect("a story's day"));
        Self::spawn_with(
            &self.bot,
            self.store.clone(),
            self.mail.clone(),
            self.runs.clone(),
            Timing {
                clock,
                quiet_after: None,
            },
            None,
        )
        .await
    }

    /// **The same world served again on a lease of its own length**, on the real
    /// clock: a restart that rebuilds the registry from the board, so a run an
    /// older build left keyless is known by its name, and a lease short enough
    /// to wait out.
    pub async fn restarted_on_a_lease_of(&self, lease: std::time::Duration) -> Self {
        self.restarted_on_a_lease_of_a_clock_ahead_by(lease, std::time::Duration::ZERO)
            .await
    }

    async fn serve(bot: &str, store: Arc<dyn jojobot_domain::memory::Memory>) -> Self {
        Self::serve_on_clock(bot, store, jojobot_domain::clock::Clock::default()).await
    }

    async fn serve_on_clock(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        clock: jojobot_domain::clock::Clock,
    ) -> Self {
        Self::serve_over_runs_on_clock(bot, store, Arc::new(InMemorySessions::new()), clock).await
    }

    async fn serve_over_runs(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        runs: Arc<InMemorySessions>,
    ) -> Self {
        Self::serve_over_runs_on_clock(bot, store, runs, jojobot_domain::clock::Clock::default())
            .await
    }

    async fn serve_over_runs_on_clock(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        runs: Arc<InMemorySessions>,
        clock: jojobot_domain::clock::Clock,
    ) -> Self {
        let bot = bot
            .strip_prefix("bot:")
            .expect("a bot handle carries its kind prefix");
        // **Mail and runs are made here rather than inside the server**, so a
        // restart can be served over the same three stores.
        let mail = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let story = Self::spawn(bot, store, mail, runs, clock, None).await;
        story.stand_up_the_bot().await;
        story
    }

    /// **One server over a world**, and nothing about the world's contents.
    /// Called for a first boot and again for a restart.
    ///
    /// **A run can be picked up by its name the moment it is written**, so a
    /// story that resumes does not wait out the lease. The one story that holds
    /// the rule itself serves with [`Story::begin_on_the_real_lease`].
    async fn spawn(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        mail_store: Arc<InMemoryMailboxes>,
        runs: Arc<InMemorySessions>,
        clock: jojobot_domain::clock::Clock,
        push: Option<(
            Arc<dyn jojobot_domain::push::Notifier>,
            jojobot_domain::push::Policy,
        )>,
    ) -> Self {
        Self::spawn_with(
            bot,
            store,
            mail_store,
            runs,
            Timing {
                clock,
                quiet_after: Some(jiff::SignedDuration::ZERO),
            },
            push,
        )
        .await
    }

    /// [`Story::spawn`] with the quiet period said: `None` is the lease an
    /// instance really runs with.
    async fn spawn_with(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        mail_store: Arc<InMemoryMailboxes>,
        runs: Arc<InMemorySessions>,
        timing: Timing,
        // **Where pushes to the operator go**, when this server is set up to
        // push: the sender, and how long a failed push is chased.
        push: Option<(
            Arc<dyn jojobot_domain::push::Notifier>,
            jojobot_domain::push::Policy,
        )>,
    ) -> Self {
        Self::spawn_serving(
            bot,
            store,
            mail_store,
            runs,
            timing,
            push,
            jojobot_mcp::orientation::skills::SKILLS,
            Default::default(),
        )
        .await
    }

    /// [`Story::spawn_with`] over a list of skills: the shipped one for every
    /// story but the one that watches a scope no shipped skill has.
    #[allow(clippy::too_many_arguments)]
    async fn spawn_serving(
        bot: &str,
        store: Arc<dyn jojobot_domain::memory::Memory>,
        mail_store: Arc<InMemoryMailboxes>,
        runs: Arc<InMemorySessions>,
        timing: Timing,
        push: Option<(
            Arc<dyn jojobot_domain::push::Notifier>,
            jojobot_domain::push::Policy,
        )>,
        skills: &'static [jojobot_mcp::orientation::skills::Skill],
        apps: Arc<jojobot::app_api::Apps>,
    ) -> Self {
        let Timing { clock, quiet_after } = timing;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let store_for_restart = store.clone();
        let requests = ledger_of(&store);
        mail_store.attach_requests(requests.clone());
        runs.attach_requests(requests.clone());
        // **Wrapped in the same two-stage decorator assembly `main.rs`
        // calls** — `jojobot::wiring`'s own doc says why it is two stages
        // and not one. A fixture that indexed the bare stores would serve a
        // jojobot whose journal and mail no rename ever reached, poorer than
        // the deployment this stands for and silently so; calling the same
        // functions the binary calls is what stops that from being possible
        // to write by accident.
        // **The kinds are declared on the bare store, before the image is
        // filled, as `open_provisioned` does for the binary.** The stories'
        // stores are fakes that open with none. Declared after the fill, each
        // declaration changed the store under a filled image and cost a
        // second fill on every started or restarted Story.
        let _ = jojobot_mcp::seed::ensure_kinds(&store).await;
        let (folded, indexed) = jojobot::wiring::assemble_memory(store).expect("index opens");
        jojobot::wiring::fill_memory(&folded, &indexed).await;
        let indexed_for_seed = indexed.clone();
        let index = indexed.index();
        let (memory_fold, memory_index) = (folded.clone(), indexed.clone());
        let wired = jojobot::wiring::assemble_ports(
            indexed.clone(),
            mail_store.clone(),
            runs.clone(),
            runs.write_stamp(),
            // **The server's own clock, as the binary wires it**: a run's history
            // is stamped on the day the server is acting out.
            clock,
        );
        // **Mail and runs are held in memory here exactly as the binary holds
        // them**, filled before anything is served, so a story reads through the
        // image and not past it.
        wired.sessions_image.rebuild().await.expect("the runs load");
        wired.mail_image.rebuild().await.expect("the board loads");
        let (mail_image, runs_image) = (wired.mail_image.clone(), wired.sessions_image.clone());
        let sessions_mentioned = wired.sessions.clone();
        let search = wired.search;
        let boxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> = wired.mailboxes;
        let boxes_for_seed = boxes.clone();
        // **The startup passes the binary runs, by the same function** —
        // the handle registry filled from the board, and the default identity,
        // the instance record and the shipped types seeded. A restart inherits a
        // store that already holds runs from an earlier spawn, so the registry
        // must be filled as a real restart fills it. Run before anything serves,
        // as the binary does.
        let seed_memory: Arc<dyn jojobot_domain::memory::Memory> = indexed_for_seed;
        let registry = jojobot::wiring::run_startup_passes(
            &sessions_mentioned,
            &seed_memory,
            &boxes_for_seed,
            &clock,
        )
        .await;
        // **The binary sends the release notes after its listener is open, in a
        // task of its own.** A story is deterministic, so it sends them here and
        // waits, which leaves every box as a started binary leaves it a moment
        // later.
        jojobot_mcp::release_notes::send_unsent_notes(&seed_memory, &boxes_for_seed, &clock).await;
        // **The operator's own window is served too, over the same state.**
        // A story that could not open a page could not tell whether what it
        // wrote is visible to the one reader who does not speak MCP — and every
        // story before this one ran against a server with no page at all, so
        // nothing it renders was ever exercised by a use case.
        let idp = crate::support::TestIdp::new();
        let (_, endpoints) = crate::support::spawn_idp(idp.token_for(READER, UI_CLIENT)).await;
        let ui = jojobot::ui::Ui::new(
            &jojobot::config::UiConfig {
                client_id: UI_CLIENT.to_string(),
                base_url: format!("http://{addr}"),
            },
            endpoints,
            idp.validator_for(UI_CLIENT, &[READER]),
            reqwest::Client::new(),
        );
        let runner = push.map(|(notifier, policy)| {
            Arc::new(jojobot::push::Runner::start(
                mail_store.outbox(),
                boxes.clone(),
                notifier,
                policy,
            ))
        });
        let push: Option<Arc<dyn jojobot_domain::push::Nudge>> = runner
            .clone()
            .map(|runner| runner as Arc<dyn jojobot_domain::push::Nudge>);
        let state = AppState {
            apps,
            metrics: None,
            resource: format!("http://{addr}/mcp"),
            issuer: Some(crate::support::ISS.to_string()),
            validator: None,
            metadata_url: format!("http://{addr}/.well-known/oauth-protected-resource"),
            memory: indexed.clone(),
            search,
            mailboxes: boxes,
            sessions: sessions_mentioned,
            teachings: {
                let ledger = Arc::new(jojobot_adapters::fold::teachings::FoldedTeachings::new(
                    Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
                ));
                ledger.rebuild().await.expect("the ledger loads");
                ledger
            },
            requests: Some(requests),
            registry,
            ui: Some(Arc::new(ui)),
            // The shape a story runs against is the one a fresh instance
            // serves: an operator who set nothing gets both lines.
            clock,
            push,
            quiet_after,
        };
        let ct = CancellationToken::new();
        let app = jojobot::build_app_serving(state, ct.child_token(), skills);
        let shutdown = ct.clone();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await
                .unwrap();
        });

        Self {
            addr,
            ct,
            bot: bot.to_string(),
            store: store_for_restart,
            mail: mail_store,
            runs,
            mail_image,
            runs_image,
            memory_fold,
            memory_index,
            runner,
            index,
        }
    }

    /// **The search index cannot take the next write.** The stores keep
    /// answering; only the projection of them fails once, so a verb that
    /// reindexes after its store write commits and then cannot refresh.
    pub fn the_search_index_fails_its_next_commit(&self) {
        self.index.fail_next_commit();
    }

    /// **Stand the story's own bot up**, once, by the identity every jojobot
    /// arrives holding.
    async fn stand_up_the_bot(&self) {
        let story = self;
        let bot = self.bot.as_str();
        // The bot exists before anything boots as it; its box opens with it.
        //
        // **Created BY the default identity**, because a memory write carries
        // one. This is the real bootstrap and not a fixture trick: every
        // jojobot arrives with `assistant`, so the first act on a fresh server
        // is somebody booting as it and standing up whoever else is needed.
        let client = story.connect().await;
        let booted = call(
            &client,
            "start_here",
            json!({"bot": jojobot_mcp::seed::DEFAULT_BOT, "brief": true}),
        )
        .await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("the default identity must boot: {booted}"))
            .to_string();
        let made = call(
            &client,
            "add_entity",
            json!({
                "kind": "bot", "handle": bot, "name": bot,
                "source": "user-named", "sid": sid,
            }),
        )
        .await;
        assert_ne!(made["status"], "blocked", "the story's bot: {made}");
        client.cancel().await.unwrap();
    }

    /// **The door itself**, for the one story that is about opening it.
    ///
    /// Every other story starts after the handshake, because [`connect`] makes
    /// one and hands back a client. A story about a client that CANNOT open
    /// has to knock on the same jojobot by hand, which is what this is for —
    /// and it is the same server, so what the refusal says and what a working
    /// session does are one instance answering.
    pub fn door(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    async fn connect(&self) -> Client {
        let transport =
            StreamableHttpClientTransport::from_uri(format!("http://{}/mcp", self.addr));
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("user-story", "0.0.1"),
        )
        .serve(transport)
        .await
        .unwrap()
    }

    /// **What the mail store holds in every box a handle owns**, counted from
    /// the store itself and not from any verb: `(new, read, processed)` summed.
    /// A story uses it for the claim a verb cannot make about itself, that a
    /// refusal moved nothing and that a post landed.
    pub async fn mail_held_by(&self, owner: &str) -> (usize, usize, usize) {
        use jojobot_domain::mailbox::Mailboxes;
        let boxes = self.mail.list_mailboxes().await.expect("the board reads");
        boxes
            .iter()
            .filter(|held| held.owner.as_str() == owner)
            .fold((0, 0, 0), |(new, read, done), held| {
                (
                    new + held.counts.new,
                    read + held.counts.read,
                    done + held.counts.processed,
                )
            })
    }

    /// **How many messages a person's box holds in each state**, `(new, read,
    /// processed)`, read off the box's own page the way its owner reads it.
    /// A story uses it for what a verb cannot say of itself: that a refusal
    /// left the box as it was and that a post landed. A box nobody has posted to
    /// has no section on the page and counts as empty.
    pub async fn box_counts_on_the_page(&self, owner: &str) -> (usize, usize, usize) {
        let page = self.page(owner).await;
        let raw = page.raw();
        if !raw.contains("<h2>Mailbox ") {
            return (0, 0, 0);
        }
        let end = raw
            .find(" processed.</p>")
            .unwrap_or_else(|| panic!("the box section carries no counts: {raw}"));
        let start = raw[..end]
            .rfind("<p>")
            .unwrap_or_else(|| panic!("the box counts are not in a paragraph: {raw}"))
            + 3;
        let counts: Vec<usize> = raw[start..end]
            .split(", ")
            .map(|part| {
                part.split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("a box count is not a number: {part:?} in {raw}"))
            })
            .collect();
        match counts[..] {
            [new, read, processed] => (new, read, processed),
            _ => panic!("the box counts are not three numbers: {counts:?} in {raw}"),
        }
    }

    /// **What a person's box holds, oldest first, as `(subject, body)`**, read
    /// from the mail store itself the way the operator's own page reads it. No
    /// bot has a verb that does this, so the story reaches past the surface, as
    /// the operator does.
    pub async fn operators_mail(&self, owner: &str) -> Vec<(String, String)> {
        use jojobot_domain::mailbox::Mailboxes;
        let boxes: Vec<_> = self
            .mail
            .list_mailboxes()
            .await
            .expect("the board reads")
            .into_iter()
            .filter(|held| held.owner.as_str() == owner)
            .map(|held| held.name)
            .collect();
        let mut held: Vec<_> = self
            .mail
            .scan_messages()
            .await
            .expect("the store scans")
            .into_iter()
            .filter(|message| boxes.contains(&message.mailbox))
            .collect();
        held.sort_by_key(|message| message.sent_at);
        held.into_iter()
            .map(|message| (message.subject.unwrap_or_default(), message.body))
            .collect()
    }

    /// **Give a bot a second box, straight in the store.** No verb on the
    /// served surface does this: a box opens with its bot. It is the damage
    /// every read of a box is entitled to refuse over, planted the only way it
    /// can arrive.
    pub async fn plant_a_second_box(&self, bot: &str, name: &str) {
        use jojobot_domain::mailbox::Mailboxes;
        let written = self
            .mail_image
            .create_mailbox(
                &jojobot_domain::mailbox::MailboxName(name.to_string()),
                &jojobot_domain::memory::EntityId(bot.to_string()),
                None,
            )
            .await
            .expect("the store writes it");
        assert!(
            matches!(written, jojobot_domain::mailbox::Guarded::Written(_)),
            "the second box {name:?} was never opened, so there is no damage to meet"
        );
    }

    /// **Do the operator's act on a message: submit the Mark processed form on
    /// their page.** The form is read off the page as served, found by the id it
    /// carries and by its button, and posted to the action it names, from this
    /// server's own origin. The operator's page is how a person marks mail
    /// processed, and no verb does it for a message in a person's box.
    pub async fn mark_processed_on_the_page(&self, owner: &str, id: &str) {
        let path = format!("/{owner}/");
        let client = crate::support::browser();
        let cookie = crate::support::log_in(&client, self.addr, &path).await;
        let page = client
            .get(format!("http://{}{path}", self.addr))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("the page answers")
            .text()
            .await
            .expect("the page is text");
        let marker = format!("name=\"id\" value=\"{id}\"");
        let form = page
            .split("<form ")
            .find(|form| form.contains(&marker) && form.contains("Mark processed"))
            .unwrap_or_else(|| panic!("the page has no Mark processed form for {id}: {page}"));
        let action = form
            .split("action=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("the form names no action: {form}"));
        let marked = client
            .post(format!("http://{}{action}", self.addr))
            .header(reqwest::header::COOKIE, &cookie)
            .header(reqwest::header::ORIGIN, format!("http://{}", self.addr))
            .form(&[("id", id), ("note", "")])
            .send()
            .await
            .expect("the form answers");
        assert!(
            marked.status().is_redirection(),
            "the page's own Mark processed form was not taken: {}",
            marked.status()
        );
    }

    /// **Retire a message in the store itself**, past the page: for a story
    /// whose subject is the mail rail and not the operator's act. A story about
    /// what the operator does uses [`Story::mark_processed_on_the_page`].
    pub async fn retire_in_the_store(&self, id: &str) {
        use jojobot_domain::mailbox::Mailboxes;
        let id = jojobot_domain::mailbox::MessageId(id.to_string());
        let filed_in = self
            .mail_image
            .message_by_id(&id)
            .await
            .expect("the store reads the message")
            .expect("the message is filed")
            .mailbox;
        self.mail_image
            .mark_processed(
                &id,
                &filed_in,
                None,
                jojobot_domain::mailbox::Acting::anyone_anytime(),
            )
            .await
            .expect("the store retires it");
    }

    /// **Open the operator's own page for a handle, the way a browser does.**
    ///
    /// The whole login runs — the gate turns the browser away, the issuer
    /// vouches for the reader, the callback opens a session — because a page
    /// reached any other way is not the page the operator reads. It takes no
    /// `sid` and starts no
    /// run: looking through the window is not a session, and nothing on this
    /// path may change what a bot sees.
    pub async fn page(&self, handle: &str) -> Answer {
        self.window(&format!("/{handle}/")).await
    }

    /// **Follow a link the page itself carried**, the way clicking one does.
    ///
    /// A story that built the address instead would assert against the URL the
    /// page was supposed to have rather than the one it has: the way in is
    /// part of what the page must get right, so the story reads the link off
    /// the page — see [`Answer::link_to`] — and hands it back here.
    ///
    /// Relative, because that is what a page carries and what a browser
    /// resolves against the page it is on.
    pub async fn follow(&self, href: &str) -> Answer {
        let path = if href.starts_with('/') {
            href.to_string()
        } else {
            format!("/{href}")
        };
        self.window(&path).await
    }

    /// **Where the server sends a browser that asked for `path`**, as the
    /// `Location` it answers with, or none when it answers the page itself.
    pub async fn redirect_of(&self, path: &str) -> Option<String> {
        let client = crate::support::browser();
        let cookie = crate::support::log_in(&client, self.addr, path).await;
        let response = client
            .get(format!("http://{}{path}", self.addr))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("the server answers");
        response
            .status()
            .is_redirection()
            .then(|| crate::support::location(&response))
    }

    async fn window(&self, path: &str) -> Answer {
        let path = path.to_string();
        let client = crate::support::browser();
        let cookie = crate::support::log_in(&client, self.addr, &path).await;
        let body = client
            .get(format!("http://{}{path}", self.addr))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("the listing answers")
            .text()
            .await
            .expect("the page is text");
        Answer {
            what: format!("the page at {path}"),
            body,
        }
    }

    /// **A run of an earlier server, carried to this one by its key**: a new
    /// connection that sends the run's `sid` and boots nothing, which is how a
    /// client that kept its key across a restart goes on.
    pub async fn session_carried_from(&self, earlier: &Session) -> Session {
        Session {
            client: self.connect().await,
            sid: earlier.sid.clone(),
            run: earlier.run.clone(),
        }
    }

    /// A new session, on its own connection.
    pub async fn session(&self) -> Session {
        let client = self.connect().await;
        let booted = call(
            &client,
            "start_here",
            json!({"bot": self.bot, "brief": true}),
        )
        .await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Session { client, sid, run }
    }

    /// **The same run on a connection of its own**: the `sid` of `session`
    /// with a fresh client, as a real client opens one for each tool call. The
    /// server builds a handler for it, so two of these share what the process
    /// shares and nothing else.
    pub async fn on_a_fresh_connection(&self, session: &Session) -> Session {
        Session {
            client: self.connect().await,
            sid: session.sid.clone(),
            run: session.run.clone(),
        }
    }

    /// **A new session that works in a named zone**, on its own connection.
    ///
    /// `resume` answers the choice a second boot of one bot is handed, which is
    /// what lets one story hold two runs in two zones at once.
    pub async fn session_in(&self, zone: &str, resume: Option<&str>) -> Session {
        let client = self.connect().await;
        let mut args = json!({"bot": self.bot, "brief": true, "timezone": zone});
        if let Some(answer) = resume {
            args["resume"] = json!(answer);
        }
        let booted = call(&client, "start_here", args).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot in {zone} handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Session { client, sid, run }
    }

    /// **A new session that sends no zone**, with its boot answer, on its own
    /// connection. `resume` answers the choice a second boot of one bot is
    /// handed. The answer is returned because what it says about the zone is the
    /// point of a story that boots this way.
    pub async fn boot_sending_no_zone(&self, resume: Option<&str>) -> (Value, Session) {
        let client = self.connect().await;
        let mut args = json!({"bot": self.bot, "brief": true});
        if let Some(answer) = resume {
            args["resume"] = json!(answer);
        }
        let booted = call(&client, "start_here", args).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot with no zone handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (booted, Session { client, sid, run })
    }

    /// **A new session that says which DAY it is working in**, on its own
    /// connection — the run that is not happening now.
    ///
    /// `resume` answers the choice a second boot of one bot is handed, exactly
    /// as it does for a run in a named zone.
    pub async fn session_on(&self, day: &str, resume: Option<&str>) -> Session {
        let client = self.connect().await;
        let mut args = json!({"bot": self.bot, "brief": true, "today": day});
        if let Some(answer) = resume {
            args["resume"] = json!(answer);
        }
        let booted = call(&client, "start_here", args).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot on {day} handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Session { client, sid, run }
    }

    /// A new session's own boot, whole — the essay included. `.session()`
    /// takes `brief` on every other story, because they act after booting;
    /// this exists for a story whose whole point is what a full boot itself
    /// hands over, before anything else is asked of it.
    pub async fn full_boot(&self) -> (Value, Session) {
        let client = self.connect().await;
        let booted = call(&client, "start_here", json!({"bot": self.bot})).await;
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (booted, Session { client, sid, run })
    }

    /// A session of ANOTHER bot on this same server — the other side of a
    /// dispatch. Its own connection, which never met the first bot's.
    pub async fn as_bot(&self, bot: &str) -> Session {
        let bot = bot.strip_prefix("bot:").unwrap_or(bot);
        let client = self.connect().await;
        let booted = call(&client, "start_here", json!({"bot": bot, "brief": true})).await;
        // **A bot with a run already in flight is handed the choice rather than
        // a handle**, and the identity the software ships always has one: the
        // fixture's own bootstrap booted as it. A story dispatching to a
        // colleague wants a run of its own, so it answers `new` — which closes
        // nothing that was already open.
        let booted = match booted["session"]["sid"].as_str() {
            Some(_) => booted,
            None => {
                call(
                    &client,
                    "start_here",
                    json!({"bot": bot, "brief": true, "resume": "new"}),
                )
                .await
            }
        };
        let sid = booted["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("boot handed back no handle: {booted}"))
            .to_string();
        let run = booted["session"]["run"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Session { client, sid, run }
    }

    /// **Any verb, before there is a session to make it from.**
    ///
    /// [`Session::call`] rides a `sid`; this is the same escape hatch one step
    /// earlier, for the calls a caller makes before it holds an identity — the
    /// door itself, and a liveness probe. Nothing is injected into `args`,
    /// because the point of these calls is what they do without a session.
    ///
    /// Hands back the session when the answer carried a `sid`, and none when
    /// it did not. **That is a real distinction and not a convenience**: a boot
    /// with a run worth picking up answers with the choice and no handle, so a
    /// story asserting the offer has to be able to see the absence.
    pub async fn call(&self, tool: &str, args: Value) -> (Answer, Option<Session>) {
        let client = self.connect().await;
        let body = call(&client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{tool} was refused, so this part of the story is not reachable: {body}"
        );
        let answer = Answer {
            what: format!("the answer from {tool}"),
            body: body.to_string(),
        };
        match body["session"]["sid"].as_str() {
            Some(sid) => {
                let sid = sid.to_string();
                let run = body["session"]["run"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                (answer, Some(Session { client, sid, run }))
            }
            None => {
                client.cancel().await.unwrap();
                (answer, None)
            }
        }
    }

    /// **A call made before there is a session, whose answer may be either**:
    /// for a story about two callers asking for one thing, where which of them
    /// is refused is the very thing under test. Hands back the answer, and the
    /// session when the answer carried a `sid`.
    pub async fn call_either(&self, tool: &str, args: Value) -> (Answer, Option<Session>) {
        let client = self.connect().await;
        let body = call(&client, tool, args).await;
        let answer = Answer {
            what: format!("the answer from {tool}"),
            body: body.to_string(),
        };
        match body["session"]["sid"].as_str() {
            Some(sid) => {
                let sid = sid.to_string();
                let run = body["session"]["run"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                (answer, Some(Session { client, sid, run }))
            }
            None => {
                client.cancel().await.unwrap();
                (answer, None)
            }
        }
    }

    /// **A call the door refuses, made before there is a session**: the
    /// counterpart of [`Story::call`] for a story that expects `blocked`. It
    /// fails on the day the call starts landing.
    pub async fn refused_at_the_door(&self, tool: &str, args: Value) -> Answer {
        let client = self.connect().await;
        let body = call(&client, tool, args).await;
        client.cancel().await.unwrap();
        assert_eq!(
            body["status"], "blocked",
            "{tool} was accepted — the gap is closed, so flip this assertion: {body}"
        );
        Answer {
            what: format!("the refusal from {tool}"),
            body: body.to_string(),
        }
    }

    /// Fetch a shipped procedure by name, through the same door a boot uses.
    /// No bot and no session: reading a skill starts nothing.
    pub async fn skill(&self, name: &str) -> Value {
        let client = self.connect().await;
        let body = call(&client, "start_here", json!({"skill": name})).await;
        client.cancel().await.unwrap();
        body
    }

    pub async fn finish(self) {
        self.ct.cancel();
    }
}

pub struct Session {
    client: Client,
    /// The run's key: what rides every call.
    sid: String,
    /// The run's public name: what records carry and a resume names.
    run: String,
}

impl Session {
    /// **The handle this session carries — unless the story named one.**
    ///
    /// The default does not move: every story here leaves `sid` out and gets
    /// this session's, and a harness that made them pass one would be a
    /// rewrite of all of them for nothing.
    ///
    /// What it must ALSO do is send the handle a story picked, because a
    /// caller sending a handle jojobot no longer holds is behaviour with a
    /// guard behind it, and a harness that overwrites the argument puts that
    /// use case out of a story's reach — rule 152, arriving through an
    /// argument rather than through a verb. **One rule, stated once: if the
    /// story put `sid` in the arguments, it stands exactly as written**, `null`
    /// included, which is how a story sends no handle at all. A named method
    /// per verb would satisfy 152's letter and rebuild the thing it exists to
    /// catch.
    fn riding(&self, args: &mut Value) {
        if args.get("sid").is_none() {
            args["sid"] = self.sid.clone().into();
        }
    }

    async fn write(&self, what: &str, tool: &str, mut args: Value) -> Value {
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{what} was refused, so this part of the story is not reachable: {body}"
        );
        body
    }

    async fn read(&self, what: String, tool: &str, mut args: Value) -> Answer {
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        Answer {
            what,
            body: body.to_string(),
        }
    }

    /// **Any verb, any arguments — the success-path twin of [`Session::refused`].**
    ///
    /// A story reaches the whole served surface through this one method, so a
    /// capability can be exercised the day it ships rather than the day
    /// somebody writes an adapter for it here. The named methods below are for
    /// the moves stories make constantly and earn their place by being used
    /// everywhere; **a verb does not get one just for existing** — that is the
    /// habit that let six verbs and fourteen arguments go unexercised while
    /// every one of them was reachable over the wire.
    ///
    /// The `sid` rides along as it does on every other call, and a story that
    /// names one sends that one — see [`riding`]. A `blocked`
    /// answer fails the beat, for the reason this file's header gives: in a
    /// story a refusal means the use case is not reachable, and [`refused`] is
    /// where that is the expected answer.
    ///
    /// [`refused`]: Session::refused
    /// [`riding`]: Session::riding
    pub async fn call(&self, tool: &str, args: Value) -> Answer {
        let mut args = args;
        self.riding(&mut args);
        let body = call(&self.client, tool, args).await;
        assert_ne!(
            body["status"], "blocked",
            "{tool} was refused, so this part of the story is not reachable: {body}"
        );
        Answer {
            what: format!("the answer from {tool}"),
            body: body.to_string(),
        }
    }

    /// **How many characters a call's answer was served in**, whatever it
    /// was. The one way a story gets an answer past the ceiling without
    /// failing, for the case that exists to measure the verbs at their widest.
    pub async fn answer_size(&self, tool: &str, args: Value) -> usize {
        let mut args = args;
        self.riding(&mut args);
        call_measured(&self.client, tool, args).await.1
    }

    /// **A call's answer and the characters it was served in**, whatever the
    /// size: for the case that reads what comes back when one record is itself
    /// past the ceiling.
    pub async fn answer_and_size(&self, tool: &str, args: Value) -> (Value, usize) {
        let mut args = args;
        self.riding(&mut args);
        call_measured(&self.client, tool, args).await
    }

    /// **A call that may land or be refused**, answered either way with no
    /// judgement: for the case that looks for the largest write a guard lets
    /// through, which can only be found by asking and reading what came back.
    pub async fn attempt(&self, tool: &str, args: Value) -> Value {
        let mut args = args;
        self.riding(&mut args);
        call_measured(&self.client, tool, args).await.0
    }

    /// This run's own handle — what a later boot offers back when this one
    /// stops without wrapping, and what a story compares against to say the
    /// run was resumed rather than replaced.
    pub fn sid(&self) -> &str {
        &self.sid
    }

    /// **This run's public name**, which records carry, an offer lists and a
    /// resume names. Never a credential.
    pub fn run(&self) -> &str {
        &self.run
    }

    /// **The tripwire for a gap the SURFACE has, rather than a record.**
    ///
    /// A gap marker says a capability is missing. Where the missing thing
    /// would be a verb or an argument, the evidence is the served surface
    /// itself: this reads the verb list a client is given and asserts the verb
    /// is not on it. The positive half is that the list arrived at all and
    /// carries the verbs the story just used — without it the assertion passes
    /// identically on an empty answer.
    ///
    /// It goes red on the day the verb ships, which is the point.
    pub async fn has_no_verb(&self, verb: &str, alongside: &[&str]) {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let names: Vec<&str> = listed.tools.iter().map(|t| t.name.as_ref()).collect();
        for known in alongside {
            assert!(
                names.contains(known),
                "the verb list must carry {known:?} — without it this proves nothing: {names:?}"
            );
        }
        assert!(
            !names.contains(&verb),
            "jojobot now serves {verb:?} — the gap is closed, so flip this assertion: {names:?}"
        );
    }

    /// **The same tripwire, for a CLASS of verb rather than one name.**
    ///
    /// Some claims are about a whole family: nothing on this surface deletes.
    /// Naming three verbs that do not exist proves nothing about a fourth, so
    /// this asserts that no served verb carries the word at all. The positive
    /// half is the same one: the list arrived and carries the verbs the story
    /// just used.
    pub async fn has_no_verb_containing(&self, needle: &str, alongside: &[&str]) {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let names: Vec<&str> = listed.tools.iter().map(|t| t.name.as_ref()).collect();
        for known in alongside {
            assert!(
                names.contains(known),
                "the verb list must carry {known:?} — without it this proves nothing: {names:?}"
            );
        }
        let carrying: Vec<&&str> = names.iter().filter(|name| name.contains(needle)).collect();
        assert!(
            carrying.is_empty(),
            "jojobot now serves a verb whose name says {needle:?}: {carrying:?}"
        );
    }

    /// **Every verb the surface serves, by name**, as a client is given them.
    /// For a claim that is about all of them at once.
    pub async fn served_verbs(&self) -> Vec<String> {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        listed.tools.iter().map(|t| t.name.to_string()).collect()
    }

    /// **The served verbs that publish an argument of this name**, at any
    /// depth of their schema. For a text that names a verb as the way to send
    /// that argument: the verb has to be one of these.
    pub async fn verbs_publishing(&self, argument: &str) -> Vec<String> {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let mut publishing = Vec::new();
        for tool in &listed.tools {
            let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
            let mut published = Vec::new();
            argument_names(&schema, &mut published);
            if published.iter().any(|name| name == argument) {
                publishing.push(tool.name.to_string());
            }
        }
        publishing
    }

    /// **The tripwire for a gap whose capability will arrive as an ARGUMENT.**
    ///
    /// [`has_no_verb`] cannot hold one of these. A verb has to earn its place
    /// against widening one that exists, so most of what is missing ships as a
    /// parameter on a read that is already served — and a check reading the
    /// verb list is green the day before that lands and green the day after.
    ///
    /// This reads the arguments the verb publishes to every client, at every
    /// depth, and asserts that none of them is named for the capability. The
    /// needle is matched as a SUBSTRING of each published name, because the
    /// spelling is settled when the thing is built: a check pinned to one
    /// guess is the test that cannot fail, one guess later. The positive half
    /// is the verb's own schema arriving and carrying the arguments the
    /// missing one would sit beside.
    ///
    /// It goes red on the day the argument ships, which is the point.
    pub async fn has_no_argument(&self, verb: &str, named_for: &str, alongside: &[&str]) {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let tool = listed
            .tools
            .iter()
            .find(|t| t.name == verb)
            .unwrap_or_else(|| {
                panic!("the verb list must carry {verb:?} for this to prove anything")
            });
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        let mut published = Vec::new();
        argument_names(&schema, &mut published);
        for known in alongside {
            assert!(
                published.iter().any(|name| name == known),
                "{verb} must publish {known:?} — without it this proves nothing: {published:?}"
            );
        }
        assert!(
            !published.iter().any(|name| name.contains(named_for)),
            "{verb} now takes an argument named for {named_for:?} — the gap is closed, \
             so flip this assertion: {published:?}"
        );
    }

    /// **What a served verb tells its reader**: the verb's own description,
    /// whole, as the verb list serves it.
    pub async fn description(&self, verb: &str) -> String {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        listed
            .tools
            .iter()
            .find(|t| t.name == verb)
            .unwrap_or_else(|| panic!("the verb list must carry {verb:?}"))
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("{verb} publishes no description"))
            .to_string()
    }

    /// **What a served argument tells its reader**, read out of the verb's own
    /// schema: the description of one argument, whole.
    pub async fn argument_description(&self, verb: &str, argument: &str) -> String {
        let listed = self.client.list_tools(None).await.expect("the verb list");
        let tool = listed
            .tools
            .iter()
            .find(|t| t.name == verb)
            .unwrap_or_else(|| panic!("the verb list must carry {verb:?}"));
        let schema = serde_json::to_value(&tool.input_schema).expect("the schema serializes");
        schema["properties"][argument]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{verb} publishes no description for {argument:?}"))
            .to_string()
    }

    /// **The tripwire for a write jojobot refuses today.**
    ///
    /// Returns the refusal so a story can say what it named. Unlike every
    /// other write here, a `blocked` answer is the expected one — this is the
    /// one place a story asserts a use case is NOT reachable, and it fails on
    /// the day the write starts landing.
    pub async fn refused(&self, tool: &str, mut args: Value) -> Answer {
        self.riding(&mut args);
        // **Both refusal shapes count, and they are different answers.** A
        // `blocked` body with a way forward is the answer jojobot writes
        // itself — either a domain refusal, or the argument gate turning back
        // a top-level argument the verb does not implement before dispatch,
        // with nothing written. A VALUE the schema cannot deserialize at all —
        // an unknown `kind`, an unknown `shape` — still fails in the client and
        // never reaches the domain. A tripwire that accepted only one would
        // pass the day a refusal moved from one shape to the other.
        let body = match within_a_deadline(
            tool,
            self.client.call_tool(
                CallToolRequestParams::new(tool.to_string())
                    .with_arguments(args.as_object().expect("arguments are an object").clone()),
            ),
        )
        .await
        {
            Err(e) => json!({"status": "blocked", "client_error": e.to_string()}),
            Ok(result) => {
                let text = result
                    .content
                    .first()
                    .and_then(|b| b.as_text())
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
            }
        };
        assert_eq!(
            body["status"], "blocked",
            "{tool} was accepted — the gap is closed, so flip this assertion: {body}"
        );
        Answer {
            what: format!("the refusal from {tool}"),
            body: body.to_string(),
        }
    }

    /// Takes the whole `kind:slug` handle, deliberately: **the fixture-roster
    /// gate scans source text for handle-shaped literals**, so a story that
    /// passed its kind and slug separately would create entities the one guard
    /// against life specifics cannot see.
    pub async fn add(&self, handle: &str, name: &str) {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        self.write(
            &format!("adding {handle}"),
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": name, "source": "user-named"}),
        )
        .await;
    }

    /// **The same creation, over a near-miss the guard surfaces.** The token
    /// that lifts it rides the refusal's own advice rather than a field of its
    /// own (rule 68), so this reads it out of the prose exactly as a real
    /// caller has to.
    pub async fn add_over_the_screen(&self, handle: &str, name: &str) {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        let refusal = self
            .refused(
                "add_entity",
                json!({"kind": kind, "handle": slug, "name": name, "source": "user-named"}),
            )
            .await;
        let how_to_proceed = refusal.json()["how_to_proceed"]
            .as_str()
            .expect("a blocked creation names how to proceed")
            .to_string();
        let token = how_to_proceed
            .split("override_token: \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("the refusal names no override_token: {how_to_proceed}"))
            .to_string();
        self.write(
            &format!("adding {handle} over the screen"),
            "add_entity",
            json!({
                "kind": kind, "handle": slug, "name": name, "source": "user-named",
                "override_token": token,
                "override_reason": "a different thing sharing a name",
            }),
        )
        .await;
    }

    /// **The same creation, under something else.** Where a thing sits is
    /// fixed when it is made and there is no reparenting verb, so this is the
    /// only moment a story can say it.
    pub async fn add_under(&self, parent: &str, handle: &str, name: &str) {
        let (kind, slug) = handle.split_once(':').expect("a handle is kind:slug");
        self.write(
            &format!("adding {handle} under {parent}"),
            "add_entity",
            json!({
                "kind": kind, "handle": slug, "name": name,
                "source": "user-named", "parent": parent,
            }),
        )
        .await;
    }

    /// Something the person said. Testimony. Hands back the fact's address,
    /// which is what `correct` later edits it through — and what rule 15 calls
    /// the receipt.
    pub async fn fact(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("a fact about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "testimony"}),
            )
            .await;
        address_of(&body)
    }

    /// Something the person said, with no date on it — and **the day jojobot
    /// stamped it with**, which is the run's own day rather than the server's.
    pub async fn undated_fact(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("an undated fact about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "testimony"}),
            )
            .await;
        assert!(
            body.get("recorded_at").is_none(),
            "the input is not echoed: {body}"
        );
        body["derived"]["recorded_at"]
            .as_str()
            .unwrap_or_else(|| panic!("a capture is stamped with a day: {body}"))
            .to_string()
    }

    /// A fact carrying a date — **the day the CLAIM is about**, which is not
    /// the day the thing happened. See [`Session::fact_that_happened_on`] and
    /// [`Session::fact_with_no_day`] for the other question.
    pub async fn fact_on(&self, subject: &str, content: &str, date: &str) {
        self.write(
            &format!("a dated fact about {subject}"),
            "capture",
            json!({
                "subject": subject, "content": content,
                "provenance": "testimony", "recorded_at": date,
            }),
        )
        .await;
    }

    /// **A claim that says WHEN THE THING HAPPENED**, beside the day the claim
    /// itself is made. Two questions, two fields.
    pub async fn fact_that_happened_on(
        &self,
        subject: &str,
        content: &str,
        happened_at: &str,
    ) -> String {
        let body = self
            .write(
                &format!("a fact about {subject}, dated when it happened"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "happened_at": happened_at,
                }),
            )
            .await;
        address_of(&body)
    }

    /// **A claim about something whose day nobody gave** — *over the summer*.
    /// It records no day for the thing, which is the honest answer and the one
    /// a single date field could not give.
    pub async fn fact_with_no_day(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("a fact about {subject}, no day given"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony",
                }),
            )
            .await;
        address_of(&body)
    }

    /// Something worked out rather than heard. Inference, and it reads back as one.
    pub async fn guess(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("an inference about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "inference"}),
            )
            .await;
        address_of(&body)
    }

    /// **A claim the operator made and is not sure of.**
    ///
    /// Both halves are recorded, because they are two questions. `provenance`
    /// says WHO BACKS IT, and the answer is `testimony`: the operator said it.
    /// `standing` says HOW SURE, and the answer is `open`: they said they were
    /// not sure. It is the one pairing no default can produce.
    pub async fn hedged(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("a hedged claim about {subject}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "standing": "open",
                }),
            )
            .await;
        address_of(&body)
    }

    /// **A hedge that also carries keys.** Same two halves as [`Session::hedged`]
    /// — testimony for who said it, `open` for how sure they were — for a claim
    /// that is also a record of something, which `hedged` cannot write because
    /// it takes no fields.
    pub async fn hedged_with(&self, subject: &str, content: &str, fields: Value) -> String {
        let body = self
            .write(&format!("a hedged record about {subject}"), "capture", {
                let (own, set) = bags(fields);
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "standing": "open",
                    "fields": own, "sets": set,
                })
            })
            .await;
        address_of(&body)
    }

    /// The operator confirms a claim that was standing as a hypothesis, and it
    /// becomes settled. Promotion is gated on exactly this — a receipt naming the
    /// day and the place of the confirmation, so nothing can promote itself.
    pub async fn confirm(&self, address: &str) {
        self.write(
            &format!("confirming {address}"),
            "update_fact",
            // **Naming `standing` is what routes this through the settling
            // gate.** A restatement of the provenance would close the hedge
            // through the ungated default instead, and never reach the gate.
            json!({
                "address": address,
                "standing": "settled",
                "receipt": {"confirmed": {"on": "2026-03-14", "at": "message m4k9xz"}},
            }),
        )
        .await;
    }

    /// A session settles a claim on a check it ran. The check is written so
    /// another agent can run it again, and provenance is left alone.
    pub async fn settle_on_a_check(&self, address: &str, check: &str) {
        self.write(
            &format!("settling {address} on a check"),
            "update_fact",
            json!({
                "address": address,
                "standing": "settled",
                "receipt": {"check": check},
            }),
        )
        .await;
    }

    /// Something worked out from ANOTHER CLAIM, not from an entity — the
    /// fact-to-fact link. `source` is the parent claim's own address.
    pub async fn guess_from(&self, subject: &str, content: &str, source: &str) -> String {
        let body = self
            .write(
                &format!("an inference about {subject}, sourced from {source}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "inference", "derived_from": source,
                }),
            )
            .await;
        address_of(&body)
    }

    /// Move a claim past: it was true in its day and is not current truth any
    /// more, so it stays on the record and stops coming back as current.
    /// Different from a correction, which rewrites a claim that was never
    /// right, and from a retraction, which is for chronology.
    pub async fn supersede(&self, address: &str) {
        self.write(
            &format!("archiving {address}"),
            "update_fact",
            json!({"address": address, "status": "archived"}),
        )
        .await;
    }

    /// Rewrite a claim that turned out to be wrong, in place — rule 58, which
    /// says a refutation FIXES THE SOURCE rather than adding a contradiction
    /// beside it. Anything less is two claims and a reader left to adjudicate.
    pub async fn correct(&self, address: &str, content: &str) {
        self.write(
            &format!("correcting {address}"),
            "update_fact",
            json!({"address": address, "content": content, "provenance": "testimony"}),
        )
        .await;
    }

    /// **Correct a claim an EARLIER session wrote**, by the route that leaves
    /// the original readable: archive it with the reason, then write the
    /// corrected claim as testimony, derived from it. Returns the new claim's
    /// address.
    ///
    /// `extra` is merged into the capture, for a correction that also moves the
    /// claim's edge (`shape` and `object`).
    pub async fn replace(&self, address: &str, content: &str, extra: Value) -> String {
        let subject = address
            .split('#')
            .next()
            .unwrap_or_else(|| panic!("an address names its subject: {address}"));
        self.call(
            "update_fact",
            json!({"address": address, "status": "archived", "details": "corrected"}),
        )
        .await;
        let mut args = json!({
            "subject": subject, "content": content,
            "provenance": "testimony", "derived_from": address,
        });
        if let (Some(into), Some(more)) = (args.as_object_mut(), extra.as_object()) {
            into.extend(more.clone());
        }
        let body = self.call("capture", args).await.json();
        address_of(&body)
    }

    /// **The receipt a correction answers with**, for a story that reads what
    /// the write said rather than only what the store now holds.
    pub async fn correct_reading_the_receipt(&self, address: &str, content: &str) -> Answer {
        let body = self
            .write(
                &format!("correcting {address}"),
                "update_fact",
                json!({"address": address, "content": content, "provenance": "testimony"}),
            )
            .await;
        Answer {
            what: format!("receipt for correcting {address}"),
            body: body.to_string(),
        }
    }

    /// **Correct a claim into its negative and take its edge off in one edit.**
    ///
    /// Rewriting the sentence alone leaves the edge standing behind it, so a
    /// claim that now denies the link goes on drawing it.
    pub async fn correct_clearing_the_edge(&self, address: &str, content: &str) {
        self.write(
            &format!("correcting {address} and taking its edge off"),
            "update_fact",
            json!({"address": address, "content": content, "clear_edge": true, "provenance": "testimony"}),
        )
        .await;
    }

    pub async fn fact_about(
        &self,
        subject: &str,
        content: &str,
        shape: &str,
        object: &str,
    ) -> String {
        let body = self
            .write(
                &format!("a fact linking {subject} to {object}"),
                "capture",
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony", "shape": shape, "object": object,
                }),
            )
            .await;
        address_of(&body)
    }

    /// Rewrite the record's own fields: set the keys named, take away the keys
    /// listed. Every other key on the record is left where it is.
    pub async fn correct_fields(&self, address: &str, set: Value, clear: &[&str]) {
        self.write(
            &format!("correcting the fields of {address}"),
            "update_fact",
            {
                let (own, setting) = bags(set);
                json!({
                    "address": address,
                    "fields": own, "sets": setting, "clear_fields": clear,
                })
            },
        )
        .await;
    }

    /// **A thing and its claims** — the records read, asked for as one.
    ///
    /// `facts` is sent rather than left to the default, because every one of
    /// this helper's call sites asserts something inside a record: the content,
    /// the provenance, the standing, an address, an edge. A story that wants
    /// the thing WITHOUT its claims asks for that shape through [`shape`],
    /// which is where a beat says what it wants; this one is named for the
    /// half it carries, and sending the argument is what keeps it true when
    /// the default moves.
    ///
    /// [`shape`]: Session::shape
    pub async fn recall(&self, subject: &str) -> Answer {
        self.read(
            format!("recall of {subject}"),
            "recall",
            json!({"subject": subject, "facts": true}),
        )
        .await
    }

    /// [`recall`](Self::recall), asking for each record's details too: a standard
    /// records read leaves them out and says how many it left out.
    pub async fn recall_with_details(&self, subject: &str) -> Answer {
        self.read(
            format!("recall of {subject} with details"),
            "recall",
            json!({"subject": subject, "facts": true, "details": true}),
        )
        .await
    }

    pub async fn find(&self, query: &str) -> Answer {
        self.read(
            format!("search for {query:?}"),
            "search",
            json!({"query": query}),
        )
        .await
    }

    /// The same front door, asking for mail as well. **Mail is opt-in**: the
    /// bare search above answers about the operator's life, and this is how a
    /// session reaches what another session filed.
    pub async fn find_including_mail(&self, query: &str) -> Answer {
        self.read(
            format!("search for {query:?}, mail included"),
            "search",
            json!({"query": query, "include_mail": true}),
        )
        .await
    }

    /// Every claim nobody vouched for, across the whole store — the filter
    /// asks about provenance rather than about words.
    pub async fn unbacked(&self) -> Answer {
        self.read(
            "everything nobody vouched for".to_string(),
            "search",
            json!({"provenance": "inference"}),
        )
        .await
    }

    /// **Every claim still in doubt, across the whole store** — the other
    /// certainty axis, asked the way [`Session::unbacked`] asks the first.
    ///
    /// `unbacked` asks WHO backs a claim; this asks HOW SURE anybody is. The
    /// two are independent, which is why one filter could never answer both.
    pub async fn hedges(&self) -> Answer {
        self.read(
            "everything still in doubt".to_string(),
            "search",
            json!({"standing": "open"}),
        )
        .await
    }

    /// Walk an edge backwards: every entity of `kind` whose fact draws a
    /// `shape` edge at `object`. The cross-entity question, in one call.
    pub async fn through(&self, shape: &str, object: &str, kind: &str) -> Answer {
        self.read(
            format!("{kind}s linked to {object} by {shape}"),
            "search",
            json!({"kind": kind, "edge": {"shape": shape, "object": object}}),
        )
        .await
    }

    /// **The graph query.** `recall` takes the shape of the answer — which
    /// objects, what of each, which edges to walk — so a story writes the
    /// question's own shape rather than reaching for a verb per question.
    ///
    /// `what` is the question in the operator's words, so a failure names the
    /// thing that could not be asked rather than a bag of arguments.
    pub async fn shape(&self, what: &str, args: Value) -> Answer {
        self.read(what.to_string(), "recall", args).await
    }

    /// The same walk with no kind at all — everything pointing at `object`,
    /// whatever sort of thing it is.
    ///
    /// **`kind` is optional on the wire and the DSL made it look required.**
    /// A story written through `through` had to name a kind per call, so
    /// "what rests on this" read as a question the surface could not answer.
    pub async fn through_any(&self, shape: &str, object: &str) -> Answer {
        self.read(
            format!("everything linked to {object} by {shape}"),
            "search",
            json!({"edge": {"shape": shape, "object": object}}),
        )
        .await
    }

    /// Put a duplicate away into the thing it duplicates. Returns the whole
    /// answer, because what a caller does next turns on how many claims changed
    /// address.
    ///
    /// `recorded_at` is the day the merge is recorded as happening — `None`
    /// leaves it to default to today, which is what every call here sent
    /// before the argument existed. A story naming a duplicate found months
    /// after the fact sends one.
    pub async fn merge_entities(
        &self,
        duplicate: &str,
        survivor: &str,
        reason: &str,
        recorded_at: Option<&str>,
    ) -> Value {
        self.write(
            &format!("merging {duplicate} into {survivor}"),
            "merge_entities",
            json!({
                "duplicate": duplicate, "survivor": survivor, "reason": reason,
                "recorded_at": recorded_at,
            }),
        )
        .await
    }

    /// Take a record back. One way: what was said stands and is marked
    /// withdrawn, where a correction rewrites a claim to the current truth.
    pub async fn retract(&self, address: &str, reason: &str) {
        self.write(
            &format!("retracting {address}"),
            "retract",
            json!({"address": address, "reason": reason}),
        )
        .await;
    }

    pub async fn list(&self, kind: &str) -> Answer {
        self.read(
            format!("listing of {kind}"),
            "list_entities",
            json!({"kind": kind}),
        )
        .await
    }

    pub async fn journal(&self, entry: &str) {
        self.write("a journal entry", "journal", json!({"entry": entry}))
            .await;
    }

    /// A record of something that HAPPENED, rather than something that is
    /// true now. **Nothing on the record says which it is**: the two are one
    /// class, and what a record is gets read off the fields it carries.
    pub async fn event(&self, subject: &str, content: &str) -> String {
        self.event_with(subject, content, json!({}), &[]).await
    }

    /// The same, carrying the record's fields and the entities it touches.
    ///
    /// **The plain helper sends neither, and that is why several stories say a
    /// number or a link has nowhere to go but prose.** `capture` takes
    /// `fields` and `refs`; the DSL dropped them, so a story written through
    /// the DSL could not reach a capability the surface already has, and the
    /// marker recording that read as a gap in jojobot rather than a gap in the
    /// fixture.
    pub async fn event_with(
        &self,
        subject: &str,
        content: &str,
        fields: Value,
        refs: &[&str],
    ) -> String {
        let body = self
            .write(&format!("a record about {subject}"), "capture", {
                let (own, set) = bags(fields);
                json!({
                    "subject": subject, "content": content,
                    "provenance": "testimony",
                    "fields": own, "sets": set, "refs": refs,
                })
            })
            .await;
        address_of(&body)
    }

    /// Leave work in another bot's box. The shape of a request: it writes
    /// without reading, so it takes no delivery and obliges nobody.
    pub async fn post(&self, mailbox: &str, subject: &str, body: &str) -> String {
        let sent = self
            .write(
                &format!("posting to {mailbox}"),
                "post_message",
                json!({"to": mailbox, "subject": subject, "body": body}),
            )
            .await;
        sent["id"]
            .as_str()
            .expect("a post hands back its id")
            .to_string()
    }

    /// Take delivery of everything in YOUR OWN box. There is no box argument —
    /// the sid says who is asking, and a bot drains the box it owns.
    pub async fn drain(&self) -> Answer {
        self.read("my mailbox".to_string(), "read_mailbox", json!({}))
            .await
    }

    /// Retire a message once it has been acted on, with the outcome. Hands
    /// back the receipt, which says what the store kept when it kept something
    /// else.
    pub async fn processed(&self, id: &str, notes: &str) -> Value {
        self.write(
            &format!("processing {id}"),
            "mark_processed",
            json!({"message_id": id, "notes": notes}),
        )
        .await
    }

    /// Ends the session and the connection with it.
    pub async fn wrap(self, story: &str) {
        self.write(
            "wrapping the session",
            "wrap_session",
            json!({"story": story}),
        )
        .await;
        self.client.cancel().await.unwrap();
    }
}

/// What a read came back with, and what the story expects to find in it.
pub struct Answer {
    what: String,
    body: String,
}

impl Answer {
    /// **How many characters the answer is** — the unit a ceiling is stated in.
    pub fn size(&self) -> usize {
        self.body.chars().count()
    }

    /// **The answer as structure**, for the claims a substring cannot make.
    ///
    /// `says` reads the payload as text, which is right for *this wording is
    /// in there* and wrong for *this list is not empty*: a key that is present
    /// and holds nothing satisfies every `says` about its name. Counting what
    /// came back needs the shape.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body))
    }

    /// **What the answer taught, as the lines it carried under `teaching`.**
    /// Empty when the key is absent. A story reads a teaching as structure:
    /// a first answer's lines are the reference, and a later answer either
    /// carries those same lines again or carries none.
    pub fn teachings(&self) -> Vec<String> {
        self.json()["teaching"]
            .as_array()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.as_str().expect("a teaching is text").to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The assertion that can actually fail on a correction: the old wording is
    /// GONE, not merely outnumbered.
    pub fn never_says(&self, needle: &str) -> &Self {
        assert!(
            !self.body.contains(needle),
            "the {} still carries {needle:?}, so nothing was corrected: {}",
            self.what,
            self.body
        );
        self
    }

    /// **The answer's selection leaves this thing out.** A thing can still be
    /// named elsewhere in the answer, as a link or in what is held for another,
    /// so a selection is read by the objects it holds.
    pub fn never_selects(&self, handle: &str) -> &Self {
        let objects = self.json()["objects"].clone();
        let held = objects
            .as_array()
            .is_some_and(|all| all.iter().any(|object| object["id"] == handle));
        assert!(!held, "the {} selects {handle:?}: {}", self.what, self.body);
        self
    }

    /// **One named table on a page**, so an assertion says which part of it it
    /// is about.
    ///
    /// A page repeats a value in more than one place on purpose — a thing's
    /// folded fields and the record each one came from — so a substring over
    /// the whole page cannot tell the two apart, and a beat written that way
    /// passes on a build where the part it names is missing.
    pub fn section(&self, id: &str) -> Answer {
        let anchor = format!("id=\"{id}\"");
        let start = self
            .body
            .find(&anchor)
            .unwrap_or_else(|| panic!("no {id} section in the {}: {}", self.what, self.body));
        let rest = &self.body[start..];
        let body = match rest.find("</table>") {
            Some(end) => &rest[..end],
            None => rest,
        };
        Answer {
            what: format!("{} section of the {}", id, self.what),
            body: body.to_string(),
        }
    }

    /// **Where the page's own link on this label goes.**
    ///
    /// A page is read by following what it offers, so a story that wants the
    /// next page takes the address off the link rather than building one: an
    /// address a story assembles is one the page never has to carry, and the
    /// beat would pass on a build where the label is not a link at all.
    ///
    /// Scope this to the section the link lives in — the whole page carries
    /// many.
    pub fn link_to(&self, label: &str) -> String {
        let anchor = format!(">{label}</a>");
        let at = self.body.find(&anchor).unwrap_or_else(|| {
            panic!(
                "the {} offers no link on {label:?}: {}",
                self.what, self.body
            )
        });
        let opening = self.body[..at]
            .rfind("href=\"")
            .unwrap_or_else(|| panic!("the {label:?} link carries no address: {}", self.body));
        let href = &self.body[opening + "href=\"".len()..];
        let end = href
            .find('"')
            .unwrap_or_else(|| panic!("the {label:?} link's address does not end: {}", self.body));
        href[..end].to_string()
    }

    /// **What ONE claim says, picked by its address.** `says` is a substring
    /// over the whole answer, so on a subject holding two facts it proves only
    /// that a token appears SOMEWHERE — which is how a story about two claims
    /// that must differ can pass while they are identical, and how a dropped
    /// argument can hide behind a neighbour supplying the same word.
    pub fn claim(&self, address: &str) -> Answer {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        // **Wherever the answer keeps its claims.** `recall` answers with
        // objects, each carrying its own and each carrying the objects it
        // reached; other answers carry a flat list. A claim is picked by its
        // ADDRESS, which is unique across every shape, so gathering from all
        // of them is not a guess about which verb replied — and reaching into
        // the walk is the point, because what a nested object carries is
        // exactly what a story about a walk is asserting on.
        fn carried(body: &Value, into: &mut Vec<Value>) {
            into.extend(body["facts"].as_array().into_iter().flatten().cloned());
            for object in body["objects"]
                .as_array()
                .into_iter()
                .chain(body["connected"].as_array())
                .flatten()
            {
                carried(object, into);
            }
        }
        let mut found = Vec::new();
        carried(&body, &mut found);
        let fact = found
            .iter()
            .find(|f| f["address"] == address)
            .unwrap_or_else(|| {
                panic!(
                    "the {} holds no claim at {address}: {}",
                    self.what, self.body
                )
            })
            .clone();
        Answer {
            what: format!("claim {address}"),
            body: fact.to_string(),
        }
    }

    /// One top-level field of the answer, as a string — the id a later beat
    /// addresses this thing through. Panics rather than returning an empty
    /// string when the field is missing, so a beat built on it cannot go on
    /// asserting against nothing.
    pub fn field(&self, name: &str) -> String {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        body[name]
            .as_str()
            .unwrap_or_else(|| panic!("the {} carries no {name}: {}", self.what, self.body))
            .to_string()
    }

    /// A value the answer's advice tells the caller to send back, read out
    /// from `key: "…"`.
    ///
    /// **The override token rides inside the advice rather than in a field of
    /// its own**, so a story that wants to send one back has to read it out of
    /// there. This keys on the ARGUMENT NAME the advice says to send it under —
    /// something a rename would be a real change to — and not on the sentence
    /// around it, which is ours to improve.
    pub fn advised(&self, key: &str) -> String {
        let body: Value = serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("the {} is not json: {e}: {}", self.what, self.body));
        let advice = body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("the {} carries no advice: {}", self.what, self.body));
        let marker = format!("{key}: \"");
        let from = advice
            .find(&marker)
            .unwrap_or_else(|| panic!("the {} names no {key}: {advice}", self.what))
            + marker.len();
        let rest = &advice[from..];
        let to = rest
            .find('"')
            .unwrap_or_else(|| panic!("the {key} in the {} is unterminated: {advice}", self.what));
        rest[..to].to_string()
    }

    /// The answer as it came back, for a beat that has to reason about the
    /// ORDER of what is in it rather than only about what is there.
    pub fn raw(&self) -> &str {
        &self.body
    }

    pub fn says(&self, needle: &str) -> &Self {
        assert!(
            self.body.contains(needle),
            "the {} should mention {needle:?}: {}",
            self.what,
            self.body
        );
        self
    }

    /// **A number, read at the place it lives rather than matched as text.**
    ///
    /// 🚨 **The payload is compact json, so `"count":3` is a PREFIX of
    /// `"count":30`.** An assertion written that way reads as *exactly three
    /// came back* and claims *some number starting with three came back* — and
    /// a reader trusts the first reading. This one cannot be satisfied by a
    /// bigger number.
    ///
    /// The pointer is written out rather than hidden behind a helper per
    /// shape: a story is read by a person, and where the number lives is part
    /// of what the story says.
    pub fn number(&self, pointer: &str, expected: i64) -> &Self {
        let found = self.json();
        let at = found.pointer(pointer).unwrap_or_else(|| {
            panic!(
                "the {} carries nothing at {pointer}: {}",
                self.what, self.body
            )
        });
        assert_eq!(
            at.as_i64(),
            Some(expected),
            "the {} holds {at} at {pointer} rather than {expected}: {}",
            self.what,
            self.body,
        );
        self
    }

    /// **The text at one place in the answer**, read there rather than matched
    /// anywhere in it. A needle matched against the whole payload is satisfied
    /// by every part of the answer that repeats the call's own input; a read at
    /// a pointer is about the one field a rule produces. Panics when nothing
    /// text lives there, so an assertion built on it cannot go on asserting
    /// against nothing.
    pub fn text_at(&self, pointer: &str) -> String {
        let found = self.json();
        found
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "the {} carries no text at {pointer}: {}",
                    self.what, self.body
                )
            })
            .to_string()
    }

    /// The text at `pointer` holds `needle`.
    pub fn holds_at(&self, pointer: &str, needle: &str) -> &Self {
        let text = self.text_at(pointer);
        assert!(
            text.contains(needle),
            "the {} holds {text:?} at {pointer}, which lacks {needle:?}: {}",
            self.what,
            self.body
        );
        self
    }

    /// The text at `pointer` does not hold `needle`.
    pub fn never_holds_at(&self, pointer: &str, needle: &str) -> &Self {
        let text = self.text_at(pointer);
        assert!(
            !text.contains(needle),
            "the {} holds {text:?} at {pointer}, which carries {needle:?}: {}",
            self.what,
            self.body
        );
        self
    }
}

/// 🚨 **What a text assertion about a number cannot do, shown once.**
///
/// Every count in these stories used to be written as `says("\"count\":3")`.
/// **Both halves**: the text needle is satisfied by a payload holding THIRTY,
/// and the structural read is not. Without the first half this proves only
/// that the new assertion works; without the second it proves nothing at all.
#[test]
fn a_count_matched_as_text_is_satisfied_by_a_bigger_count() {
    let thirty = Answer {
        what: "answer".into(),
        body: "{\"count\":30,\"objects\":[]}".into(),
    };
    assert!(
        thirty.body.contains("\"count\":3"),
        "the old needle does not match thirty, so there was nothing to fix",
    );
    assert_eq!(
        thirty.json()["count"].as_i64(),
        Some(30),
        "the structural read sees thirty where the needle saw three",
    );
}

/// How long a story waits for one verb before it fails the case.
const A_VERB_MAY_TAKE: std::time::Duration = std::time::Duration::from_secs(120);

/// **A verb that never answers fails the case instead of hanging it.** A
/// handler that panics leaves its caller waiting on a reply that cannot come,
/// and a case that waits for ever reports nothing to anybody — not even to a
/// sabotage run that wanted to know whether the case noticed.
async fn within_a_deadline<T>(tool: &str, answering: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(A_VERB_MAY_TAKE, answering)
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{tool} did not answer within {} seconds: the server may have panicked in it",
                A_VERB_MAY_TAKE.as_secs()
            )
        })
}

async fn call(client: &Client, tool: &str, args: Value) -> Value {
    let (body, size) = call_measured(client, tool, args).await;
    // **Every answer of every story is measured against the one ceiling.** A
    // story that asks for something big enough to pass it fails here, naming the
    // verb and the size, so a verb that grows past what a client can read is
    // found by the stories that already exist and not only by a fixture written
    // for the purpose.
    assert!(
        size <= jojobot_domain::text::ANSWER_CEILING || MAY_PASS_THE_CEILING.contains(&tool),
        "{tool} answered {size} characters, past the {} every answer is meant to fit under",
        jojobot_domain::text::ANSWER_CEILING,
    );
    body
}

/// **A call and the characters its answer was served in**, with no judgement on
/// the size: the case that measures a verb at its widest asks through this.
async fn call_measured(client: &Client, tool: &str, args: Value) -> (Value, usize) {
    let result = within_a_deadline(
        tool,
        client.call_tool(
            CallToolRequestParams::new(tool.to_string())
                .with_arguments(args.as_object().expect("arguments are an object").clone()),
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("{tool} call failed: {e}"));
    let text = result
        .content
        .first()
        .and_then(|b| b.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_else(|| panic!("{tool} returned no text block"));
    let size = text.chars().count();
    (
        serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })),
        size,
    )
}

/// The verbs whose answer may pass the ceiling in a story. A boot whose floor
/// is over the ceiling ships anyway and says so (`over_the_ceiling`), and a
/// story that makes a bot too heavy on purpose asks for exactly that.
const MAY_PASS_THE_CEILING: &[&str] = &["start_here"];

/// **Every argument name one verb publishes, at every depth.**
///
/// The keys of each `properties` map anywhere in the schema document, the
/// definitions an argument of its own shape is emitted through included — so a
/// name nested inside a filter or a walk is read like a top-level one. Nothing
/// here enumerates the levels a schema may have: it takes the whole document
/// the client was served and reads every level it finds.
fn argument_names(node: &Value, found: &mut Vec<String>) {
    if let Some(properties) = node.get("properties").and_then(|p| p.as_object()) {
        found.extend(properties.keys().cloned());
    }
    match node {
        Value::Object(map) => map.values().for_each(|child| argument_names(child, found)),
        Value::Array(items) => items.iter().for_each(|child| argument_names(child, found)),
        _ => {}
    }
}

/// The address a write handed back, which is how a claim is edited later.
fn address_of(body: &Value) -> String {
    body["address"]
        .as_str()
        .or_else(|| body["fact"]["address"].as_str())
        .unwrap_or_else(|| panic!("a captured fact must hand back its address: {body}"))
        .to_string()
}
