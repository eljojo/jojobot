//! **The review mark an archive by defrag earns, written as a named part of the
//! archive's request.** An archive by defrag is a review: the thing it
//! archived on is marked reviewed in the same act, or the answer says the mark
//! did not land. The mark is one part of the call, so a resend under the same
//! `request_id` finds the archive written and makes the mark once. A mark that
//! failed is never a discarded nested call (rules 130 and 498).

use jojobot_domain::request::{self, Part};
use rmcp::handler::server::wrapper::Parameters;

use super::update_entity::UpdateEntityArgs;
use super::*;

/// What the claim part of an archive by defrag keeps in its note, so a resend
/// knows the mark is still owed.
pub(crate) const MARKS_REVIEWED: &str = "marks_reviewed";

/// The name of the part that makes the mark.
const MARK_PART: &str = "review_mark";

/// **The note the claim part keeps**, from what the write will do after it lands:
/// take the stored due moment off a finished thing, and mark what defrag
/// archived. Neither is `None`, and a write that only clears keeps the one word it
/// always kept.
pub(crate) fn claim_note(clears_due_on: bool, marks_reviewed: bool) -> Option<String> {
    let mut words = Vec::new();
    if clears_due_on {
        words.push(super::due::CLEARS_DUE_ON);
    }
    if marks_reviewed {
        words.push(MARKS_REVIEWED);
    }
    (!words.is_empty()).then(|| words.join(","))
}

/// Whether a claim part's note holds `word`.
pub(crate) fn note_holds(note: Option<&str>, word: &str) -> bool {
    note.is_some_and(|note| note.split(',').any(|held| held == word))
}

/// Whether the work in flight is the mark itself.
pub(crate) fn within_the_mark() -> bool {
    request::within_part(MARK_PART)
}

/// How the mark went.
pub(crate) enum Marked {
    /// The thing is marked reviewed, by this call or by an earlier one under the id.
    Made,
    /// The mark was not made, and this is why.
    NotMade(String),
}

impl Jojobot {
    /// **Mark the thing defrag archived on as reviewed**, as the part
    /// `review_mark` of the call in flight. A part an earlier call wrote does not
    /// run again.
    pub(crate) async fn mark_what_defrag_archived(
        &self,
        subject: &EntityId,
        sid: Option<String>,
    ) -> Marked {
        let mark = UpdateEntityArgs {
            handle: subject.as_str().to_string(),
            reviewed: Some(true),
            sid,
            request_id: None,
            name: None,
            aliases: None,
            source: None,
            crm: None,
            sets: None,
            override_token: None,
            override_reason: None,
        };
        match request::part(
            MARK_PART,
            None,
            Box::pin(self.update_entity(Parameters(mark))),
        )
        .await
        {
            Part::Recorded(_) => Marked::Made,
            Part::Ran(Err(error)) => Marked::NotMade(error.message.to_string()),
            Part::Ran(Ok(answered)) => {
                let body: serde_json::Value = answered
                    .content
                    .first()
                    .and_then(|block| block.as_text())
                    .and_then(|text| serde_json::from_str(&text.text).ok())
                    .unwrap_or_default();
                if answered.is_error == Some(true) || body["status"] == "blocked" {
                    Marked::NotMade(
                        body["how_to_proceed"]
                            .as_str()
                            .unwrap_or("the mark was refused")
                            .to_string(),
                    )
                } else {
                    Marked::Made
                }
            }
        }
    }

    /// **The answer for a call under a `request_id` whose archive landed and whose
    /// mark did not**: the call is not finished, and the same call sent again
    /// makes the mark once.
    pub(crate) fn mark_still_owed(&self, why: &str) -> CallToolResult {
        crate::caller::handle_declined(
            "update_fact",
            crate::answer::WayForward::retry(format!(
                "The claim was archived, and the review mark it earns was not made ({why}). \
                 Send the same call again with the same request_id and it makes the mark \
                 once."
            )),
        )
    }
}

/// **What the receipt says about the mark.**
pub(crate) fn note_the_mark(body: &mut serde_json::Value, marked: &Marked) {
    match marked {
        Marked::Made => body["reviewed"] = true.into(),
        Marked::NotMade(why) => {
            body["review_mark_not_made"] = format!(
                "The claim was archived, and the review mark an archive by defrag earns was \
                 not made, so the thing is still in the review queue ({why}). Mark it with \
                 `update_entity` and `reviewed: true` on that thing"
            )
            .into();
        }
    }
}
