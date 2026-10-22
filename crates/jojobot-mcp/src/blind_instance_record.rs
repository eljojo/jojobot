//! **A read of the instance's record that fails is never a record that says
//! nothing** (rule 130). Each case runs two handlers over one store, the second
//! unable to read `topic:instance`, and puts the failed read beside the healthy
//! read of the same call: the verb answers a failure on the blind handler, and
//! what the healthy handler answers is the thing the failure must not turn into.

use crate::harness::*;
use crate::mailboxes::list_sent::ListSentArgs;
use crate::mailboxes::testing::name_the_operator;
use crate::memory::RenameEntityArgs;
use crate::memory::testing::*;
use crate::orientation::start_here::OrientArgs;
use rmcp::handler::server::wrapper::Parameters;

fn blind_and_healthy() -> (crate::Jojobot, crate::Jojobot) {
    healthy_and_down(Down::FieldsOf("topic:instance"))
}

/// A piece of work `bot:otto` owns, at `next`.
async fn owned_work(jojobot: &crate::Jojobot, slug: &str) {
    let sid = writing_as(jojobot);
    let added = jojobot
        .add_entity(Parameters(crate::memory::AddEntityArgs {
            sets: Some(
                [
                    ("status".to_string(), "next".to_string()),
                    ("owner".to_string(), "bot:otto".to_string()),
                ]
                .into(),
            ),
            sid: Some(sid),
            ..add_args("work", slug, slug)
        }))
        .await
        .expect("add_entity call ok");
    assert_ne!(json_of(&added)["status"], "blocked", "{added:?}");
}

async fn move_to(jojobot: &crate::Jojobot, work: &str, word: &str) -> serde_json::Value {
    capture_ok(
        jojobot,
        crate::memory::CaptureArgs {
            sets: Some([("status".to_string(), word.to_string())].into()),
            ..capture_args(work, "moves the status")
        },
    )
    .await
}

/// **Owned work moved to a word the instance maps to `now`, over an unreadable
/// instance record, leaves `working_on` where it was and says so.** An instance
/// can map its own words, so a blind read cannot say what such a word means and the
/// write lands without the fill. With the record readable the same write points
/// `working_on` at the work. **A shipped word means itself whatever the instance
/// maps**, so the shipped `now` over the same unreadable record reads nothing: it
/// fills `working_on` and its receipt says nothing was left unworked.
#[tokio::test]
async fn an_instance_word_for_now_over_an_unreadable_instance_record_does_not_fill_working_on() {
    let (healthy, blind) = blind_and_healthy();
    ensure(&healthy, "bot:otto").await;
    ensure(&healthy, "topic:instance").await;
    capture_ok(
        &healthy,
        crate::memory::CaptureArgs {
            sets: Some(
                [(
                    format!("{}doing", jojobot_domain::memory::kinds::STATUS_WORD),
                    "now".to_string(),
                )]
                .into(),
            ),
            provenance: Some("testimony".into()),
            ..capture_args("topic:instance", "maps doing to now")
        },
    )
    .await;
    owned_work(&healthy, "gamma").await;
    owned_work(&healthy, "delta").await;
    owned_work(&healthy, "epsilon").await;
    let working_on = || async {
        healthy
            .memory
            .fields(&jojobot_domain::memory::EntityId("bot:otto".into()))
            .await
            .expect("the bot reads")
            .get("working_on")
            .cloned()
    };

    let blind_landed = move_to(&blind, "work:gamma", "doing").await;
    assert_eq!(fields_of(&healthy, "work:gamma").await["status"], "doing");
    assert_eq!(working_on().await, None, "{blind_landed}");
    assert!(
        blind_landed["instance_unread"]["not_worked_out"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key == "working_on")),
        "{blind_landed}"
    );

    let shipped = move_to(&blind, "work:delta", "now").await;
    assert_eq!(working_on().await.as_deref(), Some("work:delta"));
    assert!(shipped["instance_unread"].is_null(), "{shipped}");

    let landed = move_to(&healthy, "work:epsilon", "doing").await;
    assert_eq!(working_on().await.as_deref(), Some("work:epsilon"));
    assert!(landed["instance_unread"].is_null(), "{landed}");
}

async fn list_sent_to(jojobot: &crate::Jojobot, to: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .list_sent(Parameters(ListSentArgs {
                sender: None,
                to: Some(to.into()),
                limit: None,
                offset: None,
                include_bodies: None,
                sid: Some(as_bot(jojobot, "epsilon")),
            }))
            .await
            .expect("an answer"),
    )
}

