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
//! **Every level, by the one mechanism.** An argument that is a struct of its
//! own — today only `search`'s `edge` — is judged against the names ITS schema
//! publishes, by the same walk, reached through the indirection schemars emits
//! for a nested type. Nothing here enumerates what a level may contain: the
//! permitted shape is declared once, by the structs, and read at every depth.

use super::*;

/// How deep the walk follows a schema before it stops. A struct that referred
/// to itself would otherwise resolve for ever; stopping leaves that level
/// unresolvable, which the coverage test turns into a red bar rather than a
/// silent pass.
const MAX_DEPTH: usize = 8;

/// The arguments a verb publishes at its own level, or none when its schema
/// names no properties at all — which is a verb that takes nothing, not a verb
/// that takes anything.
fn published(
    schema: &serde_json::Map<String, serde_json::Value>,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    schema.get("properties")?.as_object()
}

/// The schema a `$ref` names, looked up in the root's `$defs`.
fn definition<'a>(
    reference: &str,
    root: &'a serde_json::Map<String, serde_json::Value>,
) -> Option<&'a serde_json::Value> {
    let name = reference.strip_prefix("#/$defs/")?;
    root.get("$defs")?.as_object()?.get(name)
}

/// **What one schema node publishes**, following the indirection a nested
/// struct is emitted through.
///
/// schemars writes an optional struct field as `anyOf` of a `$ref` into
/// `$defs` and a null branch, and a required one as the `$ref` alone. Both
/// arrive here, and so does the plain inline object; each is the same question
/// — which names does this level have — asked one hop further away.
///
/// `None` means this node describes no set of names: a string, a number, or a
/// shape nothing here knows how to follow. The two are not distinguished on
/// the call path, because a caller cannot act on the difference; they are
/// distinguished by the coverage test, which is where an unfollowable shape
/// has to go red.
fn fields<'a>(
    node: &'a serde_json::Value,
    root: &'a serde_json::Map<String, serde_json::Value>,
    depth: usize,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if let Some(own) = node.get("properties").and_then(|p| p.as_object()) {
        return Some(own);
    }
    if let Some(reference) = node.get("$ref").and_then(|r| r.as_str()) {
        return definition(reference, root).and_then(|target| fields(target, root, depth + 1));
    }
    // **A list of objects publishes its names once, on the element.** Every
    // item of a list is the same shape, so the names a caller may send inside
    // one are the element's names — and a walk that stopped at the array would
    // leave every name inside it unchecked while every other test stayed green.
    if let Some(items) = node.get("items") {
        return fields(items, root, depth + 1);
    }
    // The null branch of an optional field publishes nothing, so the first
    // branch that does is the shape being described.
    ["anyOf", "oneOf", "allOf"]
        .iter()
        .filter_map(|keyword| node.get(keyword))
        .filter_map(|branches| branches.as_array())
        .flatten()
        .find_map(|branch| fields(branch, root, depth + 1))
}

/// One place a call can name an argument: the call itself, or a sub-object
/// inside it. Each is judged against the names its own schema publishes.
struct Level {
    /// The path a refusal names it by — empty at the top level, where the
    /// verb's own name is what the caller sees.
    path: String,
    /// What this level publishes, in schema order.
    takes: Vec<String>,
    /// What the caller sent here that this level does not have.
    unknown: Vec<String>,
}

impl Level {
    /// How a refusal names one of this level's arguments: bare at the top,
    /// and qualified below it, because `weight` alone sends a caller looking
    /// at the wrong level.
    fn naming(&self, argument: &str) -> String {
        if self.path.is_empty() {
            argument.to_string()
        } else {
            format!("{}.{argument}", self.path)
        }
    }

    /// What this level does take, said the way its own reader needs it.
    fn offering(&self) -> String {
        match (self.path.as_str(), self.takes.is_empty()) {
            ("", true) => "it takes no arguments at all".to_string(),
            ("", false) => format!("it takes: {}", self.takes.join(", ")),
            (path, true) => format!("{path} takes no arguments at all"),
            (path, false) => format!("{path} takes: {}", self.takes.join(", ")),
        }
    }
}

