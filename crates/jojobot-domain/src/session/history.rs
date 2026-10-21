//! **How a change to a session is recorded** (rule 498). The store, the fake and
//! anything else that writes a session make the record of the change with
//! [`next_change`], and a wording of a journal entry with [`next_wording`], so
//! the number, the value and what a value never holds are decided at one site.
//!
//! A session row and a journal entry stay the projection and answer every
//! current read. These are the writes behind them: one record for each change
//! of a run, and every wording of each entry.
//!
//! **A value that must not be read back is never made.** A [`Change`] carries
//! the typed thing that changed, and what it keeps of it is decided here: the
//! key a run works through is recorded as changed and nothing else, and a wrap
//! window is recorded as offered or open without the code that opens it.

use jiff::Timestamp;
use jiff::civil::Date;

use super::{EntryId, SessionId, SessionState, WrapWindow};

/// **The one order of runs: newest start first, the id breaking a tie**, so the
/// order is total and two reads agree (rule 598). It compares the moments, never
/// the text they are stored as: a moment written with a fraction and one written
/// without sort apart as text the reverse of how they fall in time. The test
/// store, the fold and the real store all order runs with this.
pub fn newest_first(a: (Timestamp, &SessionId), b: (Timestamp, &SessionId)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then_with(|| b.1.cmp(a.1))
}

/// **When a run last had anything to show for itself**: the latest moment of any
/// entry, written or touched, and the run's own start when it has no entry. Each
/// beat is an entry's `at` and its `touched`. [`super::Session::last_beat`] and
/// the real store's summaries both take it from here, so a summary reads what the
/// run reads (rule 598).
pub fn last_beat(
    started_at: Timestamp,
    beats: impl IntoIterator<Item = (Timestamp, Option<Timestamp>)>,
) -> Timestamp {
    beats
        .into_iter()
        .flat_map(|(at, touched)| [at, touched.unwrap_or(at)])
        .max()
        .unwrap_or(started_at)
}

/// **Which change a record is of.** The token is what the store keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// The run began. Its value is the focus it began with.
    Begin,
    /// An entry was appended. Its value is the entry's id.
    Entry,
    /// The newest entry was rewritten. Its value is the entry's id.
    AmendLast,
    /// An automatic beat was rewritten. Its value is the entry's id.
    AmendBeat,
    /// The focus was set. Its value is the new focus.
    Focus,
    /// The zone was set or cleared. A cleared zone has no value.
    Timezone,
    /// The day the run states was set or cleared. A cleared day has no value.
    StatedDay,
    /// The wrap window was opened, offered or cleared. Its value is `offered` or
    /// `open`, never the code.
    WrapWindow,
    /// The run was closed or walked back. Its value is the state it moved to.
    State,
    /// What the run has been served was set back to nothing. Its value is the
    /// total it held at the reset. An answer served adds to the total and is
    /// not a change of its own.
    ServedReset,
    /// The key the run works through changed. It never has a value.
    Key,
    /// The charter was told. Its value is the moment.
    CharterTold,
}

impl ChangeKind {
    /// Every kind, in the order the run can first meet them.
    pub const ALL: [ChangeKind; 12] = [
        ChangeKind::Begin,
        ChangeKind::Entry,
        ChangeKind::AmendLast,
        ChangeKind::AmendBeat,
        ChangeKind::Focus,
        ChangeKind::Timezone,
        ChangeKind::StatedDay,
        ChangeKind::WrapWindow,
        ChangeKind::State,
        ChangeKind::ServedReset,
        ChangeKind::Key,
        ChangeKind::CharterTold,
    ];

