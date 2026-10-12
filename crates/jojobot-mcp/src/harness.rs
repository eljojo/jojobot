//! **A `Jojobot` under test, and how to read what it answered.**
//!
//! Two halves, and they are one contract: build a handler wired to the doubles
//! a test needs, and decode the `CallToolResult` it hands back. Every context's
//! tests reach for both, which is what makes this a shared file rather than a
//! drawer — the fixtures that are ABOUT a context live in that context's own
//! `testing` module.
//!
//! Test-only, and declared by `lib.rs`.

use super::*;
use crate::memory::testing::{SpySearch, add_args};
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;
use rmcp::handler::server::wrapper::Parameters;

/// **The identity a fixture writes as.** Every memory write needs one now, and
/// nearly every test here performs a write as an ordinary identified caller —
/// so the argument builders carry this rather than making 140 call sites say
/// the same thing.
///
/// **A test that means ANONYMOUS says so explicitly**, by building its args
/// with `sid: None` instead of reaching for a builder. That is what keeps the
/// default honest: a case about anonymity that went through a builder would
/// carry an identity and pass while testing nothing.
pub(crate) const TEST_SID: &str = "sk-testtesttesttesttesttestte";

/// The public name of the run [`TEST_SID`] opens.
pub(crate) const TEST_RUN: &str = "tsttst";

/// A registry already holding [`TEST_SID`]. Seeded through `mint_with` so the
/// handle is the one the builders expect; the registry is what `identified`
/// consults, so nothing has to exist in the store for a fixture to write.
pub(crate) fn seeded_registry() -> Arc<sid::SessionRegistry> {
    let registry = Arc::new(sid::SessionRegistry::new());
    registry
        .mint_with(
            &EntityId("bot:otto".into()),
            || TEST_RUN.to_string(),
            || TEST_SID.to_string(),
        )
        .expect("a free handle in a fresh registry");
    registry
}

/// **The fixture handle, in whatever handler you hand it.** Returns
/// [`TEST_SID`], minting it into that handler's registry first if it is not
/// there — deterministically, so calling it twice gives the same handle rather
/// than a second session.
///
/// Idempotent on purpose: a helper that minted a fresh handle per call would
/// give each write its own session and break any test that counts beats.
pub(crate) fn writing_as(jojobot: &Jojobot) -> String {
    if jojobot.registry.lookup_key(TEST_SID).is_none() {
        let _ = jojobot.registry.mint_with(
            &EntityId("bot:otto".into()),
            || TEST_RUN.to_string(),
            || TEST_SID.to_string(),
        );
    }
    TEST_SID.to_string()
}

/// **The store every plain handler is built over**: the shipped kinds booted
/// WITH the keys a startup declares for them. A store that holds the kinds as
/// rows with no keys enforces none of what the kinds declare (a named set for
/// `status`, `advances_from`, `outcome`, `ended`; the floor of required keys;
/// the kind a reference points at), so a case written on it to exercise that
/// vocabulary could not fail. One place, so a constructor cannot drift back.
fn plain_store() -> InMemoryMemory {
    InMemoryMemory::seeded()
}

