//! `declare_type` — Say what a type's keys are, so a writer knows what to fill.
//!
//! One verb, one file: its arguments, the description a caller reads, and an
//! entrypoint that chains the systems below it.
//!
//! **This verb admits nothing.** A THING carrying a type's keys is found by
//! `search` whether or not this was ever called, so declaring is write-time
//! help and never a precondition. That is why there is no verb to undeclare
//! one: a declaration describes, and the only thing it can be wrong about is
//! itself.
//!
//! **The unit matched is the thing, never one write's own record.** A thing
//! answers a type over every write on it, folded — so a type is asked of what
//! the thing IS now, and something described over two sittings answers a type
//! neither sitting answers alone.
//!
//! **One thing here is refused**, and it is about the name rather than about
//! anything that answers it: a type the software ships is closed to callers. It
//! is still not a gate on what answers — nothing about that type stops being
//! matched — it is a gate on writing over a declaration the software owns.

use super::*;
use jojobot_domain::memory::kinds;

/// One key of a type, and what it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FieldArgs {
    /// The key a thing carries, exactly as the writes on it spell it. **The key
    /// name IS the schema**: `expires` and `expiry_date` are two different types
    /// and nothing will point that out, because matching is structural and a key
    /// means whatever the things carrying it mean by it.
    pub(crate) key: String,
    /// What the value holds: `text`, `number`, `date`, `date_range`, `boolean`,
    /// or `reference` (another entity's `kind:slug` handle, which is what makes
    /// it walkable). Defaults to `text`, which holds anything — or to `number`
    /// when you declare the key a counter, because a total of anything else is
    /// not a total.
    ///
    /// **A `date_range` is ONE value, not two keys**: two dates with a slash
    /// between them, `2026-04-18/2026-04-25`. Two keys cannot say they belong
    /// together, so a thing carrying a start and no end reads as a thing with a
    /// key missing rather than as a span nobody finished.
    ///
    /// **`list:` in front means zero or more of it** — `list:text`, or
    /// `list:reference:person` for the people who came along. Written as the
    /// items separated by commas, so a thing that has none of something holds
    /// an empty value rather than lacking the key. Reach for it whenever the
    /// answer can be more than one thing: a key that can hold only one is how a
    /// thing ends up described as less than what happened.
    ///
    /// **A reference can name the kind on the other end** — write
    /// `reference:place` and the key holds a place's handle and no other kind's.
    /// Plain `reference` holds a handle of any kind, so naming one narrows the
    /// key rather than describing it.
    ///
    /// **What you say here describes; it does not gate.** A declared type
    /// refuses no write at all — a value that does not hold what the key
    /// declares comes back flagged on the hit, so a read shows the mistake
    /// rather than a write being turned away.
    #[serde(default)]
    pub(crate) holds: Option<String>,
    /// **How the writes of this key come down to the one value it holds.**
    /// `newest` is the default and needs no declaring: the newest write wins,
    /// which is how every key reads unless you say otherwise.
    ///
    /// `sum` makes the key a **counter** — its writes add up. Write one donut
    /// on each of three days and the thing reads back three, so a running total
    /// is arithmetic jojobot does rather than arithmetic a session has to fetch,
    /// add and write back. **Every write is still there**: ask for the key's
    /// `history` and you get each occasion, with the claim it arrived in and
    /// that claim's date.
    ///
    /// ⚠️ **It belongs to the KEY and never to a write.** Declared here once, a
    /// counter cannot be written inconsistently; chosen per write, one caller
    /// adds while another replaces and the value quietly means two things.
    #[serde(default)]
    pub(crate) folds: Option<String>,
    /// **Whether a thing has to hold this key to be one of these at all.**
    /// Defaults to false, and leave it there unless the key really is what
    /// makes the thing what it is: the required keys are the ones a thing is
    /// measured against by the reader who asks *which of these ARE one of
    /// these*.
    ///
    /// ⚠️ **This says nothing about what the key HOLDS.** The two are separate
    /// questions, and the useful one is usually "the colour has to be a colour"
    /// rather than "everything must have a colour". An optional key is welcome,
    /// never demanded, and a thing without it is complete.
    ///
    /// ⚠️ **And a type you declare here holds no write to either answer.** On a
    /// KIND's key, a value is checked against `holds` whenever the key is
    /// written, and a required key a write would take away is refused — which
    /// is why a required key on a kind is a refusal waiting to happen. Here it
    /// is the vocabulary a reader asks with.
    #[serde(default)]
    pub(crate) required: bool,
    /// **The named set this key holds one of** — a closed vocabulary.
    ///
    /// ⚠️ **What that buys depends on who declared the key.** On a KIND's key,
    /// a write outside the set is refused with the values named. **A type you
    /// declare here refuses no write**: the set is what a reader is told about
    /// a value that does not match, so a thing carrying one comes back in an
    /// `answers_type` read with that key reported and the values named.
    ///
    /// Leave it off and the key is narrowed to nothing, which is what nearly
    /// every key wants.
    ///
    /// Reach for it when the set really is small, known and finite: the three
    /// states a task moves through, the two ways a thing can be paid for.
    /// ⛔️ **Not a way to enumerate an open-ended set** — every colour, every
    /// venue — because that is a list somebody then has to maintain, and it
    /// goes stale the day a value it does not name is the true one. There, the
    /// honest declaration is plain `text` and the values already in use are
    /// readable.
    ///
    /// It narrows what the key holds rather than replacing it, exactly as
    /// `reference:place` narrows a reference — so it works on a `list:` key
    /// too, where every item has to be one of the values.
    ///
    /// Values are matched **whole, trimmed and case-sensitively**. A set with
    /// no values, a value named twice, and a value carrying a comma are each
    /// refused, because a comma is what separates the items of a list.
    #[serde(default)]
    pub(crate) one_of: Option<Vec<String>>,
}