/// Every level of the call that was handed a name it does not have, outermost
/// first, each carrying what it does take. A level the caller got entirely
/// right is left out.
fn unimplemented(
    sent: &serde_json::Map<String, serde_json::Value>,
    here: &serde_json::Map<String, serde_json::Value>,
    root: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    found: &mut Vec<Level>,
) {
    let mut level = Level {
        path: path.to_string(),
        takes: here.keys().cloned().collect(),
        unknown: Vec::new(),
    };
    let mut deeper = Vec::new();
    for (name, value) in sent {
        let Some(node) = here.get(name) else {
            level.unknown.push(name.clone());
            continue;
        };
        // A sub-object is judged only when the caller sent one and the schema
        // describes one. Anything else is a value, and values are the
        // deserializer's business.
        let Some(names) = fields(node, root, 0) else {
            continue;
        };
        let below = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}.{name}")
        };
        match value {
            serde_json::Value::Object(inner) => deeper.push((inner, names, below)),
            // **A list element is a level too.** A caller names arguments
            // inside one exactly as inside a sub-object, so it is judged
            // against the same names — every element of a list shares one
            // shape, which is why one set of names serves them all. The index
            // rides the path, because a caller fixing a list of five needs to
            // know which element to look at.
            serde_json::Value::Array(items) => {
                for (at, item) in items.iter().enumerate() {
                    if let Some(inner) = item.as_object() {
                        deeper.push((inner, names, format!("{below}[{at}]")));
                    }
                }
            }
            _ => {}
        }
    }
    // Outermost first: a caller reads the level they typed at before the one
    // inside it.
    if !level.unknown.is_empty() {
        found.push(level);
    }
    for (inner, names, below) in deeper {
        unimplemented(inner, names, root, &below, found);
    }
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
        let root = &tool.input_schema;
        let empty = serde_json::Map::new();
        let top = published(root).unwrap_or(&empty);
        let mut levels = Vec::new();
        unimplemented(arguments, top, root, "", &mut levels);
        if levels.is_empty() {
            return None;
        }
        // **Both halves, because a caller acts on both.** Which argument was
        // not understood — by its path, so a name inside a sub-object does not
        // send them looking at the wrong level — and what the level that met it
        // does take, so the next call is one they can write without going back
        // to the schema.
        let named = levels
            .iter()
            .flat_map(|level| level.unknown.iter().map(|name| level.naming(name)))
            .collect::<Vec<_>>()
            .join(", ");
        let takes = levels
            .iter()
            .map(Level::offering)
            .collect::<Vec<_>>()
            .join("; ");
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

    /// **The same defect one level down.** `search`'s `edge` is the one
    /// argument on this surface that is a struct of its own, and its fields
    /// went unread: a caller sending `weight` inside it had the search done
    /// without it and got a success answer, which is the whole reason this
    /// check exists.
    ///
    /// The refusal names the argument by its **path**, because `weight` alone
    /// sends a caller looking at the wrong level, and it says what the
    /// sub-object takes rather than what the verb takes — the names that would
    /// have worked are the ones beside the mistake.
    ///
    /// Paired with the positive: a well-formed `edge` passes straight through.
    /// Without that, this passes identically on a check that turns back every
    /// call carrying a sub-object.
    #[tokio::test]
    async fn an_argument_a_sub_object_does_not_implement_is_refused_by_path() {
        let jojobot = handler();
        let advice = advice(jojobot.unimplemented_arguments(&call(
            "search",
            serde_json::json!({
                "query": "the guild",
                "edge": {"shape": "membership", "object": "org:guild", "weight": 3},
                "sid": "any",
            }),
        )));
        assert!(
            advice.contains("edge.weight"),
            "the refusal names which level the argument sat at: {advice}"
        );
        assert!(
            advice.contains("shape") && advice.contains("object"),
            "…and what that level does take: {advice}"
        );

        assert!(
            jojobot
                .unimplemented_arguments(&call(
                    "search",
                    serde_json::json!({
                        "query": "the guild",
                        "edge": {"shape": "membership", "object": "org:guild"},
                        "sid": "any",
                    }),
                ))
                .is_none(),
            "a well-formed sub-object was turned back"
        );
    }

    /// **The same defect inside a LIST of objects.** `declare_type`'s `fields`
    /// is a list, and a list element is a level a caller names arguments at
    /// exactly as a sub-object is: a key inside one that the surface does not
    /// have was dropped by serde and the call answered `ok`.
    ///
    /// This goes through `unimplemented_arguments` — the walk a call actually
    /// takes — rather than through `fields`, the helper that reads the schema.
    /// The helper follows a list already; the walk is what did not, so a case
    /// that asked the helper would have been green against the defect.
    ///
    /// Paired with the positive: a well-formed list passes straight through.
    /// Without it this passes identically on a check that turns back every call
    /// carrying a list.
    #[tokio::test]
    async fn an_argument_a_list_element_does_not_implement_is_refused_by_path() {
        let jojobot = handler();
        let advice = advice(jojobot.unimplemented_arguments(&call(
            "declare_type",
            serde_json::json!({
                "name": "warranty",
                "fields": [
                    {"key": "expires", "holds": "date"},
                    {"key": "cost", "holds": "number", "required": true, "unit": "eur"},
                ],
                "sid": "any",
            }),
        )));
        assert!(
            advice.contains("fields[1].required") && advice.contains("fields[1].unit"),
            "the refusal names every argument inside the list, by path and by element: {advice}"
        );
        assert!(
            advice.contains("key") && advice.contains("holds"),
            "…and what a list element does take: {advice}"
        );

        assert!(
            jojobot
                .unimplemented_arguments(&call(
                    "declare_type",
                    serde_json::json!({
                        "name": "warranty",
                        "fields": [{"key": "expires", "holds": "date"}],
                        "sid": "any",
                    }),
                ))
                .is_none(),
            "a well-formed list was turned back"
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

    /// **The two doors reached without an identity implement the handle**, so
    /// this check lets it through. A session is taught to carry its `sid` on
    /// every call, reads included, and these two are the calls it makes most:
    /// the door it re-enters to fetch a procedure, and the probe it reaches for
    /// when the surface looks wrong. Refusing the caller that does what it was
    /// told is the shape this check exists to prevent, not to produce.
    ///
    /// Paired with the negative it rests on: an argument neither door has is
    /// still refused, and the refusal names the handle among the ones they take.
    #[tokio::test]
    async fn the_doors_reached_without_an_identity_implement_the_handle() {
        let jojobot = handler();
        for door in ["start_here", "ping"] {
            assert!(
                jojobot
                    .unimplemented_arguments(&call(door, serde_json::json!({"sid": "any"})))
                    .is_none(),
                "{door} turned back the handle every session is told to carry"
            );
            let advice = advice(
                jojobot.unimplemented_arguments(&call(door, serde_json::json!({"session": "any"}))),
            );
            assert!(
                advice.contains("session") && advice.contains("sid"),
                "{door}: the refusal names what it did not understand, and the handle it does \
                 take: {advice}"
            );
        }
    }

    /// **The wording for a verb that takes nothing has no verb behind it any
    /// more**, and this is where that is said out loud rather than a test
    /// quietly deleted.
    ///
    /// `ping` was the one verb publishing no properties at all, so the refusal
    /// it earned — "it takes no arguments at all" — was the only exercise that
    /// branch had. It implements the handle now. The branch stays for the next
    /// verb that takes nothing, and this fails on the day there is one, which
    /// is the day the refusal above wants writing again.
    #[test]
    fn no_served_verb_publishes_an_empty_argument_schema() {
        for tool in Jojobot::tool_router().list_all() {
            assert!(
                published(&tool.input_schema).is_some_and(|takes| !takes.is_empty()),
                "{} publishes no arguments — the branch that answers for one is reachable again",
                tool.name
            );
        }
    }

    /// **Every sub-object the surface publishes is one this walk can follow.**
    ///
    /// This is the failure that would pass every other test here. A resolver
    /// that cannot follow a schema shape finds no names, judges nothing, and
    /// degrades exactly to the top-level check that came before it — which is
    /// green. So the day schemars emits a nested type some other way, or a verb
    /// grows a sub-object written differently, nothing would go red and the
    /// nested half would quietly stop running.
    ///
    /// The tell is read out of the schema TEXT, on purpose: asking the walk
    /// whether the walk found something proves nothing. A node whose JSON
    /// mentions a reference, an object type or properties is one a caller can
    /// send an object for, and every one of those must resolve to a set of
    /// names.
    #[test]
    fn every_sub_object_on_the_surface_is_one_the_walk_can_follow() {
        /// Does this argument's schema describe something a caller sends an
        /// object for? Read from the text, so it shares no code with `fields`.
        fn nests(node: &serde_json::Value) -> bool {
            let text = node.to_string();
            ["\"$ref\"", "\"type\":\"object\"", "\"properties\""]
                .iter()
                .any(|marker| text.contains(marker))
        }

        let mut examined = 0;
        for tool in Jojobot::tool_router().list_all() {
            let root = &tool.input_schema;
            let Some(properties) = root.get("properties").and_then(|p| p.as_object()) else {
                continue;
            };
            for (name, node) in properties {
                if !nests(node) {
                    continue;
                }
                examined += 1;
                assert!(
                    fields(node, root, 0).is_some(),
                    "{}'s `{name}` is a sub-object this walk cannot follow, so its fields go \
                     unchecked and nothing else here notices: {node}",
                    tool.name,
                );
            }
        }
        // The positive the sweep rests on: it looked at something. Without
        // this, a surface that published no sub-object at all — or a `nests`
        // that stopped recognising one — would pass this test in silence.
        assert!(
            examined > 0,
            "no sub-object was examined, so this test asserted nothing"
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
