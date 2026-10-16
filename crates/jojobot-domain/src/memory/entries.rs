//! **The entries of a list: claims on it, in an order the server minted.**
//!
//! An entry is one claim on a thing of kind `list`: a few words, or a claim
//! whose edge points at a thing. It carries an order key in its own field
//! `order`, spelled as a document section's is, and the keys sort as text
//! ([`super::order`]). A caller never picks a key: it says where the entry goes
//! (the end, the front, or after a named entry) and [`bounds`] names the two keys
//! the new one falls between.
//!
//! **Which entries are open is computed from what is on the list**, never
//! stored on an entry: an entry that is archived is not open. The state each
//! open entry reads as is [`OPEN`]; later slices compute others from tick
//! records.

use super::{EntityKind, document};
use super::{Fact, FactAddress, FactStatus};

/// The word an entry reads as while nothing has acted on it.
pub const OPEN: &str = "open";

/// The word that places an entry at the front.
pub const FIRST: &str = "first";

/// **Where a new entry goes.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// After every entry there is. The default.
    End,
    /// Before every entry there is.
    First,
    /// Straight after the named entry.
    After(FactAddress),
}

/// Why a placement cannot be honoured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlaceError {
    /// `after` is neither `first` nor an address.
    #[error("'{0}' is neither the word `first` nor the address of an entry")]
    NotAPlace(String),
    /// `after` names an address that is not an open entry of this list.
    #[error("'{named}' is not an open entry of this list")]
    NotAnEntry {
        /// What the caller named.
        named: String,
        /// The addresses of the entries there are, in order.
        entries: Vec<String>,
    },
    /// No key can be minted there.
    #[error("no room is left to place the entry there: {0}")]
    NoRoom(String),
}

/// **Whether the records on a thing of this kind are placed by an order key the
/// server mints**: a document's sections and a list's entries. The one place
/// that says so, so a gate that treats the order key as layout, and not as words
/// somebody said, asks here.
pub fn places_its_records(kind: Option<EntityKind>) -> bool {
    matches!(kind, Some(EntityKind::DOCUMENT | EntityKind::LIST))
}

/// **Whether a fact is placed by an order key the server minted**: a claim on a
/// document or a list that carries the key. The one test of it, so a gate that
/// treats such a record as layout asks here; an ordinary claim about a thing is
/// not placed.
pub fn is_placed(fact: &Fact) -> bool {
    places_its_records(fact.home.kind()) && fact.fields.contains_key(document::ORDER)
}

/// **Whether a fact is an entry of the list it sits on**: a placed claim on a
/// list.
pub fn is_entry(fact: &Fact) -> bool {
    is_placed(fact) && fact.home.kind() == Some(EntityKind::LIST)
}

/// **The open entries of a list, in order.** A claim on the list that is
/// active and carries an order key. Ordered by that key, then by the claim's id,
/// so two entries that ever carried one key still read in a fixed order.
pub fn open_entries(facts: &[Fact]) -> Vec<&Fact> {
    placed_in_order(facts)
        .into_iter()
        .filter(|fact| fact.status == FactStatus::Active && is_entry(fact))
        .collect()
}

/// **The records an order key places, in order**: every record that carries the
/// key, whatever its status or the kind its home names, by key and then by id. A
/// claim with no key is not placed. This is the one reading of that order; the
/// readers narrow it, so the open entries, a merge's last key and the entries a
/// merge moves cannot disagree about what the order is.
///
/// The kind of the home is not consulted: a store holds a record's home as the
/// thing's permanent id, which names no kind, and has already judged what the
/// thing is.
pub fn placed_in_order(facts: &[Fact]) -> Vec<&Fact> {
    let mut placed: Vec<&Fact> = facts
        .iter()
        .filter(|fact| fact.fields.contains_key(document::ORDER))
        .collect();
    placed.sort_by(|a, b| {
        (&a.fields[document::ORDER], &a.id.0).cmp(&(&b.fields[document::ORDER], &b.id.0))
    });
    placed
}