/// Arguments to `declare_type`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DeclareTypeArgs {
    /// What the type is called. Declaring a name that already exists
    /// **replaces** its keys — a type is the set of keys it names now.
    ///
    /// **Unless the software ships that type**, which is refused: a shipped
    /// type is closed and the answer says so. Pick a name of your own.
    pub(crate) name: String,
    /// The keys a thing of this type carries. **At least one**: a type is the
    /// keys it names, and a name with nothing under it is one nobody can query
    /// by.
    pub(crate) fields: Vec<FieldArgs>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Declare a type: a name and the keys a thing of it carries.
#[tool_router(router = declare_type_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Declare a type: a name, and the keys a THING of it carries. This is \
                       WRITE-TIME HELP and never a gate — it tells a writer which keys to fill \
                       and tells `search` which keys to look for. THE UNIT IS THE THING: a thing \
                       answers a type over every write on it, folded, so a thing described over \
                       two sittings answers a type neither sitting answers alone. It admits \
                       nothing: a thing \
                       carrying these keys is found by search answers_type whether or not \
                       anybody declared it, and a thing carrying some of them is found too, \
                       saying which it lacks. So declaring a type never makes a thing findable \
                       and never stops one being found, and there is nothing to undeclare. A \
                       type arrives COMPLETE: give at least one key, because a name with nothing \
                       under it is a type nobody can query by. Declaring a name that already \
                       exists REPLACES its keys, whole — a type is the keys it names now, and \
                       one that accumulated every key it ever named would report keys the writer \
                       had already dropped as keys a thing lacks. KEYS ARE SCOPED BY THE TYPE \
                       that names them and are registered nowhere: two types may use one key \
                       name and mean their own thing by it. SOME TYPES SHIP WITH THE SOFTWARE and \
                       those are CLOSED: declaring over one comes back blocked, because a caller \
                       cannot extend, shrink or replace a type the software owns — declare a name \
                       of your own instead. Every type you declare is yours. The answer names \
                       every type that exists, by name only, so you can see what is there without \
                       asking a second time."
    )]
    pub(crate) async fn declare_type(
        &self,
        Parameters(args): Parameters<DeclareTypeArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the write runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let mut fields = Vec::with_capacity(args.fields.len());
        // **What each closed set was sent as**, taken before the values are
        // trimmed, because trimming is what the receipt below reports.
        let mut sent_sets: Vec<(String, String)> = Vec::new();
        for field in &args.fields {
            let folds = match field.folds.as_deref().map(str::trim) {
                None | Some("") => Fold::Newest,
                Some(token) => Fold::of_token(token).ok_or_else(|| {
                    McpError::invalid_params(
                        format!("'{token}' is no fold: use newest or sum"),
                        None,
                    )
                })?,
            };
            let declared = match field.holds.as_deref().map(str::trim) {
                // **A counter holds a number unless the caller says otherwise**
                // (rule 9). A total of dates or of prose is not a total, so the
                // one type a counter can have is the one it gets for free; a
                // caller who names another gets the refusal that says so.
                None | Some("") if folds == Fold::Sum => Field::new(&field.key, ValueType::Number),
                None | Some("") => Field::new(&field.key, ValueType::Text),
                Some(token) => {
                    Field::of_token(&field.key, token).ok_or_else(|| why_no_field(token))?
                }
            };
            fields.push(Field {
                folds,
                required: field.required,
                // **Trimmed here and refused in the domain.** What makes a set
                // unsatisfiable — no values, a repeat, a comma — is one rule in
                // one place, so this verb and any other writer get the same
                // answer.
                one_of: field.one_of.as_ref().map(|values| {
                    sent_sets.push((field.key.clone(), values.join(", ")));
                    values.iter().map(|v| v.trim().to_string()).collect()
                }),
                ..declared
            });
        }

        let declared = match self
            .memory
            .declare_type(DeclaredType::new(&args.name, fields))
            .await
        {
            Ok(declared) => declared,
            Err(e) => return memory_declined("declare_type", e),
        };
        // Read back from the store rather than echoed, so the answer is what a
        // later reader gets and not what this call believed it sent.
        let known = self.memory.declared_types().await.map_err(memory_error)?;
        let mut body = serde_json::json!({
            "type": declared_type_json(&declared),
            // **A name and where it came from — not the keys.** A caller
            // that has just declared one is choosing what else to reach for
            // rather than weighing anybody's keys, and which of these it may
            // declare over is part of choosing.
            "types": known
                .iter()
                .map(|t| {
                    serde_json::json!({ "name": t.name, "origin": t.origin.as_token() })
                })
                .collect::<Vec<_>>(),
        });
        // **Named per key, because a set belongs to one.** A line saying
        // only that a set was trimmed would leave a caller who declared
        // several to work out which.
        //
        // **No reason given.** A trimmed value is what the argument means,
        // and both sides of a closed set compare trimmed — so this states a
        // conversion rather than warning of a consequence.
        let trimmed: Vec<crate::answer::Difference> = declared
            .fields
            .iter()
            .filter_map(|field| {
                let stored = field.one_of.as_ref()?.join(", ");
                let sent = sent_sets
                    .iter()
                    .find(|(key, _)| key == &field.key)
                    .map(|(_, sent)| sent.as_str())?;
                crate::answer::Difference::between(
                    format!("one_of on '{}'", field.key),
                    Some(sent),
                    &stored,
                )
            })
            .collect();
        crate::answer::note_delta(&mut body, trimmed);
        json_result(&body)
    }
}

