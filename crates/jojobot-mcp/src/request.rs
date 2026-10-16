//! Request ids at the dispatch. See [`jojobot_domain::request`].
//!
//! **One lookup, ahead of everything a verb does.** A call that carries a
//! `request_id` is authenticated, and the id is looked up before the verb runs,
//! before its beat, its teaching and its served accounting. A spent id returns
//! the stored receipt and runs none of them.
//!
//! **A call that wrote nothing spends no id.** The id is spent by the commit that
//! stores the write, in that commit's own transaction. A refusal, a block or a
//! no-op stores nothing, so sending it again runs it again.

use std::sync::Arc;

use jojobot_domain::request::{Request, RequestId, Scope, Stored, digest_of};
use rmcp::model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock};
use serde_json::{Map, Value};

use crate::Jojobot;
use crate::answer::WayForward;
use crate::caller::handle_declined;

/// The argument that carries the id.
pub(crate) const ARGUMENT: &str = "request_id";

/// What the dispatch does with a call, once it has looked at its id.
pub(crate) enum Begun {
    /// The call carries no id, and runs as it always did.
    Absent,
    /// The call is refused before it runs, and the answer says why.
    Refused(CallToolResult),
    /// The id is spent: this is the stored receipt, marked as a repeat.
    Repeat(CallToolResult),
    /// The id is new: the verb runs inside this scope.
    Fresh(Arc<Scope>),
}

impl Jojobot {
    /// **Look at the id of a call, and decide what the dispatch does.**
    pub(crate) async fn begin_request(
        &self,
        call: &CallToolRequestParams,
        sid: Option<&str>,
    ) -> Begun {
        let Some(arguments) = call.arguments.as_ref() else {
            return Begun::Absent;
        };
        let Some(raw) = arguments.get(ARGUMENT).and_then(Value::as_str) else {
            return Begun::Absent;
        };
        let verb = call.name.as_ref();
        let Some(ledger) = self.requests.as_ref() else {
            return Begun::Refused(handle_declined(
                verb,
                "This instance keeps no request ids, so nothing was written. Send the call \
                 without request_id.",
            ));
        };
        let id = match RequestId::parse(raw) {
            Ok(id) => id,
            Err(refused) => {
                return Begun::Refused(handle_declined(
                    verb,
                    format!(
                        "{refused}. Send a request_id of one to {} bytes with no control \
                         character and no space at either end, or leave it off. Nothing was \
                         written.",
                        jojobot_domain::request::MAX_ID_BYTES
                    ),
                ));
            }
        };
        let caller = match self.caller(sid) {
            Ok(Some(caller)) => caller,
            Ok(None) => {
                return Begun::Refused(handle_declined(
                    verb,
                    "A request_id needs a session: boot with start_here and send the sid it \
                     returns, or send the call without request_id. Nothing was written.",
                ));
            }
            Err(refused) => return Begun::Refused(refused),
        };
        let digest = digest_of(&canonical_arguments(arguments));
        let unreadable = || {
            Begun::Refused(handle_declined(
                verb,
                WayForward::retry(
                    "The ledger of request ids could not be read, so nothing was written. Send \
                     the same call again.",
                ),
            ))
        };
        match ledger.lookup(&caller.bot, &id).await {
            Ok(Some(stored)) => return Begun::Repeat(replay(&stored, verb, &digest)),
            Ok(None) => {}
            Err(_) => return unreadable(),
        }
        // **A write that commits in parts may have written some of them under
        // this id.** The call that finishes it must be the call that began it: a
        // different verb or different arguments would finish a plan it does not
        // hold, so it is refused and the parts that stand are named.
        let written = match ledger.parts(&caller.bot, &id).await {
            Ok(written) => written,
            Err(_) => return unreadable(),
        };
        if let Some(first) = written.first()
            && (first.verb != verb || first.digest != digest)
        {
            return Begun::Refused(handle_declined(
                verb,
                format!(
                    "This request_id began a {} that wrote {} and did not finish, and this call \
                     is not the same call, so nothing was written. Send the first call again \
                     unchanged to finish it, or send this one under a new request_id.",
                    first.verb,
                    crate::answer::counted(written.len(), "part", "parts"),
                ),
            ));
        }
        let request = Request {
            bot: caller.bot,
            id,
            verb: verb.to_string(),
            digest,
        };
        Begun::Fresh(if written.is_empty() {
            Scope::new(request)
        } else {
            Scope::resuming(
                request,
                written
                    .into_iter()
                    .map(|row| (row.part, row.recorded))
                    .collect(),
            )
        })
    }

