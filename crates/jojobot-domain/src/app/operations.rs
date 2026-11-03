//! **The unstable tier's grants and the one check that judges a call against
//! them** (rule 731).
//!
//! An app is granted operations by name. For a write, the grant also names the
//! kinds the app may write and the keys it may set; for a read it is yes or no.
//! [`permit`] judges one call: the operation, the arguments the caller sent, the
//! kind the call writes to and the keys it sets. It refuses by default, so an
//! argument nobody listed is refused rather than ignored.
//!
//! `bot` is an ungrantable kind and `set_charter` an ungrantable operation: an
//! app is no bot, and a claim an app wrote on a bot would read as that bot's
//! own thought (GAP: the card that first grants an app a bot as its owner makes
//! `first_writer` answer the actor and lifts this).

use std::collections::BTreeSet;

use serde::Deserialize;

use super::{AppError, AppPrincipal, key_never_granted, key_shape_ok};
use crate::memory::EntityKind;

/// The arguments an app may send to `capture`. Everything else is refused: a
/// tick, a check-in, a drop and the claim's labels are a bot's.
const CAPTURE_ARGS: [&str; 7] = [
    "subject",
    "content",
    "details",
    "recorded_at",
    "happened_at",
    "happened_through",
    "sets",
];

/// The arguments an app may send to `add_entity`.
const ADD_ENTITY_ARGS: [&str; 5] = ["kind", "handle", "name", "source", "sets"];

/// The arguments an app may send to `post_message`.
const POST_MESSAGE_ARGS: [&str; 3] = ["to", "body", "subject"];

/// **One operation an app is granted.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationGrant {
    /// The operation's name, one of [`OPERATIONS`].
    pub operation: String,
    /// The kinds a write may be made on. Empty for an operation that writes none.
    pub kinds: BTreeSet<EntityKind>,
    /// The keys a write may set. Empty for an operation that sets none.
    pub keys: BTreeSet<String>,
}

/// One granted operation as the rendered configuration file spells it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationSpec {
    /// The operation's name.
    pub operation: String,
    /// The kinds a write may be made on.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// The keys a write may set.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// **Read every operation an app's spec names**, or say why the operator is
/// wrong. The server then does not start.
pub(super) fn grants_from_specs(
    specs: &[OperationSpec],
    named: &dyn Fn(String) -> AppError,
) -> Result<Vec<OperationGrant>, AppError> {
    let mut grants: Vec<OperationGrant> = Vec::new();
    for spec in specs {
        let name = spec.operation.as_str();
        if name == "set_charter" {
            return Err(named(
                "set_charter cannot be granted: a charter belongs to a bot, and an app is \
                 granted none"
                    .into(),
            ));
        }
        // **Whether the registry serves it is the server's check at start**, which
        // knows the registry; this one reads only that the name is a plain one.
        if !key_shape_ok(name) {
            return Err(named(format!(
                "the operation {name:?} is not a plain operation name"
            )));
        }
        if grants.iter().any(|g| g.operation == name) {
            return Err(named(format!("the operation {name:?} is granted twice")));
        }
        let writes = matches!(name, "capture" | "add_entity");
        if !(writes || spec.kinds.is_empty() && spec.keys.is_empty()) {
            return Err(named(format!(
                "the operation {name:?} takes no kinds and no keys"
            )));
        }
        if writes && spec.kinds.is_empty() {
            return Err(named(format!("the grant of {name:?} names no kinds")));
        }
        let mut kinds = BTreeSet::new();
        for token in &spec.kinds {
            let kind = EntityKind::from_token(token)
                .ok_or_else(|| named(format!("{token:?} is not a kind")))?;
            if kind == EntityKind::BOT || kind == EntityKind::VOUCHER {
                return Err(named(format!(
                    "the kind {token:?} cannot be granted: an app is no bot and issues no voucher"
                )));
            }
            kinds.insert(kind);
        }
        let mut keys = BTreeSet::new();
        for key in &spec.keys {
            if !key_shape_ok(key) {
                return Err(named(format!("the key {key:?} is not a plain key name")));
            }
            if let Some(why) = key_never_granted(key) {
                return Err(named(format!(
                    "the key {key:?} is {why} and cannot be granted"
                )));
            }
            keys.insert(key.clone());
        }
        grants.push(OperationGrant {
            operation: name.to_string(),
            kinds,
            keys,
        });
    }
    Ok(grants)
}

/// **How many items a refusal lists** before it counts the rest, so an answer
/// stays one readable line however large a grant is.
const LISTED: usize = 20;