/// **Place a folded list's entries after the survivor's last entry.**
///
/// `survivor` and `folded` are the two lists' records. The folded entries that
/// stand keep their own order among themselves and take keys minted after the
/// greatest order key the survivor holds, archived entries included, so an entry
/// restored later cannot land among them. A claim on the folded list that is no
/// entry is not moved.
///
/// **Records are picked by the order key they carry, not by the kind of the
/// thing that holds them.** A store runs this while it holds a record's home as
/// the thing's permanent id, which names no kind, and it has already judged that
/// both sides are lists; [`is_entry`] reads the kind and would pick nothing there.
///
/// The keys are the server's to mint, with the spread a document's merge uses;
/// this is the one place a list merge mints them, called by every store, and
/// [`mint`] is the one place a single entry is placed.
pub fn append_after(
    survivor: &[Fact],
    folded: &[Fact],
) -> Result<Vec<document::Relocated>, super::order::OrderError> {
    let last = placed_in_order(survivor)
        .last()
        .map(|fact| fact.fields[document::ORDER].as_str());
    let standing: Vec<&Fact> = placed_in_order(folded)
        .into_iter()
        .filter(|fact| fact.status == FactStatus::Active)
        .collect();
    let orders = super::order::spread(last, None, standing.len())?;
    Ok(standing
        .into_iter()
        .zip(orders)
        .map(|(fact, order)| document::Relocated {
            fact: fact.id.clone(),
            order,
            continues: None,
        })
        .collect())
}

/// The record field of a tick that names the entry it acts on: the entry's local
/// id on the list. Stored by local id so a rename of the list moves nothing.
pub const TICK_ENTRY: &str = "tick_entry";
/// The record field of a tick that holds the act, one of [`Act`]'s words.
pub const TICK_ACT: &str = "tick_act";
/// The record field of a tick that holds the run that ticked, by its public name.
pub const TICK_SESSION: &str = "tick_session";

/// The words an entry's computed state reads as.
pub const DONE: &str = "done";
/// The state of an entry that no longer applies. It is never `done`.
pub const DROPPED: &str = "dropped";

/// **What a tick does on this list**: the list's `tick` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// A done entry stays in view at the bottom, in the order things were done.
    Sink,
    /// A done entry leaves the open view and stays in the history.
    Leave,
    /// A tick records the turn and the entry goes to the end, still open.
    Back,
}

impl Setting {
    /// The setting a list's folded fields name. A list that names none is read as
    /// a checklist.
    pub fn of(fields: &std::collections::BTreeMap<String, String>) -> Setting {
        match fields.get("tick").map(|word| word.trim()) {
            Some("leave") => Setting::Leave,
            Some("back") => Setting::Back,
            _ => Setting::Sink,
        }
    }

    /// The word the setting is spelled with.
    pub fn as_token(self) -> &'static str {
        match self {
            Setting::Sink => "sink",
            Setting::Leave => "leave",
            Setting::Back => "back",
        }
    }
}

/// **What a tick records.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// The entry is done; on a `back` list the entry takes its turn.
    Done,
    /// A done or dropped entry is open again.
    Undone,
    /// The entry takes its turn and goes to the end. Only a `back` list.
    Rotated,
    /// The entry no longer applies.
    Dropped,
}

impl Act {
    /// Every act, in the order they are named to a caller.
    pub const ALL: [Act; 4] = [Act::Done, Act::Undone, Act::Rotated, Act::Dropped];

    /// The act a word names.
    pub fn of(word: &str) -> Option<Act> {
        Act::ALL
            .into_iter()
            .find(|act| act.as_token() == word.trim())
    }

    /// The word the act is spelled with.
    pub fn as_token(self) -> &'static str {
        match self {
            Act::Done => "done",
            Act::Undone => "undone",
            Act::Rotated => "rotated",
            Act::Dropped => "dropped",
        }
    }
}

/// **Whether a fact is a tick on a list**: a claim on a list that names an entry
/// and an act and carries no order key. A tick is a record beside the entries,
/// never one of them.
pub fn is_tick(fact: &Fact) -> bool {
    fact.home.kind() == Some(EntityKind::LIST)
        && fact.fields.contains_key(TICK_ENTRY)
        && fact.fields.contains_key(TICK_ACT)
        && !fact.fields.contains_key(document::ORDER)
}

/// The place a record has in the order the list wrote them, from its id.
fn sequence(fact: &Fact) -> u64 {
    fact.id
        .0
        .trim_start_matches('f')
        .parse()
        .unwrap_or(u64::MAX)
}

