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
    pub key: String,
    /// What the value holds: `text`, `number`, `date`, `boolean`, or
    /// `reference` (another entity's `kind:slug` handle, which is what makes it
    /// walkable). Defaults to `text`, which holds anything — or to `number`
    /// when you declare the key a counter, because a total of anything else is
    /// not a total.
    ///
    /// **A reference can name the kind on the other end** — write
    /// `reference:place` and the key holds a place's handle and no other kind's.
    /// Plain `reference` holds a handle of any kind, so naming one narrows the
    /// key rather than describing it.
    ///
    /// **What you say here is enforced on a thing that already fits this type.**
    /// A value that does not hold it is refused, naming the key and what was
    /// wanted; on a thing that fits no type nothing is refused and the mismatch
    /// comes back flagged on the hit.
    #[serde(default)]
    pub holds: Option<String>,
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
    pub folds: Option<String>,
}

/// Arguments to `declare_type`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DeclareTypeArgs {
    /// What the type is called. Declaring a name that already exists
    /// **replaces** its keys — a type is the set of keys it names now.
    ///
    /// **Unless the software ships that type**, which is refused: a shipped
    /// type is closed and the answer says so. Pick a name of your own.
    pub name: String,
    /// The keys a thing of this type carries. **At least one**: a type is the
    /// keys it names, and a name with nothing under it is one nobody can query
    /// by.
    pub fields: Vec<FieldArgs>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking.
    #[serde(default)]
    pub sid: Option<String>,
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
            fields.push(Field { folds, ..declared });
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
        let body = serde_json::json!({
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
    if let Some((holds, kind)) = token.trim().split_once(':')
        && holds.trim() == ValueType::Reference.as_token()
        && let Err(why) = kinds::resolve(kind.trim())
    {
        return McpError::invalid_params(format!("'{token}': {why}"), None);
    }
    McpError::invalid_params(
        format!(
            "'{token}' is no value type: use text, number, date, boolean or reference — and a \
             reference may name the kind it points at, as 'reference:place', using one of the \
             kinds add_entity takes"
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
                })
                .collect(),
            sid: Some(TEST_SID.to_string()),
        }
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
                        },
                        FieldArgs {
                            key: "booked_by".to_string(),
                            holds: Some("reference".to_string()),
                            folds: None,
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