/// **The allowed set, in words**: the items in order, up to [`LISTED`], then how
/// many more there are. Empty is said, so a refusal never ends on a bare colon.
fn listed<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let items: Vec<&str> = items.into_iter().collect();
    if items.is_empty() {
        return "none".to_string();
    }
    let shown = items
        .iter()
        .take(LISTED)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    match items.len().checked_sub(LISTED) {
        Some(more) if more > 0 => format!("{shown}, and {more} more"),
        _ => shown,
    }
}

/// **The operations an app was granted, in words**, for a refusal that names
/// what the app may call. The one reading of the grant a refusal quotes.
pub fn granted_operations(app: &AppPrincipal) -> String {
    listed(app.operations.iter().map(|g| g.operation.as_str()))
}

/// **One call, as the check judges it.**
#[derive(Debug, Clone, Default)]
pub struct Operation {
    /// The operation's name.
    pub name: String,
    /// The names of the arguments the caller sent with a value.
    pub args_sent: Vec<String>,
    /// The kind the call writes on, as the store holds it now.
    pub kind: Option<EntityKind>,
    /// The keys the call sets.
    pub keys: Vec<String>,
    /// Whether a message is for the operator's box.
    pub to_operator: bool,
    /// **The `bound_app` the subject holds now**, for a write on an existing bot
    /// request. The status of a request is written by that app alone.
    pub bound_app: Option<String>,
}