/// **The active ticks on a list, oldest first.** A tick taken back (archived or
/// retracted) is no longer a tick; one naming an act this build does not know is
/// left out.
pub fn active_ticks(facts: &[Fact]) -> Vec<(&Fact, Act)> {
    let mut ticks: Vec<(&Fact, Act)> = facts
        .iter()
        .filter(|fact| fact.status == FactStatus::Active && is_tick(fact))
        .filter_map(|fact| Some((fact, Act::of(&fact.fields[TICK_ACT])?)))
        .collect();
    ticks.sort_by_key(|(fact, _)| sequence(fact));
    ticks
}

/// One entry of the open view with its computed state.
#[derive(Debug)]
pub struct Shown<'a> {
    /// The entry, whose words, author and provenance no tick ever changes.
    pub fact: &'a Fact,
    /// `open`, or [`DONE`] on a `sink` list.
    pub state: &'static str,
}

/// **A list read: its entries in the order the ticks leave them, each with a
/// state computed from the tick records, and what the open view left out.**
#[derive(Debug)]
pub struct View<'a> {
    /// The entries in view, in order.
    pub entries: Vec<Shown<'a>>,
    /// Done entries a `leave` list keeps out of view.
    pub done_left_out: usize,
    /// Entries dropped, on any setting.
    pub dropped_left_out: usize,
    /// Every active entry's computed state, by the entry's local id: `open`,
    /// [`DONE`] or [`DROPPED`]. What a tick is checked against.
    pub states: std::collections::BTreeMap<String, &'static str>,
}

/// **The entries of a list as its ticks leave them.** Nothing here is stored on an
/// entry: the latest tick naming an entry decides its state, and the order is the
/// keys, then the turns.
pub fn view(facts: &[Fact], setting: Setting) -> View<'_> {
    #[derive(Default, Clone, Copy)]
    struct Standing {
        done_at: Option<usize>,
        turn_at: Option<usize>,
        dropped: bool,
    }
    let entries = open_entries(facts);
    let mut standing: std::collections::HashMap<&str, Standing> = entries
        .iter()
        .map(|entry| (entry.id.0.as_str(), Standing::default()))
        .collect();
    for (at, (tick, act)) in active_ticks(facts).into_iter().enumerate() {
        let Some(now) = standing.get_mut(tick.fields[TICK_ENTRY].trim()) else {
            continue;
        };
        match (act, setting) {
            (Act::Done, Setting::Back) | (Act::Rotated, _) => {
                now.turn_at = Some(at);
                now.done_at = None;
                now.dropped = false;
            }
            (Act::Done, _) => {
                now.done_at = Some(at);
                now.dropped = false;
            }
            (Act::Undone, _) => {
                now.done_at = None;
                now.dropped = false;
            }
            (Act::Dropped, _) => {
                now.dropped = true;
                now.done_at = None;
            }
        }
    }
    let mut open: Vec<(&Fact, Standing)> = Vec::new();
    let mut done: Vec<(&Fact, usize)> = Vec::new();
    let mut seen = View {
        entries: Vec::new(),
        done_left_out: 0,
        dropped_left_out: 0,
        states: Default::default(),
    };
    for entry in entries {
        let now = standing[entry.id.0.as_str()];
        seen.states.insert(
            entry.id.0.clone(),
            match (now.dropped, now.done_at) {
                (true, _) => DROPPED,
                (false, Some(_)) => DONE,
                (false, None) => OPEN,
            },
        );
        match (now.dropped, now.done_at) {
            (true, _) => seen.dropped_left_out += 1,
            (false, Some(at)) if setting == Setting::Sink => done.push((entry, at)),
            (false, Some(_)) => seen.done_left_out += 1,
            (false, None) => open.push((entry, now)),
        }
    }
    // Entries nobody has turned keep their keys' order; turned ones follow, the
    // oldest turn first, so a full circuit of turns ends where it began.
    open.sort_by_key(|(entry, now)| {
        (
            now.turn_at.map_or(0, |at| at + 1),
            entry.fields[document::ORDER].clone(),
            entry.id.0.clone(),
        )
    });
    done.sort_by_key(|(_, at)| *at);
    seen.entries = open
        .into_iter()
        .map(|(fact, _)| Shown { fact, state: OPEN })
        .chain(
            done.into_iter()
                .map(|(fact, _)| Shown { fact, state: DONE }),
        )
        .collect();
    seen
}

