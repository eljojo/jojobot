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
use jojobot_domain::memory::types::{FoldMove, folds_moved_by};

/// One key of a type, and what it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FieldArgs {
    /// The key a thing carries, exactly as the writes on it spell it. **The key
    /// name IS the schema**: `expires` and `expiry_date` are two different types
    /// and nothing will point that out, because matching is structural and a key
    /// means whatever the things carrying it mean by it.
    pub(crate) key: String,
    /// What the value holds: `text`, `number`, `date`, `date_range`, `period`,
    /// `boolean`, `reference` (another entity's `kind:slug` handle, which is what
    /// makes it walkable) or `expression` (a line in the expression recall's
    /// `where` takes). Defaults to `text`, which holds anything — or to `number`
    /// when you declare the key a counter, because a total of anything else is
    /// not a total.
    ///
    /// **A `date_range` is ONE value, not two keys**: two dates with a slash
    /// between them, `2026-04-18/2026-04-25`. Two keys cannot say they belong
    /// together, so a thing carrying a start and no end reads as a thing with a
    /// key missing rather than as a span nobody finished. A `period` is one day
    /// (`2026-10-19`) or one ISO week (`2026-W43`).
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
    /// **What you say here describes; it does not gate.** A key's `holds`
    /// refuses no write — a value that does not hold what the key declares comes
    /// back flagged on the hit, so a read shows the mistake rather than a write
    /// being turned away. Only the type's `must` refuses.
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
    ///
    /// ⚠️ **And to the whole instance, not to the type that names it.** A key any
    /// type declares a counter adds up on every thing that holds it. A declaration
    /// that would change how a key already held reads back is refused first, with
    /// the count of things it would change.
    #[serde(default)]
    pub(crate) folds: Option<String>,
    /// **Whether a thing has to hold this key to be one of these at all.**
    /// Defaults to false, and leave it there unless the key really is what
    /// makes the thing what it is: the required keys are the floor a WRITE to a
    /// kind may not take away.
    ///
    /// ⚠️ **The reader's strict question is wider than the floor.** `fits_type`
    /// (*which of these ARE one*) keeps only the things holding EVERY key the
    /// type declares, optional ones included; `answers_type` keeps those holding
    /// SOME and names what each lacks. An optional key is never demanded of a
    /// write, and a thing without it is a complete thing, but not one `fits_type`
    /// keeps.
    ///
    /// ⚠️ **This says nothing about what the key HOLDS.** The two are separate
    /// questions, and the useful one is usually "the colour has to be a colour"
    /// rather than "everything must have a colour".
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
    /// a write outside the set is refused with the values named. **On a type you
    /// declare here the set refuses no write** (only the type's `must` does): the
    /// set is what a reader is told about a value that does not match, so a thing
    /// carrying one comes back in an `answers_type` read with that key reported
    /// and the values named.
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
    ///
    /// Six keys label a claim and cannot be declared in a type: read_from,
    /// read_ref, starred, subject, purpose and recorded_by. They stay on the
    /// claim whatever a type says, so a declaration that names one is refused.
    /// Name a key of your own.
    pub(crate) fields: Vec<FieldArgs>,
    /// **A rule every claim written about a thing of this type has to meet**, as
    /// one saved expression in the words a view's `where` uses: `=`, `!=`, `in`,
    /// `contains`, a bare key (the claim carries it, and a blank value does not
    /// count), `!`, `&&`, `||` and parentheses. **It refuses at the write**: a
    /// claim whose fields, the labels it carries and what it sets on the thing
    /// together, do not satisfy the rule is turned away, and the refusal names
    /// the type, the rule and the keys it asks for. A rule that does not read is
    /// refused here, when it is declared. Leave it off for no rule.
    ///
    /// An expression such as `!(purpose = note) || source` says: a claim whose
    /// `purpose` is note has to carry a `source`. It judges what is written, not
    /// whether it is true. An edit of a claim that already broke the rule is not
    /// refused unless it names or changes a key the rule names, so an older claim
    /// stays repairable.
    #[serde(default)]
    pub(crate) must: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. If an answer is lost, send the
    /// same call again with the same `request_id`: if
    /// the first call wrote, you get its receipt back marked `repeat` and nothing
    /// is written a second time, so the type is declared once. It needs a `sid`, and a call that
    /// wrote nothing spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    /// **The token a refusal handed you**, sent back after you read it. A
    /// declaration that would change how a key reads on things that already hold
    /// it is refused first, naming the key and how many things would change; this
    /// token lifts that one refusal and no other. It covers the exact declaration
    /// that was refused: change a field and send it again, and the refusal comes
    /// back with a new token. Send it with `override_reason`.
    #[serde(default)]
    pub(crate) override_token: Option<String>,
    /// **Why you insist**, in words, sent beside `override_token`. A token with no
    /// reason lifts nothing: the reason is what makes the override a deliberate act.
    /// The receipt answers it back as `override_reason` and lists each key whose fold
    /// moved as `fold_moved` (`key`, `things`, `now`); the server log keeps it with
    /// the type, the keys whose fold moved and how many things hold each.
    #[serde(default)]
    pub(crate) override_reason: Option<String>,
}

