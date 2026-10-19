//! **An app: an outside program that writes to jojobot with a login of its own
//! and one grant it cannot widen.**
//!
//! An app is a service principal. It is not a bot: it has no session, no
//! charter, no mailbox, no lease and no run, and nothing it sends can start,
//! stop, poke or message a session or write a role. The operator registers it
//! in the server's configuration (rule 542), which gives it an opaque token and
//! a [`Grant`]. The grant names one operation, one thread, one kind and the
//! keys the app may write. A request is judged against it in one place,
//! [`authorize`], and the only value that reaches a store is the
//! [`AuthorizedWrite`] it returns.
//!
//! **The app sends no free text.** It sends keys with values of a fixed shape,
//! and the server composes the claim's words from them (rule 511).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::memory::{EntityId, EntityKind};
use crate::request::RequestId;

/// **Every app token starts with this**, so a bearer that is an app token is
/// told from a run key or a JWT without looking anything up.
pub const TOKEN_PREFIX: &str = "ak-";

/// How many characters follow the prefix on a drawn token: 32 characters of a
/// 32-letter alphabet, 160 bits.
const TOKEN_DRAWN: usize = 32;

/// **Draw an app token**: the prefix and 32 characters of OS entropy. It is the
/// app's only credential, and the server keeps its hash and never the token.
pub fn mint_token() -> String {
    format!("{TOKEN_PREFIX}{}", crate::handle::draw(TOKEN_DRAWN))
}

/// Whether this string has the shape of an app token. Shape only.
pub fn is_token(value: &str) -> bool {
    value
        .strip_prefix(TOKEN_PREFIX)
        .is_some_and(|rest| crate::handle::is_drawn(rest, TOKEN_DRAWN))
}

/// **The hash an app's token is stored and looked up by.** The run key's hash,
/// so there is one hash in the system and not a second.
pub fn hash_token(token: &str) -> String {
    crate::session::hash_key(token)
}

/// The one operation an app can be granted.
pub const RECORD_DEPLOY_EVENT: &str = "record_deploy_event";

/// The longest an app's name may be.
pub const MAX_APP_ID: usize = 32;

/// **The name the operator gives an app.** Lowercase letters, digits and
/// hyphens, starting with a letter, so it is safe in a log line, a file name
/// and a `read_from` value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppId(String);

impl AppId {
    /// Accept a name or say why not.
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let ok = !raw.is_empty()
            && raw.len() <= MAX_APP_ID
            && raw.starts_with(|c: char| c.is_ascii_lowercase())
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if ok {
            Ok(Self(raw.to_string()))
        } else {
            Err(AppError::Config(format!(
                "the app name {raw:?} is not 1 to {MAX_APP_ID} lowercase letters, digits or \
                 hyphens, starting with a letter"
            )))
        }
    }

    /// The name as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why an app's configuration or a request from an app is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    /// The operator's configuration is wrong. The server does not start.
    #[error("{0}")]
    Config(String),
    /// The request is outside what the app was granted. Nothing is written.
    #[error("{0}")]
    OutsideGrant(String),
    /// The request is inside the grant but a value is not the shape it takes.
    #[error("{0}")]
    Invalid(String),
}

/// **What an app may do, fixed by the operator and never widened by the app.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// The operation. Today only [`RECORD_DEPLOY_EVENT`].
    pub operation: String,
    /// The thread each event is recorded under.
    pub thread: EntityId,
    /// The kind of what is written. Today only `event`.
    pub kind: EntityKind,
    /// The keys the app may send.
    pub writable_keys: BTreeSet<String>,
}

/// **An app as the server knows it.** The token is held as its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPrincipal {
    /// The app's name.
    pub id: AppId,
    /// The hash of its token, from [`hash_token`].
    pub token_hash: String,
    /// What it may do.
    pub grant: Grant,
    /// Whether the operator has switched it off. A revoked app writes nothing.
    pub revoked: bool,
}

/// One app as the rendered configuration file spells it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSpec {
    /// The app's name.
    pub name: String,
    /// The operation granted.
    pub operation: String,
    /// The thread handle events go under.
    pub thread: String,
    /// The kind of what is written.
    pub kind: String,
    /// The keys the app may send.
    pub keys: Vec<String>,
    /// Whether the app is switched off.
    #[serde(default)]
    pub revoked: bool,
}

