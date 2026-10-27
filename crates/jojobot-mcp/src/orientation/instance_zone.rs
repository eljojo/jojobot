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
pub(crate) const INSTANCE_RECORD: &str = jojobot_domain::memory::INSTANCE_RECORD;

/// The key on [`INSTANCE_RECORD`] that holds the zone, as an IANA name.
pub(crate) const INSTANCE_ZONE_KEY: &str = "timezone";

/// The key on [`INSTANCE_RECORD`] that holds the operator, as the handle of a
/// person. A work thing that waits on the operator holds the same handle under
/// `waiting_on`.
pub(crate) const INSTANCE_OPERATOR_KEY: &str = jojobot_domain::memory::OPERATOR;

/// What the instance's record says about its zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstanceZone {
    /// The record or the key is absent.
    Unset,
    /// The store could not read the record, so whether it holds a zone is not
    /// known (rule 130).
    Unreadable,
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

/// **What the instance's record holds for its zone**, asked of a Memory
/// directly so a reader that is not a [`Jojobot`] can ask too. A store that
/// cannot be read answers [`InstanceZone::Unreadable`], never an instance that
/// set no zone.
pub(crate) async fn instance_zone_of(memory: &dyn Memory) -> InstanceZone {
    let fields = match instance_fields_of(memory).await {
        Ok(fields) => fields,
        Err(e) => {
            tracing::warn!(error = %e, "the instance's record could not be read for its zone");
            return InstanceZone::Unreadable;
        }
    };
    match fields.get(INSTANCE_ZONE_KEY).map(|raw| raw.trim()) {
        None | Some("") => InstanceZone::Unset,
        Some(raw) => match jiff::tz::TimeZone::get(raw) {
            Ok(zone) => InstanceZone::Set(zone.iana_name().unwrap_or(raw).to_string()),
            Err(_) => InstanceZone::Unresolvable(raw.to_string()),
        },
    }
}

/// **The zone a read that states none is answered in**: the instance's, and UTC
/// when the instance holds none, holds one this build cannot resolve, or its
/// record cannot be read. A day-grained call must still be answered, so a
/// failed read falls back to UTC here, and the boot says so. One
/// answer for every unzoned read, public so the operator's window reads the day
/// the same way the verbs do.
pub async fn unzoned_frame(memory: &dyn Memory) -> jiff::tz::TimeZone {
    instance_zone_of(memory)
        .await
        .name()
        .and_then(|name| jiff::tz::TimeZone::get(name).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC)
}

/// **The instance's own record, folded**, for the readers that need what the
/// operator wrote on it besides a zone: the status words it maps and the columns
/// it lists. **A record nobody wrote is an instance that mapped nothing; a store
/// that could not be read is an error**, and the two are different answers
/// (rule 130). Public so the operator's window reads a status the same way the
/// verbs do.
pub async fn instance_fields_of(
    memory: &dyn Memory,
) -> Result<std::collections::BTreeMap<String, String>, jojobot_domain::memory::MemoryError> {
    match memory.fields(&EntityId(INSTANCE_RECORD.to_string())).await {
        Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }) => Ok(Default::default()),
        read => read,
    }
}

