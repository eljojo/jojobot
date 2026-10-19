use super::support::{capture, edit, ensure, other_caller};
use super::*;
use crate::app::{AppOutcome, AppStamp, AuthorizedWrite, DeployEvent};

fn stamp(app: &str) -> AppStamp {
    AppStamp {
        app: app.to_string(),
        grant: "record_deploy_event".to_string(),
        received_at: "2026-10-18T12:00:00Z".to_string(),
    }
}

async fn read_back<M: Memory>(store: &M, subject: &EntityId, id: &FactId) -> Fact {
    store
        .recall(subject)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| &f.id == id)
        .unwrap_or_else(|| panic!("recall must return the captured fact (id {id})"))
}

/// **The mark an app's write carries is kept with the claim, on every read of
/// it and on every write of it, through an archive.** A claim a bot wrote and
/// that names the app in `read_from` carries no mark.
pub async fn an_app_mark_is_kept_and_a_bot_naming_the_app_has_none<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-app-mark".into());
    let mut marked = NewFact::about(
        subject.clone(),
        "deploy event: commit abc1234",
        date(2026, 10, 1),
    );
    marked.provenance = Provenance::Observation;
    marked
        .fields
        .insert("read_from".into(), "springfield-ci".into());
    marked.app_stamp = Some(stamp("springfield-ci"));
    let written = capture(store, marked).await;
    assert_eq!(written.app_stamp, Some(stamp("springfield-ci")));
    assert_eq!(
        read_back(store, &subject, &written.id).await.app_stamp,
        Some(stamp("springfield-ci")),
        "a read of the claim carries the mark"
    );
    let writes = store
        .claim_history(&written.address())
        .await
        .expect("reads");
    assert_eq!(writes[0].app_stamp, Some(stamp("springfield-ci")));

    let archived = edit(
        store,
        &written.address(),
        FactPatch {
            status: Some(FactStatus::Archived),
            details: Some("superseded by a later run".into()),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(archived.app_stamp, Some(stamp("springfield-ci")));
    let after = store
        .claim_history(&written.address())
        .await
        .expect("reads");
    assert!(after.len() > 1, "the archive is a write of its own");
    assert!(
        after
            .iter()
            .all(|w| w.app_stamp == Some(stamp("springfield-ci"))),
        "every write of the claim carries the mark"
    );

    let mut by_a_bot = NewFact::about(
        subject.clone(),
        "a bot read the deploy page",
        date(2026, 10, 2),
    );
    by_a_bot.provenance = Provenance::Observation;
    by_a_bot
        .fields
        .insert("read_from".into(), "springfield-ci".into());
    let theirs = capture(store, by_a_bot).await;
    assert_eq!(
        theirs.app_stamp, None,
        "naming the app is not being the app"
    );
    assert_eq!(read_back(store, &subject, &theirs.id).await.app_stamp, None);
}

/// **Nobody rewrites an app's claim in place**, its words or its fields; the
/// refusal names the route, and a patch that archives it lands.
pub async fn an_app_claim_is_not_rewritten_in_place<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-app-rewrite".into());
    let mut marked = NewFact::about(
        subject.clone(),
        "deploy event: host host-alpha",
        date(2026, 10, 1),
    );
    marked.provenance = Provenance::Observation;
    marked
        .fields
        .insert("read_from".into(), "springfield-ci".into());
    marked.fields.insert("host".into(), "host-alpha".into());
    marked.app_stamp = Some(stamp("springfield-ci"));
    let written = capture(store, marked).await;

    for patch in [
        FactPatch {
            content: Some("deploy event: host host-beta".into()),
            provenance: Some(Provenance::Observation),
            ..FactPatch::default()
        },
        FactPatch {
            fields: [("host".to_string(), "host-beta".to_string())].into(),
            ..FactPatch::default()
        },
    ] {
        let refused = store
            .update_fact(&written.address(), patch, &other_caller())
            .await
            .expect_err("an app's claim is not rewritten in place");
        assert!(
            matches!(refused, MemoryError::AppClaimRewritten { .. }),
            "{refused:?}"
        );
    }
    let untouched = read_back(store, &subject, &written.id).await;
    assert_eq!(untouched.content, "deploy event: host host-alpha");
    assert_eq!(
        untouched.fields.get("host").map(String::as_str),
        Some("host-alpha")
    );
}

/// **A folded value names the app that recorded it** in its backing, and a
/// value a bot wrote beside it does not.
pub async fn a_folded_values_backing_tells_the_app_from_a_bot<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-app-backing".into());
    let mut by_app = NewFact::about(
        subject.clone(),
        "deploy event: host host-alpha",
        date(2026, 10, 1),
    );
    by_app.provenance = Provenance::Observation;
    by_app
        .fields
        .insert("read_from".into(), "springfield-ci".into());
    by_app.fields.insert("host".into(), "host-alpha".into());
    by_app.sets.insert("host".into());
    by_app.app_stamp = Some(stamp("springfield-ci"));
    capture(store, by_app).await;
    let mut by_bot = NewFact::about(subject.clone(), "a bot noted the unit", date(2026, 10, 2));
    by_bot.fields.insert("unit".into(), "web".into());
    by_bot.sets.insert("unit".into());
    capture(store, by_bot).await;

    let backing = store.backing(&subject).await.expect("backing");
    assert_eq!(
        backing.get("host").and_then(|b| b.recorded_by.clone()),
        Some(stamp("springfield-ci"))
    );
    assert!(
        backing.contains_key("unit"),
        "the bot's value has a backing"
    );
    assert_eq!(backing["unit"].recorded_by, None);
    let history = store.history(&subject, "host").await.expect("history");
    assert_eq!(history[0].recorded_by, Some(stamp("springfield-ci")));
}