    /// **After the verb has answered.** If the write spent the id, the answer is
    /// stored as the receipt of that id. If it did not, another call may have
    /// spent the id while this one ran, and then this one's answer is replaced by
    /// that call's receipt. Returns the replacement, when there is one.
    ///
    /// **This runs only when the verb answered.** A verb that ends in a protocol
    /// error returns from the dispatch before it, so a loser of a race between two
    /// calls under one id is not given the winner's receipt on that call. That is
    /// accepted and left alone on purpose: the loser is told to retry, and its
    /// resend under the same id finds the winner's row and answers as a repeat with
    /// the winner's receipt, so the caller ends with the truth one call later.
    /// Running this on the error path as well would need a dispatch test hook to
    /// prove, and the PM ruled against building one.
    pub(crate) async fn end_request(
        &self,
        scope: &Scope,
        answered: &CallToolResponse,
    ) -> Option<CallToolResult> {
        let ledger = self.requests.as_ref()?;
        let request = scope.request();
        if scope.took_parts() {
            // **A write that commits in parts spends its id when the verb says
            // every part is written.** A verb that stopped earlier leaves the id
            // open, and the same call under the same id finishes it.
            if scope.was_finished() {
                let mut kept = match answered {
                    CallToolResponse::Complete(result) => Some(result.clone()),
                    _ => None,
                };
                if let Some(result) = kept.as_mut() {
                    crate::held::without_notice(result);
                    without_resume_marks(result);
                }
                if let Some(result) = &kept
                    && let Ok(receipt) = serde_json::to_string(result)
                    && let Err(error) = ledger.finish(request, &receipt).await
                {
                    tracing::error!(%error, "the receipt of a finished request id was not stored");
                }
            }
            return None;
        }
        if scope.was_spent() {
            // **The receipt holds no notice of what changed**: it is the answer
            // to the write, replayed to a caller whose held things did not move
            // at that moment.
            let mut kept = match answered {
                CallToolResponse::Complete(result) => Some(result.clone()),
                _ => None,
            };
            if let Some(result) = kept.as_mut() {
                crate::held::without_notice(result);
            }
            if let Some(result) = &kept
                && let Ok(receipt) = serde_json::to_string(result)
                && let Err(error) = ledger.fill(request, &receipt).await
            {
                tracing::error!(%error, "the receipt of a spent request id was not stored");
            }
            return None;
        }
        match ledger.lookup(&request.bot, &request.id).await {
            Ok(Some(stored)) => Some(replay(&stored, &request.verb, &request.digest)),
            _ => None,
        }
    }
}

/// **The receipt of a finished write reads as the write, not as the call that
/// finished it**: the marks [`note_resumed`] added are taken off before it is
/// stored, and a repeat adds its own.
fn without_resume_marks(result: &mut CallToolResult) {
    let Some(block) = result.content.first_mut() else {
        return;
    };
    let Some(text) = block.as_text() else {
        return;
    };
    let Ok(mut body) = serde_json::from_str::<Value>(&text.text) else {
        return;
    };
    let Some(object) = body.as_object_mut() else {
        return;
    };
    for key in ["repeat", "repeat_note", "parts_already_written"] {
        object.remove(key);
    }
    *block = ContentBlock::text(body.to_string());
}

/// **Mark an answer built by a call that skipped parts an earlier call wrote.**
/// The answer is marked as a repeat and names the parts that were already
/// written, so a caller reads that this call finished the write rather than
/// made it. Returns whether any part was skipped.
pub(crate) fn note_resumed(body: &mut Value) -> bool {
    let skipped = jojobot_domain::request::skipped();
    if skipped.is_empty() {
        return false;
    }
    body["repeat"] = true.into();
    body["parts_already_written"] = skipped.into();
    body["repeat_note"] = "This request id began a write that commits in parts and did not \
        finish. The parts named in `parts_already_written` were written by an earlier call and \
        were not written again; this call wrote the rest."
        .into();
    true
}