/// **The one check for an app's call.** Does the app's grant cover it?
///
/// `Ok` means the call may run as the app. A refusal names what is outside the
/// grant, and nothing has been written.
pub fn permit(app: &AppPrincipal, call: &Operation) -> Result<(), AppError> {
    let outside = |why: String| Err(AppError::OutsideGrant(why));
    if app.revoked {
        return outside(format!("app {} is revoked and calls nothing", app.id));
    }
    let name = call.name.as_str();
    let Some(grant) = app.operations.iter().find(|g| g.operation == name) else {
        return outside(format!(
            "the operation {name:?} is not one this app was granted; it may call: {}",
            granted_operations(app)
        ));
    };
    let allowed: &[&str] = match name {
        "capture" => &CAPTURE_ARGS,
        "add_entity" => &ADD_ENTITY_ARGS,
        "post_message" => &POST_MESSAGE_ARGS,
        _ => &[],
    };
    let unlisted = call
        .args_sent
        .iter()
        .find(|a| name != "recall" && !allowed.contains(&a.as_str()));
    if let Some(arg) = unlisted {
        return outside(format!(
            "the argument {arg:?} is not one an app may send to {name}; it may send: {}",
            listed(allowed.iter().copied())
        ));
    }
    if name == "post_message" && !call.to_operator {
        return outside("an app may post to the operator's box only".into());
    }
    if matches!(name, "capture" | "add_entity") {
        match call.kind {
            Some(kind) if grant.kinds.contains(&kind) => {}
            Some(kind) => {
                return outside(format!(
                    "the kind {} is not one this app may write with {name}; it may write: {}",
                    kind.as_token(),
                    listed(grant.kinds.iter().map(|k| k.as_token()))
                ));
            }
            None => {
                return outside(format!(
                    "{name} names no kind this app may write; it may write: {}",
                    listed(grant.kinds.iter().map(|k| k.as_token()))
                ));
            }
        }
        let on_a_request = call
            .kind
            .is_some_and(|kind| kind.as_token() == crate::memory::bot_request::KIND);
        for key in &call.keys {
            // **A bot request's keys are judged by who the request binds.** The
            // status is the bound app's and the spec is a manager's, so no grant
            // widens either.
            if on_a_request {
                if crate::memory::bot_request::STATUS_KEYS.contains(&key.as_str()) {
                    if call.bound_app.as_deref() != Some(app.id.as_str()) {
                        return outside(format!(
                            "the key {key:?} is the status of a bot request, which only the app \
                             named in its bound_app writes"
                        ));
                    }
                } else if crate::memory::bot_request::SPEC_KEYS.contains(&key.as_str())
                    || key == crate::memory::bot_request::GENERATION
                {
                    return outside(format!(
                        "the key {key:?} is the spec of a bot request, which only a manager \
                         above its target writes"
                    ));
                }
            }
            if let Some(why) = key_never_granted(key) {
                return outside(format!("the key {key:?} is {why} and no app may set it"));
            }
            if !grant.keys.contains(key) {
                return outside(format!(
                    "the key {key:?} is not one this app may set; it may set: {}",
                    listed(grant.keys.iter().map(String::as_str))
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppSpec, hash_token, mint_token};

    /// **The allowed set is cut at the limit and counts the rest; an empty set
    /// says so.** Each branch beside the set that stays under the limit.
    #[test]
    fn the_allowed_set_is_cut_at_the_limit_and_an_empty_one_says_none() {
        let named =
            |count: usize| -> Vec<String> { (0..count).map(|n| format!("k{n:02}")).collect() };
        let exactly = named(LISTED);
        let said = listed(exactly.iter().map(String::as_str));
        assert!(said.contains("k19") && !said.contains("more"), "{said}");
        let over = named(LISTED + 3);
        let said = listed(over.iter().map(String::as_str));
        assert!(said.contains("k19") && !said.contains("k20"), "{said}");
        assert!(said.ends_with("and 3 more"), "{said}");
        assert_eq!(listed(std::iter::empty()), "none");
    }

    fn spec(operations: Vec<OperationSpec>) -> AppSpec {
        AppSpec {
            name: "springfield-watch".into(),
            operation: String::new(),
            thread: String::new(),
            kind: String::new(),
            keys: vec![],
            operations,
            revoked: false,
        }
    }

    fn op(operation: &str, kinds: &[&str], keys: &[&str]) -> OperationSpec {
        OperationSpec {
            operation: operation.into(),
            kinds: kinds.iter().map(|s| s.to_string()).collect(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn app(operations: Vec<OperationSpec>) -> AppPrincipal {
        crate::memory::kinds::load_shipped();
        AppPrincipal::from_spec(&spec(operations), &hash_token(&mint_token())).expect("a grant")
    }

    /// `mailbox_counts` is a read that takes no argument: a call that sends one is
    /// refused naming it, and the bare call is permitted.
    #[test]
    fn mailbox_counts_takes_no_argument() {
        let reader = app(vec![op("mailbox_counts", &[], &[])]);
        assert!(permit(&reader, &call("mailbox_counts", &[], None, &[])).is_ok());
        let sent = refusal(permit(
            &reader,
            &call("mailbox_counts", &["bot"], None, &[]),
        ));
        assert!(sent.contains("bot"), "{sent}");
    }

    fn watcher() -> AppPrincipal {
        app(vec![
            op("recall", &[], &[]),
            op("capture", &["role", "work"], &["watch_state", "watch_seen"]),
            op("post_message", &[], &[]),
        ])
    }

    fn call(name: &str, args: &[&str], kind: Option<EntityKind>, keys: &[&str]) -> Operation {
        Operation {
            name: name.into(),
            args_sent: args.iter().map(|s| s.to_string()).collect(),
            kind,
            keys: keys.iter().map(|s| s.to_string()).collect(),
            to_operator: false,
            bound_app: None,
        }
    }

    fn refusal(result: Result<(), AppError>) -> String {
        match result.expect_err("refused") {
            AppError::OutsideGrant(why) => why,
            other => panic!("not an outside-the-grant refusal: {other:?}"),
        }
    }

    /// **A bot request's status is written by the app it binds and by no other,
    /// and its spec by no app.** The same words on any other kind are ordinary
    /// keys, judged by the grant alone.
    #[test]
    fn a_request_status_is_the_bound_apps_and_its_spec_no_apps() {
        let request = Some(EntityKind::BOT_REQUEST);
        // The spellings are the app's contract and nothing else declares them, so
        // they are written out and not read from the table under test.
        let status_keys = [
            "observed_state",
            "observed_generation",
            "runtime_ref",
            "last_result",
        ];
        let spec_keys = [
            "target_bot",
            "desired",
            "machine",
            "model",
            "bound_app",
            "generation",
        ];
        let granted = app(vec![op(
            "capture",
            &["bot-request"],
            &[status_keys.as_slice(), spec_keys.as_slice()].concat(),
        )]);
        let id = granted.id.to_string();
        for key in status_keys {
            let mut status = call("capture", &["subject", "sets"], request, &[key]);
            status.bound_app = Some(id.clone());
            assert_eq!(
                permit(&granted, &status),
                Ok(()),
                "the bound app writes {key}"
            );
            status.bound_app = Some("someone-else".into());
            assert!(
                refusal(permit(&granted, &status)).contains("bound_app"),
                "{key}: another app"
            );
            status.bound_app = None;
            assert!(
                refusal(permit(&granted, &status)).contains("bound_app"),
                "{key}: no bound app"
            );
        }
        for key in spec_keys {
            let mut spec = call("capture", &["subject", "sets"], request, &[key]);
            spec.bound_app = Some(id.clone());
            assert!(
                refusal(permit(&granted, &spec)).contains("manager"),
                "{key}"
            );
        }
        // The same words on another kind are ordinary keys, judged by the grant.
        let machine = app(vec![op(
            "capture",
            &["machine"],
            &["desired", "observed_state"],
        )]);
        let other = call(
            "capture",
            &["subject", "sets"],
            Some(EntityKind::MACHINE),
            &["desired", "observed_state"],
        );
        assert_eq!(permit(&machine, &other), Ok(()));
    }

    #[test]
    fn a_granted_capture_on_a_granted_kind_and_key_is_permitted() {
        let c = call(
            "capture",
            &["subject", "content", "sets"],
            Some(EntityKind::ROLE),
            &["watch_state"],
        );
        assert_eq!(permit(&watcher(), &c), Ok(()));
    }

    #[test]
    fn an_operation_the_app_was_not_granted_is_refused_naming_it() {
        let c = call("add_entity", &["kind"], Some(EntityKind::ROLE), &[]);
        let why = refusal(permit(&watcher(), &c));
        assert!(why.contains("add_entity"), "{why}");
        // The granted operations ride beside it, read from the grant.
        assert!(
            why.contains("capture") && why.contains("post_message"),
            "{why}"
        );
    }

    #[test]
    fn a_kind_outside_the_grant_is_refused_naming_it() {
        let c = call("capture", &["subject"], Some(EntityKind::THREAD), &[]);
        let why = refusal(permit(&watcher(), &c));
        assert!(why.contains("thread"), "{why}");
        assert!(why.contains("role") && why.contains("work"), "{why}");
    }

    #[test]
    fn a_key_outside_the_grant_is_refused_naming_it() {
        let c = call(
            "capture",
            &["sets"],
            Some(EntityKind::ROLE),
            &["watch_other"],
        );
        let why = refusal(permit(&watcher(), &c));
        assert!(why.contains("watch_other"), "{why}");
        assert!(
            why.contains("watch_state") && why.contains("watch_seen"),
            "{why}"
        );
    }

    #[test]
    fn an_identity_due_or_label_key_is_refused_whatever_the_grant_lists() {
        for key in ["runs_out", "read_from"] {
            let c = call("capture", &["sets"], Some(EntityKind::ROLE), &[key]);
            let why = refusal(permit(&watcher(), &c));
            assert!(
                why.contains(key) && why.contains("no app may set it"),
                "{key}: {why}"
            );
        }
        // A key the operator simply did not list is outside the grant too.
        for key in ["holder", "claimed_at", "agent"] {
            let c = call("capture", &["sets"], Some(EntityKind::ROLE), &[key]);
            let why = refusal(permit(&watcher(), &c));
            assert!(
                why.contains(key) && why.contains("not one this app may set"),
                "{key}: {why}"
            );
        }
    }

    #[test]
    fn an_argument_nobody_listed_is_refused_naming_it() {
        for arg in [
            "check_in",
            "tick",
            "drop",
            "fields",
            "override_token",
            "provenance",
        ] {
            let c = call("capture", &["subject", arg], Some(EntityKind::ROLE), &[]);
            let why = refusal(permit(&watcher(), &c));
            assert!(why.contains(arg), "{arg}: {why}");
            // The arguments an app may send to capture ride beside it.
            assert!(why.contains("content") && why.contains("sets"), "{why}");
        }
    }

    #[test]
    fn a_message_is_permitted_to_the_operator_and_to_nobody_else() {
        let mut c = call("post_message", &["to", "body"], None, &[]);
        assert!(refusal(permit(&watcher(), &c)).contains("operator"));
        c.to_operator = true;
        assert_eq!(permit(&watcher(), &c), Ok(()));
        c.args_sent.push("supersedes".into());
        assert!(refusal(permit(&watcher(), &c)).contains("supersedes"));
    }

    #[test]
    fn a_revoked_app_calls_nothing() {
        let mut a = watcher();
        a.revoked = true;
        assert!(refusal(permit(&a, &call("recall", &[], None, &[]))).contains("revoked"));
    }

    #[test]
    fn the_operator_cannot_grant_a_bot_a_voucher_set_charter_or_a_second_copy() {
        crate::memory::kinds::load_shipped();
        let hash = hash_token(&mint_token());
        for bad in [
            op("capture", &["bot"], &[]),
            op("capture", &["voucher"], &[]),
            op("set_charter", &[], &[]),
            op("capture", &[], &["k"]),
            op("capture", &["role"], &["runs_out"]),
            op("recall", &["role"], &[]),
            op("mailbox_counts", &["role"], &[]),
            op("Dance!", &[], &[]),
        ] {
            assert!(
                AppPrincipal::from_spec(&spec(vec![bad.clone()]), &hash).is_err(),
                "{bad:?}"
            );
        }
        let twice = spec(vec![op("recall", &[], &[]), op("recall", &[], &[])]);
        assert!(AppPrincipal::from_spec(&twice, &hash).is_err());
        assert!(AppPrincipal::from_spec(&spec(vec![]), &hash).is_err());
    }
}
