//! A check-in's arithmetic is a claim of its own, derived from the caller's
//! claim. Its frozen plan keeps a request-id resend from advancing twice.

use super::*;
use jiff::civil::Date;
use jojobot_domain::request;

const PLAN: &str = "check_in_schedule:";

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Schedule {
    fields: std::collections::BTreeMap<String, String>,
    pub(super) clears_due_on: bool,
    must_day: Option<Date>,
}

impl Schedule {
    pub(super) fn take_from(claim: &mut NewFact, clears_due_on: bool) -> Self {
        let fields = std::mem::take(&mut claim.derived)
            .into_iter()
            .filter_map(|key| claim.fields.remove(&key).map(|value| (key, value)))
            .collect::<std::collections::BTreeMap<_, _>>();
        claim.sets.retain(|key| !fields.contains_key(key));
        Self {
            fields,
            clears_due_on,
            must_day: claim.must_day,
        }
    }

    pub(super) fn frozen(&self) -> String {
        format!(
            "{PLAN}{}",
            serde_json::to_string(self).expect("string fields")
        )
    }

    pub(super) fn is_note(note: &str) -> bool {
        note.starts_with(PLAN)
    }

    pub(super) fn thawed(note: &str) -> Option<Self> {
        serde_json::from_str(note.strip_prefix(PLAN)?).ok()
    }
}

impl Jojobot {
    /// The second record takes no caller slot: it records jojobot's arithmetic,
    /// rather than another sentence the caller must decide whether to keep.
    pub(super) async fn write_check_in_schedule(
        &self,
        source: &Fact,
        schedule: &Schedule,
        caller: &crate::caller::Caller,
    ) -> Result<Option<jojobot_domain::memory::search::Behind>, MemoryError> {
        let mut fields = schedule.fields.clone();
        let derived = fields.keys().cloned().collect();
        fields.insert(
            jojobot_domain::memory::capacity::ENGINE_RECORD.to_string(),
            "check_in_schedule".to_string(),
        );
        let write = self.memory.capture(NewFact {
            provenance: Provenance::Inference,
            derived_from: Some(source.address()),
            fields,
            derived,
            session: Some(caller.sid.as_str().to_string()),
            written_by_bot: Some(caller.bot.clone()),
            must_day: schedule.must_day,
            ..NewFact::about(
                source.subject.clone(),
                "The schedule computed from this check-in.",
                source.recorded_at,
            )
        });
        match request::part("check_in_schedule", None, write).await {
            request::Part::Ran(Ok(Guarded::Written(_))) | request::Part::Recorded(_) => Ok(None),
            request::Part::Ran(Err(MemoryError::FoldBehind {
                landed: Landed::Fact(_),
                behind,
                ..
            })) => Ok(Some(behind)),
            request::Part::Ran(Err(error)) => Err(error),
            request::Part::Ran(Ok(Guarded::Blocked { attempted, .. })) => {
                Err(MemoryError::InvalidFact(format!(
                    "the rhythm {attempted} no longer accepts the computed schedule"
                )))
            }
        }
    }

    /// A second write failing cannot turn the first into "nothing was written".
    /// Keep its address and leave an id open, so a resend finishes the same plan.
    pub(super) fn check_in_schedule_not_written(
        &self,
        source: &Fact,
        day: Date,
        error: MemoryError,
    ) -> Result<CallToolResult, McpError> {
        let word = super::declined::memory_fix_by(&error);
        let mut body = fact_receipt_json(source, day);
        body["schedule_not_written"] = format!(
            "Your claim landed at {}, but the computed schedule was not written: {error}",
            source.address()
        )
        .into();
        let how = if request::carries_an_id() {
            "Resolve the schedule failure, then send the same call with the same request_id \
             to finish its saved schedule without writing your claim again or advancing twice. \
             If the call itself must change, use a new request_id: that writes a new claim, \
             and the claim already stored here remains."
        } else {
            "Your claim is stored, but the rhythm's schedule has not been updated. Once \
             the cause is fixed, capture a new check-in to update it; that writes a new \
             claim. Use a request_id so an interrupted call can finish without duplication."
        };
        let way = match word {
            Some(crate::answer::FixBy::Retry) => crate::answer::WayForward::retry(how),
            Some(crate::answer::FixBy::Person) => crate::answer::WayForward::person(how),
            Some(crate::answer::FixBy::Change) => crate::answer::WayForward::from(how),
            None => crate::answer::WayForward::memory_store_failure(how),
        };
        way.write_into(&mut body);
        json_result(&body)
    }
}
