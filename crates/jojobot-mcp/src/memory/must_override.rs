//! **A `must` is a norm, and no norm is a dead end** (rule 472).
//!
//! A type's `must` turns away a claim whose fields do not meet it. The refusal
//! names the way through: the same call carrying the token the refusal minted
//! and a written reason lands the claim. The token is minted and checked the
//! way the other refusals' tokens are (`guard::override_token`), and the
//! reason is kept on the claim under [`MUST_OVERRIDDEN`] so a reader of the
//! claim, and the review of the thing it landed on, can see why.
//!
//! The rule itself is judged by the store (see `memory::must`). This module
//! only checks what a caller sent and asks the store again with the written
//! reason, so a call that carries no override is one store call as before.

use jojobot_domain::memory::{
    EntityId, Fact, FactAddress, FactPatch, Guarded, Memory, MemoryError, NewFact, must,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;

use super::declined::{TokenSlot, blocked_body, memory_declined};

/// **The label a landed override leaves on its claim**: the type whose rule was
/// lifted and the reason, in words.
pub(crate) const MUST_OVERRIDDEN: &str = "must_overridden";

/// **The reason that lifts this refusal**, when the call sent the token the
/// refusal mints and a reason that counts.
fn lifting<'a>(
    token: Option<&str>,
    reason: Option<&'a str>,
    refused: &MemoryError,
) -> Option<(&'a str, String)> {
    let MemoryError::BreaksMust {
        subject,
        name,
        rule,
        ..
    } = refused
    else {
        return None;
    };
    let reason = super::declined::kept_override_reason(token, reason)?;
    (token == Some(must::override_token(subject, name, rule).as_str()))
        .then(|| (reason, format!("{name}: {reason}")))
}

/// **`capture`, asking again with the written reason when the first answer is a
/// `must` refusal the call's token lifts.**
pub(crate) async fn capture_lifting(
    memory: &dyn Memory,
    new: NewFact,
    token: Option<&str>,
    reason: Option<&str>,
) -> Result<Guarded<Fact>, MemoryError> {
    let first = memory.capture(new.clone()).await;
    let Err(refused) = &first else {
        return first;
    };
    let Some((reason, label)) = lifting(token, reason, refused) else {
        return first;
    };
    let mut again = new;
    again.must_override = Some(reason.to_string());
    again.fields.insert(MUST_OVERRIDDEN.to_string(), label);
    memory.capture(again).await
}

/// **`update_fact`, asking again with the written reason**, as [`capture_lifting`].
pub(crate) async fn update_fact_lifting(
    memory: &dyn Memory,
    address: &FactAddress,
    patch: FactPatch,
    caller: &EntityId,
    token: Option<&str>,
    reason: Option<&str>,
) -> Result<Guarded<Fact>, MemoryError> {
    let first = memory.update_fact(address, patch.clone(), caller).await;
    let Err(refused) = &first else {
        return first;
    };
    let Some((reason, label)) = lifting(token, reason, refused) else {
        return first;
    };
    let mut again = patch;
    again.must_override = Some(reason.to_string());
    again.fields.insert(MUST_OVERRIDDEN.to_string(), label);
    memory.update_fact(address, again, caller).await
}

/// **The answer to a refused write**: a `must` refusal names the way through
/// (the token and the reason it takes); any other refusal is answered as it
/// always was. Only a verb that can build a [`TokenSlot`] reaches this, so no
/// verb promises an override it does not take.
pub(crate) fn declined_or_lifted(
    verb: &'static str,
    refused: MemoryError,
    slot: &TokenSlot,
) -> Result<CallToolResult, McpError> {
    let MemoryError::BreaksMust {
        ref subject,
        ref name,
        ref rule,
        ref missing,
    } = refused
    else {
        return memory_declined(verb, refused);
    };
    let token = must::override_token(subject, name, rule);
    let how_to = if slot.reason_owed() {
        format!(
            "Nothing was written. The override_token was not enough: lifting this takes a \
             reason. Re-call {verb} with override_token: \"{token}\" and override_reason: \
             \"<why this claim should land without it>\"."
        )
    } else {
        format!(
            "Nothing was written: {refused}. The type '{name}' declares this rule for its \
             claims, so sending this call again will not change the answer. Send {verb} again \
             with a value under {} in `fields` (or in `sets`, to set it on the thing too): the \
             rule `{rule}` reads the claim's own fields, and a blank value does not count. If \
             the claim should land without it, re-call {verb} with override_token: \"{token}\" \
             and override_reason: \"<why>\"; the claim then carries the reason under \
             `{MUST_OVERRIDDEN}` and the thing goes to the review queue. That token belongs to \
             THIS refusal and lifts no other.",
            missing.join(" or ")
        )
    };
    Ok(blocked_body(&EntityId(String::new()), &[], how_to))
}
