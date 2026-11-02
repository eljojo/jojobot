use super::*;

#[test]
fn clear_backing_judges_only_active_writes_that_reach_the_thing() {
    let carried = BTreeMap::from([("mood".into(), "content".into())]);
    let writes = [KeyWrite {
        fact: FactId("f2".into()),
        ..wrote("mood", 2, Some("queasy"))
    }];
    let rows = [
        ("older setting", FactStatus::Active, None, true, false),
        ("archived setting", FactStatus::Archived, None, true, true),
        (
            "archiving setting",
            FactStatus::Active,
            Some(FactStatus::Archived),
            true,
            true,
        ),
        ("own field", FactStatus::Active, None, false, true),
    ];
    let mut outcomes = Vec::new();
    for (name, status, next_status, reaches, wanted) in rows {
        let mut edited = thought("f1", "work:sigma");
        edited.status = status;
        edited.fields = carried.clone();
        // The store's projection supplies the held key's write class: only the
        // own-field row has no reaching key, as a Stays write is served.
        if reaches {
            edited.reaching.insert("mood".into());
            edited.sets.insert("mood".into());
        }
        let patch = FactPatch {
            clear_fields: vec!["mood".into()],
            status: next_status,
            ..Default::default()
        };
        apply_fact_patch(&mut edited, &patch).expect("the patch is valid");
        let result = guard_clear_backing(&writes, &edited, &patch, &carried, &[]);
        if !wanted {
            assert!(matches!(
                result,
                Err(MemoryError::ClearsAnOlderWrite { .. })
            ));
        }
        outcomes.push((name, result.is_ok(), wanted));
    }
    assert!(
        outcomes.iter().all(|(_, got, wanted)| got == wanted),
        "clear-backing decisions (name, landed, wanted): {outcomes:?}"
    );
}

#[tokio::test]
async fn clear_backing_preserves_the_newer_setting_on_the_fake() {
    contract::a_clear_on_an_older_claim_is_refused_and_one_on_the_backing_claim_takes_the_key_off(
        &InMemoryMemory::booted(),
    )
    .await;
}
