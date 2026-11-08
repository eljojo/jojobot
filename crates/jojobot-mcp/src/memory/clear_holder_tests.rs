//! **A clear whose holder cannot be read is refused**, whichever way the clear
//! would have gone. `clears_that_leave_the_thing` asks which claim holds a key.
//! When the store cannot say, the stored due moment and a bot request's generation
//! both refuse the write with the store error, because a guess either way names a
//! state that is not the state held, and the clear of the stored due moment needs
//! the same read. Each case is paired with the same call over a store that can
//! read.

use super::testing::{
    BackingDown, InMemoryMemory, SpySearch, capture_args, capture_ok, ensure, update_args,
};
use super::*;
use crate::harness::*;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

const REQUEST: &str = "bot-request:theta";

/// A handler over `memory` whose store cannot read the backing, writing as the
/// same run the healthy handler wrote as.
fn blind(memory: &Arc<InMemoryMemory>) -> Jojobot {
    let jojobot = Jojobot::new(
        Arc::new(BackingDown(memory.clone())),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    writing_as(&jojobot);
    jojobot
}

async fn claim_address(jojobot: &Jojobot, args: CaptureArgs) -> String {
    capture_ok(jojobot, args).await["address"]
        .as_str()
        .expect("an address")
        .to_string()
}

fn setting(key: &str, value: &str) -> Option<std::collections::BTreeMap<String, String>> {
    Some([(key.to_string(), value.to_string())].into_iter().collect())
}

fn cleared(address: &str, key: &str) -> crate::memory::UpdateFactArgs {
    crate::memory::UpdateFactArgs {
        clear_fields: Some(vec![key.to_string()]),
        ..update_args(address)
    }
}

/// Whether the claim at `address` on `thing` still carries `key`.
async fn carries(memory: &Arc<InMemoryMemory>, thing: &str, address: &str, key: &str) -> bool {
    memory
        .recall(&EntityId::person(thing))
        .await
        .expect("the claims read")
        .into_iter()
        .find(|fact| fact.address().to_string() == address)
        .is_some_and(|fact| fact.fields.contains_key(key))
}

/// **A request whose spec was set twice: the older claim is cleared while the
/// store cannot read the backing, and the write is refused.** The spec and its
/// generation are as they were. **Paired**: the same clear over a store that can
/// read lands, and it leaves the generation where it was, because the newer claim
/// holds the spec.
#[tokio::test]
async fn a_spec_clear_over_an_unreadable_backing_is_refused_and_a_readable_one_lands() {
    let memory = Arc::new(InMemoryMemory::booted());
    let healthy = handler_over(memory.clone());
    writing_as(&healthy);
    for handle in ["bot:otto", "bot:lisa", REQUEST] {
        ensure(&healthy, handle).await;
    }
    healthy
        .memory
        .capture(jojobot_domain::memory::NewFact {
            fields: [("reports_to".to_string(), "bot:otto".to_string())].into(),
            sets: ["reports_to".to_string()].into(),
            ..jojobot_domain::memory::NewFact::about(
                EntityId::person("bot:lisa"),
                "reports to otto",
                jiff::civil::date(2026, 10, 1),
            )
        })
        .await
        .expect("the chart is placed");
    let mut first = capture_args(REQUEST, "the first spec");
    first.sets = Some(
        [
            ("target_bot".to_string(), "bot:lisa".to_string()),
            ("desired".to_string(), "running".to_string()),
        ]
        .into_iter()
        .collect(),
    );
    let older = claim_address(&healthy, first).await;
    let mut second = capture_args(REQUEST, "the second spec");
    second.sets = setting("desired", "stopped");
    claim_address(&healthy, second).await;
    let fields = |memory: &Arc<InMemoryMemory>| {
        let memory = memory.clone();
        async move {
            memory
                .fields(&EntityId::person(REQUEST))
                .await
                .expect("the fields read")
        }
    };
    let before = fields(&memory).await;
    assert_eq!(before.get("desired").map(String::as_str), Some("stopped"));

    let refused = blind(&memory)
        .update_fact(Parameters(cleared(&older, "desired")))
        .await;
    assert!(refused.is_err(), "a holder nobody can read is refused");
    assert_eq!(fields(&memory).await, before, "no write, no new number");
    assert!(
        carries(&memory, REQUEST, &older, "desired").await,
        "the refused clear withdrew nothing"
    );

    let landed = json_of(
        &healthy
            .update_fact(Parameters(cleared(&older, "desired")))
            .await
            .expect("the call answers"),
    );
    assert_ne!(landed["status"], "blocked", "{landed}");
    assert!(
        !carries(&memory, REQUEST, &older, "desired").await,
        "the readable clear withdrew the older claim's write"
    );
    assert_eq!(
        fields(&memory).await,
        before,
        "the newer claim holds the spec, so a clear of the older one moves no number"
    );
}

/// A thing whose expiry was set twice, March then April; returns the address of the
/// March claim.
async fn two_expiries(jojobot: &Jojobot, thing: &str) -> String {
    ensure(jojobot, thing).await;
    let mut older = capture_args(thing, "Return window runs out 8 Mar 2026.");
    older.sets = setting("runs_out", "2026-03-08");
    let older = claim_address(jojobot, older).await;
    let mut newer = capture_args(thing, "Return window now runs out 8 Apr 2026.");
    newer.sets = setting("runs_out", "2026-04-08");
    claim_address(jojobot, newer).await;
    older
}

async fn due_on(memory: &Arc<InMemoryMemory>, thing: &str) -> Option<String> {
    memory
        .fields(&EntityId::person(thing))
        .await
        .expect("the fields read")
        .get("due_on")
        .cloned()
}

/// **A clear of an older expiry claim over a store that cannot read the backing
/// is refused and withdraws nothing**: the claim still carries its date and the
/// stored due moment is where it was. **Paired**: over a store that can read, the
/// same clear on a second desk lands, and the newer claim keeps the due moment.
#[tokio::test]
async fn an_expiry_clear_over_an_unreadable_backing_is_refused_and_a_readable_one_keeps_the_due_moment()
 {
    let memory = Arc::new(InMemoryMemory::booted());
    let healthy = handler_over(memory.clone());
    writing_as(&healthy);
    let blind_older = two_expiries(&healthy, "thing:standing-desk").await;
    let sighted_older = two_expiries(&healthy, "thing:boots").await;
    for desk in ["thing:standing-desk", "thing:boots"] {
        assert_eq!(due_on(&memory, desk).await.as_deref(), Some("2026-04-08"));
    }

    let refused = blind(&memory)
        .update_fact(Parameters(cleared(&blind_older, "runs_out")))
        .await;
    assert!(refused.is_err(), "a holder nobody can read is refused");
    assert!(
        carries(&memory, "thing:standing-desk", &blind_older, "runs_out").await,
        "the refused clear withdrew nothing"
    );
    assert_eq!(
        due_on(&memory, "thing:standing-desk").await.as_deref(),
        Some("2026-04-08")
    );

    let landed = json_of(
        &healthy
            .update_fact(Parameters(cleared(&sighted_older, "runs_out")))
            .await
            .expect("the call answers"),
    );
    assert_ne!(landed["status"], "blocked", "{landed}");
    assert!(
        !carries(&memory, "thing:boots", &sighted_older, "runs_out").await,
        "the readable clear withdrew the older claim's date"
    );
    assert_eq!(
        due_on(&memory, "thing:boots").await.as_deref(),
        Some("2026-04-08"),
        "the newer claim holds the date and the due moment stays"
    );
}
