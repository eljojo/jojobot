//! **The day a past run is dated with is read in the instance's zone, and a UTC
//! instance reads the day UTC does.** The page of a piece of work lists the past
//! runs that declared it and gives each the day its moment falls on; the page
//! asks the instance's zone for that day, then the clock turns the moment into a
//! day in it. This holds the two pieces the page composes, at a moment close
//! enough to midnight that the zones disagree about the day. The story
//! `working_on_page` holds the page itself.

use jiff::Timestamp;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use jojobot_mcp::orientation::instance_zone::unzoned_frame;

/// An instance whose record states `zone`, or states none.
async fn instance(zone: Option<&str>) -> InMemoryMemory {
    let memory = InMemoryMemory::booted();
    let record = EntityId("topic:instance".to_string());
    memory
        .add_entity(NewEntity::new(
            record.clone(),
            "Our instance",
            "the operator",
        ))
        .await
        .expect("the record is written")
        .written()
        .expect("nothing collides with it");
    if let Some(zone) = zone {
        let mut stated = NewFact::about(
            record,
            "the zone the instance dates a day in",
            jiff::civil::date(2026, 3, 1),
        );
        stated.fields = [("timezone".to_string(), zone.to_string())]
            .into_iter()
            .collect();
        stated.sets = ["timezone".to_string()].into_iter().collect();
        memory.capture(stated).await.expect("the zone is written");
    }
    memory
}

/// The day the page would give a run at `moment`, on an instance in `zone`.
async fn dated(zone: Option<&str>, moment: &str) -> String {
    let memory = instance(zone).await;
    let frame = unzoned_frame(&memory).await;
    Clock::default()
        .day_of(moment.parse::<Timestamp>().expect("a moment"), &frame)
        .to_string()
}

/// A run at half past two in the morning UTC is still the evening before in
/// New York and already the next day in Auckland, and a UTC instance, or one that
/// states no zone, dates it the day UTC does. Each zone is paired with the UTC
/// reading of the same moment, so the case cannot pass on a build that always
/// reads UTC.
#[tokio::test]
async fn a_run_near_midnight_is_dated_by_the_instances_zone_and_a_utc_instance_agrees_with_utc() {
    let moment = "2026-03-05T02:30:00Z";
    assert_eq!(dated(Some("UTC"), moment).await, "2026-03-05");
    assert_eq!(dated(None, moment).await, "2026-03-05", "no zone is UTC");
    assert_eq!(
        dated(Some("America/New_York"), moment).await,
        "2026-03-04",
        "the evening before, west of UTC"
    );
    assert_eq!(
        dated(Some("Pacific/Auckland"), "2026-03-05T12:30:00Z").await,
        "2026-03-06",
        "already the next day"
    );
    assert_eq!(
        dated(Some("UTC"), "2026-03-05T12:30:00Z").await,
        "2026-03-05",
        "the same moment on a UTC instance"
    );
}
