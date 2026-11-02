//! **The bot request: a manager asks for a bot, and only the app bound to the
//! request reports on it** (rules 525, 579, 731).
//!
//! A request is a memory record of the `bot-request` kind. Its spec says what
//! should run; its status says what runs. Who may write each is a row of the
//! one permission check in [`super::GUARDED_KEYS`], never a route.

/// **The kind of a request**, as it spells in a handle. A handle is stored and
/// nothing outside this process declares the spelling, so a case pins the
/// literal.
pub const KIND: &str = "bot-request";

/// The bot the request is for. Fixed once the request exists.
pub const TARGET_BOT: &str = "target_bot";
/// Whether the bot should run: `running` or `stopped`.
pub const DESIRED: &str = "desired";
/// The machine the bot should run on.
pub const MACHINE: &str = "machine";
/// The model the bot should run with.
pub const MODEL: &str = "model";
/// The name of the one app that reports on the request.
pub const BOUND_APP: &str = "bound_app";
/// **The generation of the spec**, worked out by jojobot and never sent. A write
/// of any spec key raises it.
pub const GENERATION: &str = "generation";

/// What the bound app saw running.
pub const OBSERVED_STATE: &str = "observed_state";
/// The generation the bound app last acted on.
pub const OBSERVED_GENERATION: &str = "observed_generation";
/// The app's own reference to what it started.
pub const RUNTIME_REF: &str = "runtime_ref";
/// How the app's last attempt ended.
pub const LAST_RESULT: &str = "last_result";

/// The keys a manager above the target writes.
pub const SPEC_KEYS: [&str; 5] = [TARGET_BOT, DESIRED, MACHINE, MODEL, BOUND_APP];

/// The keys only the bound app writes.
pub const STATUS_KEYS: [&str; 4] = [
    OBSERVED_STATE,
    OBSERVED_GENERATION,
    RUNTIME_REF,
    LAST_RESULT,
];

/// **Whether a subject is a bot request**, read off its handle. The handle is
/// what the stores hand the check, and a case without the loaded kind set can
/// build one.
pub fn is_request(subject: &super::EntityId) -> bool {
    subject
        .as_str()
        .strip_prefix(KIND)
        .is_some_and(|rest| rest.starts_with(':'))
}

/// **Whether `caller` may write the spec key `key` of `subject`.** Anybody may
/// on a subject that is no request. On a request the target is read from the
/// chart: the bots above the held target, or above the target a creation names.
/// Once the request holds a target, nobody moves it.
pub fn spec_licensed(
    key: &str,
    subject: &super::EntityId,
    caller: &super::EntityId,
    lineage: Option<&super::Lineage>,
) -> bool {
    if !is_request(subject) {
        return true;
    }
    let Some(l) = lineage else {
        return false;
    };
    if l.holds.contains(TARGET_BOT) {
        key != TARGET_BOT && l.above.contains(caller)
    } else {
        l.named.is_some() && l.above_named.contains(caller)
    }
}

/// **The bots a refusal names as able to write the spec key `key`**: those above
/// the held target, else those above the target a creation names.
pub fn who_may_spec(key: &str, l: &super::Lineage) -> Vec<String> {
    let names = |bots: &[super::EntityId]| bots.iter().map(ToString::to_string).collect();
    if l.holds.contains(TARGET_BOT) {
        if key == TARGET_BOT {
            Vec::new()
        } else {
            names(&l.above)
        }
    } else {
        names(&l.above_named)
    }
}

/// **Where the walk up the chart starts for `subject`.** A request has no
/// manager of its own: the chain that judges its spec is the target bot's. So a
/// request that holds a target starts there, and anything else starts at itself.
/// `fold` is the subject's fold with its links served as handles.
pub fn chain_start(
    subject: &super::EntityId,
    fold: &std::collections::BTreeMap<String, String>,
) -> super::EntityId {
    if !is_request(subject) {
        return subject.clone();
    }
    fold.get(TARGET_BOT)
        .and_then(|value| super::manager_in(value))
        .unwrap_or_else(|| subject.clone())
}

/// **The generation a write of the spec makes, or `None` when it makes none.**
/// A write that sets a spec key to a value the request does not hold raises the
/// generation by one; the first spec written makes it 1. A resend of what the
/// request holds changes nothing, so it raises nothing. Worked out here and
/// never sent by a caller: [`GENERATION`] is a row nobody may write.
pub fn next_generation(
    subject: &super::EntityId,
    held: &std::collections::BTreeMap<String, String>,
    written: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    if !is_request(subject) {
        return None;
    }
    let changes = SPEC_KEYS.iter().any(|key| {
        written
            .get(*key)
            .is_some_and(|value| held.get(*key).map(|now| now.trim()) != Some(value.trim()))
    });
    changes.then(|| {
        let now = held
            .get(GENERATION)
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        (now + 1).to_string()
    })
}