/// **Build every app the configuration names**, each with the hash of its
/// token. The configuration is refused whole when one app is wrong, when a name
/// appears twice, or when an app has no token hash: the server then does not
/// start, and a grant is never served that was not fully read.
pub fn build_apps(
    specs: &[AppSpec],
    token_hashes: &BTreeMap<String, String>,
) -> Result<Vec<AppPrincipal>, AppError> {
    let mut seen = BTreeSet::new();
    let mut apps = Vec::new();
    for spec in specs {
        if !seen.insert(spec.name.as_str()) {
            return Err(AppError::Config(format!(
                "the apps file names {:?} twice",
                spec.name
            )));
        }
        let hash = token_hashes.get(&spec.name).ok_or_else(|| {
            AppError::Config(format!("app {:?} has no token hash credential", spec.name))
        })?;
        apps.push(AppPrincipal::from_spec(spec, hash)?);
    }
    Ok(apps)
}

/// The key shape: lowercase letters, digits and underscores, starting with a
/// letter.
fn key_shape_ok(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 32
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// **Why a key may never be in a grant**, or `None` when it may.
///
/// An app writes a record's own fields, and a key that means something to
/// jojobot must not be one an outside writer can set: a guarded key binds an
/// identity, a reserved key is one jojobot writes itself, a label key says who
/// read what, and a key that makes a thing fall due would put the app's words
/// into what the operator is owed.
pub fn key_never_granted(key: &str) -> Option<&'static str> {
    if crate::memory::GUARDED_KEYS.iter().any(|g| g.key == key) {
        return Some("a guarded key");
    }
    if crate::memory::reserved_key(key) {
        return Some("a reserved key");
    }
    if [
        crate::memory::READ_FROM,
        crate::memory::READ_REF,
        "starred",
        "subject",
        "purpose",
        "recorded_by",
    ]
    .contains(&key)
    {
        return Some("a key that labels a record");
    }
    let carriers = crate::attention::shipped();
    let refs: Vec<&dyn crate::attention::Carrier> = carriers.iter().map(|c| c.as_ref()).collect();
    if crate::attention::due_keys(&refs).iter().any(|k| k == key)
        || crate::attention::COMPUTED_KEYS.contains(&key)
        || key == crate::attention::ENDED
        || key == crate::attention::REGARDING
    {
        return Some("a key that makes a thing fall due");
    }
    None
}

impl AppPrincipal {
    /// **Build an app from the operator's spec and its token hash.** Every way
    /// the operator can be wrong is refused here, which is what lets the server
    /// refuse to start rather than serve a grant it cannot hold.
    pub fn from_spec(spec: &AppSpec, token_hash: &str) -> Result<Self, AppError> {
        let id = AppId::parse(&spec.name)?;
        let named = |why: String| AppError::Config(format!("app {id}: {why}"));
        if spec.operation != RECORD_DEPLOY_EVENT {
            return Err(named(format!(
                "the operation {:?} is not one an app can be granted; the only one is \
                 {RECORD_DEPLOY_EVENT}",
                spec.operation
            )));
        }
        if spec.kind != EntityKind::EVENT.as_token() {
            return Err(named(format!(
                "the kind {:?} is not one an app can write; the only one is event",
                spec.kind
            )));
        }
        let thread = EntityId(spec.thread.trim().to_string());
        if thread.kind_token() != EntityKind::THREAD.as_token() || thread.slug().is_empty() {
            return Err(named(format!(
                "the thread {:?} is not a thread handle",
                spec.thread
            )));
        }
        if spec.keys.is_empty() {
            return Err(named("the grant names no keys".into()));
        }
        let mut writable_keys = BTreeSet::new();
        for key in &spec.keys {
            if !key_shape_ok(key) {
                return Err(named(format!("the key {key:?} is not a plain key name")));
            }
            if let Some(why) = key_never_granted(key) {
                return Err(named(format!(
                    "the key {key:?} is {why} and cannot be granted"
                )));
            }
            writable_keys.insert(key.clone());
        }
        if token_hash.len() != 64 || !token_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(named("its token hash is not 64 hex characters".into()));
        }
        Ok(AppPrincipal {
            id,
            token_hash: token_hash.to_ascii_lowercase(),
            grant: Grant {
                operation: spec.operation.clone(),
                thread,
                kind: EntityKind::EVENT,
                writable_keys,
            },
            revoked: spec.revoked,
        })
    }
}

