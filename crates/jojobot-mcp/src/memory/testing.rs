//! **Memory's test fixtures** — the arguments a verb takes, the entities a test
//! needs standing, and the doubles that make a store behave badly on purpose.
//!
//! Named for the context they belong to, mirroring `jojobot_domain::memory::
//! testing`. What builds a handler lives in [`crate::harness`]; what is ABOUT
//! Memory lives here.

use super::*;
use crate::harness::*;
use std::sync::Mutex;

impl Default for SpySearch {
    fn default() -> Self {
        SpySearch {
            seen: Mutex::new(None),
            hits: Mutex::new(Vec::new()),
            coverage: MailCoverage::Loaded,
        }
    }
}

impl SpySearch {
    pub(crate) fn answering(hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            ..Default::default()
        }
    }

    /// A search port at a given mail coverage — the states a degraded index
    /// reports.
    pub(crate) fn covering(coverage: MailCoverage, hits: Vec<Hit>) -> Self {
        SpySearch {
            hits: Mutex::new(hits),
            coverage,
            ..Default::default()
        }
    }

    /// A search port whose mailbox world was never readable — the state an
    /// index is in when the boot scan of the board failed and nothing has
    /// indexed a message since.
    pub(crate) fn with_no_mail_indexed() -> Self {
        Self::covering(MailCoverage::Unread, Vec::new())
    }

    pub(crate) fn query(&self) -> SearchQuery {
        self.seen
            .lock()
            .unwrap()
            .clone()
            .expect("search must have reached the port")
    }
}

impl Search for SpySearch {
    fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        *self.seen.lock().unwrap() = Some(query.clone());
        Ok(self.hits.lock().unwrap().clone())
    }

    fn mail_coverage(&self) -> MailCoverage {
        self.coverage
    }
}

/// A [`Search`] double: it records the query it was handed and answers with
/// canned hits. On this path the MCP layer's whole job is translating
/// arguments into a query and hits into JSON, and that is exactly what this
/// pins — the ranking and matching are the index's tests, not these.
pub(crate) struct SpySearch {
    seen: Mutex<Option<SearchQuery>>,
    hits: Mutex<Vec<Hit>>,
    /// How much of the mail board this double claims to hold. Default
    /// loaded: an index that has read the board is the ordinary case, and
    /// the degraded ones are worth writing down at a call site.
    coverage: MailCoverage,
}

pub(crate) fn capture_args(subject: &str, content: &str) -> CaptureArgs {
    CaptureArgs {
        subject: subject.into(),
        content: content.into(),
        details: None,
        provenance: None,
        date: None,
        shape: None,
        object: None,
        sid: None,
    }
}

pub(crate) fn update_args(address: &str) -> UpdateFactArgs {
    UpdateFactArgs {
        address: address.into(),
        content: None,
        details: None,
        status: None,
        provenance: None,
        confirmed_by_user: None,
        shape: None,
        object: None,
        sid: None,
    }
}

/// Make sure a handle names an entity, so the write guard's **existence
/// gate** is not what a spec about something else trips over. Idempotent —
/// an add that comes back blocked means it is already there.
pub(crate) async fn ensure(jojobot: &Jojobot, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            kind: kind.as_token().into(),
            handle: id.slug().into(),
            name: id.slug().into(),
            aliases: None,
            source: "test-fixture".into(),
            crm: None,
            boot: None,
            create_new: None,
            sid: None,
        }))
        .await
        .expect("add_entity call ok");
}

/// [`ensure`], attributed to a session. Beats are written for whoever the
/// handle names, so a spec about the tally has to say who is calling.
pub(crate) async fn ensure_as(jojobot: &Jojobot, sid: &str, handle: &str) {
    let id = EntityId::person(handle);
    let kind = id.kind().expect("test handles are well-formed");
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            sid: Some(sid.to_string()),
            ..add_args(kind.as_token(), id.slug(), id.slug())
        }))
        .await
        .expect("add_entity call ok");
}

/// [`capture_ok`], attributed to a session — same reason.
pub(crate) async fn capture_as(
    jojobot: &Jojobot,
    sid: &str,
    args: CaptureArgs,
) -> serde_json::Value {
    capture_ok(
        jojobot,
        CaptureArgs {
            sid: Some(sid.to_string()),
            ..args
        },
    )
    .await
}

/// Capture through the handler, expecting the guard to wave it through —
/// provisioning the subject and any edge object first, because every write
/// that names an entity now requires one that exists.
pub(crate) async fn capture_ok(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
    ensure(jojobot, &args.subject).await;
    if let Some(object) = args.object.as_deref() {
        ensure(jojobot, object).await;
    }
    let result = jojobot.capture(Parameters(args)).await.expect("capture ok");
    let body = json_of(&result);
    assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
    body
}

/// The `address` field of a rendered fact — every read carries one.
pub(crate) fn address_of(fact: &serde_json::Value) -> String {
    fact["address"]
        .as_str()
        .expect("every fact on the wire carries its address")
        .to_string()
}

pub(crate) fn add_args(kind: &str, handle: &str, name: &str) -> AddEntityArgs {
    AddEntityArgs {
        kind: kind.into(),
        handle: handle.into(),
        name: name.into(),
        aliases: None,
        source: "user-named".into(),
        crm: None,
        boot: None,
        create_new: None,
        sid: None,
    }
}

pub(crate) fn search_args() -> SearchArgs {
    SearchArgs {
        query: None,
        kind: None,
        status: None,
        provenance: None,
        subject: None,
        edge: None,
        include_mail: None,
        limit: None,
        sid: None,
    }
}
