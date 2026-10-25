//! Observations of calls to the persisted store interfaces, below their folds.
//!
//! Only closed operation names, outcomes and durations reach the observer.
//! No arguments or error text cross this boundary.

use std::sync::Arc;
use std::time::Duration;

macro_rules! operations {
    ($name:ident { $($variant:ident => $word:literal,)* }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant,)* }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $word,)* }
            }
        }
    };
}

operations!(MemoryOperation {
    AddEntity => "add_entity",
    AddEntityWithFirstClaim => "add_entity_with_first_claim",
    RecordAppEvent => "record_app_event",
    UpdateEntityWithClaim => "update_entity_with_claim",
    ListEntities => "list_entities",
    FormerHandles => "former_handles",
    Backing => "backing",
    BuiltOn => "built_on",
    KeysHolding => "keys_holding",
    OrderedFirstK => "ordered_first_k",
    ThoughtWriters => "thought_writers",
    ReferringTo => "referring_to",
    Children => "children",
    UpdateEntity => "update_entity",
    RenameEntity => "rename_entity",
    ArchiveEntity => "archive_entity",
    RestoreEntity => "restore_entity",
    Capture => "capture",
    Recall => "recall",
    UpdateFact => "update_fact",
    History => "history",
    ClaimHistory => "claim_history",
    EntityHistory => "entity_history",
    KeysKeptOffTheThing => "keys_kept_off_the_thing",
    ClaimHistories => "claim_histories",
    MergeEvents => "merge_events",
    MovedAddress => "moved_address",
    UnmappedClaims => "unmapped_claims",
    KeyTrails => "key_trails",
    Fields => "fields",
    FieldAges => "field_ages",
    EchoedDefaults => "echoed_defaults",
    FieldsVersioned => "fields_versioned",
    Retract => "retract",
    Merge => "merge",
    SetProse => "set_prose",
    Scan => "scan",
    ScanEntity => "scan_entity",
    HasProse => "has_prose",
    ComposedProse => "composed_prose",
    WriteSummary => "write_summary",
    DeclareType => "declare_type",
    DeclaredTypes => "declared_types",
    DeclaredTypesShared => "declared_types_shared",
    DisplacedType => "displaced_type",
    DeclareKind => "declare_kind",
    ReclaimKind => "reclaim_kind",
    DeclaredKinds => "declared_kinds",
    NumberingOf => "numbering_of",
    NumbersHeld => "numbers_held",
    NumberedScope => "numbered_scope",
});

operations!(MailboxOperation {
    CreateMailbox => "create_mailbox",
    RepointOwner => "repoint_owner",
    ListMailboxes => "list_mailboxes",
    ScanMessages => "scan_messages",
    MessageById => "message_by_id",
    MessagesById => "messages_by_id",
    SentBy => "sent_by",
    PostMessage => "post_message",
    ReadMailbox => "read_mailbox",
    ReadMessage => "read_message",
    MarkProcessed => "mark_processed",
    Quarantine => "quarantine",
    MessageHistory => "message_history",
    ScanMessageEvents => "scan_message_events",
});

operations!(SessionOperation {
    SessionsOf => "sessions_of",
    SummariesOf => "summaries_of",
    AllSummaries => "all_summaries",
    SummaryOf => "summary_of",
    RunCounts => "run_counts",
    RunTotals => "run_totals",
    ServedDays => "served_days",
    CountCall => "count_call",
    RunCalls => "run_calls",
    CallCounts => "call_counts",
    AllSessions => "all_sessions",
    WriteSummary => "write_summary",
    ReadSession => "read_session",
    Begin => "begin",
    Append => "append",
    AmendLast => "amend_last",
    AmendBeat => "amend_beat",
    SetFocus => "set_focus",
    SetTimezone => "set_timezone",
    SetStatedDay => "set_stated_day",
    SetWrapWindow => "set_wrap_window",
    Close => "close",
    CloseDisplaced => "close_displaced",
    AddServed => "add_served",
    ResetServed => "reset_served",
    Reopen => "reopen",
    SetCharterTold => "set_charter_told",
    ScanSessionHistory => "scan_session_history",
    SessionHistory => "session_history",
    JournalHistory => "journal_history",
    SetKeyHash => "set_key_hash",
    NoteMinted => "note_minted",
    MintedWithoutACard => "minted_without_a_card",
    PruneMinted => "prune_minted",
    NoteCharterChange => "note_charter_change",
    CharterChangeOf => "charter_change_of",
    CharterChanges => "charter_changes",
});

operations!(TeachingOperation {
    FirstContact => "first_contact",
    HasContact => "has_contact",
    Forget => "forget",
    Taught => "taught",
});