/// **The mark an app's write carries on the claim it made.**
///
/// It is a typed field on the claim and never a key in its fields, so no caller
/// can send one and no filter on keys can mistake it for data. A reader tells
/// "the app recorded this" from "a bot said so" by this mark alone: a bot that
/// writes an app's name in `read_from` gets no mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppStamp {
    /// The app that wrote the claim.
    pub app: String,
    /// The operation the grant allowed.
    pub grant: String,
    /// The moment the server took the request in, as an RFC 3339 instant.
    pub received_at: String,
}

/// **One app write, ready for a store.** The entity is the new event, the
/// claim is its first, and the attempt is what makes a retry safe.
#[derive(Debug, Clone)]
pub struct AppEvent {
    /// The event to create, under the grant's thread.
    pub entity: crate::memory::NewEntity,
    /// Its first claim, carrying the app's mark.
    pub first: crate::memory::NewFact,
    /// The app's name.
    pub app: String,
    /// The attempt id the app sent.
    pub attempt: String,
    /// A digest of the body the app sent, so a repeat can be told from a
    /// different body under the same attempt.
    pub digest: String,
}

/// **What the first call of an attempt landed**, kept so a repeat can answer
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppReceipt {
    /// The event's handle.
    pub handle: String,
    /// The first claim's address.
    pub address: String,
}

/// What [`Memory::record_app_event`](crate::memory::Memory::record_app_event)
/// answers.
#[derive(Debug, Clone)]
pub enum AppOutcome {
    /// The attempt was new. The guard may still have blocked the write, and then
    /// nothing was written and the attempt is not spent.
    Landed(Box<crate::memory::Guarded<(crate::memory::Entity, crate::memory::Fact)>>),
    /// The attempt had landed with this body. Nothing was written.
    Repeat(AppReceipt),
    /// The attempt had landed with a different body. Nothing was written.
    Conflict,
}

impl AppOutcome {
    /// The event and its first claim, when this call wrote them.
    pub fn landed(self) -> Option<(crate::memory::Entity, crate::memory::Fact)> {
        match self {
            AppOutcome::Landed(landed) => landed.written(),
            AppOutcome::Repeat(_) | AppOutcome::Conflict => None,
        }
    }
}

/// **What an app sends to record a deploy event.** Every value has a fixed
/// shape and none is free text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployEvent {
    /// The caller's name for this attempt. The same app sending the same
    /// attempt again is a retry, not a second event.
    pub attempt_id: String,
    /// The keys and their values.
    pub fields: BTreeMap<String, String>,
}

/// **A request that passed [`authorize`].** Only that function makes one, so a
/// store call that takes it has been judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedWrite {
    app: AppId,
    thread: EntityId,
    attempt_id: RequestId,
    fields: BTreeMap<String, String>,
}

impl AuthorizedWrite {
    /// The app that sends it.
    pub fn app(&self) -> &AppId {
        &self.app
    }

    /// The thread the event goes under.
    pub fn thread(&self) -> &EntityId {
        &self.thread
    }

    /// The attempt id.
    pub fn attempt_id(&self) -> &RequestId {
        &self.attempt_id
    }

    /// The keys and values, all judged.
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }

    /// **A digest of what the app sent**, the keys and their values in key
    /// order. The attempt id is left out: it is the key of the attempt, and the
    /// digest answers whether a repeat of the attempt sent the same body.
    pub fn digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for (key, value) in &self.fields {
            hasher.update(key.as_bytes());
            hasher.update([0]);
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// **The write a store takes**: a new event under the thread, whose first
    /// claim is the server's composed words, an observation read from the app,
    /// carrying the app's mark and setting the app's keys on the event. The
    /// event's handle is drawn here, from entropy, so its name shares nothing
    /// with its siblings and the likeness screen needs no override.
    ///
    /// `grant` is the operation the app's grant allowed, `received_at` the
    /// moment the server took the request in, and `today` the server's day.
    pub fn into_event(
        self,
        grant: &str,
        received_at: String,
        today: jiff::civil::Date,
    ) -> AppEvent {
        let slug = format!("deploy-{}", crate::handle::draw(10));
        let id = EntityId::new(EntityKind::EVENT, &slug);
        let content = self.content();
        let digest = self.digest();
        let mut fields = self.fields.clone();
        let sets: BTreeSet<String> = fields.keys().cloned().collect();
        fields.insert(crate::memory::READ_FROM.to_string(), self.app.to_string());
        let mut entity =
            crate::memory::NewEntity::new(id.clone(), &slug, format!("app {}", self.app));
        entity.parent = Some(self.thread.clone());
        let mut first = crate::memory::NewFact::about(id, content, today);
        first.provenance = crate::memory::Provenance::Observation;
        first.fields = fields;
        first.sets = sets;
        first.app_stamp = Some(AppStamp {
            app: self.app.to_string(),
            grant: grant.to_string(),
            received_at,
        });
        AppEvent {
            entity,
            first,
            app: self.app.to_string(),
            attempt: self.attempt_id.as_str().to_string(),
            digest,
        }
    }

    /// **The claim's words, composed by the server** from the keys in key
    /// order. The app's values appear as values and nowhere else.
    pub fn content(&self) -> String {
        let parts: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        format!("Deploy event: {}", parts.join(", "))
    }
}

