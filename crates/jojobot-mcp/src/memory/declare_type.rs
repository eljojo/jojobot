//! `declare_type` — Say what a type's keys are, so a writer knows what to fill.
//!
//! One verb, one file: its arguments, the description a caller reads, and an
//! entrypoint that chains the systems below it.
//!
//! **This verb admits nothing.** A record carrying a type's keys is found by
//! `search` whether or not this was ever called, so declaring is write-time
//! help and never a precondition. That is why there is no verb to undeclare
//! one and no gate anywhere below this: a declaration describes, and the only
//! thing it can be wrong about is itself.

use super::*;

/// One key of a type, and what it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FieldArgs {
    /// The key a record carries, exactly as a record spells it. **The key name
    /// IS the schema**: `expires` and `expiry_date` are two different types and
    /// nothing will point that out, because matching is structural and a key
    /// means whatever the records carrying it mean by it.
    pub key: String,
    /// What the value holds: `text`, `number`, `date`, `boolean`, or
    /// `reference` (another entity's `kind:slug` handle, which is what makes it
    /// walkable). Defaults to `text`, which holds anything.
    ///
    /// **Nothing is rejected for holding the wrong thing.** A value that does
    /// not hold what you say here comes back flagged on the hit, on a record
    /// that is still found.
    #[serde(default)]
    pub holds: Option<String>,
}

/// Arguments to `declare_type`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DeclareTypeArgs {
    /// What the type is called. Declaring a name that already exists
    /// **replaces** its keys — a type is the set of keys it names now.
    pub name: String,
    /// The keys a record of this type carries. **At least one**: a type is the
    /// keys it names, and a name with nothing under it is one nobody can query
    /// by.
    pub fields: Vec<FieldArgs>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Declare a type: a name and the keys a record of it carries.
#[tool_router(router = declare_type_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Declare a type: a name, and the keys a record of it carries. This is \
                       WRITE-TIME HELP and never a gate — it tells a writer which keys to fill \
                       and tells `search` which keys to look for. It admits nothing: a record \
                       carrying these keys is found by search answers_type whether or not \
                       anybody declared it, and a record carrying some of them is found too, \
                       saying which it lacks. So declaring a type never makes a record findable \
                       and never stops one being found, and there is nothing to undeclare. A \
                       type arrives COMPLETE: give at least one key, because a name with nothing \
                       under it is a type nobody can query by. Declaring a name that already \
                       exists REPLACES its keys, whole — a type is the keys it names now, and \
                       one that accumulated every key it ever named would report keys the writer \
                       had already dropped as keys a record lacks. KEYS ARE SCOPED BY THE TYPE \
                       that names them and are registered nowhere: two types may use one key \
                       name and mean their own thing by it. The answer names every type that \
                       exists, by name only, so you can see what is there without asking a \
                       second time."
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
            let holds = match field.holds.as_deref().map(str::trim) {
                None | Some("") => ValueType::Text,
                Some(token) => ValueType::of_token(token).ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "'{token}' is no value type: use text, number, date, boolean or \
                             reference"
                        ),
                        None,
                    )
                })?,
            };
            fields.push(Field::new(&field.key, holds));
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
            // Names only. A caller that has just declared one is choosing what
            // else to reach for, not weighing anybody's keys.
            "types": known.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}
