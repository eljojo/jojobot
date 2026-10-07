//! "I am in New York. My evening is not tomorrow."
//!
//! **Because a day-grained answer has to be resolved in some zone.** Resolved
//! in UTC, a claim captured at nine in the evening is stamped with the next
//! day, and a recurring loop falls due up to a day early. A surface that says
//! nothing about which zone it used, and offers no way to set one, produces the
//! shape of fault an agent works around rather than reports.
//!
//! **The session supplies the frame.** A run says which zone it works in when
//! it boots. An instance whose operator wrote a zone answers a run that sends
//! none in it, and with no zone written jojobot assumes none of its own. That is
//! the rule the overdue read already follows one level down — it takes the day
//! it is asked about rather than reading a clock — applied to the run itself.
//!
//! ⚠️ **The consequence is the point of this story: two runs in two zones
//! legitimately disagree about what today is for one stored claim.** It is not
//! a fault and there is nothing to work around, and the boot says so in its own
//! words so that an agent meeting it does not start compensating.
//!
//! **The two zones here are the extremes on purpose** — twenty-six hours apart,
//! the widest the map goes — so their local dates differ at every instant and
//! nothing here passes or fails by the hour it runs at. Every date is worked
//! out from the leading zone's own today, so nothing rots when the calendar
//! moves either.

use serde_json::json;

use super::dsl::Story;

/// The day it is right now in a zone, which is what a run in it will say.
fn day_in(zone: &str) -> jiff::civil::Date {
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone this build can resolve"))
        .date()
}

/// **The days a zone was on while `call` ran**: its day before the call and its
/// day after. The server reads its own clock once, somewhere between the two, so
/// the day it answers with is one of them. That is one day, unless midnight in
/// that zone fell inside the call, and then either is right. Comparing an answer
/// with a day read at any other moment could fail on a run that straddled
/// midnight, which is what this replaces.
async fn days_during<T>(
    zone: &str,
    call: impl std::future::Future<Output = T>,
) -> (T, Vec<jiff::civil::Date>) {
    let before = day_in(zone);
    let answered = call.await;
    let after = day_in(zone);
    let days = if before == after {
        vec![before]
    } else {
        vec![before, after]
    };
    (answered, days)
}

/// A claim's stamped day must be one of the days its zone was on during the call.
fn assert_one_of(stamped: &str, days: &[jiff::civil::Date], why: &str) {
    assert!(
        days.iter().any(|day| day.to_string() == stamped),
        "{why}: stamped {stamped}, the zone was on {days:?}",
    );
}

/// **A call that straddled midnight is right on either day**, and only on those
/// two. The window is built by hand because a midnight cannot be summoned.
#[test]
fn a_call_across_midnight_is_right_on_either_day_and_on_no_other() {
    let before = jiff::civil::date(2026, 10, 7);
    let after = jiff::civil::date(2026, 10, 8);
    for stamped in ["2026-10-07", "2026-10-08"] {
        assert_one_of(stamped, &[before, after], "either side of midnight");
    }
    let outside = std::panic::catch_unwind(|| {
        assert_one_of("2026-10-09", &[before, after], "a day after the call");
    });
    assert!(outside.is_err(), "a day outside the window was accepted");
    let earlier = std::panic::catch_unwind(|| {
        assert_one_of("2026-10-06", &[before], "a day before the call");
    });
    assert!(
        earlier.is_err(),
        "a day before a one-day window was accepted"
    );
}