/// The longest a bounded token value may be.
pub const MAX_TOKEN_VALUE: usize = 64;

/// The shape one key's value takes: a commit is 7 to 40 hex digits, and every
/// other value is a bounded token of letters, digits, dots, underscores and
/// hyphens.
fn value_shape_ok(key: &str, value: &str) -> bool {
    if key == "commit" {
        (7..=40).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
    } else {
        !value.is_empty()
            && value.len() <= MAX_TOKEN_VALUE
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    }
}

/// **The one check.** Does this app's grant cover this request?
pub fn authorize(app: &AppPrincipal, event: &DeployEvent) -> Result<AuthorizedWrite, AppError> {
    if app.revoked {
        return Err(AppError::OutsideGrant(format!(
            "app {} is revoked and writes nothing",
            app.id
        )));
    }
    if app.grant.operation != RECORD_DEPLOY_EVENT || app.grant.kind != EntityKind::EVENT {
        return Err(AppError::OutsideGrant(
            "the grant does not cover recording a deploy event".into(),
        ));
    }
    if event.fields.is_empty() {
        return Err(AppError::Invalid("the event names no keys".into()));
    }
    for key in event.fields.keys() {
        if !app.grant.writable_keys.contains(key) {
            return Err(AppError::OutsideGrant(format!(
                "the key {key:?} is not one this app may write"
            )));
        }
    }
    for (key, value) in &event.fields {
        if !value_shape_ok(key, value) {
            return Err(AppError::Invalid(format!(
                "the value for {key:?} is not the shape that key takes"
            )));
        }
    }
    let attempt_id = RequestId::parse(&event.attempt_id)
        .map_err(|e| AppError::Invalid(format!("attempt_id: {e}")))?;
    Ok(AuthorizedWrite {
        app: app.id.clone(),
        thread: app.grant.thread.clone(),
        attempt_id,
        fields: event.fields.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(keys: &[&str]) -> AppSpec {
        AppSpec {
            name: "springfield-ci".into(),
            operation: RECORD_DEPLOY_EVENT.into(),
            thread: "thread:deploys-alpha".into(),
            kind: "event".into(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
            revoked: false,
        }
    }

    fn principal() -> AppPrincipal {
        AppPrincipal::from_spec(
            &spec(&["commit", "host", "unit"]),
            &hash_token(&mint_token()),
        )
        .expect("a plain grant")
    }

    fn event(fields: &[(&str, &str)]) -> DeployEvent {
        DeployEvent {
            attempt_id: "run-1".into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn a_token_has_the_prefix_and_a_shape_a_run_key_does_not() {
        let token = mint_token();
        assert!(token.starts_with("ak-"));
        assert!(is_token(&token));
        assert!(!is_token(&crate::session::mint_key()));
        assert!(!is_token("ak-short"));
        assert_ne!(token, mint_token());
    }

    #[test]
    fn a_token_is_hashed_by_the_one_hash_the_system_has() {
        let token = mint_token();
        assert_eq!(hash_token(&token), crate::session::hash_key(&token));
    }

    #[test]
    fn a_covered_request_is_authorized_and_carries_its_thread_and_words() {
        let write = authorize(
            &principal(),
            &event(&[("commit", "abc1234"), ("host", "host-alpha")]),
        )
        .expect("inside the grant");
        assert_eq!(write.thread().as_str(), "thread:deploys-alpha");
        assert_eq!(write.app().as_str(), "springfield-ci");
        assert_eq!(
            write.content(),
            "Deploy event: commit abc1234, host host-alpha"
        );
    }

    #[test]
    fn a_key_the_grant_does_not_list_is_outside_the_grant() {
        let refused = authorize(
            &principal(),
            &event(&[("commit", "abc1234"), ("note", "x")]),
        )
        .expect_err("note is not granted");
        assert!(matches!(refused, AppError::OutsideGrant(_)), "{refused:?}");
    }

    #[test]
    fn a_revoked_app_is_refused_even_for_a_covered_request() {
        let mut app = principal();
        app.revoked = true;
        let refused = authorize(&app, &event(&[("commit", "abc1234")])).expect_err("revoked");
        assert!(matches!(refused, AppError::OutsideGrant(_)), "{refused:?}");
    }

    #[test]
    fn an_empty_request_is_refused() {
        assert!(authorize(&principal(), &event(&[])).is_err());
    }

    #[test]
    fn a_value_that_is_not_its_shape_is_invalid() {
        for bad in ["abc12", "xyzxyzxyz", &"a".repeat(41), "abc 1234"] {
            let refused = authorize(&principal(), &event(&[("commit", bad)])).expect_err(bad);
            assert!(
                matches!(refused, AppError::Invalid(_)),
                "{bad}: {refused:?}"
            );
        }
        for bad in ["", "two words", "new\nline", &"h".repeat(65)] {
            let refused = authorize(&principal(), &event(&[("host", bad)])).expect_err(bad);
            assert!(
                matches!(refused, AppError::Invalid(_)),
                "{bad:?}: {refused:?}"
            );
        }
    }

    #[test]
    fn an_attempt_id_is_held_to_the_request_id_rule() {
        let mut bad = event(&[("commit", "abc1234")]);
        bad.attempt_id = " padded ".into();
        assert!(matches!(
            authorize(&principal(), &bad),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn the_operator_cannot_grant_a_guarded_reserved_label_or_due_key() {
        for key in [
            "reports_to",
            "claims_role",
            "retracts",
            "read_from",
            "recorded_by",
            "runs_out",
            "decide_by",
            "promised_by",
            "cadence_days",
            "ended",
        ] {
            let refused =
                AppPrincipal::from_spec(&spec(&["commit", key]), &hash_token("x")).expect_err(key);
            assert!(matches!(refused, AppError::Config(_)), "{key}: {refused:?}");
        }
    }

    #[test]
    fn the_operator_cannot_grant_another_kind_operation_or_a_non_thread() {
        let mut other_kind = spec(&["commit"]);
        other_kind.kind = "person".into();
        let mut other_op = spec(&["commit"]);
        other_op.operation = "write_anything".into();
        let mut not_thread = spec(&["commit"]);
        not_thread.thread = "bot:assistant".into();
        for bad in [other_kind, other_op, not_thread, spec(&[])] {
            assert!(
                AppPrincipal::from_spec(&bad, &hash_token("x")).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_token_hash_that_is_not_64_hex_is_refused() {
        assert!(AppPrincipal::from_spec(&spec(&["commit"]), "not-a-hash").is_err());
    }

    #[test]
    fn the_apps_are_built_whole_or_refused() {
        let hash = hash_token(&mint_token());
        let hashes: BTreeMap<String, String> =
            [("springfield-ci".to_string(), hash)].into_iter().collect();
        let built = build_apps(&[spec(&["commit"])], &hashes).expect("one app");
        assert_eq!(built.len(), 1);
        assert!(build_apps(&[spec(&["commit"]), spec(&["commit"])], &hashes).is_err());
        assert!(build_apps(&[spec(&["commit"])], &BTreeMap::new()).is_err());
        assert!(build_apps(&[spec(&["reports_to"])], &hashes).is_err());
        assert!(build_apps(&[], &hashes).expect("none is fine").is_empty());
    }

    #[test]
    fn an_app_name_is_a_plain_slug() {
        assert!(AppId::parse("springfield-ci").is_ok());
        for bad in ["", "Upper", "1st", "has space", &"a".repeat(33)] {
            assert!(AppId::parse(bad).is_err(), "{bad:?}");
        }
    }
}
