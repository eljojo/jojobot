//! Rules and the engine's records share a bot, but only rules bind it.

use super::support::{capture, ensure};
use super::*;

const BOT: &str = "bot:smithers";

pub async fn rules_index_records<M: Memory + ?Sized>(store: &M) -> Vec<Fact> {
    let bot = EntityId(BOT.into());
    ensure(store, &EntityId("work:delta".into())).await;
    let mut written = Vec::new();
    for (words, fields, sets) in [
        ("A carried rule".into(), vec![("starred", "true")], vec![]),
        ("An unseated rule".into(), vec![], vec![]),
        (
            format!("Reviewed @{BOT}."),
            vec![("review_mark", "true")],
            vec![],
        ),
        (
            "What this bot is working on now".into(),
            vec![("working_on", "work:delta")],
            vec!["working_on"],
        ),
        (
            format!("Set example_mode on @{BOT}."),
            vec![("example_mode", "on")],
            vec!["example_mode"],
        ),
    ] {
        written.push(
            capture(
                store,
                NewFact {
                    fields: fields
                        .into_iter()
                        .map(|(k, v)| (k.into(), v.into()))
                        .collect(),
                    sets: sets.into_iter().map(String::from).collect(),
                    ..NewFact::about(bot.clone(), words, date(2026, 7, 1))
                },
            )
            .await,
        );
    }
    let ids: std::collections::BTreeSet<_> = written.iter().map(|f| f.id.clone()).collect();
    store
        .recall(&bot)
        .await
        .expect("the bot reads")
        .into_iter()
        .filter(|fact| ids.contains(&fact.id))
        .collect()
}

pub async fn rules_index_counts_only_rules<M: Memory>(store: &M) {
    let records = rules_index_records(store).await;
    assert_eq!(records.len(), 5, "all fixture records reached the store");
    let rules = crate::memory::rules_in_force(&records);
    let mut words: Vec<_> = rules.iter().map(|f| f.content.as_str()).collect();
    words.sort_unstable();
    assert_eq!(words, ["A carried rule", "An unseated rule"]);
    let seats = crate::memory::seats::carried_seats_status(&rules, 1).unwrap();
    assert_eq!(seats.starred, 1);
}

pub async fn rules_index_preserves_authored_settings<M: Memory>(store: &M) {
    use crate::memory::review::{SETTINGS_RECORD, is_rule};
    let records = rules_index_records(store).await;
    let settings = records
        .iter()
        .find(|f| f.content.starts_with("Set "))
        .unwrap();
    assert!(
        !is_rule(settings),
        "the exact legacy settings sentence is bookkeeping"
    );
    let mut authored = settings.clone();
    authored.content = "Keep every assertion when editing release workflows.".into();
    assert!(is_rule(&authored), "authored settings need no rule label");
    let mut labelled = authored.clone();
    labelled
        .fields
        .insert(SETTINGS_RECORD.into(), "true".into());
    assert!(
        !is_rule(&labelled),
        "generated creation words need their engine label"
    );
    for key in ["subject", "purpose", "starred"] {
        let mut own_label = settings.clone();
        own_label.fields.insert(key.into(), "true".into());
        assert!(
            is_rule(&own_label),
            "an own {key} makes this an authored claim"
        );
    }
    let mut testimony = settings.clone();
    testimony.provenance = Provenance::Testimony;
    assert!(is_rule(&testimony));
    let mut details = settings.clone();
    details.details = Some("The caller's explanation.".into());
    assert!(is_rule(&details));
    let mut no_sets = settings.clone();
    no_sets.sets.clear();
    assert!(is_rule(&no_sets));
    let mut cleared_work = records
        .iter()
        .find(|f| f.content == "What this bot is working on now")
        .unwrap()
        .clone();
    cleared_work.fields.clear();
    cleared_work.sets.clear();
    assert!(
        !is_rule(&cleared_work),
        "clearing work does not turn its record into a rule"
    );
    let mut engine = authored.clone();
    engine
        .fields
        .insert("engine_record".into(), "restore".into());
    assert!(!is_rule(&engine));
    let mut reviewed = records
        .iter()
        .find(|f| f.fields.contains_key("review_mark"))
        .unwrap()
        .clone();
    reviewed.fields.insert("starred".into(), "true".into());
    assert!(
        !is_rule(&reviewed),
        "starring bookkeeping does not give it a rule seat"
    );
    authored.status = FactStatus::Archived;
    assert!(!is_rule(&authored));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn rules_index_fixture_joins_the_chart_heads_shared_collection() {
        let store = InMemoryMemory::booted();
        super::super::the_head_of_a_chart_places_itself(&store).await;
        rules_index_counts_only_rules(&store).await;
        rules_index_preserves_authored_settings(&store).await;
    }
}