impl Jojobot {
    /// **What the instance's record holds**, folded; see [`instance_fields_of`].
    pub(crate) async fn instance_fields(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, jojobot_domain::memory::MemoryError>
    {
        instance_fields_of(&*self.memory).await
    }

    /// **What the instance's record holds for its zone.**
    pub(crate) async fn instance_zone(&self) -> InstanceZone {
        instance_zone_of(&*self.memory).await
    }

    /// The frame an unzoned read is answered in; see [`unzoned_frame`].
    pub(crate) async fn unzoned_frame(&self) -> jiff::tz::TimeZone {
        unzoned_frame(&*self.memory).await
    }
}

/// **The sentence that says the operator has no entity yet, and which two
/// writes make one.** The boot carries it as its operator line and a post to a
/// person who is not named carries it as the way forward, so the two cannot
/// tell a caller different things.
pub(crate) fn no_operator_yet() -> String {
    format!(
        "no entity yet: add_entity a person, then capture {INSTANCE_OPERATOR_KEY} in sets on \
         {INSTANCE_RECORD}"
    )
}

/// **The sentence for an instance record that could not be read**, so who the
/// operator is is not known. The boot and every refusal for it carry this one
/// sentence.
pub(crate) fn operator_unreadable() -> String {
    format!("not known: {INSTANCE_RECORD} could not be read just now, so try again")
}

/// **The sentence for a record that names something other than a person as the
/// operator.** The operator is a person, so the handle the record holds does not
/// count, and the sentence says what it holds, its kind, and where a person is
/// written. The entity exists, so it never says there is none. The boot and a
/// post to that handle both carry this one sentence, so they cannot disagree.
pub fn operator_is_not_a_person(held: &EntityId) -> String {
    let kind = held.kind().map_or("unknown kind", |kind| kind.as_token());
    format!(
        "the instance's record holds '{held}' as its operator, which is a {kind} and not a \
         person, so there is no operator yet: capture {INSTANCE_OPERATOR_KEY} in sets with a \
         person's handle on {INSTANCE_RECORD}"
    )
}

/// **What the instance's record makes of its operator.** The one reading of the
/// record, public so the operator's window asks it what the verbs ask it
/// (rule 598): the record names nobody, names something that is not a person, or
/// names a person. Defined beside the window's read handle; [`operator_is_not_a_person`]
/// says what a record naming something that is not a person means.
pub use jojobot_query_api::OperatorIs;

/// **The handle the instance's record holds as its operator, whatever its kind**,
/// or `None` when the record or the key is absent. **A store that cannot be read
/// is an error**, never an instance with no operator (rule 130): the caller
/// cannot be told who the operator is by guessing, and must not be told to name
/// one.
pub async fn instance_operator_of(
    memory: &dyn Memory,
) -> Result<Option<EntityId>, jojobot_domain::memory::MemoryError> {
    Ok(instance_fields_of(memory)
        .await?
        .get(INSTANCE_OPERATOR_KEY)
        .map(|raw| raw.trim().to_string())
        .filter(|handle| !handle.is_empty())
        .map(EntityId))
}

/// **The operator, as the verbs and the window both read them.** The person rule
/// is here and nowhere else: a handle of any other kind names nobody.
pub async fn operator_of(
    memory: &dyn Memory,
) -> Result<OperatorIs, jojobot_domain::memory::MemoryError> {
    Ok(match instance_operator_of(memory).await? {
        None => OperatorIs::Nobody,
        Some(held) if held.kind() == Some(EntityKind::PERSON) => OperatorIs::Person(held),
        Some(held) => OperatorIs::NotAPerson(held),
    })
}

/// **The sentence for an operator who owns more than one mailbox.** One person has
/// exactly one, so several is damage and not a choice: no caller picks one of them,
/// and a person has to repair it. The verbs' refusal and the operator's page both
/// say this sentence, so they cannot tell a caller different things (rule 498).
pub fn operator_owns_several_boxes(names: &[String]) -> String {
    format!(
        "The operator owns more than one mailbox ({}), and one person has exactly one. This is \
         stored damage rather than a mistake in any call: nothing sent through a verb or this \
         page changes it, and a person who runs this instance has to repair it. A session that \
         meets it reports it.",
        names.join(", ")
    )
}

impl Jojobot {
    /// **The handle the instance's record names as its operator**; see
    /// [`instance_operator_of`].
    pub(crate) async fn instance_operator(
        &self,
    ) -> Result<Option<EntityId>, jojobot_domain::memory::MemoryError> {
        instance_operator_of(&*self.memory).await
    }