    /// The token the store keeps.
    pub fn as_token(self) -> &'static str {
        match self {
            ChangeKind::Begin => "begin",
            ChangeKind::Entry => "entry",
            ChangeKind::AmendLast => "amend_last",
            ChangeKind::AmendBeat => "amend_beat",
            ChangeKind::Focus => "focus",
            ChangeKind::Timezone => "timezone",
            ChangeKind::StatedDay => "stated_day",
            ChangeKind::WrapWindow => "wrap_window",
            ChangeKind::State => "state",
            ChangeKind::ServedReset => "served_reset",
            ChangeKind::Key => "key",
            ChangeKind::CharterTold => "charter_told",
        }
    }

    /// Parse a stored token; `None` for one this build does not know.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_token() == token)
    }

    /// **The field of the run this change is a change of**, which is the name
    /// `history` takes on a session. A run's starting focus is a change of its
    /// focus, and the three that write a journal entry are changes of the
    /// journal.
    pub fn field(self) -> &'static str {
        match self {
            ChangeKind::Begin | ChangeKind::Focus => "focus",
            ChangeKind::Entry | ChangeKind::AmendLast | ChangeKind::AmendBeat => JOURNAL,
            ChangeKind::Timezone => "timezone",
            ChangeKind::StatedDay => "stated_day",
            ChangeKind::WrapWindow => "wrap_window",
            ChangeKind::State => "state",
            ChangeKind::ServedReset => "served_chars",
            ChangeKind::Key => "key",
            ChangeKind::CharterTold => "charter_told",
        }
    }

    /// **Whether this change is one the search index reads**: the text of an
    /// entry and the focus of a run, and the run coming to be. Only these move
    /// the signal a refresh checks, so a change nothing searches for — a zone, a
    /// reset of what was served — is recorded and moves nothing.
    pub fn moves_the_index(self) -> bool {
        matches!(
            self,
            ChangeKind::Begin
                | ChangeKind::Entry
                | ChangeKind::AmendLast
                | ChangeKind::AmendBeat
                | ChangeKind::Focus
        )
    }
}

/// The name `history` takes on a session to trace the wordings of its journal
/// entries rather than one field.
pub const JOURNAL: &str = "journal";

/// **The changes of one field of a run: a part of them, oldest first, and how
/// many there are.** The part is the newest `most` changes after the newest
/// `skip`, which a caller has already read. A record an older build appended
/// names no field and is in no field's account.
pub fn trace_field(
    field: &str,
    changes: &[SessionChange],
    most: usize,
    skip: usize,
) -> (Vec<SessionChange>, usize) {
    let of_field: Vec<SessionChange> = changes
        .iter()
        .filter(|c| c.changed.is_some_and(|kind| kind.field() == field))
        .cloned()
        .collect();
    let total = of_field.len();
    let end = total.saturating_sub(skip);
    let start = end.saturating_sub(most);
    (of_field[start..end].to_vec(), total)
}

/// **One amended entry in a part of the journal's history**: its id, how many
/// wordings it has in all, and the wordings of it the part carries, oldest
/// first.
pub type EntryPart = (EntryId, usize, Vec<Wording>);

/// **What `history: "journal"` on a run answers with**: a part of the wordings
/// of the entries written more than once, and how many such wordings there are.
///
/// **Newest means written last**, by the moment the store made each wording, so
/// a beat of the run's that was rewritten after a later entry holds the newest
/// wording of all. Two wordings made at one moment keep the run's order. The
/// part is the newest `most` of them after the newest `skip`, which a caller has
/// already read; it comes back grouped by entry, in the run's order, each
/// entry's wordings oldest first.
pub fn trace_journal(
    entries: Vec<(EntryId, Vec<Wording>)>,
    most: usize,
    skip: usize,
) -> (Vec<EntryPart>, usize) {
    let amended: Vec<(EntryId, Vec<Wording>)> = entries
        .into_iter()
        .filter(|(_, wordings)| wordings.len() > 1)
        .collect();
    // Every wording's place in the run's order, oldest write first.
    let mut by_write: Vec<(Timestamp, usize)> = amended
        .iter()
        .flat_map(|(_, wordings)| wordings.iter().map(|w| w.written_at))
        .enumerate()
        .map(|(place, at)| (at, place))
        .collect();
    by_write.sort();
    let total = by_write.len();
    let end = total.saturating_sub(skip);
    let start = end.saturating_sub(most);
    let kept_places: std::collections::BTreeSet<usize> = by_write[start..end]
        .iter()
        .map(|(_, place)| *place)
        .collect();
    let mut part = Vec::new();
    let mut place = 0usize;
    for (entry, wordings) in amended {
        let held = wordings.len();
        let kept: Vec<Wording> = wordings
            .into_iter()
            .enumerate()
            .filter(|(at, _)| kept_places.contains(&(place + at)))
            .map(|(_, wording)| wording)
            .collect();
        place += held;
        if !kept.is_empty() {
            part.push((entry, held, kept));
        }
    }
    (part, total)
}