/// **Why a `holds` token is no field**, in the token's own terms.
///
/// A reference may name the kind it points at, so half of these tokens carry a
/// kind — and a kind fails for reasons of its own. **A process that loaded no
/// kinds fails every one of them**, and the sentence about value types would
/// then send a caller to edit a declaration that is correct (rule 68). The
/// kind's own answer is used where there is one, and it recites nothing this
/// process does not hold (rule 213).
fn why_no_field(token: &str) -> McpError {
    // **The list prefix comes off first**, because what is wrong with
    // `list:reference:nonsense` is the kind, and a check reading the outermost
    // half would report the list wrapper as the fault.
    let held = token.trim().strip_prefix("list:").unwrap_or(token.trim());
    if let Some((holds, kind)) = held.split_once(':')
        && holds.trim() == ValueType::Reference.as_token()
        && let Err(why) = kinds::resolve(kind.trim())
    {
        return McpError::invalid_params(format!("'{token}': {why}"), None);
    }
    McpError::invalid_params(
        format!(
            "'{token}' is no value type: use text, number, date, date_range, boolean or \
             reference — and a reference may name the kind it points at, as 'reference:place', \
             using one of the kinds add_entity takes. Put 'list:' in front of any of them for a \
             key holding zero or more, as 'list:reference:person'"
        ),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use jojobot_domain::memory::types::Origin;

    fn declare_args(name: &str, keys: &[&str]) -> DeclareTypeArgs {
        DeclareTypeArgs {
            name: name.to_string(),
            fields: keys
                .iter()
                .map(|key| FieldArgs {
                    key: (*key).to_string(),
                    holds: None,
                    folds: None,
                    required: false,
                    one_of: None,
                })
                .collect(),
            sid: Some(TEST_SID.to_string()),
        }
    }

    /// 🚨 **A trimmed set value is reported, named by the key it belongs to.**
    ///
    /// **Paired with a set that needed no trimming**, which carries no delta at
    /// all — a line on every declaration is one a reader learns to skip.
    ///
    /// ⚠️ **The trim is behaviourally inert and the line is still right.** Both
    /// sides of a closed set compare trimmed, so a caller who sent padding is
    /// not surprised later. What the line removes is having to diff your own
    /// call against the answer to know the store kept something else.
    ///
    /// **Named per key**, because a type may narrow several and a line that
    /// could not say which would leave a reader guessing.
    #[tokio::test]
    async fn a_trimmed_set_value_is_reported_and_a_clean_one_is_not() {
        let jojobot = handler();
        let narrowed = |name: &str, values: &[&str]| DeclareTypeArgs {
            fields: vec![FieldArgs {
                key: "outcome".into(),
                holds: None,
                folds: None,
                required: false,
                one_of: Some(values.iter().map(|v| (*v).to_string()).collect()),
            }],
            ..declare_args(name, &[])
        };

        let padded = json_of(
            &jojobot
                .declare_type(Parameters(narrowed("chore", &[" ran ", "skipped"])))
                .await
                .expect("declare_type ok"),
        );
        assert_eq!(
            padded["delta"][0]["field"], "one_of on 'outcome'",
            "{padded}",
        );
        assert_eq!(padded["delta"][0]["sent"], " ran , skipped", "{padded}");
        assert_eq!(padded["delta"][0]["stored"], "ran, skipped", "{padded}");
        assert_eq!(
            padded["delta"][0]["because"],
            serde_json::Value::Null,
            "a difference with nothing to explain carries no explanation: {padded}",
        );

        let clean = json_of(
            &jojobot
                .declare_type(Parameters(narrowed("errand", &["ran", "skipped"])))
                .await
                .expect("declare_type ok"),
        );
        assert!(
            clean.get("delta").is_none(),
            "a set that needed no trimming carries no delta: {clean}",
        );
    }

    /// One type out of the store, by name.
    async fn stored(jojobot: &Jojobot, name: &str) -> DeclaredType {
        jojobot
            .memory
            .declared_types()
            .await
            .expect("the roster reads")
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("the store must hold '{name}'"))
    }

    /// **A caller cannot declare over a type the software ships, and the
    /// refusal is an answer rather than a failure** (rule 68).
    ///
    /// The way forward is the feature: a shipped type is closed, so re-sending
    /// the same call will never work and the caller has to be told to use a
    /// name of its own. Advice that read "fix the call and send it again"
    /// would send a model round a loop that cannot end.
    #[tokio::test]
    async fn declaring_over_a_shipped_type_is_refused_and_writes_nothing() {
        let jojobot = handler();
        writing_as(&jojobot);
        let shipped = DeclaredType::shipped(
            "rota",
            vec![
                Field::new("starts", ValueType::Date),
                Field::new("cover", ValueType::Reference),
            ],
        );
        jojobot
            .memory
            .declare_type(shipped.clone())
            .await
            .expect("the software declares its own types");

        let result = jojobot
            .declare_type(Parameters(declare_args("rota", &["starts"])))
            .await
            .expect("a refusal is an answer, not a protocol failure");

        let body = blocked(&result);
        let advice = body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal carries its way forward: {body}"));
        assert!(
            advice.contains("rota"),
            "the refusal names the type it is about: {advice}"
        );
        assert!(
            advice.contains("declare_type"),
            "…and names the verb to call with a name of your own: {advice}"
        );

        assert_eq!(
            stored(&jojobot, "rota").await,
            shipped,
            "the shipped type is untouched — keys, order and origin",
        );
    }

    /// **Where a type came from reaches the wire — on the type, and on every
    /// type in the list beside it.**
    ///
    /// Without it a caller cannot tell a type the software ships from one it
    /// declared itself, and the only way to find out is to declare over it and
    /// read the refusal. That makes a closed type undiscoverable rather than
    /// merely undocumented, which is rule 62 the wrong way round: the surface
    /// should let a caller avoid the refusal, not spring it.
    ///
    /// Both origins in one read, because an answer that says `declared` on
    /// everything passes a case that only ever looks at a caller's own type.
    #[tokio::test]
    async fn a_type_says_where_it_came_from_on_the_wire_and_in_the_list() {
        let jojobot = handler();
        writing_as(&jojobot);
        jojobot
            .memory
            .declare_type(DeclaredType::shipped(
                "rota",
                vec![Field::new("starts", ValueType::Date)],
            ))
            .await
            .expect("the software declares its own types");

        let body = json_of(
            &jojobot
                .declare_type(Parameters(declare_args("service", &["cost"])))
                .await
                .expect("declaring a name of my own is accepted"),
        );
        assert_eq!(
            body["type"]["origin"], "declared",
            "a type this caller declared says so: {body}"
        );

        let listed = |name: &str| {
            body["types"]
                .as_array()
                .unwrap_or_else(|| panic!("the answer lists the types that exist: {body}"))
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("the list holds '{name}': {body}"))
                .clone()
        };
        assert_eq!(
            listed("rota")["origin"],
            "shipped",
            "…and the closed one in the list beside it says THAT, which is what \
             tells a caller not to declare over it: {body}"
        );
        assert_eq!(listed("service")["origin"], "declared");
    }

    /// **A reference declares the kind it points at, through the served
    /// surface and back out of the store.**
    ///
    /// Sent as a caller writes it, read back from the store rather than from
    /// the answer, and served again — because a narrowing the verb parsed and
    /// the store dropped would pass any beat that only read the response to the
    /// call that wrote it.
    ///
    /// The unnarrowed reference is declared in the same type, so this cannot
    /// pass on a build that pins every reference to one kind.
    #[tokio::test]
    async fn a_reference_names_the_kind_it_points_at_and_the_store_keeps_it() {
        let jojobot = handler();
        writing_as(&jojobot);

        let body = json_of(
            &jojobot
                .declare_type(Parameters(DeclareTypeArgs {
                    name: "stay".to_string(),
                    fields: vec![
                        FieldArgs {
                            key: "venue".to_string(),
                            holds: Some("reference:place".to_string()),
                            folds: None,
                            required: false,
                            one_of: None,
                        },
                        FieldArgs {
                            key: "booked_by".to_string(),
                            holds: Some("reference".to_string()),
                            folds: None,
                            required: false,
                            one_of: None,
                        },
                    ],
                    sid: Some(TEST_SID.to_string()),
                }))
                .await
                .expect("declaring a type of my own is accepted"),
        );
        assert_eq!(
            body["type"]["fields"][0]["holds"], "reference:place",
            "the narrowing the caller wrote comes back on the wire: {body}"
        );
        assert_eq!(
            body["type"]["fields"][1]["holds"], "reference",
            "…and a reference that named no kind still says just that: {body}"
        );

        let held = stored(&jojobot, "stay").await;
        assert_eq!(
            held.fields[0].points_at,
            Some(EntityKind::PLACE),
            "the store kept the kind, so a later reader sees it too: {held:?}",
        );
        assert_eq!(held.fields[1].points_at, None, "{held:?}");
    }

    /// **A closed set crosses the served surface, survives the store, and is
    /// served again.**
    ///
    /// Sent as a caller writes it, read back from the STORE rather than from
    /// the answer to the call that wrote it, and then read off the served
    /// declaration — because a set the verb parsed and the store dropped would
    /// pass any beat that only looked at its own response.
    ///
    /// A plain text key is declared in the same type, so this cannot pass on a
    /// build that narrows every key to something.
    #[tokio::test]
    async fn a_closed_set_crosses_the_surface_and_the_store_keeps_it() {
        let jojobot = handler();
        writing_as(&jojobot);

        let body = json_of(
            &jojobot
                .declare_type(Parameters(DeclareTypeArgs {
                    name: "errand".to_string(),
                    fields: vec![
                        FieldArgs {
                            key: "stage".to_string(),
                            holds: None,
                            folds: None,
                            required: false,
                            one_of: Some(vec![
                                "draft".to_string(),
                                "building".to_string(),
                                "done".to_string(),
                            ]),
                        },
                        FieldArgs {
                            key: "note".to_string(),
                            holds: None,
                            folds: None,
                            required: false,
                            one_of: None,
                        },
                    ],
                    sid: Some(TEST_SID.to_string()),
                }))
                .await
                .expect("a key may be narrowed to a named set"),
        );
        assert_eq!(
            body["type"]["fields"][0]["one_of"],
            serde_json::json!(["draft", "building", "done"]),
            "what a caller reads back is what it would send to say this again: {body}",
        );
        assert_eq!(
            body["type"]["fields"][1]["one_of"],
            serde_json::Value::Null,
            "…and a key nobody narrowed says so rather than carrying an empty set: {body}",
        );

        let held = stored(&jojobot, "errand").await;
        assert_eq!(
            held.fields[0].one_of.as_deref(),
            Some(["draft", "building", "done"].map(String::from).as_slice()),
            "the store keeps the values: {held:?}",
        );
        assert_eq!(held.fields[1].one_of, None, "{held:?}");
    }

    /// **A set nothing can satisfy is refused at the door, and each refusal
    /// says which mistake it was.**
    ///
    /// Paired with the same declaration and a good set, because a case sending
    /// only bad ones passes on a build that refuses every closed set.
    #[tokio::test]
    async fn a_closed_set_that_cannot_be_satisfied_is_refused_at_the_door() {
        let jojobot = handler();
        writing_as(&jojobot);
        let declaring = |values: Vec<&str>| {
            let values: Vec<String> = values.into_iter().map(str::to_string).collect();
            jojobot.declare_type(Parameters(DeclareTypeArgs {
                name: "errand".to_string(),
                fields: vec![FieldArgs {
                    key: "stage".to_string(),
                    holds: None,
                    folds: None,
                    required: false,
                    one_of: Some(values),
                }],
                sid: Some(TEST_SID.to_string()),
            }))
        };

        for (sent, names) in [
            (vec![], "no values"),
            (vec!["draft", "draft"], "twice"),
            (vec!["draft", "half, done"], "comma"),
        ] {
            let refused = json_of(
                &declaring(sent.clone())
                    .await
                    .expect("a refusal is an answer rather than a failure"),
            );
            let said = refused.to_string();
            assert!(
                said.contains(names) && said.contains("stage"),
                "the refusal names the key and which mistake it was: {said}",
            );
        }

        declaring(vec!["draft", "building", "done"])
            .await
            .expect("…and a set of three plain values goes through the same door");
    }

    /// **A caller can send the three narrowing properties together, and a
    /// combination nothing could satisfy is refused at the door.**
    ///
    /// The domain owns the question; what this pins is that the verb builds a
    /// field carrying all three, so the combination is expressible through the
    /// surface at all. Both refusals are paired with the same declaration
    /// minus one property, which is the whole difference between them.
    #[tokio::test]
    async fn a_declaration_no_value_could_satisfy_is_refused_at_the_door() {
        let jojobot = handler();
        writing_as(&jojobot);
        let declaring = |holds: Option<&str>, folds: Option<&str>, values: Vec<&str>| {
            let holds = holds.map(str::to_string);
            let folds = folds.map(str::to_string);
            let one_of = (!values.is_empty())
                .then(|| values.into_iter().map(str::to_string).collect::<Vec<_>>());
            jojobot.declare_type(Parameters(DeclareTypeArgs {
                name: "snacking".to_string(),
                fields: vec![FieldArgs {
                    key: "donuts".to_string(),
                    holds,
                    folds,
                    required: false,
                    one_of,
                }],
                sid: Some(TEST_SID.to_string()),
            }))
        };

        // A set naming nothing the key holds.
        let said = json_of(
            &declaring(Some("number"), None, vec!["cherry", "plain"])
                .await
                .expect("an answer"),
        )
        .to_string();
        assert!(
            said.contains("donuts") && said.contains("number"),
            "the refusal names the key and the half that rules the set out: {said}",
        );

        // A counter narrowed to a set: the total is any number, the set is not.
        let said = json_of(
            &declaring(Some("number"), Some("sum"), vec!["1", "2"])
                .await
                .expect("an answer"),
        )
        .to_string();
        assert!(
            said.contains("donuts") && said.contains("total"),
            "…and a counter is read as a total, which a set cannot hold: {said}",
        );

        // The same declaration minus one property, twice — without these the
        // two above pass on a build that refuses every set and every counter.
        declaring(Some("number"), None, vec!["1", "2"])
            .await
            .expect("a narrowed key that folds newest is ordinary");
        declaring(Some("number"), Some("sum"), vec![])
            .await
            .expect("and a counter holding a number is what a counter is");
    }

    /// **A declaration reads back complete enough to send again.**
    ///
    /// Two beats, and the second is the one that cannot pass by accident. The
    /// first asks whether the answer says which keys are required, with a
    /// required key and an optional one in the same read — a build that marked
    /// every key one way passes half of it. The second takes what came back,
    /// sends exactly that, and asserts the store holds the same declaration:
    /// **a property that stays true as the declaration grows a property**,
    /// where a beat naming today's fields goes stale the day one is added.
    #[tokio::test]
    async fn a_declaration_reads_back_as_what_would_declare_it_again() {
        let jojobot = handler();
        writing_as(&jojobot);

        let body = json_of(
            &jojobot
                .declare_type(Parameters(DeclareTypeArgs {
                    name: "errand".to_string(),
                    fields: vec![
                        FieldArgs {
                            key: "stage".to_string(),
                            holds: None,
                            folds: None,
                            required: true,
                            one_of: Some(vec!["draft".to_string(), "done".to_string()]),
                        },
                        FieldArgs {
                            key: "spent".to_string(),
                            holds: Some("number".to_string()),
                            folds: Some("sum".to_string()),
                            required: false,
                            one_of: None,
                        },
                        FieldArgs {
                            key: "venue".to_string(),
                            holds: Some("reference:place".to_string()),
                            folds: None,
                            required: false,
                            one_of: None,
                        },
                    ],
                    sid: Some(TEST_SID.to_string()),
                }))
                .await
                .expect("declare ok"),
        );
        let served = body["type"]["fields"]
            .as_array()
            .unwrap_or_else(|| panic!("the answer carries the fields: {body}"))
            .clone();
        assert_eq!(
            served[0]["required"],
            serde_json::json!(true),
            "the answer says which keys a thing has to hold: {body}",
        );
        assert_eq!(
            served[1]["required"],
            serde_json::json!(false),
            "…and which it does not, in the same read: {body}",
        );

        // **The round trip.** Every field, read off the answer and sent
        // straight back — nothing here names a property, so this goes on
        // holding when the declaration grows one.
        let sent_back: Vec<FieldArgs> = served
            .iter()
            .map(|f| FieldArgs {
                key: f["key"].as_str().expect("a key").to_string(),
                holds: f["holds"].as_str().map(str::to_string),
                folds: f["folds"].as_str().map(str::to_string),
                required: f["required"].as_bool().expect("required is stated"),
                one_of: f["one_of"].as_array().map(|values| {
                    values
                        .iter()
                        .map(|v| v.as_str().expect("a set value").to_string())
                        .collect()
                }),
            })
            .collect();
        jojobot
            .declare_type(Parameters(DeclareTypeArgs {
                name: "errand".to_string(),
                fields: sent_back,
                sid: Some(TEST_SID.to_string()),
            }))
            .await
            .expect("what came back is a declaration this verb takes");
        assert_eq!(
            stored(&jojobot, "errand").await,
            DeclaredType {
                name: "errand".to_string(),
                origin: Origin::Declared,
                fields: vec![
                    Field::one_of("stage", ["draft", "done"]).needed(),
                    Field::summing("spent"),
                    Field::pointing_at("venue", EntityKind::PLACE),
                ],
            },
            "sending the answer back says the same thing it said",
        );
    }

    /// **A kind nobody has is refused at the door**, and the refusal says what
    /// the token can be.
    ///
    /// Paired with the good token in the same shape, because a case that only
    /// ever sent the bad one passes on a build that refuses every reference.
    #[tokio::test]
    async fn a_reference_to_a_kind_that_does_not_exist_is_refused() {
        let jojobot = handler();
        writing_as(&jojobot);
        let declaring = |holds: &str| {
            let holds = holds.to_string();
            jojobot.declare_type(Parameters(DeclareTypeArgs {
                name: "stay".to_string(),
                fields: vec![FieldArgs {
                    key: "venue".to_string(),
                    holds: Some(holds),
                    folds: None,
                    required: false,
                    one_of: None,
                }],
                sid: Some(TEST_SID.to_string()),
            }))
        };
        let refused = declaring("reference:sofa")
            .await
            .expect_err("a kind that is no kind is refused");
        assert!(
            refused.to_string().contains("reference:sofa"),
            "the refusal quotes what was sent: {refused}"
        );
        declaring("reference:place")
            .await
            .expect("…and the kind that is a kind goes through the same door");
    }

    /// **A caller's own type still replaces on redeclare, through the served
    /// surface**, and what the verb writes is a caller's type.
    ///
    /// The positive the refusal above rests on. Without it that case passes on
    /// a build where `declare_type` refuses every redeclaration, which would
    /// take a capability away from every caller rather than closing the
    /// shipped ones.
    #[tokio::test]
    async fn a_callers_own_type_still_replaces_and_reads_back_as_a_callers() {
        let jojobot = handler();
        writing_as(&jojobot);

        jojobot
            .declare_type(Parameters(declare_args("kiln-firing", &["glaze"])))
            .await
            .expect("declare ok");
        assert_eq!(
            stored(&jojobot, "kiln-firing").await.origin,
            Origin::Declared,
            "the verb declares a caller's type, whatever anybody asked for",
        );

        jojobot
            .declare_type(Parameters(declare_args("kiln-firing", &["cone", "peak"])))
            .await
            .expect("a caller's own type is theirs to redeclare");
        let held = stored(&jojobot, "kiln-firing").await;
        assert_eq!(
            held.fields
                .iter()
                .map(|f| f.key.as_str())
                .collect::<Vec<_>>(),
            vec!["cone", "peak"],
            "the second declaration is what the type is now: {held:?}",
        );
    }
}