/// **The arguments of a call as one text, the same for the same arguments**: keys
/// in order at every depth, and without `sid` and `request_id`, which differ
/// between two sends of one call.
fn canonical_arguments(arguments: &Map<String, Value>) -> String {
    let mut kept = Map::new();
    for (key, value) in arguments {
        if key != ARGUMENT && key != "sid" {
            kept.insert(key.clone(), value.clone());
        }
    }
    let mut out = String::new();
    write_canonical(&Value::Object(kept), &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (place, key) in keys.into_iter().enumerate() {
                if place > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (place, item) in items.iter().enumerate() {
                if place > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// **The stored receipt, marked as a repeat.** It writes nothing and says so.
///
/// When the receipt was not kept (the process stopped between the commit and the
/// store of the receipt), the answer says that the write landed and gives what
/// the write returned.
fn replay(stored: &Stored, verb: &str, digest: &str) -> CallToolResult {
    let kept = stored
        .receipt
        .as_deref()
        .and_then(|text| serde_json::from_str::<CallToolResult>(text).ok())
        .and_then(|result| {
            let body = result
                .content
                .first()
                .and_then(|block| block.as_text())
                .and_then(|text| serde_json::from_str::<Value>(&text.text).ok())?;
            body.is_object().then_some((result, body))
        });
    let (mut result, mut body) = match kept {
        Some(kept) => kept,
        None => {
            let landed = serde_json::from_str::<Value>(&stored.landed)
                .unwrap_or_else(|_| Value::String(stored.landed.clone()));
            let body = serde_json::json!({
                "status": "landed",
                "receipt_kept": false,
                "landed": landed,
            });
            (CallToolResult::success(Vec::new()), body)
        }
    };
    body["repeat"] = true.into();
    let mut note = String::from(
        "This request id was spent by a write that landed. This is the receipt that call \
         returned, and nothing was written now.",
    );
    if stored.receipt.is_none() {
        note.push_str(
            " The receipt was not kept, because the process stopped after the write landed; \
             `landed` says what the write returned.",
        );
    }
    if stored.verb != verb {
        body["first_verb"] = stored.verb.as_str().into();
        note.push_str(&format!(
            " The first call was a {}, not a {verb}.",
            stored.verb
        ));
    }
    if stored.digest != digest {
        body["arguments_differed"] = true.into();
        note.push_str(
            " The arguments sent now differ from the arguments of the first call, and the first \
             call's write stands.",
        );
    }
    body["repeat_note"] = note.into();
    result.content = vec![ContentBlock::text(body.to_string())];
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(receipt: Option<&str>) -> Stored {
        Stored {
            verb: "capture".into(),
            digest: digest_of("{}"),
            landed: "{\"address\":\"person:milhouse#f1\"}".into(),
            receipt: receipt.map(str::to_string),
        }
    }

    fn body(result: &CallToolResult) -> Value {
        serde_json::from_str(&result.content[0].as_text().expect("text").text).expect("json")
    }

    #[test]
    fn a_repeat_whose_receipt_was_not_kept_says_the_write_landed_and_gives_its_ids() {
        let replayed = replay(&stored(None), "capture", &digest_of("{}"));
        let body = body(&replayed);
        assert_eq!(body["repeat"], true);
        assert_eq!(body["receipt_kept"], false);
        assert_eq!(body["status"], "landed");
        assert_eq!(body["landed"]["address"], "person:milhouse#f1");
    }

    #[test]
    fn a_repeat_with_a_kept_receipt_gives_it_back_with_the_marks_and_nothing_else_changed() {
        let first = CallToolResult::success(vec![ContentBlock::text(
            serde_json::json!({"address": "person:milhouse#f1", "wrote": true}).to_string(),
        )]);
        let kept = serde_json::to_string(&first).expect("serializes");
        let replayed = replay(&stored(Some(&kept)), "capture", &digest_of("{}"));
        let body = body(&replayed);
        assert_eq!(body["address"], "person:milhouse#f1");
        assert_eq!(body["wrote"], true);
        assert_eq!(body["repeat"], true);
        assert!(body.get("receipt_kept").is_none());
        assert!(body.get("arguments_differed").is_none());
        assert!(body.get("first_verb").is_none());
    }

    #[test]
    fn a_repeat_on_another_verb_or_with_other_arguments_says_which() {
        let replayed = replay(&stored(None), "update_fact", &digest_of("{\"a\":1}"));
        let body = body(&replayed);
        assert_eq!(body["first_verb"], "capture");
        assert_eq!(body["arguments_differed"], true);
    }

    #[test]
    fn the_arguments_of_a_call_read_the_same_whatever_order_they_arrive_in() {
        let one: Map<String, Value> = serde_json::from_str(
            r#"{"b":{"y":2,"x":1},"a":[3,{"q":1,"p":2}],"sid":"k","request_id":"r1"}"#,
        )
        .expect("json");
        let other: Map<String, Value> = serde_json::from_str(
            r#"{"request_id":"r2","a":[3,{"p":2,"q":1}],"sid":"j","b":{"x":1,"y":2}}"#,
        )
        .expect("json");
        assert_eq!(canonical_arguments(&one), canonical_arguments(&other));
        assert!(!canonical_arguments(&one).contains("request_id"));
        assert!(!canonical_arguments(&one).contains("\"sid\""));
        // The positive the equality rests on: a different value is a different text.
        let differing: Map<String, Value> =
            serde_json::from_str(r#"{"b":{"x":1,"y":3},"a":[3,{"p":2,"q":1}]}"#).expect("json");
        assert_ne!(canonical_arguments(&one), canonical_arguments(&differing));
    }
}
