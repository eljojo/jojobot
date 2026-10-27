//! **A receipt over a store that cannot say which keys a record holds off its
//! thing does not tell the caller that every key reached it.** The keys a record
//! sets beside its own fields stay on the record, and the store says which; when it
//! cannot, the receipt names the account it could not make (rule 130).

use super::testing::*;
use super::*;
use crate::harness::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

/// **A store that answers everything but which keys a record holds off its thing**: every write lands and the fold behind it never confirms — the surface's
/// own view of what `Folded` returns when its refresh fails after a
/// successful inner write.** `jojobot-adapters`' own tests prove `Folded`
/// converts that shape into [`MemoryError::FoldBehind`]; this fake produces
/// the same error directly, so a verb's handling of it is tested without
/// standing up a real fold and a store that fails on cue underneath it.
struct KeysKeptUnreadable(pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for KeysKeptUnreadable {
    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        let _ = address;
        Err(MemoryError::Store(
            "the keys held off the thing cannot be read".into(),
        ))
    }
}

/// **A receipt over a store that cannot say which keys it holds off the thing does
/// not claim every key reached it.** A key sent as the claim's own field stays on the
/// claim; with the store silent about which, the old receipt said it moved to the
/// thing. The write lands, the receipt claims no key reached, and it names the
/// account it could not make. Paired with the healthy store, which says the own field
/// stayed and the setting reached.
#[tokio::test]
async fn a_receipt_over_a_store_that_cannot_place_the_keys_does_not_claim_they_reached() {
    let memory = Arc::new(InMemoryMemory::seeded());
    let blind = Jojobot::new(
        Arc::new(KeysKeptUnreadable(memory.clone())),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let healthy = handler_over(memory);
    ensure(&healthy, "person:homer").await;
    let write = || CaptureArgs {
        fields: Some([("mood".to_string(), "glad".to_string())].into()),
        sets: Some([("favourite".to_string(), "donuts".to_string())].into()),
        ..capture_args("person:homer", "homer likes donuts")
    };

    let told = capture_ok(&healthy, write()).await;
    assert!(told.get("reads_left").is_none(), "{told}");
    assert_eq!(
        told["reached_the_thing"],
        serde_json::json!(["favourite"]),
        "the setting reached the thing, and the receipt lists it: {told}"
    );
    assert!(
        told["postcondition"]
            .as_str()
            .unwrap_or_default()
            .contains("favourite"),
        "the setting reached the thing: {told}"
    );

    let blind_told = json_of(
        &blind
            .capture(Parameters(write()))
            .await
            .expect("the write lands"),
    );
    assert!(
        blind_told["reached_the_thing"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "no key is listed as having reached the thing: {blind_told}"
    );
    let left: Vec<&str> = blind_told["reads_left"]["not_worked_out"]
        .as_array()
        .map(|left| left.iter().filter_map(|l| l["what"].as_str()).collect())
        .unwrap_or_default();
    assert!(left.contains(&"reach_note"), "{blind_told}");
}
