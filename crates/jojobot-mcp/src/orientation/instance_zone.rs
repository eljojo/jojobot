//! **The zone the instance's operator lives in, held as data on the instance's
//! own record** and read when a session sends none.
//!
//! A session that boots with no `timezone` used to be answered in UTC, so it
//! disagreed with its operator about the day for part of every twenty-four
//! hours unless it had been told to send one. The operator writes the zone once
//! and every run that sends none is answered in it. A run that sends one still
//! wins. Nothing here names a zone: the software holds a handle and a key.

use super::*;

/// The record the instance holds about itself. The operator creates it and
/// writes [`INSTANCE_ZONE_KEY`] on it once.
pub(crate) const INSTANCE_RECORD: &str = "topic:instance";

/// The key on [`INSTANCE_RECORD`] that holds the zone, as an IANA name.
pub(crate) const INSTANCE_ZONE_KEY: &str = "timezone";

/// What the instance's record says about its zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstanceZone {
    /// The record or the key is absent, or the store could not be read.
    Unset,
    /// The key holds a value this build cannot resolve as a zone.
    Unresolvable(String),
    /// A zone this build resolves, by its IANA name.
    Set(String),
}

impl InstanceZone {
    /// The IANA name of the zone, when the record holds one this build resolves.
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            InstanceZone::Set(name) => Some(name),
            _ => None,
        }
    }
}

impl Jojobot {
    /// **What the instance's record holds for its zone.** A store that cannot be
    /// read answers [`InstanceZone::Unset`]: a day-grained call must still be
    /// answered, in the frame an instance with no zone gets.
    pub(crate) async fn instance_zone(&self) -> InstanceZone {
        let Ok(fields) = self
            .memory
            .fields(&EntityId(INSTANCE_RECORD.to_string()))
            .await
        else {
            return InstanceZone::Unset;
        };
        match fields.get(INSTANCE_ZONE_KEY).map(|raw| raw.trim()) {
            None | Some("") => InstanceZone::Unset,
            Some(raw) => match jiff::tz::TimeZone::get(raw) {
                Ok(zone) => InstanceZone::Set(zone.iana_name().unwrap_or(raw).to_string()),
                Err(_) => InstanceZone::Unresolvable(raw.to_string()),
            },
        }
    }
}

/// **Where a run's zone came from**, as the boot says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoneFrom {
    /// The session sent it on this call.
    Session,
    /// The session sent none, and the instance's record holds one.
    Instance,
    /// A resumed run keeps the zone it already had.
    Run,
    /// Neither the session nor the instance named one: UTC.
    Default,
}

impl ZoneFrom {
    fn token(self) -> &'static str {
        match self {
            ZoneFrom::Session => "session",
            ZoneFrom::Instance => "instance",
            ZoneFrom::Run => "run",
            ZoneFrom::Default => "default",
        }
    }
}

