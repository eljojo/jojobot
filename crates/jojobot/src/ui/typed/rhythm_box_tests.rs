//! The box of a rhythm: how often it comes round, when it was last checked in,
//! when it falls due next and how late it is.

use super::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{Memory, NewEntity};

fn day(day: i8) -> jiff::civil::Date {
    jiff::civil::date(2026, 4, day)
}

async fn a_loop() -> Entity {
    let memory = InMemoryMemory::booted();
    for (handle, name, parent) in [
        ("person:alpha", "Alpha", None),
        ("rhythm:polish", "Polish the silver", Some("person:alpha")),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "the fixture roster");
        new.parent = parent.map(|parent| EntityId(parent.into()));
        memory
            .add_entity(new)
            .await
            .expect("the entity is written")
            .written()
            .expect("nothing collides with it");
    }
    memory
        .list_entities(None)
        .await
        .expect("the index reads")
        .into_iter()
        .find(|entity| entity.id.0 == "rhythm:polish")
        .expect("the loop is listed")
}

fn keys(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// **A loop that fell due says how late, and is the only one drawn late.**
#[tokio::test]
async fn a_loop_past_its_day_says_how_late_and_one_not_yet_due_does_not() {
    let rhythm = a_loop().await;
    let held = keys(&[("cadence_days", "7"), ("last_check_in", "2026-04-02")]);

    let late = rhythm_box(&rhythm, &held, Some((attention::Due::On(day(9)), Some(3))));
    assert!(late.contains("every 7 days"), "{late}");
    assert!(late.contains("2026-04-02"), "{late}");
    assert!(
        late.contains("3 days overdue (fell due 2026-04-09)"),
        "{late}"
    );
    assert!(late.contains("class=\"late\""), "{late}");

    for (what, overdue) in [("ahead", None), ("on the day", Some(0))] {
        let not_late = rhythm_box(&rhythm, &held, Some((attention::Due::On(day(9)), overdue)));
        assert!(
            not_late.contains("next due 2026-04-09"),
            "{what}: {not_late}"
        );
        assert!(!not_late.contains("overdue"), "{what}: {not_late}");
        assert!(!not_late.contains("class=\"late\""), "{what}: {not_late}");
    }
}

/// **Each mark that carries no day says why**, and an answer that could not be
/// read at all says the same as a mark that cannot say.
#[tokio::test]
async fn a_loop_without_a_due_day_says_why() {
    let rhythm = a_loop().await;
    let held = keys(&[("cadence_days", "7")]);
    for (marked, said) in [
        (
            Some((attention::Due::NotYetOpened, None)),
            "not checked in yet",
        ),
        (Some((attention::Due::Never, None)), "nothing falls due"),
        (
            Some((attention::Due::Unreadable, None)),
            "next due cannot be worked out from its keys",
        ),
        (None, "next due cannot be worked out from its keys"),
    ] {
        let page = rhythm_box(&rhythm, &held, marked);
        assert!(page.contains(said), "{said}: {page}");
        assert!(!page.contains("class=\"late\""), "{said}: {page}");
    }
}

/// **A calendar rule shows in words and a loop with no cadence in a dash**, and
/// neither shows the stored rule.
#[tokio::test]
async fn a_calendar_rule_shows_in_words_and_no_cadence_in_a_dash() {
    let rhythm = a_loop().await;
    let ruled = rhythm_box(
        &rhythm,
        &keys(&[("rrule", "FREQ=MONTHLY;BYMONTHDAY=1")]),
        None,
    );
    assert!(
        ruled.contains("<dt>cadence</dt><dd>on day 1 of every month</dd>"),
        "{ruled}"
    );
    assert!(!ruled.contains("FREQ="), "{ruled}");
    let none = rhythm_box(&rhythm, &keys(&[]), None);
    assert!(none.contains("<dt>cadence</dt><dd>—</dd>"), "{none}");
    assert!(none.contains("<dt>last check-in</dt><dd>—</dd>"), "{none}");
}
