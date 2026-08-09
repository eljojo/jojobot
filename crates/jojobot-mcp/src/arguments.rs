//! **An argument a verb does not implement is refused, not discarded.**
//!
//! Every `Args` struct on this surface ignores what it does not recognise, so a
//! caller sending an argument the verb has no field for got the work done
//! without it and a success answer that said nothing. The concrete case is
//! parentage: an entity's parent is stored, `add_entity` takes no argument for
//! it, and a caller passing one got a flat entity and `status: ok` — with no
//! way to tell "parentage is not on the surface yet" from "I set it and it
//! worked".
//!
//! **The check reads the schema jojobot serves**, not a list kept beside it. A
//! verb's arguments are already published to every client in `input_schema`, so
//! deriving the known names from there means the check cannot drift from the
//! struct, and a verb that gains an argument is covered the moment it does.
//!
//! **Not `#[serde(deny_unknown_fields)]`**, which was the obvious reach and is
//! the wrong shape twice over. It fails inside the deserializer, so the caller
//! gets a protocol error carrying serde's words instead of a blocked answer
//! with a way forward (rule 68) — and it would have to be spelled on every
//! struct, which is a list that goes stale the first time somebody forgets it.
//!
//! **Top level only.** A sub-object's own fields — today only `search`'s
//! `edge` — are not walked, because resolving the `$ref` a nested schema is
//! published as is a second mechanism and this one is honest about its edge
//! rather than pretending to cover it.

use super::*;

/// The argument names a verb publishes, or none when its schema names no
/// properties at all — which is a verb that takes nothing, not a verb that
/// takes anything.
fn published(schema: &serde_json::Map<String, serde_json::Value>) -> Vec<&str> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// Every argument in the call that the verb does not implement, in the order
/// the caller sent them.
fn unimplemented<'a>(
    arguments: &'a serde_json::Map<String, serde_json::Value>,
    known: &[&str],
) -> Vec<&'a str> {
    arguments
        .keys()
        .map(String::as_str)
        .filter(|name| !known.contains(name))
        .collect()
}

impl Jojobot {
    /// The refusal a call earns by naming an argument its verb does not
    /// implement, or `None` when every argument is one the verb has.
    ///
    /// **Nothing has run when this answers.** It is checked before dispatch, so
    /// a refused call has written nothing — which is the half that matters: a
    /// refusal that still created the entity would have fixed the message and
    /// kept the bug.
    ///
    /// A verb this server does not serve is not this check's business and
    /// passes through to the router, which owns that answer.
    pub(crate) fn unimplemented_arguments(
        &self,
        request: &CallToolRequestParams,
    ) -> Option<CallToolResult> {
        let arguments = request.arguments.as_ref()?;
        let tool = self.tool_router.get(&request.name)?;
        let known = published(&tool.input_schema);
        let unknown = unimplemented(arguments, &known);
        if unknown.is_empty() {
            return None;
        }
        // **Both halves, because a caller acts on both.** Which argument was
        // not understood, so they can tell a typo from a capability that is not
        // here yet; and what the verb does take, so the next call is one they
        // can write without going back to the schema.
        let named = unknown.join(", ");
        let takes = if known.is_empty() {
            "it takes no arguments at all".to_string()
        } else {
            format!("it takes: {}", known.join(", "))
        };
        Some(misused(format!(
            "Nothing was written. {} does not implement {named} — {takes}. An argument this \
             surface does not have is refused rather than dropped, because a call that quietly \
             ignored it would report success for work it did not do. Send the call again without \
             it, or use the argument above that means what you meant.",
            request.name,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

    /// A call as a client sends one.
    fn call(tool: &str, arguments: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams::new(tool.to_string()).with_arguments(
            arguments
                .as_object()
                .expect("arguments are an object")
                .clone(),
        )
    }

    /// The advice a refusal carries, or a panic naming what came back instead.
    fn advice(refusal: Option<CallToolResult>) -> String {
        let body = json_of(&refusal.expect("this call names an argument the verb does not have"));
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
        body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal says what to do instead: {body}"))
            .to_string()
    }

    /// **The concrete case.** `add_entity` implements no `parent`, so a call
    /// carrying one is refused and the refusal names it.
    ///
    /// Paired with the positive it depends on: the same call without that one
    /// argument is not refused. Without it this passes identically against a
    /// check that refuses everything.
    #[tokio::test]
    async fn an_argument_the_verb_does_not_implement_is_refused_by_name() {
        let jojobot = handler();
        let whole = serde_json::json!({
            "kind": "thing", "handle": "bike-chain", "name": "Chain",
            "source": "user-named", "parent": "thing:gravel-bike", "sid": "any",
        });
        assert!(
            advice(jojobot.unimplemented_arguments(&call("add_entity", whole))).contains("parent"),
            "the refusal names the argument that was not understood"
        );

        let implemented = serde_json::json!({
            "kind": "thing", "handle": "bike-chain", "name": "Chain",
            "source": "user-named", "sid": "any",
        });
        assert!(
            jojobot
                .unimplemented_arguments(&call("add_entity", implemented))
                .is_none(),
            "a call naming only arguments the verb has passes straight through"
        );
    }

    /// **Every argument that was not understood, not just the first**, and what
    /// the verb does take beside them — a caller fixing one at a time
    /// round-trips once per mistake, and one told only what is wrong has to go
    /// back to the schema to write the next call.
    #[tokio::test]
    async fn the_refusal_names_every_argument_it_did_not_understand() {
        let advice = advice(handler().unimplemented_arguments(&call(
            "capture",
            serde_json::json!({
                "subject": "person:alpha", "content": "a claim",
                "confidence": "high", "happened_at": "2026-08-09", "sid": "any",
            }),
        )));
        assert!(advice.contains("confidence"), "{advice}");
        assert!(advice.contains("happened_at"), "{advice}");
        assert!(advice.contains("provenance"), "{advice}");
    }

    /// **A verb that takes nothing says so**, rather than offering an empty
    /// list of arguments to choose from. `ping` is the one verb with no
    /// parameters at all, so an empty property set means "takes nothing" and
    /// never "takes anything".
    #[tokio::test]
    async fn a_verb_with_no_arguments_refuses_every_one_of_them() {
        let jojobot = handler();
        let advice = advice(
            jojobot.unimplemented_arguments(&call("ping", serde_json::json!({"sid": "any"}))),
        );
        assert!(advice.contains("no arguments"), "{advice}");
        assert!(
            jojobot
                .unimplemented_arguments(&call("ping", serde_json::json!({})))
                .is_none(),
            "a probe that sends nothing is not refused"
        );
    }

    /// **A verb this server does not serve is the router's answer, not this
    /// check's.** Refusing here would report an argument problem for a call
    /// whose problem is the verb, and the caller would go looking for the wrong
    /// mistake.
    #[tokio::test]
    async fn a_verb_that_does_not_exist_is_left_to_the_router() {
        assert!(
            handler()
                .unimplemented_arguments(&call("teleport", serde_json::json!({"to": "anywhere"})))
                .is_none()
        );
    }
}