/// **A handler over a store from before own fields stayed on their record**: a
/// key sent as an own field reaches its thing, as it did. Only a case that sets
/// the old shape beside the new one builds this; [`handler`] is the store as it is.
pub(crate) fn handler_reaching() -> Jojobot {
    Jojobot::new(
        Arc::new(plain_store().with_own_fields_reaching()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

pub(crate) fn handler() -> Jojobot {
    Jojobot::new(
        Arc::new(plain_store()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler over a store that declares no keys for the shipped kinds** — the
/// plain handler as it was before it was seeded. It enforces no vocabulary, so
/// it is for a case that is not about one and that a seeded store cannot hold
/// for a stated reason. Name that reason where it is used.
pub(crate) fn handler_bare() -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler over a store the case holds**, so a case can stage a state the
/// verbs refuse to write — see `InMemoryMemory::fields_past_the_guard`.
pub(crate) fn handler_over(memory: Arc<InMemoryMemory>) -> Jojobot {
    Jojobot::new(
        memory,
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler over a store with the mention layer in front of it**, which is how
/// `main` wires every store. A find and replace is applied by that layer and by
/// no other, so a case about one is built here: over the bare store it would be
/// handed an edit nothing applies.
pub(crate) fn handler_with_mentions() -> Jojobot {
    handler_over_mentions(Arc::new(plain_store()))
}

/// [`handler_with_mentions`], over a store the caller keeps a handle to.
pub(crate) fn handler_over_mentions(memory: Arc<dyn Memory>) -> Jojobot {
    Jojobot::new(
        Arc::new(jojobot_domain::memory::mention::Mentioning::new(memory)),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler wrapped in what this crate's OWN `provisions()` ships**, the
/// same decorator `main.rs` wraps the real store in.
///
/// [`handler`] never sees a shipped provision at all — its store declares the
/// shipped kinds' keys but holds no supplied record, so a shipped view or a
/// shipped charter is a stand-in the test declares for itself. That proves the mechanism a shipped provision runs through, never
/// the shipped data itself: a typo in `views.rs`'s handle, or a shipped
/// `shows` key nobody meant to ship, would still pass every case built on
/// [`handler`]. This is the one seam that can go red on the data.
pub(crate) fn handler_shipped() -> Jojobot {
    let supplied = crate::provisions();
    // **The inner store is told what the build supplies too.** Its own
    // existence gate is what `Provisioned::fields()` leans on for a record
    // it holds no row for — a plain `InMemoryMemory` errors `UnknownEntity`
    // on a handle it has never seen, `?`-propagating straight past the
    // supplied-record fallback beneath it. `.knowing` is the same call the
    // adapter's own tests make for this exact shape.
    Jojobot::new(
        Arc::new(jojobot_adapters::provisioned::Provisioned::new(
            InMemoryMemory::seeded().knowing(supplied.clone()),
            supplied,
        )),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **[`handler_shipped`] with the real index in front of it**, in the order
/// `main.rs` wires them: the index over the store that already answers for
/// what the build supplies. A recall that filters on a key asks the index for
/// its carriers, so this is the seam where a supplied record the index never
/// held would drop out of the answer.
pub(crate) fn handler_shipped_indexed() -> Jojobot {
    use jojobot_adapters::search::{IndexedMemory, Retrieval};
    let supplied = crate::provisions();
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(jojobot_adapters::provisioned::Provisioned::new(
            InMemoryMemory::seeded().knowing(supplied.clone()),
            supplied,
        )))
        .expect("the index opens"),
    );
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    Jojobot::new(
        indexed,
        search,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler wired with `mention::Mentioning` in front of memory**, the
/// same decorator `jojobot/src/wiring.rs` wraps the real store in.
///
/// [`handler`]'s store is bare: a mention written through it stores the raw
/// `@kind:slug` text a caller typed rather than the row's permanent badge,
/// so anything reading it back — including a `follow` walk's mention
/// reversal — sees ordinary words, not a link. That is the right store for
/// a case about something else; a case about mentions themselves needs this
/// one, or it is proving the mechanism against a store production never
/// runs.
pub(crate) fn handler_mentioning() -> Jojobot {
    let bare = Arc::new(plain_store());
    let mentioning: Arc<dyn jojobot_domain::memory::Memory> =
        Arc::new(jojobot_domain::memory::mention::Mentioning::new(bare));
    Jojobot::new(
        mentioning,
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler wrapped in one shipped field default of the test's own
/// choosing** — for a case about the mechanism, never the real shipped data
/// [`handler_shipped`] proves.
pub(crate) fn handler_field_provisioned(at: EntityId, key: &str, value: &str) -> Jojobot {
    let supplied = jojobot_domain::memory::owned::Provisions::new(vec![
        jojobot_domain::memory::owned::Provision::field(at, key, value),
    ]);
    Jojobot::new(
        Arc::new(jojobot_adapters::provisioned::Provisioned::new(
            plain_store().knowing(supplied.clone()),
            supplied,
        )),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **A handler over a store the case holds, with the real index in front of
/// it.** A recall that filters on a key asks the index which things carry the
/// key, so a case about that answer needs an index that answers from what the
/// store holds; [`SpySearch`] answers with canned hits and so selects nothing.
pub(crate) fn handler_indexed_over(memory: Arc<InMemoryMemory>) -> Jojobot {
    handler_indexed_carrying(memory, jojobot_domain::attention::shipped())
}

/// [`handler_indexed_over`] over a store of its own.
pub(crate) fn handler_indexed() -> Jojobot {
    handler_indexed_over(Arc::new(plain_store()))
}

/// [`handler_indexed_over`] told which carriers answer for a due moment.
pub(crate) fn handler_indexed_carrying(
    memory: Arc<InMemoryMemory>,
    carriers: Vec<Box<dyn jojobot_domain::attention::Carrier>>,
) -> Jojobot {
    use jojobot_adapters::search::{IndexedMemory, Retrieval};
    let indexed = Arc::new(IndexedMemory::new(memory).expect("the index opens"));
    let search = Arc::new(Retrieval::new(indexed.index(), vec![indexed.clone()]));
    Jojobot::carrying(
        indexed,
        search,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
        carriers,
    )
}

/// A handler whose search port is a spy the test keeps a handle on.
pub(crate) fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
    Jojobot::new(
        Arc::new(plain_store()),
        spy,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// Pull the single text block out of a tool result.
pub(crate) fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|b| b.as_text())
        .map(|t| t.text.clone())
        .expect("tool result should carry a text block")
}

/// The JSON body of a tool result.
pub(crate) fn json_of(result: &CallToolResult) -> serde_json::Value {
    serde_json::from_str(&text_of(result)).expect("tool results carry a JSON body")
}

/// A tool result the guard blocked: a **successful** call whose body says
/// nothing was written. Returns the body.
pub(crate) fn blocked(result: &CallToolResult) -> serde_json::Value {
    assert_ne!(
        result.is_error,
        Some(true),
        "'needs confirmation' is an answer, not a protocol failure: {}",
        text_of(result)
    );
    let body = json_of(result);
    assert_eq!(body["status"], "blocked", "got {body}");
    assert_eq!(
        body["wrote"], false,
        "a blocked write says so in the body: {body}"
    );
    body
}

/// **A refusal and the word it wears.** The body is a blocked answer whose
/// `fix_by` is `word`, and the word sits ahead of the prose in the text a
/// caller reads, so it is read before `how_to_proceed`. `label` names the
/// refusal kind the row stands for.
pub(crate) fn assert_fix_by(label: &str, result: &CallToolResult, word: &str) {
    let body = blocked(result);
    assert_eq!(body["fix_by"], word, "{label}: {body}");
    let text = text_of(result);
    assert!(
        text.find("\"fix_by\"") < text.find("\"how_to_proceed\""),
        "{label}: the word must come before the prose: {text}"
    );
}

/// **A refusal and the word it may not wear**: with `Some(word)` this is
/// [`assert_fix_by`]; with `None` the answer is blocked and carries no
/// `fix_by` at all. A storage failure's word is `None` while the store-failure
/// switch is off, and its rows expect whatever the switch says.
pub(crate) fn assert_word(label: &str, result: &CallToolResult, word: Option<&str>) {
    match word {
        Some(word) => assert_fix_by(label, result, word),
        None => {
            let body = blocked(result);
            assert!(body.get("fix_by").is_none(), "{label}: {body}");
        }
    }
}

/// The same for a protocol error: the word rides in `data`, or there is none.
pub(crate) fn assert_error_word(label: &str, error: &McpError, word: Option<&str>) {
    let carried = error.data.as_ref().and_then(|data| data["fix_by"].as_str());
    assert_eq!(carried, word, "{label}: {error:?}");
}

// ── a handler under test, booted as somebody ────────────────────────────────
//
// **Here rather than in a context's own `testing`, because an identity is not
// one context's business.** Every context's tests need a caller: a memory write
// needs a `sid` to be attributed to, a mailbox read needs one to say whose box
// it opens, a session write needs one to address. What these build is a handler
// in a booted state, which is the same contract as the constructors above.

/// A handle bound to this bot, minted straight from the registry — the same
/// thing the door hands back, without the boot.
///
/// Every verb is addressed by handle now, so a spec about a mailbox still
/// needs one to call with. Booting for it would have to stand the bot up in
/// Memory first, which moves the entity counts other specs assert on and
/// would make these mailbox specs pay for an identity they never look at.
pub(crate) fn as_bot(jojobot: &Jojobot, bot: &str) -> String {
    jojobot
        .registry
        .mint(&EntityId::new(EntityKind::BOT, bot))
        .expect("a free handle")
        .as_str()
        .to_string()
}

/// Stand up a bot the way an operator would: an entity of kind `bot`, its
/// charter as prose, its rules as facts.
///
/// The box is not a parameter, because it is not a choice: a bot's box is
/// its handle, so standing the bot up IS opening it. A second string, next
/// to the handle, could name a different box than the bot's own — never
/// accept one.
pub(crate) async fn make_bot(jojobot: &Jojobot, slug: &str) {
    // **Minted from the handler's OWN registry**, not from the shared fixture
    // handle. A few tests here run against a bare registry because what they
    // are testing IS what a registry holds, and a helper that reached for a
    // constant could not write in those. Asking the handler in front of it
    // works in both.
    let sid = writing_as(jojobot);
    let result = jojobot
        .add_entity(Parameters(AddEntityArgs {
            sid: Some(sid),
            ..add_args("bot", slug, slug)
        }))
        .await
        .expect("add_entity call ok");
    // **A blocked write is a SUCCESSFUL result**, so `.expect` alone let a
    // refusal pass as a created bot — and a fixture that silently created
    // nothing makes every assertion built on it vacuous.
    let body = json_of(&result);
    assert_ne!(
        body["status"], "blocked",
        "the fixture bot {slug:?} was not created: {body}"
    );
    // The box rides with it, so a fixture cannot leave a bot that cannot be
    // written to — the state the operator said must not exist.
    assert_eq!(
        body["mailbox"], slug,
        "the fixture bot {slug:?} got no box: {body}"
    );
}

/// **Stand a bot up through the PORTS, leaving no session behind.**
///
/// `make_bot` goes through the gated verb, which is right for almost every
/// test — the write is attributed, so jojobot writes a beat for it and the
/// fixture identity gets a session card. That card is invisible to most tests
/// and fatal to the handful whose subject IS the session rail: they count
/// cards and recovered handles, and would be counting the fixture as much as
/// the thing under test.
///
/// So those use this. It is not a way around the gate — the gate is on the
/// surface, and this is below it, which is exactly what a fixture is allowed
/// to be.
/// Memory only: these callers are session tests and never touch a box.
pub(crate) async fn seed_bot(memory: &Arc<InMemoryMemory>, slug: &str) {
    memory
        .add_entity(jojobot_domain::memory::NewEntity::new(
            EntityId::new(EntityKind::BOT, slug),
            slug,
            "test-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the fixture bot is not blocked");
}

/// A boot that states the day its run is in, the way a run that is not
/// happening now does.
pub(crate) async fn boot_on(jojobot: &Jojobot, name: &str, today: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: None,
                today: Some(today.into()),
                bot: Some(name.into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    )
}

/// A boot that names its own zone, and a day when it has one: the call a client
/// that knows where it is makes.
pub(crate) async fn boot_in_zone(
    jojobot: &Jojobot,
    name: &str,
    timezone: &str,
    today: Option<&str>,
) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: Some(timezone.into()),
                today: today.map(str::to_string),
                bot: Some(name.into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    )
}

pub(crate) async fn boot(jojobot: &Jojobot, name: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: None,
                today: None,
                bot: Some(name.into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    )
}

/// Answer the choice a boot handed back.
pub(crate) async fn boot_answering(
    jojobot: &Jojobot,
    name: &str,
    answer: &str,
) -> serde_json::Value {
    let answer = &resume_answer(jojobot, answer);
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: None,
                today: None,
                bot: Some(name.into()),
                brief: None,
                skill: None,
                section: None,
                resume: Some(answer.into()),
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    )
}

/// **What a resume answer is, whichever the case gave.** A case holding the key
/// of the run it means to pick up names it by that key; a resume names a run by
/// its public name, so the key is turned into the name it opens. Every other
/// answer (`new`, a name, a wrap code) goes through as it is.
fn resume_answer(jojobot: &Jojobot, answer: &str) -> String {
    if jojobot_domain::session::is_key(answer)
        && let Some(held) = jojobot.registry.lookup_key(answer)
    {
        return held.name.as_str().to_string();
    }
    answer.to_string()
}

/// Answer the choice a boot handed back, naming `today` and/or `timezone`
/// on this same call — what a resume that moves the day, the zone, or
/// neither actually sends.
pub(crate) async fn boot_answering_dated(
    jojobot: &Jojobot,
    name: &str,
    answer: &str,
    today: Option<&str>,
    timezone: Option<&str>,
) -> serde_json::Value {
    let answer = &resume_answer(jojobot, answer);
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: timezone.map(str::to_string),
                today: today.map(str::to_string),
                bot: Some(name.into()),
                brief: None,
                skill: None,
                section: None,
                resume: Some(answer.into()),
                sid: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    )
}

/// The handle a boot handed back, or `None` when it handed none back.
pub(crate) fn sid_of(body: &serde_json::Value) -> Option<String> {
    body["session"]["sid"].as_str().map(str::to_string)
}

/// **The public name of the run a key opens**, for the cases that compare what a
/// record carries (a role's holder, a claim's session) with the run that wrote
/// it. A key is never what a record carries.
pub(crate) fn run_name(jojobot: &Jojobot, key: &str) -> String {
    jojobot
        .caller(Some(key))
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("the key opens no run"))
        .sid
        .to_string()
}

/// The public name of the run a boot handed back, or `None` when it handed none.
pub(crate) fn run_of(body: &serde_json::Value) -> Option<String> {
    body["session"]["run"].as_str().map(str::to_string)
}

/// Boot as this bot and take the handle the door hands back.
pub(crate) async fn booted(jojobot: &Jojobot, name: &str) -> String {
    sid_of(&boot(jojobot, name).await).unwrap_or_else(|| panic!("{name} booted without a handle"))
}

/// **Boot a run that works in a named zone**, and take its handle.
///
/// `resume` answers the choice a second boot of one bot is handed: a bot may
/// have several runs at once, which is what lets one case hold two of them in
/// two zones.
pub(crate) async fn booted_in(
    jojobot: &Jojobot,
    name: &str,
    zone: &str,
    resume: Option<&str>,
) -> String {
    let resume = resume.map(|answer| resume_answer(jojobot, answer));
    let body = json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: Some(zone.into()),
                bot: Some(name.into()),
                brief: Some(true),
                skill: None,
                section: None,
                resume,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
            }))
            .await
            .expect("the boot call is ok"),
    );
    sid_of(&body).unwrap_or_else(|| panic!("{name} booted in {zone} without a handle: {body}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::capture::CaptureArgs;
    use crate::memory::testing::*;

    /// **The shipped keys, asked of one handler**: a key that holds a handle
    /// refuses text that is no handle and nothing is written, and a handle that
    /// exists lands. A work item's status refuses nothing: a word outside the
    /// shipped five lands and the answer names it, and a word inside lands quiet.
    async fn holds_the_shipped_keys(jojobot: &Jojobot, label: &str) {
        ensure(jojobot, "work:phi").await;
        ensure(jojobot, "person:lisa").await;
        let said = |key: &str, value: &str| CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some([(key.to_string(), value.to_string())].into_iter().collect()),
            ..capture_args("work:phi", "a value for the guard to judge")
        };
        let refused = jojobot
            .capture(Parameters(said("owner", "not a handle")))
            .await
            .expect("capture call ok");
        let refused = json_of(&refused);
        assert_eq!(
            refused["status"], "blocked",
            "{label}: text that is no handle was accepted under a reference key: {refused}"
        );
        assert!(
            fields_of(jojobot, "work:phi").await.get("owner").is_none(),
            "{label}: a refused owner left a trace"
        );
        // The positive the refusal rests on: a handle that exists lands, so the
        // refusal above is about the value.
        capture_ok(jojobot, said("owner", "person:lisa")).await;
        assert_eq!(
            fields_of(jojobot, "work:phi").await["owner"],
            "person:lisa",
            "{label}"
        );
        // A status word outside the five lands, and the answer names it.
        let landed = json_of(
            &jojobot
                .capture(Parameters(said("status", "nonsense")))
                .await
                .expect("capture call ok"),
        );
        assert!(
            landed["status_word_note"]
                .as_str()
                .is_some_and(|note| note.contains("nonsense")),
            "{label}: a status outside the vocabulary was not named: {landed}"
        );
        assert_eq!(
            fields_of(jojobot, "work:phi").await["status"],
            "nonsense",
            "{label}"
        );
        // One inside the five lands with nothing to say.
        let quiet = json_of(
            &jojobot
                .capture(Parameters(said("status", "now")))
                .await
                .expect("capture call ok"),
        );
        assert!(quiet.get("status_word_note").is_none(), "{label}: {quiet}");
        assert_eq!(
            fields_of(jojobot, "work:phi").await["status"],
            "now",
            "{label}"
        );
    }

    /// **Every plain handler a verb test can reach holds the shipped keys.** The
    /// plain handler used to hold the shipped kinds with no keys, so a value a
    /// key does not hold was accepted with a write receipt and a test whose
    /// point was a kind's keys could not fail. Each constructor that builds its
    /// own store is asked, so one that falls back to a bare store reddens here by
    /// name.
    #[tokio::test]
    async fn every_plain_handler_holds_the_shipped_keys_and_lets_a_status_word_land() {
        for (label, jojobot) in plain_handlers() {
            holds_the_shipped_keys(&jojobot, label).await;
        }
    }

    /// **Every constructor that builds its own store**, by name.
    fn plain_handlers() -> Vec<(&'static str, Jojobot)> {
        vec![
            ("handler", handler()),
            ("handler_mentioning", handler_mentioning()),
            (
                "handler_field_provisioned",
                handler_field_provisioned(EntityId("work:sigma".into()), "note", "x"),
            ),
            ("handler_indexed", handler_indexed()),
            ("handler_with", handler_with(Arc::new(SpySearch::default()))),
        ]
    }

    /// One write on a project, carrying `fields`, as a caller sends it.
    async fn project_write(
        jojobot: &Jojobot,
        subject: &str,
        fields: &[(&str, &str)],
    ) -> serde_json::Value {
        let sent = CaptureArgs {
            provenance: Some("testimony".into()),
            sets: Some(
                fields
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            ),
            ..capture_args(subject, "where the project stands")
        };
        json_of(
            &jojobot
                .capture(Parameters(sent))
                .await
                .expect("capture call ok"),
        )
    }

    /// **A project's own columns, asked of one handler.** A project filed under
    /// no project is governed by its own list: a word outside the shipped five
    /// lands and names the project until the project lists it, and one write
    /// can carry the list and a status that uses it. A project filed under it
    /// keeps that list, and a list of its own does not widen it.
    async fn holds_the_project_columns(jojobot: &Jojobot, label: &str) {
        ensure(jojobot, "project:atlas").await;
        let status_of = |subject: &'static str| async move {
            fields_of(jojobot, subject)
                .await
                .get("status")
                .and_then(|v| v.as_str().map(str::to_string))
        };

        // No list: the shipped five hold, and the note names the project whose
        // list a word is written on.
        let named = project_write(jojobot, "project:atlas", &[("status", "inbox")]).await;
        assert!(
            named["status_word_note"]
                .as_str()
                .is_some_and(|note| note.contains("project:atlas")),
            "{label}: a word no list names did not name the project that governs: {named}"
        );
        assert_eq!(
            status_of("project:atlas").await.as_deref(),
            Some("inbox"),
            "{label}: a word no list names did not land"
        );

        // One write carries the list and a status that uses it, and says nothing.
        let landed = project_write(
            jojobot,
            "project:atlas",
            &[
                ("columns", "inbox, someday, next, now, waiting, done"),
                ("status", "inbox"),
            ],
        )
        .await;
        assert_ne!(landed["status"], "blocked", "{label}: {landed}");
        assert!(
            landed.get("status_word_note").is_none(),
            "{label}: {landed}"
        );

        // A word the list does not name lands and the list is named; one it
        // does name lands with nothing to say.
        let outside = project_write(jojobot, "project:atlas", &[("status", "backlog")]).await;
        assert!(
            outside["status_word_note"]
                .as_str()
                .is_some_and(|note| note.contains("inbox")),
            "{label}: a word outside the list did not name the list: {outside}"
        );
        let kept = project_write(jojobot, "project:atlas", &[("status", "now")]).await;
        assert_ne!(kept["status"], "blocked", "{label}: {kept}");
        assert!(kept.get("status_word_note").is_none(), "{label}: {kept}");

        // A project filed under it is held to that list.
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("project:atlas".into()),
                ..add_args("project", "the-shed", "The Shed")
            }))
            .await
            .expect("add_entity call ok");
        let inherited = project_write(jojobot, "project:the-shed", &[("status", "inbox")]).await;
        assert!(
            inherited.get("status_word_note").is_none(),
            "{label}: a project under atlas did not take atlas's list: {inherited}"
        );
        let widened = project_write(
            jojobot,
            "project:the-shed",
            &[
                ("columns", "inbox, someday, next, now, waiting, done, shed"),
                ("status", "shed"),
            ],
        )
        .await;
        assert!(
            widened["status_word_note"]
                .as_str()
                .is_some_and(|note| note.contains("project:atlas") && note.contains("inbox")),
            "{label}: a list of its own widened the parent's: {widened}"
        );
    }

    /// **Every plain handler holds a project to its own columns.** The shared
    /// contract case covers the store and the double; this asks the handlers a
    /// verb test defaults to, through the verbs a caller uses, so one that
    /// falls back to a store which declares no keys reddens by name.
    #[tokio::test]
    async fn every_plain_handler_holds_a_project_to_its_own_columns() {
        for (label, jojobot) in plain_handlers() {
            holds_the_project_columns(&jojobot, label).await;
        }
    }
}