#[tokio::test]
async fn two_runs_in_two_zones_read_one_claim_in_their_own_day() {
    const AHEAD: &str = "Pacific/Kiritimati";
    const BEHIND: &str = "Etc/GMT+12";

    let story = Story::begin("bot:otto").await;

    // ── the boot says what a zone is for, before anybody trips over it ──────
    let (boot, first) = story.full_boot().await;
    let essay = boot["orientation"]
        .as_str()
        .expect("a full boot carries the orientation")
        .to_string();
    for taught in ["timezone", "disagree", "not a fault"] {
        assert!(
            essay.contains(taught),
            "the boot teaches the consequence rather than leaving it to be discovered: \
             {taught:?} is absent",
        );
    }

    // ── the things ──────────────────────────────────────────────────────────
    first.add("thing:the-fern", "The Fern").await;
    first
        .add_under("thing:the-fern", "rhythm:water-the-fern", "Water the fern")
        .await;

    // A loop that falls due on the leading zone's TODAY: one day's cadence,
    // counting from that zone's yesterday.
    let counts_from = day_in(AHEAD).yesterday().expect("the day before");
    first
        .event_with(
            "rhythm:water-the-fern",
            "watered it, the soil was dry again",
            json!({
                "name": "Water the fern",
                "last_check_in": counts_from.to_string(),
                "counts_from": counts_from.to_string(),
                "advances_from": "due_date",
                "cadence_days": "1",
            }),
            &[],
        )
        .await;

    // ── two runs of one bot, in two zones ───────────────────────────────────
    //
    // A bot may have several runs at once, which is what lets one story hold
    // both frames against one store.
    let ahead = story.session_in(AHEAD, Some("new")).await;
    let behind = story.session_in(BEHIND, Some("new")).await;

    // ── a claim with no date is stamped with the run's own day ──────────────
    first.add("person:milhouse", "Milhouse").await;
    let dated = async |s: &super::dsl::Session, zone: &str| {
        let (stamped, days) = days_during(
            zone,
            s.undated_fact("person:milhouse", "said he would come along"),
        )
        .await;
        assert_one_of(
            &stamped,
            &days,
            "the claim is stamped with the day it is where the run is",
        );
    };
    dated(&ahead, AHEAD).await;
    dated(&behind, BEHIND).await;

    // ── and the same loop has fallen due for one of them and not the other ──
    //
    // ⭐ **One stored claim, one question, two right answers.** The run whose
    // day it already is finds the loop due; the run still on an earlier day
    // does not. Both beats are here because a build ignoring zones gives the
    // two runs one answer, whichever answer that is.
    let due_for = async |s: &super::dsl::Session| {
        s.shape(
            "what has gone quiet",
            json!({ "kind": "rhythm", "overdue": {} }),
        )
        .await
    };
    due_for(&ahead).await.says("rhythm:water-the-fern");
    due_for(&behind).await.never_says("rhythm:water-the-fern");

    // …and each answer says which day it was asked about, in its own frame.
    for (run, zone) in [(&ahead, AHEAD), (&behind, BEHIND)] {
        let (answer, days) = days_during(zone, due_for(run)).await;
        let body = answer.json().to_string();
        assert!(
            days.iter()
                .any(|day| body.contains(&format!("\"overdue_as_of\":\"{day}\""))),
            "the answer does not say which day it was asked about, in its own frame \
             ({zone} was on {days:?}): {body}",
        );
    }

    // ── a run that named no zone is answered in the stated fallback ─────────
    //
    // The negative the whole story rests on: without it, a build that always
    // used UTC and a build that reads the run's zone are told apart by nothing
    // above — because UTC is a zone like any other.
    let unframed = story.session_in("UTC", Some("new")).await;
    let (stamped, days) = days_during(
        "UTC",
        unframed.undated_fact("person:milhouse", "and he did"),
    )
    .await;
    assert_one_of(
        &stamped,
        &days,
        "a run with no frame of its own is answered in UTC",
    );

    ahead
        .wrap("read the fern's loop from the far side of the date line")
        .await;
    story.finish().await;
}

/// **"I live in this zone. I should not have to say so at every boot."**
///
/// The operator writes their zone once, on the instance's own record. A run that
/// sends none is then answered in it, the boot says so, and a run that sends a
/// zone of its own still gets that one.
#[tokio::test]
async fn an_instance_that_holds_its_operators_zone_answers_a_run_that_sends_none() {
    // One of the two widest zones is always on another day than UTC, so the day
    // a claim is stamped with says which zone answered.
    let (operators, other) = ["Etc/GMT+12", "Pacific/Kiritimati"]
        .into_iter()
        .find(|zone| day_in(zone) != day_in("UTC"))
        .map(|zone| {
            (
                zone,
                if zone == "Etc/GMT+12" {
                    "Pacific/Kiritimati"
                } else {
                    "Etc/GMT+12"
                },
            )
        })
        .expect("one of the widest zones is on another day than UTC");

    let story = Story::begin("bot:otto").await;
    let (first_boot, first) = story.boot_sending_no_zone(Some("new")).await;
    assert_eq!(first_boot["timezone"]["from"], "default", "{first_boot}");
    assert_eq!(first_boot["timezone"]["zone"], "UTC", "{first_boot}");
    first.add("person:milhouse", "Milhouse").await;
    let (stamped, days) =
        days_during("UTC", first.undated_fact("person:milhouse", "said hello")).await;
    assert_one_of(
        &stamped,
        &days,
        "with no zone written, a run that sends none is answered in UTC",
    );

    // The operator writes their zone, once.
    first.add("topic:instance", "This instance").await;
    first
        .event_with(
            "topic:instance",
            "where the operator lives",
            json!({ "timezone": operators }),
            &[],
        )
        .await;

    let (boot, later) = story.boot_sending_no_zone(Some("new")).await;
    assert_eq!(boot["timezone"]["from"], "instance", "{boot}");
    assert_eq!(boot["timezone"]["zone"], operators, "{boot}");
    let (stamped, days) = days_during(
        operators,
        later.undated_fact("person:milhouse", "said goodbye"),
    )
    .await;
    assert_one_of(
        &stamped,
        &days,
        "a run that sends none is answered in the instance's zone",
    );

    // A run that sends a zone of its own still gets that one.
    let own = story.session_in(other, Some("new")).await;
    let (stamped, days) = days_during(other, own.undated_fact("person:milhouse", "waved")).await;
    assert_one_of(
        &stamped,
        &days,
        "a zone the session sends wins over the instance's",
    );
    story.finish().await;
}