operations!(RequestOperation {
    Lookup => "lookup",
    Fill => "fill",
    Parts => "parts",
    Finish => "finish",
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Memory(MemoryOperation),
    Mailboxes(MailboxOperation),
    Sessions(SessionOperation),
    Teachings(TeachingOperation),
    Requests(RequestOperation),
}

impl Operation {
    pub fn rail(self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Mailboxes(_) => "mailboxes",
            Self::Sessions(_) => "sessions",
            Self::Teachings(_) => "teachings",
            Self::Requests(_) => "requests",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Memory(operation) => operation.as_str(),
            Self::Mailboxes(operation) => operation.as_str(),
            Self::Sessions(operation) => operation.as_str(),
            Self::Teachings(operation) => operation.as_str(),
            Self::Requests(operation) => operation.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Refused,
    StoreError,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::StoreError => "store_error",
        }
    }
}

/// An observer cannot return a failure to the operation it watches, and must
/// perform no store calls of its own.
pub trait Observer: Send + Sync {
    fn finished(&self, operation: Operation, outcome: Outcome, elapsed: Duration);
}

#[derive(Clone, Default)]
pub struct Observation(Option<Arc<dyn Observer>>);

impl Observation {
    pub fn new(observer: Arc<dyn Observer>) -> Self {
        Self(Some(observer))
    }

    pub fn memory(
        &self,
        inner: Arc<dyn jojobot_domain::memory::Memory>,
    ) -> Arc<dyn jojobot_domain::memory::Memory> {
        match &self.0 {
            Some(observer) => Arc::new(Observed {
                inner,
                observer: observer.clone(),
            }),
            None => inner,
        }
    }

    pub fn mailboxes(
        &self,
        inner: Arc<dyn jojobot_domain::mailbox::Mailboxes>,
    ) -> Arc<dyn jojobot_domain::mailbox::Mailboxes> {
        match &self.0 {
            Some(observer) => Arc::new(Observed {
                inner,
                observer: observer.clone(),
            }),
            None => inner,
        }
    }

    pub fn sessions(
        &self,
        inner: Arc<dyn jojobot_domain::session::Sessions>,
    ) -> Arc<dyn jojobot_domain::session::Sessions> {
        match &self.0 {
            Some(observer) => Arc::new(Observed {
                inner,
                observer: observer.clone(),
            }),
            None => inner,
        }
    }

    pub fn teachings(
        &self,
        inner: Arc<dyn jojobot_domain::teaching::Teachings>,
    ) -> Arc<dyn jojobot_domain::teaching::Teachings> {
        match &self.0 {
            Some(observer) => Arc::new(Observed {
                inner,
                observer: observer.clone(),
            }),
            None => inner,
        }
    }

    pub fn requests(
        &self,
        inner: Arc<dyn jojobot_domain::request::Requests>,
    ) -> Arc<dyn jojobot_domain::request::Requests> {
        match &self.0 {
            Some(observer) => Arc::new(Observed {
                inner,
                observer: observer.clone(),
            }),
            None => inner,
        }
    }
}

mod mailboxes;
mod memory;
mod requests;
mod sessions;
mod teachings;

struct Observed<T: ?Sized> {
    inner: Arc<T>,
    observer: Arc<dyn Observer>,
}

trait StoreFailure {
    fn is_store_error(&self) -> bool;
}

macro_rules! store_failures {
    ($($error:ty),* $(,)?) => {
        $(impl StoreFailure for $error {
            fn is_store_error(&self) -> bool {
                matches!(self, Self::Store(_))
            }
        })*
    };
}

store_failures!(
    jojobot_domain::memory::MemoryError,
    jojobot_domain::mailbox::MailboxError,
    jojobot_domain::session::SessionError,
    jojobot_domain::teaching::TeachingError,
    jojobot_domain::request::RequestError,
);

impl<T: ?Sized> Observed<T> {
    async fn result<R, E: StoreFailure>(
        &self,
        operation: Operation,
        future: impl std::future::Future<Output = Result<R, E>>,
    ) -> Result<R, E> {
        let started = std::time::Instant::now();
        let result = future.await;
        let outcome = match &result {
            Ok(_) => Outcome::Ok,
            Err(error) if error.is_store_error() => Outcome::StoreError,
            Err(_) => Outcome::Refused,
        };
        self.observer
            .finished(operation, outcome, started.elapsed());
        result
    }

    async fn answer<R>(
        &self,
        operation: Operation,
        future: impl std::future::Future<Output = R>,
    ) -> R {
        let started = std::time::Instant::now();
        let result = future.await;
        self.observer
            .finished(operation, Outcome::Ok, started.elapsed());
        result
    }
}
