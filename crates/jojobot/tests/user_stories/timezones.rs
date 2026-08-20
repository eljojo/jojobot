//! "I am in New York. My evening is not tomorrow."
//!
//! **Because a day-grained answer has to be resolved in some zone.** Resolved
//! in UTC, a claim captured at nine in the evening is stamped with the next
//! day, and a recurring loop falls due up to a day early. A surface that says
//! nothing about which zone it used, and offers no way to set one, produces the
//! shape of fault an agent works around rather than reports.
//!
//! **The session supplies the frame.** A run says which zone it works in when
//! it boots, and jojobot never assumes one. That is the rule the overdue read
//! already follows one level down — it takes the day it is asked about rather
//! than reading a clock — applied to the run itself.
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
        let stamped = s
            .undated_fact("person:milhouse", "said he would come along")
            .await;
        assert_eq!(
            stamped,
            day_in(zone).to_string(),
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
    due_for(&ahead)
        .await
        .says(&format!("\"overdue_as_of\":\"{}\"", day_in(AHEAD)));
    due_for(&behind)
        .await
        .says(&format!("\"overdue_as_of\":\"{}\"", day_in(BEHIND)));

    // ── a run that named no zone is answered in the stated fallback ─────────
    //
    // The negative the whole story rests on: without it, a build that always
    // used UTC and a build that reads the run's zone are told apart by nothing
    // above — because UTC is a zone like any other.
    let unframed = story.session_in("UTC", Some("new")).await;
    let stamped = unframed.undated_fact("person:milhouse", "and he did").await;
    assert_eq!(
        stamped,
        day_in("UTC").to_string(),
        "a run with no frame of its own is answered in UTC",
    );

    ahead
        .wrap("read the fern's loop from the far side of the date line")
        .await;
    story.finish().await;
}