/// **Why a tick cannot be recorded on this list as it stands**, or nothing. The
/// act is checked against the list's setting and the entry's state now.
pub fn refuse_tick(setting: Setting, state: &str, act: Act) -> Option<String> {
    if act == Act::Rotated && setting != Setting::Back {
        return Some(format!(
            "`rotated` takes an entry's turn on a list whose tick setting is back, and this list's \
             is {}. Send `done` to tick an entry here",
            setting.as_token()
        ));
    }
    if act == Act::Undone && setting == Setting::Back {
        return Some(
            "a turn on a back list is not undone: the entry took it and went to the end. Send \
             `done` on it again when its next turn comes"
                .to_string(),
        );
    }
    if state == DROPPED && act != Act::Undone {
        return Some(
            "that entry is dropped, so it no longer applies. Send `undone` to bring it back, then \
             tick it"
                .to_string(),
        );
    }
    None
}

/// **The placement a caller asked for**: nothing is the end, `first` the front,
/// anything else an entry's address.
pub fn parse_placement(after: Option<&str>) -> Result<Placement, PlaceError> {
    let Some(named) = after.map(str::trim).filter(|named| !named.is_empty()) else {
        return Ok(Placement::End);
    };
    if named == FIRST {
        return Ok(Placement::First);
    }
    FactAddress::parse(named)
        .map(Placement::After)
        .map_err(|_| PlaceError::NotAPlace(named.to_string()))
}

/// **The keys a new entry falls between**, as `(lower, upper)`, from the entries
/// the list has now. `None` is the start or the end of the order.
pub fn bounds(
    entries: &[&Fact],
    placement: &Placement,
) -> Result<(Option<String>, Option<String>), PlaceError> {
    let order = |fact: &Fact| fact.fields[document::ORDER].clone();
    match placement {
        Placement::End => Ok((entries.last().map(|fact| order(fact)), None)),
        Placement::First => Ok((None, entries.first().map(|fact| order(fact)))),
        Placement::After(address) => {
            let Some(at) = entries
                .iter()
                .position(|fact| fact.home == address.home && fact.id == address.local)
            else {
                return Err(PlaceError::NotAnEntry {
                    named: format!("{}#{}", address.home.as_str(), address.local.0),
                    entries: entries
                        .iter()
                        .map(|fact| fact.address().to_string())
                        .collect(),
                });
            };
            Ok((
                Some(order(entries[at])),
                entries.get(at + 1).map(|fact| order(fact)),
            ))
        }
    }
}