/// **`list_sent` to an addressee whose box has not opened, over an unreadable
/// instance record, is a failed read** that names the operator question and
/// says to retry: it cannot tell the operator from a stranger. Healthy, the
/// operator resolves to an empty list and a stranger is "no such addressee".
#[tokio::test]
async fn list_sent_to_an_unopened_box_over_an_unreadable_instance_record_is_a_failed_read() {
    let (healthy, blind) = blind_and_healthy();
    crate::mailboxes::testing::owning(&healthy, "epsilon").await;
    name_the_operator(&healthy, "milhouse").await;

    let failed = list_sent_to(&blind, "person:milhouse").await;
    assert_eq!(failed["status"], "blocked", "{failed}");
    assert_eq!(failed["fix_by"], "retry", "{failed}");
    assert!(
        failed["how_to_proceed"]
            .as_str()
            .is_some_and(|said| said.contains("operator") && !said.contains("No such addressee")),
        "{failed}"
    );

    let operator = list_sent_to(&healthy, "person:milhouse").await;
    assert_ne!(operator["status"], "blocked", "{operator}");
    let stranger = list_sent_to(&healthy, "person:nelson").await;
    assert_eq!(stranger["status"], "blocked", "{stranger}");
    assert_eq!(stranger["fix_by"], "change", "{stranger}");
}

async fn rename_to(jojobot: &crate::Jojobot, handle: &str, to: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .rename_entity(Parameters(RenameEntityArgs {
                handle: handle.into(),
                to: to.into(),
                parent: None,
                recorded_at: None,
                override_token: None,
                override_reason: None,
                request_id: None,
                sid: Some(writing_as(jojobot)),
            }))
            .await
            .expect("an answer"),
    )
}

/// **A rename out of the person kind over an unreadable instance record is
/// refused with retry and renames nothing**, because it cannot tell whether the
/// person is the operator. Healthy, renaming the operator out of the person kind
/// is refused as before, and a person who is not the operator is renamed.
#[tokio::test]
async fn a_rename_out_of_the_person_kind_over_an_unreadable_instance_record_is_refused_with_retry()
{
    let (healthy, blind) = blind_and_healthy();
    name_the_operator(&healthy, "milhouse").await;
    ensure(&healthy, "person:nelson").await;

    let failed = rename_to(&blind, "person:nelson", "bot:nelson").await;
    assert_eq!(failed["status"], "blocked", "{failed}");
    assert_eq!(failed["fix_by"], "retry", "{failed}");
    assert!(
        fields_of(&healthy, "person:nelson").await.is_object(),
        "nothing was renamed: {failed}"
    );

    let operator = rename_to(&healthy, "person:milhouse", "bot:milhouse").await;
    assert_eq!(operator["status"], "blocked", "{operator}");
    assert_ne!(operator["fix_by"], "retry", "{operator}");
    let renamed = rename_to(&healthy, "person:nelson", "bot:nelson").await;
    assert_ne!(renamed["status"], "blocked", "{renamed}");
}

async fn ask_for_code(jojobot: &crate::Jojobot) -> serde_json::Value {
    json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: Some(true),
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("an answer"),
    )
}

/// **Asking for a code over an unreadable instance record fails as a failed
/// read and writes no challenge.** Healthy, the same ask writes one: the ask
/// that follows the blind one is not turned back for a challenge already
/// standing.
#[tokio::test]
async fn an_ask_for_a_code_over_an_unreadable_instance_record_writes_no_challenge() {
    let (healthy, blind) = blind_and_healthy();
    name_the_operator(&healthy, "milhouse").await;
    make_bot(&healthy, "gamma").await;

    let failed = ask_for_code(&blind).await;
    assert_eq!(failed["status"], "blocked", "{failed}");
    assert_eq!(failed["fix_by"], "retry", "{failed}");
    assert!(failed["challenge"].is_null(), "{failed}");

    let asked = ask_for_code(&healthy).await;
    assert!(asked["challenge"]["handle"].is_string(), "{asked}");
}

/// **No operator recorded and an unreadable record are told apart in one
/// boot.** Healthy, an instance that names no operator is told to name one, by
/// the route that does it. With the record unreadable the same boot says the
/// record could not be read and never says to name an operator. Once an operator
/// is named, the healthy boot names the person.
#[tokio::test]
async fn the_boot_tells_no_operator_recorded_from_an_unreadable_record() {
    let (healthy, blind) = blind_and_healthy();
    crate::mailboxes::testing::owning(&healthy, "epsilon").await;

    let absent = boot_answering_dated(&healthy, "epsilon", "new", None, None).await;
    let said = absent["operator"].as_str().unwrap_or_default();
    assert!(said.contains("add_entity"), "{absent}");

    let unread = boot_answering_dated(&blind, "epsilon", "new", None, None).await;
    let said = unread["operator"].as_str().unwrap_or_default();
    assert!(
        said.contains("topic:instance") && !said.contains("add_entity"),
        "{unread}"
    );

    name_the_operator(&healthy, "milhouse").await;
    let named = boot_answering_dated(&healthy, "epsilon", "new", None, None).await;
    assert_eq!(named["operator"], "person:milhouse", "{named}");
}