/// **A deliberate, visible act**: this verb's own refusal promises an override
/// because this impl says its `Args` really carries one. See [`AcceptsOverride`].
impl AcceptsOverride for DeclareTypeArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
    fn override_reason(&self) -> Option<&str> {
        self.override_reason.as_deref()
    }
}

/// Declare a type: a name and the keys a thing of it carries.
#[tool_router(router = declare_type_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Declare a type: a name, and the keys a THING of it carries. Its \
                       `fields` are KEY DEFINITIONS — each a key with optional `holds`, \
                       `folds`, `required` and `one_of` — not the key/value bag capture and \
                       update_fact take as `fields`, nor the key filters recall takes. Keys \
                       are WRITE-TIME HELP, never a gate (`must` aside) — they tell writers \
                       which keys to fill and `search` which to look for. THE UNIT IS THE THING: \
                       a thing answers a type over every write on it, folded, so a thing \
                       described over two sittings answers a type neither sitting answers \
                       alone. It admits nothing: a thing carrying these keys is found by \
                       search answers_type whether or not anybody declared it, and a thing \
                       carrying some of them is found too, saying which it lacks. So \
                       declaring a type never makes a thing findable and never stops one \
                       being found, and there is nothing to undeclare. A type arrives \
                       COMPLETE: give at least one key, because a name with nothing under it \
                       is a type nobody can query by. Declaring a name that already exists \
                       REPLACES its keys, whole — a type is the keys it names now, and one \
                       that accumulated every key it ever named would report keys the writer \
                       had already dropped as keys a thing lacks. KEYS ARE REGISTERED \
                       NOWHERE; what a key holds is the type's, HOW IT FOLDS IS THE INSTANCE'S: \
                       a counter in any type adds up on every thing holding the key, so a \
                       declaration that changes how a held key reads is refused first, with \
                       the count (override_token and override_reason lift it). SOME TYPES SHIP \
                       WITH THE SOFTWARE \
                       and those are CLOSED: declaring over one comes back blocked, because a \
                       caller cannot extend, shrink or replace a type the software owns — \
                       declare a name of your own instead. Every type you declare is yours. \
                       The answer names every type that exists, by name and origin, so you can see \
                       what is there without asking again. The receipt's `changed` \
                       lists changes in the type, or is `nothing` or `not_read`. A `request_id` makes a resend safe: a repeat returns the first receipt and writes nothing."
    )]
    pub(crate) async fn declare_type(
        &self,
        Parameters(args): Parameters<DeclareTypeArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the write runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        if let Err(refused) = self.attributable_for_write(args.sid.as_deref()).await {
            return Ok(refused);
        }
        // Good by the check above, so a carried handle resolves; a call that
        // carried none is declared by no run.
        let caller = self.caller(args.sid.as_deref()).ok().flatten();
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

        // **A declaration that would change how a key reads on things that already
        // hold it is a norm** (rule 472): it refuses first, and lets through a call
        // that carries the refusal's token and a written reason. **A read that
        // fails refuses the call as an outage**: an empty list of moves would let
        // a fold change land that the guard exists to stop.
        let incoming = DeclaredType::new(&args.name, fields.clone())
            .with_must(args.must.as_deref().unwrap_or(""));
        // **Validated before the fold guard**, so a declaration the domain would
        // refuse is refused as invalid and is never handed an override token.
        if let Err(invalid) = jojobot_domain::memory::types::validate_type(&incoming) {
            return memory_declined("declare_type", invalid);
        }
        let held = self.memory.declared_types().await.map_err(memory_error)?;
        let moved = self
            .fold_moves_on_held_keys(&incoming, &held)
            .await
            .map_err(memory_error)?;
        let mut overridden: Option<String> = None;
        if !moved.is_empty() {
            let token = fold_move_token(&incoming, &moved);
            let lifted = args.override_token().is_some_and(|sent| sent == token);
            match kept_override_reason(args.override_token(), args.override_reason()) {
                Some(reason) if lifted => overridden = Some(reason.to_string()),
                _ => {
                    return Ok(refusal_for_fold_moves(
                        &incoming,
                        &moved,
                        &token,
                        &TokenSlot::from(&args),
                    ));
                }
            }
        }

        // **The declaration as it stood, read before the write replaces it.**
        let before = self
            .memory
            .declared_types()
            .await
            .map(|types| types.into_iter().find(|t| t.name == args.name.trim()));
        let declared = match jojobot_domain::request::armed(self.memory.declare_type(
            DeclaredType::new(&args.name, fields).with_must(args.must.as_deref().unwrap_or("")),
            caller.as_ref().map(|caller| caller.sid.as_str()),
        ))
        .await
        {
            Ok(declared) => declared,
            Err(e) => return memory_declined("declare_type", e),
        };
        // **The override is logged before anything else is read**, so a read that
        // fails after the write still leaves the one record of it that outlives the
        // receipt (rule 545). A held value is never logged: only the type, the keys
        // whose fold moved and how many things hold each, and the reason as one
        // capped line.
        if let Some(reason) = &overridden {
            let keys = moved
                .iter()
                .map(|(change, things)| format!("{}={things}", change.key))
                .collect::<Vec<_>>()
                .join(",");
            tracing::warn!(
                r#type = %declared.name,
                %keys,
                reason = %loggable(reason),
                "a type declaration moved how held keys fold"
            );
        }
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
        if let Some(reason) = &overridden {
            // **The override and its reason are kept twice** (rule 545): in the
            // receipt, and in the server log written above.
            body["override_reason"] = reason.clone().into();
        }
        if !moved.is_empty() {
            body["fold_moved"] = moved
                .iter()
                .map(|(change, things)| {
                    serde_json::json!({
                        "key": change.key,
                        "things": things,
                        "now": change.to.as_token(),
                    })
                })
                .collect();
        }
        crate::changed::note_changed(
            &mut body,
            match &before {
                Ok(before) => crate::changed::Changed::of_type(before.as_ref(), &declared),
                Err(_) => crate::changed::Changed::not_read(),
            },
        );
        // **A declaration is a write, and a write renews the caller's claim.**
        // It has no beat of its own to do it.
        self.renew_role_claims_of(args.sid.as_deref()).await;
        json_result(&body)
    }
}