/// **The zone a boot used and where it came from**, for the boot's answer. It
/// is kept to those two words because the anonymous boot is near its ceiling;
/// the essay says where the operator writes a zone. A value on the record that
/// is no zone is named, because nothing else would tell the operator it failed.
pub(crate) fn zone_answer(
    zone: Option<&str>,
    from: ZoneFrom,
    instance: &InstanceZone,
) -> serde_json::Value {
    let mut answer = serde_json::json!({
        "zone": zone.unwrap_or(crate::memory::parse::FALLBACK_ZONE),
        "from": from.token(),
    });
    if let (ZoneFrom::Default, InstanceZone::Unresolvable(raw), Some(object)) =
        (from, instance, answer.as_object_mut())
    {
        object.insert(
            "note".into(),
            format!(
                "{INSTANCE_ZONE_KEY} on {INSTANCE_RECORD} is {raw:?}, no zone this build knows"
            )
            .into(),
        );
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// The day it is right now in a zone.
    fn day_in(zone: &str) -> String {
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone this build resolves"))
            .date()
            .to_string()
    }

    /// **A zone whose day is not UTC's at this moment**, so a day stamped in it
    /// cannot be mistaken for the fallback. One of the two widest zones always
    /// qualifies, whatever hour this runs at.
    fn a_zone_whose_day_is_not_utcs() -> &'static str {
        ["Etc/GMT+12", "Pacific/Kiritimati"]
            .into_iter()
            .find(|zone| day_in(zone) != day_in("UTC"))
            .expect("one of the widest zones is on another day than UTC")
    }

    /// **The other of the two widest zones**, so a session's own zone and the
    /// instance's are two different ones whichever was chosen first.
    fn the_other_wide_zone(zone: &str) -> &'static str {
        ["Etc/GMT+12", "Pacific/Kiritimati"]
            .into_iter()
            .find(|other| *other != zone)
            .expect("two zones")
    }

    /// Write the zone onto the instance's record, the way the operator does.
    async fn the_operator_sets_the_zone(jojobot: &Jojobot, zone: &str) {
        let mut args = capture_args(INSTANCE_RECORD, "where the operator lives");
        args.provenance = Some("testimony".into());
        args.fields = Some([(INSTANCE_ZONE_KEY.to_string(), zone.to_string())].into());
        capture_ok(jojobot, args).await;
    }

    async fn stamped_with_no_date(jojobot: &Jojobot, sid: &str) -> String {
        let mut args = capture_args("person:milhouse", "no date on this one");
        args.sid = Some(sid.to_string());
        args.recorded_at = None;
        capture_ok(jojobot, args).await["recorded_at"]
            .as_str()
            .expect("a capture is stamped with a day")
            .to_string()
    }

    /// **A boot that sends no zone is answered in the instance's, and says
    /// so.** The day a capture gets is the instance zone's own day, which is
    /// not UTC's, so nothing here passes by the hour it runs at.
    #[tokio::test]
    async fn a_boot_with_no_zone_resolves_days_in_the_instances_zone() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;
        let zone = a_zone_whose_day_is_not_utcs();
        the_operator_sets_the_zone(&jojobot, zone).await;

        let body = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        assert_eq!(body["timezone"]["zone"], zone, "{body}");
        assert_eq!(body["timezone"]["from"], "instance", "{body}");
        let sid = sid_of(&body).unwrap_or_else(|| panic!("a boot with a handle: {body}"));
        assert_eq!(stamped_with_no_date(&jojobot, &sid).await, day_in(zone));
    }

    /// **A session that sends a zone wins over the instance's**, and the answer
    /// says it came from the session.
    #[tokio::test]
    async fn a_zone_the_session_sends_wins_over_the_instances() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;
        let instance = a_zone_whose_day_is_not_utcs();
        let sent = the_other_wide_zone(instance);
        the_operator_sets_the_zone(&jojobot, instance).await;

        let body = boot_answering_dated(&jojobot, "otto", "new", None, Some(sent)).await;
        assert_eq!(body["timezone"]["zone"], sent, "{body}");
        assert_eq!(body["timezone"]["from"], "session", "{body}");
        let sid = sid_of(&body).unwrap_or_else(|| panic!("a boot with a handle: {body}"));
        assert_eq!(stamped_with_no_date(&jojobot, &sid).await, day_in(sent));
    }

    /// **An instance whose operator set no zone says so and answers in UTC**, as
    /// every instance did. The essay a full boot carries names the record and the
    /// key that set one.
    #[tokio::test]
    async fn an_instance_with_no_zone_says_so_and_uses_utc() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;

        let body = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        assert_eq!(body["timezone"]["zone"], "UTC", "{body}");
        assert_eq!(body["timezone"]["from"], "default", "{body}");
        let sid = sid_of(&body).unwrap_or_else(|| panic!("a boot with a handle: {body}"));
        assert_eq!(stamped_with_no_date(&jojobot, &sid).await, day_in("UTC"));

        let essay = crate::orientation::essay::orientation();
        assert!(
            essay.contains(INSTANCE_RECORD) && essay.contains(INSTANCE_ZONE_KEY),
            "the essay names the record and the key that set a zone"
        );
    }

    /// **A value on the record that is no zone is said, not guessed at.** The
    /// run is answered in UTC and the note says the value did not resolve.
    #[tokio::test]
    async fn a_value_that_is_no_zone_is_said_and_answers_in_utc() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        the_operator_sets_the_zone(&jojobot, "Not/A_Zone").await;

        let body = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        assert_eq!(body["timezone"]["zone"], "UTC", "{body}");
        assert_eq!(body["timezone"]["from"], "default", "{body}");
        let note = body["timezone"]["note"].as_str().unwrap_or_default();
        assert!(
            note.contains("Not/A_Zone"),
            "the bad value is named: {body}"
        );
    }

    /// **A resume that sends none keeps the zone the run had**, rather than
    /// moving it to the instance's.
    #[tokio::test]
    async fn a_resume_that_sends_no_zone_keeps_the_runs_own() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;
        let instance = a_zone_whose_day_is_not_utcs();
        let own = the_other_wide_zone(instance);
        the_operator_sets_the_zone(&jojobot, instance).await;
        let sid = booted_in(&jojobot, "otto", own, Some("new")).await;
        // The card is lazy: a first write is what puts the run on the board.
        stamped_with_no_date(&jojobot, &sid).await;

        let body = boot_answering_dated(&jojobot, "otto", &sid, None, None).await;
        assert_eq!(body["timezone"]["zone"], own, "{body}");
        assert_eq!(body["timezone"]["from"], "run", "{body}");
        assert_eq!(stamped_with_no_date(&jojobot, &sid).await, day_in(own));
    }

    /// 🚨 **A resume that sends no zone reads the boot's own day in the run's
    /// zone.** The reported zone and every later stamp were the run's, but the
    /// day the BOOT used, which decides whether the bot's rules have aged, came
    /// from the instance's zone. The run is in the zone whose day is later and
    /// the instance in the earlier one, and a rule is good through the
    /// instance's day: past in the run's day, not past in the instance's. The
    /// control is a fresh boot that sends none, which is the instance's, and
    /// reads the rule as good.
    #[tokio::test]
    async fn a_resume_that_sends_no_zone_reads_a_rules_staleness_in_the_runs_day() {
        let (runs, instances) = ("Pacific/Kiritimati", "Etc/GMT+12");
        assert!(
            day_in(runs) > day_in(instances),
            "the case needs the run's zone a day ahead of the instance's"
        );
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                stale_after: Some(day_in(instances)),
                fields: Some(
                    [("starred".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("bot:otto", "checks the board before starting")
            },
        )
        .await;
        the_operator_sets_the_zone(&jojobot, instances).await;
        let sid = booted_in(&jojobot, "otto", runs, Some("new")).await;
        stamped_with_no_date(&jojobot, &sid).await;

        let stale = |body: &serde_json::Value| {
            body["identity"]["rules"]
                .as_array()
                .and_then(|rules| rules.first().cloned())
                .unwrap_or_else(|| panic!("the boot carries the bot's rules: {body}"))
                .get("stale")
                .is_some()
        };
        let resumed = boot_answering_dated(&jojobot, "otto", &sid, None, None).await;
        assert_eq!(resumed["timezone"]["from"], "run", "{resumed}");
        assert!(
            stale(&resumed),
            "the boot read the rule in the instance's day, not the run's: {resumed}"
        );

        let fresh = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        assert_eq!(fresh["timezone"]["from"], "instance", "{fresh}");
        assert!(
            !stale(&fresh),
            "a fresh boot with no zone is the instance's, where the rule still stands: {fresh}"
        );
    }

    /// **A read that carries no handle is answered in the instance's zone too.**
    #[tokio::test]
    async fn a_read_with_no_handle_is_answered_in_the_instances_zone() {
        let jojobot = handler();
        let zone = a_zone_whose_day_is_not_utcs();
        the_operator_sets_the_zone(&jojobot, zone).await;

        let day = jojobot.dated(None, None).await.expect("a day");
        assert_eq!(day.to_string(), day_in(zone));
    }
}