/// **The key for a new entry placed there.**
pub fn mint(entries: &[&Fact], placement: &Placement) -> Result<String, PlaceError> {
    let (lower, upper) = bounds(entries, placement)?;
    super::order::between(lower.as_deref(), upper.as_deref())
        .map_err(|e| PlaceError::NoRoom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{EntityId, FactId};

    fn entry(local: &str, order: &str, status: FactStatus) -> Fact {
        // A handle's kind is read from the loaded set, which another case in
        // this process may have unloaded.
        crate::memory::kinds::load_shipped();
        let home = EntityId("list:packing".into());
        Fact {
            id: FactId(local.into()),
            home: home.clone(),
            subject: home,
            content: format!("entry {local}"),
            details: None,
            provenance: crate::memory::Provenance::Testimony,
            standing: crate::memory::Standing::Open,
            status,
            recorded_at: jiff::civil::date(2026, 10, 16),
            happened_at: None,
            happened_through: None,
            inserted_at: None,
            stale_after: None,
            receipt: None,
            edge: None,
            fields: [(document::ORDER.to_string(), order.to_string())]
                .into_iter()
                .collect(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            sets: Default::default(),
        }
    }

    /// **Open entries read in key order, whatever order they were written in,
    /// and an archived entry or a claim with no key is not one.**
    #[test]
    fn open_entries_read_in_key_order_and_leave_out_the_archived_and_the_keyless() {
        let mut keyless = entry("f4", "5", FactStatus::Active);
        keyless.fields.remove(document::ORDER);
        let facts = vec![
            entry("f1", "8", FactStatus::Active),
            entry("f2", "4", FactStatus::Active),
            entry("f3", "6", FactStatus::Archived),
            keyless,
        ];
        let ids: Vec<&str> = open_entries(&facts)
            .iter()
            .map(|f| f.id.0.as_str())
            .collect();
        assert_eq!(ids, ["f2", "f1"]);
    }

    /// **Each placement lands between the keys it should, and a key minted there
    /// sorts there.** The end follows the last entry, the front precedes the first
    /// and `after` sits between an entry and the one that follows it.
    #[test]
    fn a_placement_mints_a_key_between_its_neighbours() {
        let facts = vec![
            entry("f1", "4", FactStatus::Active),
            entry("f2", "8", FactStatus::Active),
        ];
        let open = open_entries(&facts);
        let end = mint(&open, &Placement::End).unwrap();
        assert!(end.as_str() > "8", "{end}");
        let first = mint(&open, &Placement::First).unwrap();
        assert!(first.as_str() < "4", "{first}");
        let after_first = mint(
            &open,
            &Placement::After(FactAddress::new(
                EntityId("list:packing".into()),
                FactId("f1".into()),
            )),
        )
        .unwrap();
        assert!(
            after_first.as_str() > "4" && after_first.as_str() < "8",
            "{after_first}"
        );
        let after_last = mint(
            &open,
            &Placement::After(FactAddress::new(
                EntityId("list:packing".into()),
                FactId("f2".into()),
            )),
        )
        .unwrap();
        assert!(after_last.as_str() > "8", "{after_last}");
        assert_eq!(
            mint(&[], &Placement::End).unwrap(),
            order_of_an_empty_list()
        );
    }

    fn order_of_an_empty_list() -> String {
        crate::memory::order::between(None, None).unwrap()
    }

    /// **`after` must name an open entry of this list, and the refusal lists the
    /// entries there are.**
    #[test]
    fn after_naming_something_else_is_refused_with_the_entries() {
        let facts = vec![entry("f1", "4", FactStatus::Active)];
        let open = open_entries(&facts);
        let miss = Placement::After(FactAddress::new(
            EntityId("list:packing".into()),
            FactId("f9".into()),
        ));
        match bounds(&open, &miss) {
            Err(PlaceError::NotAnEntry { entries, .. }) => {
                assert_eq!(entries, ["list:packing#f1"]);
            }
            other => panic!("expected a refusal naming the entries: {other:?}"),
        }
        assert_eq!(parse_placement(None), Ok(Placement::End));
        assert_eq!(parse_placement(Some("first")), Ok(Placement::First));
        assert!(matches!(
            parse_placement(Some("nonsense")),
            Err(PlaceError::NotAPlace(_))
        ));
    }

    /// **Another session changing only an entry's order key is no rewrite of what
    /// was said**, exactly as for a document's section: the key says where the
    /// entry sits and never what it says. The same patch on a claim on anything
    /// else is a rewrite of testimony and is refused, which is what makes the
    /// first answer mean something.
    #[test]
    fn a_change_of_only_the_order_key_of_an_entry_is_not_a_rewrite_of_testimony() {
        let patch = || crate::memory::FactPatch {
            fields: [(document::ORDER.to_string(), "9".to_string())]
                .into_iter()
                .collect(),
            session: Some("another".to_string()),
            ..Default::default()
        };
        let on_a_list = entry("f1", "4", FactStatus::Active);
        let settled = crate::memory::settle_rewrite(
            &on_a_list.address(),
            &on_a_list,
            patch(),
            Some("writer"),
            &[],
        );
        assert!(settled.is_ok(), "{settled:?}");

        let mut elsewhere = entry("f1", "4", FactStatus::Active);
        elsewhere.home = EntityId("thing:gamma".into());
        elsewhere.subject = elsewhere.home.clone();
        let refused = crate::memory::settle_rewrite(
            &elsewhere.address(),
            &elsewhere,
            patch(),
            Some("writer"),
            &[],
        );
        assert!(
            matches!(
                refused,
                Err(crate::memory::MemoryError::TestimonyRewritten { .. })
            ),
            "{refused:?}"
        );
    }

    /// **One predicate says a record is placed by an order key**: a claim on a
    /// document or a list that carries the key. The two narrower readers go
    /// through it, so a section is never an entry and an entry never a section,
    /// and a claim that carries no key, or sits on anything else, is neither.
    #[test]
    fn a_placed_record_is_a_claim_with_a_key_on_a_document_or_a_list() {
        let on = |home: &str, keyed: bool| {
            let mut fact = entry("f1", "4", FactStatus::Active);
            fact.home = EntityId(home.into());
            fact.subject = fact.home.clone();
            if !keyed {
                fact.fields.remove(document::ORDER);
            }
            fact
        };
        let section = on("document:homer", true);
        let list_entry = on("list:packing", true);
        let truth = [
            // (fact, placed, section, entry)
            (&section, true, true, false),
            (&list_entry, true, false, true),
            (&on("document:homer", false), false, false, false),
            (&on("list:packing", false), false, false, false),
            (&on("person:homer", true), false, false, false),
        ];
        for (fact, placed, is_a_section, is_an_entry) in truth {
            assert_eq!(is_placed(fact), placed, "placed: {}", fact.home);
            assert_eq!(
                document::is_section(fact),
                is_a_section,
                "section: {}",
                fact.home
            );
            assert_eq!(is_entry(fact), is_an_entry, "entry: {}", fact.home);
        }
    }

    /// **A merge places the folded list's entries after the survivor's last, in
    /// their own order.** The survivor's last key counts archived entries too, so
    /// an entry restored later cannot land among the folded ones. An archived
    /// folded entry and a claim that is no entry are not moved.
    #[test]
    fn a_merge_places_the_folded_entries_after_the_survivors_last_in_their_own_order() {
        let survivor = vec![
            entry("f1", "4", FactStatus::Active),
            entry("f2", "8", FactStatus::Active),
            entry("f3", "k", FactStatus::Archived),
        ];
        let mut remark = entry("f7", "2", FactStatus::Active);
        remark.fields.remove(document::ORDER);
        let folded = vec![
            entry("f5", "6", FactStatus::Active),
            entry("f4", "4", FactStatus::Active),
            entry("f6", "5", FactStatus::Archived),
            remark,
        ];
        let placed = append_after(&survivor, &folded).expect("room");
        let ids: Vec<&str> = placed.iter().map(|place| place.fact.0.as_str()).collect();
        assert_eq!(ids, ["f4", "f5"], "own order, no archived entry, no remark");
        let keys: Vec<&str> = placed.iter().map(|place| place.order.as_str()).collect();
        assert!(keys[0] > "k" && keys[0] < keys[1], "{keys:?}");
        assert!(placed.iter().all(|place| place.continues.is_none()));
    }

    /// **One function puts the records an order key places in order.** Every
    /// record that carries a key, whatever its status or the kind its home names,
    /// by key and then id; a claim with no key is not placed. The readers narrow
    /// it, so a list's open entries, a merge's last key and its folded entries
    /// cannot come to disagree about what the order is.
    #[test]
    fn placed_records_come_in_key_order_with_the_archived_and_without_the_keyless() {
        let mut keyless = entry("f4", "5", FactStatus::Active);
        keyless.fields.remove(document::ORDER);
        let facts = vec![
            entry("f2", "8", FactStatus::Active),
            entry("f1", "8", FactStatus::Active),
            entry("f3", "k", FactStatus::Archived),
            entry("f5", "4", FactStatus::Active),
            keyless,
        ];
        let ids: Vec<&str> = placed_in_order(&facts)
            .iter()
            .map(|fact| fact.id.0.as_str())
            .collect();
        assert_eq!(ids, ["f5", "f1", "f2", "f3"], "key, then id; archived kept");
        let open: Vec<&str> = open_entries(&facts)
            .iter()
            .map(|fact| fact.id.0.as_str())
            .collect();
        assert_eq!(open, ["f5", "f1", "f2"], "the open entries narrow it");
    }

    /// A tick record on the list, as a capture with a `tick` writes it.
    fn tick(local: &str, entry: &str, act: &str) -> Fact {
        let mut fact = entry_fact(local, "unused");
        fact.fields.remove(document::ORDER);
        fact.content = format!("{act} {entry}");
        fact.fields
            .insert(TICK_ENTRY.to_string(), entry.to_string());
        fact.fields.insert(TICK_ACT.to_string(), act.to_string());
        fact
    }

    fn entry_fact(local: &str, order: &str) -> Fact {
        entry(local, order, FactStatus::Active)
    }

    /// The texts and states `view` serves, in order.
    fn shown(facts: &[Fact], setting: Setting) -> Vec<(String, &'static str)> {
        view(facts, setting)
            .entries
            .into_iter()
            .map(|shown| (shown.fact.id.0.clone(), shown.state))
            .collect()
    }

    fn states(pairs: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
        pairs
            .iter()
            .map(|(id, state)| (id.to_string(), *state))
            .collect()
    }

    /// **A sink keeps what is done visible, at the bottom, in the order it was
    /// done** — and an entry ticked undone is where its key put it, open.
    #[test]
    fn a_sink_list_keeps_done_entries_at_the_bottom_in_the_order_done() {
        let mut facts = vec![
            entry_fact("f1", "4"),
            entry_fact("f2", "6"),
            entry_fact("f3", "8"),
        ];
        facts.push(tick("f4", "f2", "done"));
        facts.push(tick("f5", "f1", "done"));
        assert_eq!(
            shown(&facts, Setting::Sink),
            states(&[("f3", "open"), ("f2", "done"), ("f1", "done")])
        );
        facts.push(tick("f6", "f2", "undone"));
        assert_eq!(
            shown(&facts, Setting::Sink),
            states(&[("f2", "open"), ("f3", "open"), ("f1", "done")])
        );
    }

    /// **A queue drops a done entry from the open view** and counts it, so the
    /// answer says what it left out.
    #[test]
    fn a_leave_list_drops_done_entries_from_the_open_view_and_counts_them() {
        let facts = vec![
            entry_fact("f1", "4"),
            entry_fact("f2", "6"),
            tick("f3", "f1", "done"),
        ];
        let seen = view(&facts, Setting::Leave);
        assert_eq!(seen.entries.len(), 1);
        assert_eq!(seen.entries[0].fact.id.0, "f2");
        assert_eq!((seen.done_left_out, seen.dropped_left_out), (1, 0));
    }

    /// **A rotation records the turn and the entry goes to the end, still
    /// open.** Two full circuits end in the original order.
    #[test]
    fn a_back_list_rotates_and_two_circuits_end_in_the_original_order() {
        let mut facts = vec![
            entry_fact("f1", "4"),
            entry_fact("f2", "6"),
            entry_fact("f3", "8"),
        ];
        let ids = |facts: &[Fact]| -> Vec<String> {
            shown(facts, Setting::Back)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        let mut next = 4;
        for circuit in 0..2 {
            for turn in ["f1", "f2", "f3"] {
                facts.push(tick(&format!("f{next}"), turn, "done"));
                next += 1;
                let after: Vec<String> = ids(&facts);
                assert!(
                    shown(&facts, Setting::Back)
                        .iter()
                        .all(|(_, state)| *state == "open"),
                    "a turn leaves the entry open: circuit {circuit}"
                );
                assert_eq!(after.last().map(String::as_str), Some(turn), "{after:?}");
            }
            assert_eq!(ids(&facts), ["f1", "f2", "f3"], "circuit {circuit}");
        }
    }

    /// **Dropping is not doing**: a dropped entry is out of the open view, counted
    /// as dropped, and never reads as done on any setting.
    #[test]
    fn a_dropped_entry_is_counted_as_dropped_and_never_as_done() {
        for setting in [Setting::Sink, Setting::Leave, Setting::Back] {
            let facts = vec![
                entry_fact("f1", "4"),
                entry_fact("f2", "6"),
                tick("f3", "f1", "dropped"),
            ];
            let seen = view(&facts, setting);
            assert_eq!(seen.entries.len(), 1, "{setting:?}");
            assert_eq!(
                (seen.done_left_out, seen.dropped_left_out),
                (0, 1),
                "{setting:?}"
            );
            assert!(seen.entries.iter().all(|shown| shown.state != "done"));
        }
    }

    /// **A tick is a record on the list and never an entry.** It carries no order
    /// key, so neither reader that asks "is this placed" takes it for one.
    #[test]
    fn a_tick_record_is_not_an_entry_and_is_found_as_a_tick() {
        let record = tick("f4", "f1", "done");
        assert!(is_tick(&record));
        assert!(!is_entry(&record));
        assert!(!is_placed(&record));
        assert!(!is_tick(&entry_fact("f1", "4")));
    }

    /// The settings and acts a caller spells, and the default.
    #[test]
    fn the_setting_defaults_to_sink_and_acts_parse_from_their_words() {
        assert_eq!(Setting::of(&Default::default()), Setting::Sink);
        for (word, setting) in [
            ("sink", Setting::Sink),
            ("leave", Setting::Leave),
            ("back", Setting::Back),
        ] {
            let fields = [("tick".to_string(), word.to_string())]
                .into_iter()
                .collect();
            assert_eq!(Setting::of(&fields), setting);
        }
        for word in ["done", "undone", "rotated", "dropped"] {
            assert_eq!(Act::of(word).map(Act::as_token), Some(word));
        }
        assert_eq!(Act::of("finished"), None);
    }
}