/// **What changed, as the writer has it.** The value is made from this, so a
/// writer cannot hand over a secret to be kept.
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    /// The run began with this focus.
    Begin(&'a str),
    /// This entry was appended.
    Entry(&'a EntryId),
    /// This entry, the newest, was rewritten.
    AmendLast(&'a EntryId),
    /// This automatic beat was rewritten.
    AmendBeat(&'a EntryId),
    /// The focus was set to this.
    Focus(&'a str),
    /// The zone was set to this, or cleared.
    Timezone(Option<&'a str>),
    /// The stated day was set to this, or cleared.
    StatedDay(Option<Date>),
    /// The wrap window was set to this, or cleared.
    WrapWindow(Option<&'a WrapWindow>),
    /// The run moved to this state.
    State(SessionState),
    /// What was served was set back to nothing from this total.
    ServedReset(u64),
    /// The key changed.
    Key,
    /// The charter was told at this moment.
    CharterTold(Timestamp),
}

impl Change<'_> {
    /// Which change this is.
    pub fn kind(&self) -> ChangeKind {
        match self {
            Change::Begin(_) => ChangeKind::Begin,
            Change::Entry(_) => ChangeKind::Entry,
            Change::AmendLast(_) => ChangeKind::AmendLast,
            Change::AmendBeat(_) => ChangeKind::AmendBeat,
            Change::Focus(_) => ChangeKind::Focus,
            Change::Timezone(_) => ChangeKind::Timezone,
            Change::StatedDay(_) => ChangeKind::StatedDay,
            Change::WrapWindow(_) => ChangeKind::WrapWindow,
            Change::State(_) => ChangeKind::State,
            Change::ServedReset(_) => ChangeKind::ServedReset,
            Change::Key => ChangeKind::Key,
            Change::CharterTold(_) => ChangeKind::CharterTold,
        }
    }

    /// **What is kept of the change**, and nothing a reader must not have.
    pub fn value(&self) -> Option<String> {
        match self {
            Change::Begin(focus) | Change::Focus(focus) => Some((*focus).to_string()),
            Change::Entry(id) | Change::AmendLast(id) | Change::AmendBeat(id) => {
                Some(id.to_string())
            }
            Change::Timezone(zone) => zone.map(str::to_string),
            Change::StatedDay(day) => day.map(|day| day.to_string()),
            Change::WrapWindow(window) => window.map(|window| match window {
                WrapWindow::Offered(_) => "offered".to_string(),
                WrapWindow::Open(_) => "open".to_string(),
            }),
            Change::State(state) => Some(state.as_token().to_string()),
            Change::ServedReset(total) => Some(total.to_string()),
            Change::CharterTold(at) => Some(at.to_string()),
            Change::Key => None,
        }
    }
}

/// **The moment of the write in flight, shared by whatever records it.** A layer
/// above the store that also records every change — the fold image — takes the
/// moment once and holds it here for the length of its write; the store beneath
/// reads it instead of its own clock, so the record the layer makes and the one
/// the store makes are one record rather than two with different times. A store
/// that is written to with nothing held stamps on its own clock.
///
/// Wiring hands one of these to the store and to the layer above it. It holds no
/// moment between writes: the guard that holds it clears it however the write
/// ends.
#[derive(Clone, Default)]
pub struct WriteStamp(std::sync::Arc<std::sync::Mutex<Option<Timestamp>>>);

impl WriteStamp {
    /// A stamp holding nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// **Hold `at` as the moment of the write that follows**, until the guard
    /// drops. A hold inside another puts the outer one back when it ends.
    #[must_use = "the moment is held only while the guard lives"]
    pub fn hold(&self, at: Timestamp) -> HeldStamp {
        let before = self.0.lock().expect("stamp lock").replace(at);
        HeldStamp {
            stamp: self.clone(),
            before,
        }
    }

    /// The moment a write in flight holds, if one does.
    pub fn current(&self) -> Option<Timestamp> {
        *self.0.lock().expect("stamp lock")
    }
}

/// The moment held by [`WriteStamp::hold`], released when it drops.
pub struct HeldStamp {
    stamp: WriteStamp,
    before: Option<Timestamp>,
}

impl Drop for HeldStamp {
    fn drop(&mut self) {
        *self.stamp.0.lock().expect("stamp lock") = self.before;
    }
}

/// **One change of a run, as it happened.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChange {
    /// Which change this is for the run, counting from one.
    pub ordinal: usize,
    /// What changed. `None` for a record an older build appended, which said
    /// only that the run was written.
    pub changed: Option<ChangeKind>,
    /// What was kept of it, which not every change has.
    pub value: Option<String>,
    /// When the store made the record. `None` for a record an older build
    /// appended, which kept no moment.
    pub written_at: Option<Timestamp>,
}

/// **The next change record of a run**: the change, numbered one past
/// `highest_held`, the highest ordinal of the records the run already has, and
/// stamped `at`. A run that holds none numbers its first change one.
pub fn next_change(
    highest_held: Option<usize>,
    change: Change<'_>,
    at: Timestamp,
) -> SessionChange {
    SessionChange {
        ordinal: highest_held.unwrap_or(0) + 1,
        changed: Some(change.kind()),
        value: change.value(),
        written_at: Some(at),
    }
}

/// **One wording of a journal entry.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wording {
    /// Which wording this is for the entry, counting from one.
    pub revision: usize,
    /// The text the entry held.
    pub text: String,
    /// The moment the entry was marked as moved, for a wording that moved it.
    pub touched: Option<Timestamp>,
    /// When the store made the record.
    pub written_at: Timestamp,
}

/// **Every record of every run, for the image that answers the history reads
/// from memory.** The changes of each run that has any, oldest first, and the
/// wordings of each entry that has any, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionHistories {
    /// The changes of each run.
    pub changes: Vec<(SessionId, Vec<SessionChange>)>,
    /// The wordings of each entry.
    pub wordings: Vec<(SessionId, EntryId, Vec<Wording>)>,
}

/// **The next wording of an entry**: numbered one past `highest_held`, the
/// revision of the newest wording the entry already has, as [`next_change`]
/// numbers a change.
pub fn next_wording(
    highest_held: Option<usize>,
    text: &str,
    touched: Option<Timestamp>,
    at: Timestamp,
) -> Wording {
    Wording {
        revision: highest_held.unwrap_or(0) + 1,
        text: text.to_string(),
        touched,
        written_at: at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(1_790_000_000 + second).expect("a valid instant")
    }

    /// **The number is one past the highest held**, and one for a run that holds
    /// none.
    #[test]
    fn a_change_is_numbered_one_past_the_highest_held() {
        let change = Change::Focus("reading");
        assert_eq!(next_change(None, change, at(0)).ordinal, 1);
        assert_eq!(next_change(Some(1), change, at(0)).ordinal, 2);
        assert_eq!(next_change(Some(5), change, at(0)).ordinal, 6);
    }

    /// **A wording is numbered the same way.**
    #[test]
    fn a_wording_is_numbered_one_past_the_highest_held() {
        assert_eq!(next_wording(None, "first", None, at(0)).revision, 1);
        assert_eq!(next_wording(Some(2), "third", None, at(0)).revision, 3);
        let moved = next_wording(Some(1), "second", Some(at(5)), at(6));
        assert_eq!(moved.text, "second");
        assert_eq!(moved.touched, Some(at(5)));
        assert_eq!(moved.written_at, at(6));
    }

    /// **Each change keeps what it should and the key keeps nothing.** The window
    /// is recorded as offered or open, and the code that opens it is not in the
    /// value anywhere.
    #[test]
    fn what_a_change_keeps_of_itself() {
        let entry = EntryId("e1".into());
        let code = "ab12cd34".to_string();
        let kept = |change: Change<'_>| {
            let made = next_change(None, change, at(1));
            (made.changed, made.value, made.written_at)
        };
        assert_eq!(
            kept(Change::Focus("reading")),
            (Some(ChangeKind::Focus), Some("reading".into()), Some(at(1)))
        );
        assert_eq!(
            kept(Change::Entry(&entry)),
            (Some(ChangeKind::Entry), Some("e1".into()), Some(at(1)))
        );
        assert_eq!(
            kept(Change::Timezone(None)),
            (Some(ChangeKind::Timezone), None, Some(at(1)))
        );
        assert_eq!(
            kept(Change::State(SessionState::Wrapped)).1,
            Some("wrapped".into())
        );
        assert_eq!(kept(Change::ServedReset(42)).1, Some("42".into()));
        assert_eq!(
            kept(Change::Key),
            (Some(ChangeKind::Key), None, Some(at(1)))
        );
        let offered = WrapWindow::Offered(code.clone());
        let open = WrapWindow::Open(code.clone());
        assert_eq!(
            kept(Change::WrapWindow(Some(&offered))).1,
            Some("offered".into())
        );
        assert_eq!(kept(Change::WrapWindow(Some(&open))).1, Some("open".into()));
        assert_eq!(kept(Change::WrapWindow(None)).1, None);
    }

    /// **A moment is held for the length of a write and gone after it**, and a
    /// hold inside another gives the outer moment back.
    #[test]
    fn a_held_moment_is_current_until_its_guard_drops() {
        let stamp = WriteStamp::new();
        assert_eq!(stamp.current(), None, "nothing is held to begin with");
        let outer = stamp.hold(at(1));
        assert_eq!(stamp.current(), Some(at(1)));
        {
            let _inner = stamp.hold(at(2));
            assert_eq!(stamp.current(), Some(at(2)));
        }
        assert_eq!(stamp.current(), Some(at(1)), "the outer moment is back");
        drop(outer);
        assert_eq!(stamp.current(), None, "nothing is held after the write");
    }

    /// **A clone shares the moment**, which is how the store and the layer above
    /// it see the same one.
    #[test]
    fn a_clone_of_the_stamp_sees_the_same_moment() {
        let stamp = WriteStamp::new();
        let seen_by_the_store = stamp.clone();
        let _held = stamp.hold(at(5));
        assert_eq!(seen_by_the_store.current(), Some(at(5)));
    }

    /// **A change belongs to the field a caller names it by**, and the journal
    /// entries' three kinds are changes of the journal.
    #[test]
    fn a_change_belongs_to_the_field_a_caller_names_it_by() {
        let fields: Vec<&str> = ChangeKind::ALL.into_iter().map(ChangeKind::field).collect();
        assert_eq!(
            fields,
            [
                "focus",
                "journal",
                "journal",
                "journal",
                "focus",
                "timezone",
                "stated_day",
                "wrap_window",
                "state",
                "served_chars",
                "key",
                "charter_told"
            ]
        );
    }

    /// **A field's account keeps its own changes and the newest of them.** The
    /// total says how many there are, and a record of an older build is in none.
    #[test]
    fn a_fields_account_is_its_own_changes_cut_to_the_newest() {
        let made = |kind: Option<ChangeKind>, ordinal: usize| SessionChange {
            ordinal,
            changed: kind,
            value: Some(format!("v{ordinal}")),
            written_at: None,
        };
        let changes = vec![
            made(None, 1),
            made(Some(ChangeKind::Begin), 2),
            made(Some(ChangeKind::Timezone), 3),
            made(Some(ChangeKind::Focus), 4),
            made(Some(ChangeKind::Focus), 5),
        ];
        let (all, total) = trace_field("focus", &changes, 20, 0);
        assert_eq!(all.iter().map(|c| c.ordinal).collect::<Vec<_>>(), [2, 4, 5]);
        assert_eq!(total, 3);
        let (newest, total) = trace_field("focus", &changes, 2, 0);
        assert_eq!(newest.iter().map(|c| c.ordinal).collect::<Vec<_>>(), [4, 5]);
        assert_eq!(total, 3, "the cut says how many exist");
        let (older, _) = trace_field("focus", &changes, 2, 2);
        assert_eq!(older.iter().map(|c| c.ordinal).collect::<Vec<_>>(), [2]);
        assert_eq!(trace_field("focus", &changes, 2, 9), (Vec::new(), 3));
        assert_eq!(trace_field("colour", &changes, 20, 0), (Vec::new(), 0));
    }

    /// **The journal's account is the wordings of the entries written more than
    /// once**, read newest first in parts and grouped by entry.
    #[test]
    fn the_journals_account_is_the_wordings_of_the_entries_written_more_than_once() {
        let wording = |text: &str, revision: usize, written: i64| Wording {
            revision,
            text: text.to_string(),
            touched: None,
            written_at: at(written),
        };
        let entries = vec![
            (EntryId("a".into()), vec![wording("only", 1, 1)]),
            (
                EntryId("b".into()),
                vec![wording("one", 1, 2), wording("two", 2, 3)],
            ),
            (
                EntryId("c".into()),
                vec![wording("x", 1, 4), wording("y", 2, 5)],
            ),
        ];
        let texts = |part: &[EntryPart]| {
            part.iter()
                .flat_map(|(id, held, wordings)| {
                    wordings
                        .iter()
                        .map(move |w| format!("{}/{held}:{}", id.as_str(), w.text))
                })
                .collect::<Vec<_>>()
        };
        let (shown, total) = trace_journal(entries.clone(), 20, 0);
        assert_eq!(texts(&shown), ["b/2:one", "b/2:two", "c/2:x", "c/2:y"]);
        assert_eq!(total, 4, "the count is of wordings");
        let (newest, total) = trace_journal(entries.clone(), 3, 0);
        assert_eq!(texts(&newest), ["b/2:two", "c/2:x", "c/2:y"]);
        assert_eq!(total, 4);
        let (older, _) = trace_journal(entries.clone(), 3, 3);
        assert_eq!(texts(&older), ["b/2:one"], "the skip reads on");
        assert_eq!(trace_journal(entries, 3, 4).0, Vec::new());
    }

    /// **Newest is the wording written last, whichever entry holds it.** Entry
    /// `b` is earlier in the run and was rewritten after `c` was, so a part of one
    /// holds `b`'s second wording, and the part after it holds `c`'s.
    #[test]
    fn the_newest_wording_is_the_one_written_last_from_whichever_entry() {
        let wording = |text: &str, revision: usize, written: i64| Wording {
            revision,
            text: text.to_string(),
            touched: None,
            written_at: at(written),
        };
        let entries = vec![
            (
                EntryId("b".into()),
                vec![wording("one", 1, 1), wording("two", 2, 5)],
            ),
            (
                EntryId("c".into()),
                vec![wording("x", 1, 2), wording("y", 2, 3)],
            ),
        ];
        let texts = |part: &[EntryPart]| {
            part.iter()
                .flat_map(|(_, _, wordings)| wordings.iter().map(|w| w.text.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&trace_journal(entries.clone(), 1, 0).0), ["two"]);
        assert_eq!(texts(&trace_journal(entries.clone(), 1, 1).0), ["y"]);
        assert_eq!(texts(&trace_journal(entries.clone(), 1, 2).0), ["x"]);
        assert_eq!(texts(&trace_journal(entries, 1, 3).0), ["one"]);
    }

    /// **A token round-trips, and only the five the index reads move it.**
    #[test]
    fn a_token_round_trips_and_only_five_kinds_move_the_index() {
        for kind in ChangeKind::ALL {
            assert_eq!(ChangeKind::from_token(kind.as_token()), Some(kind));
        }
        assert_eq!(ChangeKind::from_token("nonesuch"), None);
        let moving: Vec<&str> = ChangeKind::ALL
            .into_iter()
            .filter(|kind| kind.moves_the_index())
            .map(ChangeKind::as_token)
            .collect();
        assert_eq!(
            moving,
            ["begin", "entry", "amend_last", "amend_beat", "focus"]
        );
    }
}
