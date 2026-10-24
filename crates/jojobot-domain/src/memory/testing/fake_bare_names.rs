//! A bare name under a reference key is refused by the in-memory store, as the
//! real store refuses it.

use super::*;
use jiff::civil::date;

#[tokio::test]
async fn a_bare_line_name_is_refused_in_the_fake() {
    use crate::memory::types::{DeclaredType, Field};
    let memory = InMemoryMemory::default();
    crate::memory::kinds::seed(&memory).await.expect("kinds");
    memory
        .declare_type(
            DeclaredType::new(
                "dispatch",
                vec![Field::pointing_at("line", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .expect("declare line");
    let work = EntityId("work:alpha-queue".into());
    let bot = EntityId("bot:gamma".into());
    for (id, name) in [(&work, "Alpha"), (&bot, "Gamma")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("created");
    }
    let line = |value: &str| {
        let mut fact = NewFact::about(work.clone(), "assigned a line", date(2026, 9, 1));
        fact.fields.insert("line".into(), value.into());
        fact.sets.insert("line".into());
        fact
    };
    let result = memory.capture(line("dev7")).await;
    eprintln!("bare capture: {result:?}; fake has no field_link table");
    let refused = result.expect_err("a bare name needs a full handle");
    assert!(
        matches!(&refused, MemoryError::InvalidFact(_)),
        "{refused:?}"
    );
    assert!(
        refused
            .to_string()
            .contains(&format!("{}:dev7", EntityKind::BOT.as_token())),
        "{refused}"
    );
    assert!(refused.to_string().contains("line"), "{refused}");
    assert!(memory.recall(&work).await.expect("records").is_empty());
    assert!(
        !memory
            .fields(&work)
            .await
            .expect("fields")
            .contains_key("line")
    );
    let captured = memory
        .capture(line(bot.as_str()))
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    assert_eq!(captured.fields["line"], bot.as_str());
    assert_eq!(
        memory.fields(&work).await.expect("fields")["line"],
        bot.as_str()
    );
    assert_eq!(
        memory
            .referring_to(&bot)
            .await
            .expect("links")
            .iter()
            .map(Fact::address)
            .collect::<Vec<_>>(),
        vec![captured.address()]
    );
}