impl Jojobot {
    /// **The keys `incoming` would fold differently, with how many things hold
    /// each**, leaving out any key nothing holds (rule 498: whether a fold moves is
    /// [`folds_moved_by`]'s answer; this adds only who is affected). A thing holds
    /// a key when its fields carry it; a thing folded into another is read through
    /// the survivor, and an archived thing still reads back what it holds.
    async fn fold_moves_on_held_keys(
        &self,
        incoming: &DeclaredType,
        held: &[DeclaredType],
    ) -> Result<Vec<(FoldMove, usize)>, MemoryError> {
        let moves = folds_moved_by(incoming, held);
        if moves.is_empty() {
            return Ok(Vec::new());
        }
        let things = self.memory.list_entities(None).await?;
        let mut counts = vec![0usize; moves.len()];
        for thing in things.iter().filter(|thing| thing.merged_into.is_none()) {
            let fields = self.memory.fields(&thing.id).await?;
            for (at, change) in moves.iter().enumerate() {
                if fields.keys().any(|key| key.trim() == change.key) {
                    counts[at] += 1;
                }
            }
        }
        Ok(moves
            .into_iter()
            .zip(counts)
            .filter(|(_, things)| *things > 0)
            .collect())
    }
}

/// **The most characters of an override reason the server log keeps.**
const LOGGED_REASON_CHARS: usize = 300;

/// **A caller's reason as one log line**: cut to [`LOGGED_REASON_CHARS`] and with
/// every newline and control character written out, so a reason cannot carry a
/// second record into the log.
fn loggable(reason: &str) -> String {
    reason
        .chars()
        .take(LOGGED_REASON_CHARS)
        .collect::<String>()
        .escape_debug()
        .to_string()
}

