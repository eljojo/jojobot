//! The rest of a project's page: what it is, what happened lately, what sits in it.

// TEMPORARY(9.3.8): this case seeds the in-memory store through the `Memory` port to
// get the records a page is handed. The port leaves with 9.3.8, and so does this allow.
#![allow(clippy::disallowed_types)]

use super::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{Memory, NewEntity, NewFact, Provenance};

fn day(day: i8) -> jiff::civil::Date {
    jiff::civil::date(2026, 4, day)
}

async fn put(memory: &InMemoryMemory, handle: &str, name: &str, parent: Option<&str>) {
    let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
    new.parent = parent.map(|parent| EntityId(parent.into()));
    memory
        .add_entity(new)
        .await
        .expect("the entity is written")
        .written()
        .expect("nothing collides with it");
}

async fn set(memory: &InMemoryMemory, project: &EntityId, key: &str, value: &str, how: Provenance) {
    let mut claim = NewFact::about(project.clone(), format!("the {key}"), day(1));
    claim.provenance = how;
    claim.fields = BTreeMap::from([(key.to_string(), value.to_string())]);
    claim.sets = [key.to_string()].into();
    memory.capture(claim).await.expect("the key is written");
}

/// The atlas project with an owner somebody said, a budget worked out, the
/// board's own `columns` key, twelve dated claims and two things under it of two
/// kinds, read back through the store as the page reads it.
async fn rendered() -> String {
    let memory = InMemoryMemory::booted();
    let atlas = EntityId("project:atlas".into());
    put(&memory, "person:alpha", "Alpha", None).await;
    put(&memory, "project:atlas", "Atlas", None).await;
    put(
        &memory,
        "thing:omega",
        "The tool bench",
        Some("project:atlas"),
    )
    .await;
    put(&memory, "work:tau", "Paint the wall", Some("project:atlas")).await;
    set(
        &memory,
        &atlas,
        "owner",
        "person:alpha",
        Provenance::Testimony,
    )
    .await;
    set(&memory, &atlas, "budget", "40k", Provenance::Inference).await;
    set(
        &memory,
        &atlas,
        "columns",
        "someday, next, now, waiting, done, considering, review",
        Provenance::Inference,
    )
    .await;
    for number in 1..=12 {
        memory
            .capture(NewFact::about(
                atlas.clone(),
                format!("Claim number {number:02}"),
                day(number),
            ))
            .await
            .expect("the claim is written");
    }
    let entities = memory.list_entities(None).await.expect("the index reads");
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();
    let children: Vec<&Entity> = entities
        .iter()
        .filter(|e| e.parent.as_ref() == Some(&atlas))
        .collect();
    project_rest(
        &memory.fields(&atlas).await.expect("fields read"),
        &memory.backing(&atlas).await.expect("backing reads"),
        &memory.recall(&atlas).await.expect("facts read"),
        &children,
        &by_id,
    )
}

/// **About shows what the project holds and who wrote each value behind a fold**,
/// and the board's own `columns` key is not a value of the project. Paired: the
/// owner (said) and the budget (worked out) are each named by their own write.
#[tokio::test]
async fn about_shows_the_values_and_who_wrote_each_behind_a_fold() {
    let page = rendered().await;
    let about = page.split("id=\"recent\"").next().expect("about leads");
    assert!(about.contains("href=\"/person:alpha/\""), "{about}");
    assert!(about.contains("40k"), "{about}");
    assert!(
        !about.contains("considering"),
        "the board's columns are not a value: {about}"
    );
    let (shown, behind) = about
        .split_once("<details>")
        .unwrap_or_else(|| panic!("who wrote each value is behind a fold: {about}"));
    assert!(!shown.contains("testimony"), "{shown}");
    let owner = behind.split("<dt>owner</dt>").nth(1).expect("the owner");
    assert!(owner.starts_with("<dd>testimony"), "{behind}");
    let budget = behind.split("<dt>budget</dt>").nth(1).expect("the budget");
    assert!(budget.starts_with("<dd>inference"), "{behind}");
}

/// **Recent shows the newest ten claims, newest first, and counts the older
/// ones.** Paired: the tenth newest is shown, the eleventh is not.
#[tokio::test]
async fn recent_shows_the_newest_ten_and_counts_the_rest() {
    let page = rendered().await;
    let recent = page
        .split_once("id=\"recent\"")
        .and_then(|(_, rest)| rest.split_once("id=\"contents\""))
        .expect("recent sits between about and contents")
        .0;
    assert!(recent.contains("Claim number 12"), "{recent}");
    assert!(recent.contains("Claim number 03"), "{recent}");
    assert!(
        !recent.contains("Claim number 02"),
        "the eleventh newest is not shown: {recent}"
    );
    assert!(
        recent.contains("5 older"),
        "and the older ones are counted: {recent}"
    );
    assert!(
        recent.find("Claim number 12") < recent.find("Claim number 11"),
        "newest first: {recent}"
    );
}

/// **A project with nothing under it has no contents box**, where the same
/// function over a project with children makes one. Paired in the same read.
#[tokio::test]
async fn a_project_with_nothing_under_it_has_no_contents_box() {
    assert_eq!(contents(&[], &HashMap::new()), "");
    assert!(rendered().await.contains("id=\"contents\""));
}

/// **Contents lists everything under the project with its kind**, work included.
#[tokio::test]
async fn contents_lists_everything_under_the_project_with_its_kind() {
    let page = rendered().await;
    let contents = page
        .split_once("id=\"contents\"")
        .expect("contents ends the page")
        .1;
    assert!(
        contents.contains("href=\"/project:atlas/thing:omega/\""),
        "{contents}"
    );
    assert!(
        contents.contains("href=\"/project:atlas/work:tau/\""),
        "{contents}"
    );
    assert!(
        contents.contains(">thing<") && contents.contains(">work<"),
        "{contents}"
    );
}