fn principal() -> crate::app::AppPrincipal {
    crate::app::AppPrincipal::from_spec(
        &crate::app::AppSpec {
            name: "springfield-ci".into(),
            operation: "record_deploy_event".into(),
            thread: "thread:contract-deploys".into(),
            kind: "event".into(),
            keys: vec!["commit".into(), "host".into()],
            revoked: false,
        },
        &crate::app::hash_token(&crate::app::mint_token()),
    )
    .expect("a plain grant")
}

fn authorized(attempt: &str, commit: &str, host: &str) -> AuthorizedWrite {
    crate::app::authorize(
        &principal(),
        &DeployEvent {
            attempt_id: attempt.into(),
            fields: [
                ("commit".to_string(), commit.to_string()),
                ("host".to_string(), host.to_string()),
            ]
            .into(),
        },
    )
    .expect("inside the grant")
}

async fn events_under<M: Memory>(store: &M, thread: &EntityId) -> Vec<Entity> {
    store
        .list_entities(Some(EntityKind::EVENT))
        .await
        .expect("lists")
        .into_iter()
        .filter(|e| e.parent.as_ref() == Some(thread))
        .collect()
}

/// **An app's event lands once.** The first call writes a child event of the
/// thread whose claim is the server's words, an observation from the app, with
/// the mark; the same attempt with the same body answers the first receipt and
/// writes nothing; the same attempt with another body is a conflict and writes
/// nothing; and a second attempt for the same commit and host is a second
/// event.
pub async fn an_app_event_lands_once_and_a_retry_writes_nothing<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-deploys".into());
    ensure(store, &thread).await;
    let today = date(2026, 10, 18);
    let send = |write: AuthorizedWrite| {
        write.into_event("record_deploy_event", "2026-10-18T12:00:00Z".into(), today)
    };

    let first = send(authorized("run-1", "abc1234", "host-alpha"));
    let (entity, fact) = store
        .record_app_event(first.clone())
        .await
        .expect("lands")
        .landed()
        .expect("a new attempt must land");
    assert_eq!(entity.parent.as_ref(), Some(&thread));
    assert_eq!(fact.provenance, Provenance::Observation);
    assert_eq!(
        fact.app_stamp.as_ref().map(|s| s.app.as_str()),
        Some("springfield-ci")
    );
    assert_eq!(
        fact.fields.get("read_from").map(String::as_str),
        Some("springfield-ci")
    );
    assert_eq!(
        fact.fields.get("commit").map(String::as_str),
        Some("abc1234")
    );
    let fold = store.fields(&entity.id).await.expect("fields");
    assert_eq!(fold.get("host").map(String::as_str), Some("host-alpha"));
    assert_eq!(events_under(store, &thread).await.len(), 1);

    // The same attempt, sent again as a new event value: the store answers from
    // the first receipt and writes nothing.
    let again = send(authorized("run-1", "abc1234", "host-alpha"));
    let AppOutcome::Repeat(receipt) = store.record_app_event(again).await.expect("answers") else {
        panic!("the same attempt must repeat");
    };
    assert_eq!(receipt.handle, entity.id.to_string());
    assert_eq!(receipt.address, fact.address().to_string());
    assert_eq!(
        events_under(store, &thread).await.len(),
        1,
        "a retry writes nothing"
    );

    let different = send(authorized("run-1", "abc1234", "host-beta"));
    assert!(matches!(
        store.record_app_event(different).await.expect("answers"),
        AppOutcome::Conflict
    ));
    assert_eq!(
        events_under(store, &thread).await.len(),
        1,
        "a conflict writes nothing"
    );

    // A redeploy of the same commit to the same host is a second attempt.
    let redeploy = send(authorized("run-2", "abc1234", "host-alpha"));
    assert!(
        store
            .record_app_event(redeploy)
            .await
            .expect("lands")
            .landed()
            .is_some()
    );
    assert_eq!(events_under(store, &thread).await.len(), 2);

    // Four more under the one thread all land: no sibling name is near another.
    for n in 0..4 {
        let more = send(authorized(
            &format!("run-more-{n}"),
            "abc1234",
            "host-gamma",
        ));
        assert!(
            store
                .record_app_event(more)
                .await
                .expect("lands")
                .landed()
                .is_some(),
            "event {n} lands"
        );
    }
    assert_eq!(events_under(store, &thread).await.len(), 6);
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_app<M: Memory>(store: &M) {
    an_app_mark_is_kept_and_a_bot_naming_the_app_has_none(store).await;
    an_app_claim_is_not_rewritten_in_place(store).await;
    a_folded_values_backing_tells_the_app_from_a_bot(store).await;
    an_app_event_lands_once_and_a_retry_writes_nothing(store).await;
}
