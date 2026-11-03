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

/// **Whether a claim's own keys are the generation and nothing else**, the
/// label that marks a generated record aside. Such a claim is the one jojobot
/// writes beside an act that took a spec claim off; it holds no spec.
pub fn sets_only_the_generation(fields: &std::collections::BTreeMap<String, String>) -> bool {
    let mut keys = fields
        .keys()
        .filter(|key| key.as_str() != super::review::SETTINGS_RECORD);
    keys.next().is_some_and(|key| key == GENERATION) && keys.next().is_none()
}

/// **The refusal to take back a claim that sets only the generation of a
/// request**, for every caller. The number is jojobot's own, worked out and never
/// sent, so no bot unwrites it: taking the claim off rolls the folded generation
/// back under a spec that is still the one numbered above it. A claim that holds
/// a spec key as well stays the manager's to retract or archive. `None` for a
/// subject that is no request.
pub fn refuses_withdrawing_the_generation(
    subject: &super::EntityId,
    claim_fields: &std::collections::BTreeMap<String, String>,
) -> Option<super::MemoryError> {
    (is_request(subject) && sets_only_the_generation(claim_fields)).then(|| {
        super::MemoryError::KeyNotYours {
            subject: subject.to_string(),
            key: GENERATION.to_string(),
            may: super::MayWrite::RequestGeneration,
            allowed: Vec::new(),
        }
    })
}

/// **Whether a value of a spec key is the one the request holds**, spaces around
/// either aside. The one comparison of a held value with a written one: the
/// generation reads a resend through it, and the app permit reads the app a
/// request binds through it, so the two never disagree about a padded value.
pub fn same_value(held: Option<&str>, written: &str) -> bool {
    held.map(str::trim) == Some(written.trim())
}

/// **The highest generation ever written on a request**, read from the history
/// of its [`GENERATION`] key: every write, a retracted or archived claim's
/// included, because a number that was ever issued names a spec for good. A
/// clear, a value that is no number and a request that never held one count as
/// 0.
pub fn highest_generation_ever(writes: &[super::FieldWrite]) -> u64 {
    writes
        .iter()
        .filter_map(|write| write.value.as_deref()?.trim().parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// **The spec a fold holds**: the spec keys it carries, spaces around a value
/// aside, and none that is blank.
pub fn spec_of(
    fold: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    SPEC_KEYS
        .iter()
        .filter_map(|key| {
            let value = fold.get(*key)?.trim();
            (!value.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

/// **The spec a write leaves**: the spec `held` carries, with the spec keys the
/// write sets put in and the spec keys it clears taken out. A key set to a blank
/// value is a key taken off.
pub fn spec_after(
    held: &std::collections::BTreeMap<String, String>,
    written: &std::collections::BTreeMap<String, String>,
    cleared: &[String],
) -> std::collections::BTreeMap<String, String> {
    let mut spec = spec_of(held);
    for key in SPEC_KEYS {
        if cleared.iter().any(|gone| gone == key) {
            spec.remove(key);
        }
        if let Some(value) = written.get(key) {
            match value.trim() {
                "" => spec.remove(key),
                value => spec.insert(key.to_string(), value.to_string()),
            };
        }
    }
    spec
}

/// **The generation an act makes, or `None` when it makes none.** An act that
/// leaves the request with a different folded spec than it had makes the next
/// number; the first spec written makes 1. An act that leaves the spec as it was
/// (a resend, a clear of a key not held, a withdrawal of a claim the fold did not
/// read) makes none. The two specs are the folds before and after the act
/// ([`spec_of`], [`spec_after`]), never the write alone: a retract or an archive
/// that uncovers an older value changes the spec though it sends no key. Worked
/// out here and never sent by a caller: [`GENERATION`] is a row nobody may write.
///
/// **The number is one past `highest_ever`, the highest ever written on the
/// request, and never one past the folded value.** A retract takes the newest
/// claim out of the fold, and a number read from the fold would then name a second
/// spec. The caller reads `highest_ever` ([`highest_generation_ever`]) and writes
/// under the request's lock, so two writes do not read the same number.
pub fn next_generation(
    subject: &super::EntityId,
    before: &std::collections::BTreeMap<String, String>,
    after: &std::collections::BTreeMap<String, String>,
    highest_ever: u64,
) -> Option<String> {
    (is_request(subject) && before != after).then(|| (highest_ever + 1).to_string())
}

/// **The spec the history of the spec keys holds once one claim is taken out.**
/// Each spec key reads its newest write among the active claims other than
/// `withdrawn`; a clear, or a blank value, leaves the key off. This is the fold
/// of a withdrawal as the store will leave it, read before the act so the number
/// it makes can be written with it.
pub fn spec_withdrawing(
    history: &std::collections::BTreeMap<String, Vec<super::FieldWrite>>,
    withdrawn: Option<&super::FactId>,
) -> std::collections::BTreeMap<String, String> {
    SPEC_KEYS
        .iter()
        .filter_map(|key| {
            let newest = history
                .get(*key)?
                .iter()
                .rfind(|write| {
                    write.status == super::FactStatus::Active
                        && withdrawn != Some(&write.fact.local)
                })?
                .value
                .as_deref()?
                .trim();
            (!newest.is_empty()).then(|| (key.to_string(), newest.to_string()))
        })
        .collect()
}