    /// **The boot's one line about the operator**: the handle the instance's
    /// record holds under [`INSTANCE_OPERATOR_KEY`], or a sentence saying there
    /// is no entity yet and which two writes make one. A record that is absent
    /// and a key that is absent say the same thing. A store that cannot be read
    /// says that, and never tells the caller to name an operator.
    pub(crate) async fn operator_answer(&self) -> serde_json::Value {
        match self.instance_operator().await {
            Ok(Some(held)) if held.kind() != Some(EntityKind::PERSON) => {
                operator_is_not_a_person(&held).into()
            }
            Ok(Some(operator)) => operator.as_str().into(),
            Ok(None) => no_operator_yet().into(),
            Err(e) => {
                tracing::warn!(error = %e, "the instance's record could not be read for its operator");
                operator_unreadable().into()
            }
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
    // **A zone the session sent that is not the instance's is said once, naming
    // both.** A sent zone wins, so a guessed one moves what today means for the
    // whole run and nothing else would show it. The boot is not refused for it.
    if let (ZoneFrom::Session, Some(sent), Some(held), Some(object)) =
        (from, zone, instance.name(), answer.as_object_mut())
        && sent != held
    {
        object.insert(
            "note".into(),
            format!(
                "this run sent {sent} and the instance holds {held}; the sent zone decides this \
                 run's days. Omit timezone to be answered in {held}."
            )
            .into(),
        );
    }
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
    // **A record that could not be read is said, beside the zone used instead**,
    // so a run answered in UTC for a failed read is not taken for an instance
    // that set no zone.
    if let (ZoneFrom::Default, InstanceZone::Unreadable, Some(object)) =
        (from, instance, answer.as_object_mut())
    {
        object.insert(
            "note".into(),
            format!(
                "{INSTANCE_RECORD} could not be read, so this run's days are counted in UTC; \
                 send timezone, or boot again once the store answers"
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
        args.sets = Some([(INSTANCE_ZONE_KEY.to_string(), zone.to_string())].into());
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

    /// **An instance record that cannot be read is said, and the run is answered
    /// in UTC** (rule 130). Two handlers over one store, the second unable to
    /// read the instance's record: the healthy boot answers the zone the
    /// operator set, and the other answers UTC with a note naming the record.
    /// Neither reads as an instance that set no zone.
    #[tokio::test]
    async fn an_instance_record_that_cannot_be_read_is_said_and_answers_in_utc() {
        let (healthy, blind) = healthy_and_down(Down::FieldsOf(INSTANCE_RECORD));
        make_bot(&healthy, "otto").await;
        let zone = a_zone_whose_day_is_not_utcs();
        the_operator_sets_the_zone(&healthy, zone).await;

        let read = boot_answering_dated(&healthy, "otto", "new", None, None).await;
        assert_eq!(read["timezone"]["zone"], zone, "{read}");
        assert_eq!(read["timezone"]["from"], "instance", "{read}");
        assert!(read["timezone"].get("note").is_none(), "{read}");

        let unread = boot_answering_dated(&blind, "otto", "new", None, None).await;
        assert_eq!(unread["timezone"]["zone"], "UTC", "{unread}");
        assert_eq!(unread["timezone"]["from"], "default", "{unread}");
        let note = unread["timezone"]["note"].as_str().unwrap_or_default();
        assert!(
            note.contains(INSTANCE_RECORD),
            "the note names the record that could not be read: {unread}"
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
        let sid = sid_of(&body).expect("a resume hands back the run's new key");
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
    /// 🚨 **A record that names something other than a person as operator is
    /// said to, not passed off as an operator and not called an absence.** The
    /// operator is a person. A record can still hold a handle of another kind
    /// (data from before a retype was refused), and the boot used to hand that
    /// handle over as the operator. The line now names what the record holds,
    /// says it is not a person, and says where a person is written. It is NOT
    /// "no entity yet": the entity exists. Paired with a record that names a
    /// person, which the boot hands over as before.
    #[tokio::test]
    async fn a_record_naming_a_bot_as_operator_is_said_not_to_be_a_person() {
        let memory = std::sync::Arc::new(InMemoryMemory::seeded());
        let jojobot = handler_over(memory.clone());
        make_bot(&jojobot, "otto").await;
        make_bot(&jojobot, "gamma").await;
        let names = |handle: &str| {
            let mut args = capture_args(INSTANCE_RECORD, "who the operator is");
            args.provenance = Some("testimony".into());
            args.sets = Some([(INSTANCE_OPERATOR_KEY.to_string(), handle.to_string())].into());
            args
        };

        // The shipped topic kind declares the operator key a reference to a
        // person, so a bot named there is refused. A record written before that
        // was declared holds it, and the boot has to say what it finds: the
        // state is staged past the guard, as the damage it is.
        ensure(&jojobot, INSTANCE_RECORD).await;
        memory.fields_past_the_guard(
            &EntityId(INSTANCE_RECORD.into()),
            &[(INSTANCE_OPERATOR_KEY, "bot:gamma")],
        );
        let body = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        let said = body["operator"].as_str().expect("one operator line");
        assert!(
            said.contains("bot:gamma"),
            "names what the record holds: {said}"
        );
        assert!(
            !said.starts_with("bot:"),
            "a bot is not handed over as the operator: {said}"
        );
        assert!(
            !said.contains("no entity yet"),
            "the entity exists, so that would be false: {said}"
        );
        assert!(
            said.contains(INSTANCE_RECORD),
            "and where to write a person: {said}"
        );

        // **Changing the operator once one is named is the head of the chart's**,
        // so otto is given a report and nothing above it.
        let head = as_bot(&jojobot, "otto");
        make_bot(&jojobot, "assistant").await;
        ensure(&jojobot, "bot:omega").await;
        capture_as(
            &jojobot,
            &as_bot(&jojobot, "assistant"),
            CaptureArgs {
                sets: Some([("reports_to".to_string(), "bot:otto".to_string())].into()),
                ..capture_args("bot:omega", "omega reports to otto")
            },
        )
        .await;
        ensure(&jojobot, "person:lisa").await;
        capture_as(&jojobot, &head, names("person:lisa")).await;
        let body = boot_answering_dated(&jojobot, "otto", "new", None, None).await;
        assert_eq!(body["operator"], "person:lisa", "{body}");
    }

    /// **The sentence for an operator who owns several boxes is pinned once, by
    /// its parts.** The verb, the page and their cases all compute the expected
    /// sentence with this function, so none of them would see its words change
    /// (rule 498). The pin names the boxes in the order given, joined by a comma,
    /// and says it is damage that a person repairs.
    #[test]
    fn the_sentence_for_an_operator_with_several_boxes_names_them_and_says_damage() {
        let said = operator_owns_several_boxes(&["homer".to_string(), "marge".to_string()]);
        assert!(said.contains("(homer, marge)"), "{said}");
        assert!(said.contains("damage"), "{said}");
        assert!(said.contains("repair"), "{said}");
    }
}
