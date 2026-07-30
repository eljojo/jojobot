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
use rmcp::handler::server::wrapper::Parameters;

pub(crate) fn handler() -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::new()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(sid::SessionRegistry::new()),
    )
}

/// A handler whose search port is a spy the test keeps a handle on.
pub(crate) fn handler_with(spy: Arc<SpySearch>) -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::new()),
        spy,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(sid::SessionRegistry::new()),
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
        .mint(&EntityId::new(EntityKind::Bot, bot), None)
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
    let result = jojobot
        .add_entity(Parameters(add_args("bot", slug, slug)))
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

pub(crate) async fn boot(jojobot: &Jojobot, name: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                bot: Some(name.into()),
                brief: None,
                resume: None,
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
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                bot: Some(name.into()),
                brief: None,
                resume: Some(answer.into()),
            }))
            .await
            .expect("the boot call is ok"),
    )
}

/// The handle a boot handed back, or `None` when it handed none back.
pub(crate) fn sid_of(body: &serde_json::Value) -> Option<String> {
    body["session"]["sid"].as_str().map(str::to_string)
}

/// Boot as this bot and take the handle the door hands back.
pub(crate) async fn booted(jojobot: &Jojobot, name: &str) -> String {
    sid_of(&boot(jojobot, name).await).unwrap_or_else(|| panic!("{name} booted without a handle"))
}