/// **The token that lifts the refusal of one declaration's fold moves, and no
/// other**: a function of the type's name, of each key with how it would fold and
/// of the keys the declaration names.
fn fold_move_token(incoming: &DeclaredType, moved: &[(FoldMove, usize)]) -> String {
    let what = moved
        .iter()
        .map(|(change, _)| {
            format!(
                "{}:{}>{}",
                change.key,
                change.from.as_token(),
                change.to.as_token()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // **The declaration's keys are part of what the token answers for**, so a token
    // from the refusal of one declaration lifts no other of the same name.
    let declared = format!("{:?}", incoming.fields);
    jojobot_domain::memory::guard::override_token(
        &EntityId(format!("type:{}#{what}#{declared}", incoming.name)),
        &[],
    )
}

/// **The refusal**: each key, how it reads now and would read, how many things
/// would change, and what lifts it.
fn refusal_for_fold_moves(
    incoming: &DeclaredType,
    moved: &[(FoldMove, usize)],
    token: &str,
    slot: &TokenSlot,
) -> CallToolResult {
    let words = |fold: Fold| match fold {
        Fold::Sum => "a total of its writes",
        _ => "its newest write",
    };
    let keys = moved
        .iter()
        .map(|(change, things)| {
            format!(
                "`{}` reads {} now and would read {}: {things} {} would change",
                change.key,
                words(change.from),
                words(change.to),
                if *things == 1 { "thing" } else { "things" },
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let advice = if slot.reason_owed() {
        format!(
            "Nothing was written. The override_token was not enough: lifting this takes a \
             reason. Re-call declare_type with override_token: \"{token}\" and \
             override_reason: \"<why these things should read differently>\"."
        )
    } else {
        format!(
            "Nothing was written. A key folds one way for the whole instance, whichever type \
             names it, so declaring '{}' would change what things that already hold a key read \
             back: {keys}. If that is what you mean, re-call declare_type with override_token: \
             \"{token}\" and override_reason: \"<why these things should read differently>\". \
             That token belongs to THIS refusal and lifts no other.",
            incoming.name,
        )
    };
    blocked_body(&EntityId(format!("type:{}", incoming.name)), &[], advice)
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
            "'{token}' is no value type: use text, number, date, date_range, period, boolean, \
             reference or expression — and a reference may name the kind it points at, as 'reference:place', \
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
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::types::Origin;
    use std::sync::Arc;

    fn declare_args(name: &str, keys: &[&str]) -> DeclareTypeArgs {
        DeclareTypeArgs {
            must: None,
            request_id: None,
            override_token: None,
            override_reason: None,
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

    /// **The store is told which run sent a declaration.** A call carrying a
    /// session is declared by that run's public name, never by the key it sent;
    /// a call carrying none is declared by no run.
    #[tokio::test]
    async fn a_declaration_reaches_the_store_with_the_run_that_sent_it() {
        let memory = Arc::new(InMemoryMemory::seeded());
        let jojobot = handler_over(memory.clone());
        writing_as(&jojobot);

        jojobot
            .declare_type(Parameters(declare_args("recipe", &["flavor"])))
            .await
            .expect("declare ok");
        let mut anonymous = declare_args("recipe", &["flavor", "glaze"]);
        anonymous.sid = None;
        jojobot
            .declare_type(Parameters(anonymous))
            .await
            .expect("declare ok");

        assert_eq!(
            memory.type_write_sessions("recipe"),
            vec![Some(TEST_RUN.to_string()), None]
        );
    }

    /// **A declaration names what it changed in the type** (card 1333). A new
    /// type is `created`; the same declaration again changes nothing; a
    /// redeclaration names each key added, dropped or changed, with a short
    /// token for what the key held and holds.
    ///
    /// Paired in one case so the `nothing` depends on the `created` before it,
    /// and the key entries on the declaration they replace.
    #[tokio::test]
    async fn a_declaration_names_what_it_changed_in_the_type() {
        let jojobot = handler();
        let declare = |args: DeclareTypeArgs| {
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .declare_type(Parameters(args))
                        .await
                        .expect("declare_type answers"),
                )
            }
        };

        let first = declare(declare_args("rota", &["starts", "ends"])).await;
        let changed = first["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {first}"));
        assert_eq!(changed.len(), 1, "{first}");
        assert_eq!(changed[0]["on"], "rota", "{first}");
        assert_eq!(changed[0]["what"], "created", "{first}");

        let again = declare(declare_args("rota", &["starts", "ends"])).await;
        assert_eq!(again["changed"], "nothing", "{again}");

        let mut redeclared = declare_args("rota", &["ends", "weight"]);
        redeclared.fields[0].holds = Some("date".into());
        redeclared.fields[1].holds = Some("number".into());
        redeclared.fields[1].folds = Some("sum".into());
        redeclared.fields[1].required = true;
        let second = declare(redeclared).await;
        let changed = second["changed"]
            .as_array()
            .unwrap_or_else(|| panic!("a list of changes: {second}"));
        assert_eq!(changed.len(), 3, "{second}");
        for entry in changed {
            assert_eq!(entry["on"], "rota", "{second}");
            assert_eq!(entry["what"], "key", "{second}");
        }
        assert_eq!(changed[0]["key"], "ends", "{second}");
        assert_eq!(changed[0]["from"], "text, newest", "{second}");
        assert_eq!(changed[0]["to"], "date, newest", "{second}");
        assert_eq!(changed[1]["key"], "starts", "{second}");
        assert_eq!(changed[1]["from"], "text, newest", "{second}");
        assert!(changed[1].get("to").is_none(), "a dropped key: {second}");
        assert_eq!(changed[2]["key"], "weight", "{second}");
        assert!(changed[2].get("from").is_none(), "an added key: {second}");
        assert_eq!(changed[2]["to"], "number, sum, required", "{second}");
    }

    /// A thing that wrote `visits` 3 and then 5, so it reads 5 until a type counts.
    async fn kettle_with_visits(jojobot: &Jojobot) {
        use crate::memory::capture::CaptureArgs;
        use crate::memory::testing::{capture_args, capture_ok, ensure};
        ensure(jojobot, "thing:kettle").await;
        for visits in ["3", "5"] {
            capture_ok(
                jojobot,
                CaptureArgs {
                    sets: Some([("visits".to_string(), visits.to_string())].into()),
                    ..capture_args("thing:kettle", &format!("visited, total {visits}"))
                },
            )
            .await;
        }
    }

    async fn kettle_visits(jojobot: &Jojobot) -> Option<String> {
        jojobot
            .memory
            .fields(&EntityId("thing:kettle".into()))
            .await
            .expect("the kettle's fields read")
            .get("visits")
            .cloned()
    }

    /// A declaration of `name` whose one key `key` is a counter (or, with `sum`
    /// false, a plain number).
    fn naming_visits(name: &str, key: &str, sum: bool) -> DeclareTypeArgs {
        let mut args = declare_args(name, &[key]);
        args.fields[0].holds = Some("number".into());
        args.fields[0].folds = Some(if sum { "sum" } else { "newest" }.into());
        args
    }

    /// The token a refusal names, as a caller has to read it out of the advice.
    fn token_in(body: &serde_json::Value) -> String {
        let advice = body["how_to_proceed"].as_str().expect("advice");
        let after = advice
            .split("override_token: \"")
            .nth(1)
            .unwrap_or_else(|| panic!("the advice names the token: {advice}"));
        after.split('"').next().expect("a token").to_string()
    }

    async fn declared(jojobot: &Jojobot, args: DeclareTypeArgs) -> CallToolResult {
        jojobot
            .declare_type(Parameters(args))
            .await
            .expect("declare_type answers")
    }

    async fn type_names(jojobot: &Jojobot) -> Vec<String> {
        jojobot
            .memory
            .declared_types()
            .await
            .expect("the types read")
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    /// 🚨 **A declaration never silently changes what an existing thing reads
    /// back.** A thing wrote `visits` 3 then 5 and reads 5. A type that counts
    /// `visits` would make it read 8, for every thing holding the key, so the
    /// declaration is refused first, naming the key and how many things would
    /// change, and writes nothing. The same call with the token and a written
    /// reason lands, and the receipt says how many things now read a total.
    #[tokio::test]
    async fn a_declaration_that_would_make_a_held_key_a_total_is_refused_until_overridden() {
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        assert_eq!(kettle_visits(&jojobot).await.as_deref(), Some("5"));

        // ── refused, naming the key and the count, writing nothing ──────────
        let refused = blocked(&declared(&jojobot, naming_visits("tally", "visits", true)).await);
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(advice.contains("`visits`"), "names the key: {advice}");
        assert!(
            advice.contains("1 thing would change"),
            "names how many: {advice}"
        );
        assert!(
            !type_names(&jojobot).await.contains(&"tally".to_string()),
            "a refused declaration writes nothing"
        );
        assert_eq!(
            kettle_visits(&jojobot).await.as_deref(),
            Some("5"),
            "the kettle reads as it did"
        );

        // ── a token alone, or a reason beside a wrong token, lifts nothing ──
        let token = token_in(&refused);
        let mut token_only = naming_visits("tally", "visits", true);
        token_only.override_token = Some(token.clone());
        blocked(&declared(&jojobot, token_only).await);
        let mut wrong = naming_visits("tally", "visits", true);
        wrong.override_token = Some("not-the-token".into());
        wrong.override_reason = Some("the kettle really is a counter".into());
        blocked(&declared(&jojobot, wrong).await);
        assert_eq!(kettle_visits(&jojobot).await.as_deref(), Some("5"));

        // ── the token and a written reason land it, and the receipt counts ──
        let mut lifted = naming_visits("tally", "visits", true);
        lifted.override_token = Some(token);
        lifted.override_reason = Some("the kettle really is a counter".into());
        let landed = json_of(&declared(&jojobot, lifted).await);
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert_eq!(landed["fold_moved"][0]["key"], "visits", "{landed}");
        assert_eq!(landed["fold_moved"][0]["things"], 1, "{landed}");
        assert_eq!(landed["fold_moved"][0]["now"], "sum", "{landed}");
        assert_eq!(kettle_visits(&jojobot).await.as_deref(), Some("8"));
    }

    /// **The other direction is a change too.** A type that stops counting a key
    /// turns a total back into the newest write, for every thing holding it.
    #[tokio::test]
    async fn a_declaration_that_would_stop_a_held_key_being_a_total_is_refused_too() {
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let first = blocked(&declared(&jojobot, naming_visits("tally", "visits", true)).await);
        let mut counting = naming_visits("tally", "visits", true);
        counting.override_token = Some(token_in(&first));
        counting.override_reason = Some("the kettle really is a counter".into());
        declared(&jojobot, counting).await;
        assert_eq!(kettle_visits(&jojobot).await.as_deref(), Some("8"));

        let refused = blocked(&declared(&jojobot, naming_visits("tally", "visits", false)).await);
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(advice.contains("`visits`"), "{advice}");
        assert!(advice.contains("1 thing would change"), "{advice}");
        assert_eq!(
            kettle_visits(&jojobot).await.as_deref(),
            Some("8"),
            "a refused call changes nothing the kettle reads"
        );

        let mut stopping = naming_visits("tally", "visits", false);
        stopping.override_token = Some(token_in(&refused));
        stopping.override_reason = Some("it was never a counter".into());
        let landed = json_of(&declared(&jojobot, stopping).await);
        assert_eq!(landed["fold_moved"][0]["now"], "newest", "{landed}");
        assert_eq!(kettle_visits(&jojobot).await.as_deref(), Some("5"));
    }

    /// **A declaration that changes no held key's fold is never refused**: a
    /// counter on a key nothing holds, the same declaration again, and a type
    /// that counts a key another type already counts.
    #[tokio::test]
    async fn a_declaration_that_changes_no_held_keys_fold_is_never_refused() {
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        for (name, key) in [("laps", "laps"), ("laps", "laps")] {
            let landed = json_of(&declared(&jojobot, naming_visits(name, key, true)).await);
            assert_ne!(landed["status"], "blocked", "{landed}");
            assert!(landed.get("fold_moved").is_none(), "{landed}");
        }
        assert_eq!(
            kettle_visits(&jojobot).await.as_deref(),
            Some("5"),
            "the kettle is untouched by declarations about other keys"
        );
    }

    /// **A read the fold-move guard needs fails, and the declaration is refused
    /// as the outage it is**, never let through as if nothing held the key. The
    /// paired positive: a handler over the same store with every read working
    /// still asks first, so the guard was live and the failed call wrote nothing.
    async fn a_failed_read_refuses_the_declaration(down: crate::memory::testing::Down) {
        let (healthy, watching) = crate::memory::testing::healthy_and_down(down);
        writing_as(&healthy);
        writing_as(&watching);
        kettle_with_visits(&healthy).await;

        let refused = watching
            .declare_type(Parameters(naming_visits("tally", "visits", true)))
            .await
            .expect_err("an unreadable store refuses the declaration");
        assert_eq!(refused.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
        assert_eq!(
            refused
                .data
                .as_ref()
                .map(|data| &data[crate::answer::FixBy::KEY]),
            Some(&serde_json::json!("retry")),
            "{refused:?}"
        );
        assert!(
            !type_names(&healthy).await.contains(&"tally".to_string()),
            "a refused declaration writes nothing"
        );
        assert_eq!(kettle_visits(&healthy).await.as_deref(), Some("5"));

        let asked = blocked(&declared(&healthy, naming_visits("tally", "visits", true)).await);
        let advice = asked["how_to_proceed"].as_str().expect("advice");
        assert!(advice.contains("1 thing would change"), "{advice}");
    }

    #[tokio::test]
    async fn a_declaration_is_refused_when_the_declared_types_cannot_be_read() {
        a_failed_read_refuses_the_declaration(crate::memory::testing::Down::TypeRoster).await;
    }

    #[tokio::test]
    async fn a_declaration_is_refused_when_the_things_cannot_be_listed() {
        a_failed_read_refuses_the_declaration(crate::memory::testing::Down::EntityIndex).await;
    }

    #[tokio::test]
    async fn a_declaration_is_refused_when_a_things_fields_cannot_be_read() {
        a_failed_read_refuses_the_declaration(crate::memory::testing::Down::ThingFields).await;
    }

    /// **What the process's one log sink has kept**, installed on first use. It
    /// keeps warnings, so the override is logged at that level, and every case
    /// looks for its own type name in the shared text.
    fn logged() -> String {
        jojobot_adapters::testing::install_store_log_sink();
        jojobot_adapters::testing::store_log()
    }

    /// **An overridden declaration keeps its reason twice** (rule 545): in the
    /// receipt and in the server log, which names the type, the key and how many
    /// things hold it, and never what they hold.
    #[tokio::test]
    async fn an_overridden_declaration_keeps_its_reason_in_the_receipt_and_the_log() {
        let _ = logged();
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let refused = blocked(&declared(&jojobot, naming_visits("tallyho", "visits", true)).await);
        assert!(
            !logged().contains("tallyho"),
            "a refused declaration moves nothing, so it logs no override"
        );
        let mut lifted = naming_visits("tallyho", "visits", true);
        lifted.override_token = Some(token_in(&refused));
        lifted.override_reason = Some("every kettle visit adds up".into());
        let landed = json_of(&declared(&jojobot, lifted).await);
        assert_eq!(
            landed["override_reason"], "every kettle visit adds up",
            "{landed}"
        );
        let line = logged()
            .lines()
            .find(|line| line.contains("tallyho"))
            .expect("the override is in the log")
            .to_string();
        for part in ["WARN", "visits=1", "every kettle visit adds up"] {
            assert!(line.contains(part), "{part} is missing from {line}");
        }
        assert!(
            !line.contains("kettle visited"),
            "no stored value is logged: {line}"
        );
    }

    /// **An invalid declaration over a held key is refused as invalid, with no token
    /// offered.** The fold guard used to run first, hand out an override token and
    /// then see the declaration refused anyway. The declaration names `visits`
    /// twice, which no write accepts.
    #[tokio::test]
    async fn an_invalid_declaration_over_a_held_key_is_refused_as_invalid_with_no_token() {
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let mut args = declare_args("tallyfour", &["visits", "visits"]);
        for field in &mut args.fields {
            field.holds = Some("number".into());
            field.folds = Some("sum".into());
        }
        let refused = blocked(&declared(&jojobot, args).await);
        let advice = refused["how_to_proceed"].as_str().expect("advice");
        assert!(
            !advice.contains("override_token"),
            "an invalid declaration is offered no token: {advice}"
        );
        assert!(
            !type_names(&jojobot)
                .await
                .contains(&"tallyfour".to_string()),
            "nothing is stored"
        );
        // The paired positive: the same key once is the fold refusal that offers one.
        let valid = blocked(&declared(&jojobot, naming_visits("tallyfour", "visits", true)).await);
        assert!(
            valid["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains("override_token")),
            "a valid declaration over the held key is offered its token: {valid}"
        );
    }

    /// **A reason with a newline logs as one line, and a long one is cut.** The
    /// reason is a caller's text on its way into the server log, where a second
    /// line would pass for a record of its own.
    #[tokio::test]
    async fn a_reason_with_a_newline_logs_as_one_capped_line() {
        let _ = logged();
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let refused =
            blocked(&declared(&jojobot, naming_visits("tallyfive", "visits", true)).await);
        let mut lifted = naming_visits("tallyfive", "visits", true);
        lifted.override_token = Some(token_in(&refused));
        lifted.override_reason = Some(format!(
            "first line\nWARN forgedmark a second record\n{}",
            "x".repeat(5000)
        ));
        let landed = json_of(&declared(&jojobot, lifted).await);
        assert_ne!(landed["status"], "blocked", "{landed}");
        let log = logged();
        let lines: Vec<&str> = log
            .lines()
            .filter(|line| line.contains("forgedmark"))
            .collect();
        assert_eq!(
            lines.len(),
            1,
            "the reason is one line of the log: {lines:?}"
        );
        assert!(
            lines[0].contains("tallyfive") && lines[0].len() < 1500,
            "the line is the override's own and is capped: {}",
            lines[0].len()
        );
    }

    /// **A token from the refusal of one set of keys does not lift a declaration of
    /// another.** The type has the same name and its `visits` moves the same fold,
    /// but it also names `laps`, so it is not the declaration that was refused.
    #[tokio::test]
    async fn a_token_does_not_lift_a_declaration_with_different_keys() {
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let refused = blocked(&declared(&jojobot, naming_visits("tallysix", "visits", true)).await);
        let mut other = declare_args("tallysix", &["visits", "laps"]);
        other.fields[0].holds = Some("number".into());
        other.fields[0].folds = Some("sum".into());
        other.override_token = Some(token_in(&refused));
        other.override_reason = Some("it is the same type".into());
        let answer = blocked(&declared(&jojobot, other).await);
        assert!(
            !type_names(&jojobot).await.contains(&"tallysix".to_string()),
            "a token for other keys lifted nothing: {answer}"
        );
        // The paired positive: the token lifts the declaration it was issued for.
        let mut same = naming_visits("tallysix", "visits", true);
        same.override_token = Some(token_in(&refused));
        same.override_reason = Some("it is the same type".into());
        let landed = json_of(&declared(&jojobot, same).await);
        assert_ne!(landed["status"], "blocked", "{landed}");
    }

    /// **The override is logged even when the read after the write fails.** The
    /// type lands, the read that builds the receipt fails, and the log line is the
    /// one record of the override that is left.
    #[tokio::test]
    async fn the_override_is_logged_even_when_the_read_after_the_write_fails() {
        let _ = logged();
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (healthy, watching) = crate::memory::testing::healthy_and_down(
            crate::memory::testing::Down::TypeRosterAfterDeclare(flag),
        );
        writing_as(&healthy);
        writing_as(&watching);
        kettle_with_visits(&healthy).await;
        let refused =
            blocked(&declared(&watching, naming_visits("tallyseven", "visits", true)).await);
        let mut lifted = naming_visits("tallyseven", "visits", true);
        lifted.override_token = Some(token_in(&refused));
        lifted.override_reason = Some("the kettle counts visits".into());
        let _ = watching.declare_type(Parameters(lifted)).await;
        assert!(
            type_names(&healthy)
                .await
                .contains(&"tallyseven".to_string()),
            "the type landed"
        );
        let log = logged();
        let line = log
            .lines()
            .find(|line| line.contains("tallyseven"))
            .unwrap_or_else(|| panic!("the override is not in the log: {log}"));
        assert!(line.contains("the kettle counts visits"), "{line}");
    }

    /// **A declaration that moves no held fold logs nothing**, and its receipt
    /// carries no reason. The positive is the case above.
    #[tokio::test]
    async fn a_declaration_that_moves_no_held_fold_logs_and_carries_nothing() {
        let _ = logged();
        let jojobot = handler();
        writing_as(&jojobot);
        kettle_with_visits(&jojobot).await;
        let landed = json_of(&declared(&jojobot, naming_visits("quietly", "laps", true)).await);
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert!(landed.get("override_reason").is_none(), "{landed}");
        assert!(
            !logged().contains("quietly"),
            "nothing moved, so nothing is logged"
        );
    }

    /// **A caller cannot declare a key that describes its record** (rule 360).
    /// The word is a fold the build attaches to its own keys, and a type that
    /// could carry it would stop a key folding for every other bot. Refused as
    /// an answer with a way forward, and nothing is stored under the name.
    #[tokio::test]
    async fn a_caller_cannot_declare_a_key_that_describes_its_record() {
        let jojobot = handler();
        let mut args = declare_args("my-labels", &["my_label"]);
        args.fields[0].folds = Some("describes".into());
        let refused = jojobot
            .declare_type(Parameters(args))
            .await
            .expect("a caller mistake is an answer, not a protocol failure");
        let body = blocked(&refused);
        assert_eq!(body["wrote"], false, "{body}");
        // And a plain declaration of the same key lands, so the refusal is about
        // the fold and not the type.
        let mut plain = declare_args("my-labels", &["my_label"]);
        plain.fields[0].folds = Some("newest".into());
        let landed = json_of(
            &jojobot
                .declare_type(Parameters(plain))
                .await
                .expect("a plain declaration is taken"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
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
            must: None,
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
            .declare_type(shipped.clone(), None)
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

    /// 🚨 **A shipped write that lands on a caller's own declaration is not
    /// silent.**
    ///
    /// The failing case this proves: before this, `declare_type` refused a
    /// redeclaration by name alone. The caller's own keys — real ones, held
    /// until the software's write took the name over — were gone with no
    /// trace anywhere the served surface reaches. The refusal now names
    /// them.
    ///
    /// `jojobot.memory.declare_type` stands in for the boot-time seed here,
    /// the same way [`declaring_over_a_shipped_type_is_refused_and_writes_nothing`]
    /// stands in for it: the seed is not a tool a caller can invoke, so
    /// nothing on the served surface can trigger a shipped write directly.
    /// What IS on the served surface, and what this asserts on, is the
    /// refusal a caller meets afterwards.
    #[tokio::test]
    async fn redeclaring_a_name_the_build_displaced_names_what_it_held() {
        let jojobot = handler();
        writing_as(&jojobot);
        jojobot
            .declare_type(Parameters(declare_args("roster", &["shift_lead"])))
            .await
            .expect("the caller's own declaration lands");

        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
        jojobot
            .memory
            .declare_type(
                DeclaredType::shipped("roster", vec![Field::new("starts", ValueType::Date)]),
                None,
            )
            .await
            .expect("the software's own write replaces a caller's");

        let result = jojobot
            .declare_type(Parameters(declare_args("roster", &["starts"])))
            .await
            .expect("a refusal is an answer, not a protocol failure");

        let body = blocked(&result);
        let advice = body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal carries its way forward: {body}"));
        assert!(
            advice.contains("shift_lead"),
            "names the key the caller's own declaration held: {advice}"
        );
        assert!(
            advice.contains(&today.to_string()),
            "…and the day the software's write took the name over: {advice}"
        );
    }

    /// **The positive the case above rests on**: a shipped type declared for
    /// the first time — never a caller's — names nothing displaced. Without
    /// this, a refusal that always invented something to say would pass the
    /// case above for the wrong reason.
    #[tokio::test]
    async fn redeclaring_a_name_that_was_always_shipped_names_nothing_displaced() {
        let jojobot = handler();
        writing_as(&jojobot);
        jojobot
            .memory
            .declare_type(
                DeclaredType::shipped(
                    "always-shipped",
                    vec![Field::new("starts", ValueType::Date)],
                ),
                None,
            )
            .await
            .expect("the software declares its own types");

        let result = jojobot
            .declare_type(Parameters(declare_args("always-shipped", &["starts"])))
            .await
            .expect("a refusal is an answer, not a protocol failure");

        let body = blocked(&result);
        let advice = body["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal carries its way forward: {body}"));
        assert!(
            !advice.contains("Before this"),
            "nothing was ever a caller's under this name, so nothing is named as displaced: \
             {advice}"
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
            .declare_type(
                DeclaredType::shipped("rota", vec![Field::new("starts", ValueType::Date)]),
                None,
            )
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
                    must: None,
                    request_id: None,
                    override_token: None,
                    override_reason: None,
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
                    must: None,
                    request_id: None,
                    override_token: None,
                    override_reason: None,
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
                must: None,
                request_id: None,
                override_token: None,
                override_reason: None,
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
                must: None,
                request_id: None,
                override_token: None,
                override_reason: None,
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
                    must: None,
                    request_id: None,
                    override_token: None,
                    override_reason: None,
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
                must: None,
                request_id: None,
                override_token: None,
                override_reason: None,
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
                must: None,
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
                must: None,
                request_id: None,
                override_token: None,
                override_reason: None,
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
