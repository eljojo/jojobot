//! **The entries of a list: claims on it, in an order the server minted.**
//!
//! An entry is one claim on a thing of kind `list`: a few words, or a claim
//! whose edge points at a thing. It carries an order key in its own field
//! `order`, spelled as a document section's is, and the keys sort as text
//! ([`super::order`]). A caller never picks a key: it says where the entry goes
//! (the end, the front, or after a named entry) and [`place_new`] names the key
//! and the turn bucket the new one takes, by the placement a move makes.
//!
//! **Which entries are open is computed from what is on the list**, never
//! stored on an entry: an entry that is archived is not open. The state each
//! open entry reads as is [`OPEN`]; later slices compute others from tick
//! records.

use super::{EntityId, EntityKind, document};
use super::{Fact, FactAddress, FactStatus};

/// The word an entry reads as while nothing has acted on it.
pub const OPEN: &str = "open";

/// The word that places an entry at the front.
pub const FIRST: &str = "first";

/// The word that places an entry at the end. A new entry with no `after` goes
/// there too; a move, which has to say something, says this.
pub const LAST: &str = "last";

/// **The record field a move writes: the turn bucket the entry is held in.** A
/// list that rotates reads its entries by the turn each last took and then by
/// key, so a key alone cannot put a turned entry first; this says which bucket
/// the move put the entry in. The server mints it with the key and a caller never
/// sends it.
pub const HELD_TURN: &str = "held_turn";

/// **The record field a move writes: the highest claim number the list held when
/// the entry was moved.** A move holds only while no tick on the entry is newer
/// than this, which is how the view tells a move from the tick that came after it
/// without a clock. The server mints it and a caller never sends it.
pub const HELD_SINCE: &str = "held_since";

/// **Whether a record field is the server's to mint when it places an entry or a
/// section**: the order key and the two a move of an entry writes. One list, so a
/// gate that lets a move rewrite them without the writer's say-so, and one that
/// refuses a caller's own copy, ask the same question.
pub fn is_placement_key(key: &str) -> bool {
    matches!(key.trim(), document::ORDER | HELD_TURN | HELD_SINCE)
}

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
    /// `after` is neither `first`, `last` nor an address.
    #[error("'{0}' is neither the word `first`, the word `last` nor the address of an entry")]
    NotAPlace(String),
    /// `after` names an address that is not an open entry of this list.
    #[error("'{named}' is not an open entry of this list")]
    NotAnEntry {
        /// What the caller named.
        named: String,
        /// The addresses of the entries there are, in order.
        entries: Vec<String>,
    },
    /// The entry a move was asked of is not open: a done entry sits where it was
    /// done and a dropped one is out of view, so neither has a place to move to.
    #[error("'{named}' is {state}, so it has no place to move to")]
    NotMovable {
        /// The entry the move was asked of.
        named: String,
        /// What it reads as: `done` or `dropped`.
        state: &'static str,
    },
    /// An entry was told to follow itself.
    #[error("'{0}' cannot follow itself")]
    FollowsItself(String),
    /// `after` names an entry of the list that is not open, so it holds no place
    /// to follow.
    #[error("'{named}' is {state}, so there is no place behind it")]
    NotAPlaceToFollow {
        /// What the caller named.
        named: String,
        /// What it reads as: `done` or `dropped`.
        state: &'static str,
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
    document::placed_in_order(facts)
        .into_iter()
        .filter(|fact| fact.status == FactStatus::Active && is_entry(fact))
        .collect()
}

/// **The things a list's entries link, counted once each.** The distinct objects
/// linked by every active entry that is not dropped, whether the entry is in view
/// or not: a tick lets an entry leave the view and changes what the list is
/// about by nothing, while a dropped entry is no longer part of it. An entry
/// that links nothing is not counted, and is not an error.
pub fn counted_links(facts: &[Fact], setting: Setting) -> std::collections::BTreeSet<EntityId> {
    let seen = view(facts, setting);
    open_entries(facts)
        .into_iter()
        .filter(|entry| seen.states.get(&entry.id.0) != Some(&DROPPED))
        .filter_map(|entry| entry.edge.as_ref().map(|edge| edge.object.clone()))
        .collect()
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
    let last = document::placed_in_order(survivor)
        .last()
        .map(|fact| fact.fields[document::ORDER].as_str());
    let standing: Vec<&Fact> = document::placed_in_order(folded)
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

/// **A tick a list merge points at the new id of its entry.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickMoved {
    /// The tick record, by the id it was stored under on the folded list.
    pub fact: super::FactId,
    /// The id its entry has under the survivor.
    pub entry: String,
}

/// **The ticks of a folded list, each with the id its entry takes on the
/// survivor.** A tick names its entry by the entry's local id, and a merge gives
/// every folded record a new id on the survivor, where the old one may name an
/// entry of the survivor's own. `claims` is the merge's map from each folded
/// record's old id to the new one. A tick whose entry the map does not hold is
/// left as it is.
///
/// **Ticks are picked by the fields they carry, not by the kind of the thing that
/// holds them**, for the reason [`append_after`] picks entries so: a store runs
/// this while it holds a record's home as an id that names no kind. A tick taken
/// back is moved too, so one that is restored later still names its entry.
pub fn ticks_following(
    folded: &[Fact],
    claims: &[(super::FactId, super::FactId)],
) -> Vec<TickMoved> {
    folded
        .iter()
        .filter(|fact| {
            fact.fields.contains_key(TICK_ENTRY)
                && fact.fields.contains_key(TICK_ACT)
                && !fact.fields.contains_key(document::ORDER)
        })
        .filter_map(|tick| {
            let named = tick.fields[TICK_ENTRY].trim();
            let (_, now) = claims.iter().find(|(was, _)| was.0 == named)?;
            Some(TickMoved {
                fact: tick.id.clone(),
                entry: now.0.clone(),
            })
        })
        .collect()
}

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

/// **Where an open entry sits in the order the view reads**: the bucket it is in,
/// then its key. Entries nobody has turned are in bucket 0 and read by key; an
/// entry that took a turn is in the bucket of that turn, which grows with time,
/// so the oldest turn reads first. A move that still holds puts its entry in the
/// bucket the move chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The entry's local id.
    pub id: String,
    /// The entry's address, as a caller names it.
    pub address: String,
    /// The bucket the entry is in.
    pub bucket: u64,
    /// The entry's order key.
    pub order: String,
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
    /// The open entries in the order they read, with the place each holds in it.
    /// What a move is placed against.
    pub slots: Vec<Slot>,
}

/// **The claim number an id carries**, or none when it carries none.
fn claim_number(fact: &Fact) -> Option<u64> {
    fact.id.0.trim_start_matches('f').parse().ok()
}

/// **The bucket a move still holds an entry in**, or none when the entry has none
/// or a tick on it came after the move. A tick is newer than the move when its
/// claim number is above the one the move recorded: claim numbers only grow on a
/// list, so no clock is read.
fn held_bucket(entry: &Fact, last_tick: Option<u64>) -> Option<u64> {
    let turn = entry.fields.get(HELD_TURN)?.trim().parse::<u64>().ok()?;
    let since = entry.fields.get(HELD_SINCE)?.trim().parse::<u64>().ok()?;
    last_tick.is_none_or(|tick| tick <= since).then_some(turn)
}

/// **The entries of a list as its ticks leave them.** Nothing here is stored on an
/// entry: the latest tick naming an entry decides its state, and the order is the
/// keys, then the turns. A move an entry carries holds until the entry is next
/// ticked.
pub fn view(facts: &[Fact], setting: Setting) -> View<'_> {
    #[derive(Default, Clone, Copy)]
    struct Standing {
        /// The claim number of the tick that last made the entry done.
        done_at: Option<u64>,
        /// The claim number of the tick that last turned the entry.
        turn_at: Option<u64>,
        /// The claim number of the latest tick of any act on the entry.
        last_tick: Option<u64>,
        dropped: bool,
    }
    let entries = open_entries(facts);
    let mut standing: std::collections::HashMap<&str, Standing> = entries
        .iter()
        .map(|entry| (entry.id.0.as_str(), Standing::default()))
        .collect();
    for (tick, act) in active_ticks(facts) {
        let Some(now) = standing.get_mut(tick.fields[TICK_ENTRY].trim()) else {
            continue;
        };
        let at = sequence(tick);
        now.last_tick = Some(at);
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
    let mut open: Vec<(&Fact, u64)> = Vec::new();
    let mut done: Vec<(&Fact, u64)> = Vec::new();
    let mut seen = View {
        entries: Vec::new(),
        done_left_out: 0,
        dropped_left_out: 0,
        states: Default::default(),
        slots: Vec::new(),
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
            (false, None) => open.push((
                entry,
                held_bucket(entry, now.last_tick).unwrap_or_else(|| now.turn_at.unwrap_or(0)),
            )),
        }
    }
    // Entries nobody has turned keep their keys' order; turned ones follow, the
    // oldest turn first, so a full circuit of turns ends where it began. An entry
    // a move still holds reads in the bucket the move chose.
    open.sort_by_key(|(entry, bucket)| {
        (
            *bucket,
            entry.fields[document::ORDER].clone(),
            entry.id.0.clone(),
        )
    });
    done.sort_by_key(|(_, at)| *at);
    seen.slots = open
        .iter()
        .map(|(entry, bucket)| Slot {
            id: entry.id.0.clone(),
            address: entry.address().to_string(),
            bucket: *bucket,
            order: entry.fields[document::ORDER].clone(),
        })
        .collect();
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

/// **What a move writes on the entry it moves.** The three fields are the
/// server's: the key, the bucket and the claim number the move holds from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// The entry's new order key.
    pub order: String,
    /// The bucket the entry is held in.
    pub held_turn: u64,
    /// The highest claim number the list held at the move.
    pub held_since: u64,
}

/// **Place an existing entry where the caller asked, and say what to write.**
///
/// The place is read against the order the list reads now, which is not the key
/// order once entries have taken turns, so the entry goes where a reader of the
/// list sees it should go. Only an open entry moves, and only behind an open
/// entry: a done entry sits where it was done, and a dropped one is out of view.
/// The move holds until the entry is next ticked ([`view`]).
pub fn move_entry(
    facts: &[Fact],
    setting: Setting,
    entry: &FactAddress,
    placement: &Placement,
) -> Result<Moved, PlaceError> {
    let seen = view(facts, setting);
    let named = format!("{}#{}", entry.home.as_str(), entry.local.0);
    match seen.states.get(entry.local.0.as_str()) {
        None => {
            return Err(PlaceError::NotAnEntry {
                named,
                entries: open_addresses(&seen),
            });
        }
        Some(&state) if state != OPEN => {
            return Err(PlaceError::NotMovable { named, state });
        }
        Some(_) => {}
    }
    place(&seen, facts, &entry.home, Some(&entry.local.0), placement)
}

/// **Place a new entry where the caller asked, and say what to write.** The same
/// placement a move makes, against the order the list reads now, so a new entry
/// goes behind the entry it names or at the end the list reads, and a done or
/// dropped entry holds no place to follow. `list` is the list's handle.
pub fn place_new(
    facts: &[Fact],
    setting: Setting,
    list: &super::EntityId,
    placement: &Placement,
) -> Result<Moved, PlaceError> {
    place(&view(facts, setting), facts, list, None, placement)
}

/// The addresses of the open entries, in the order the list reads them.
fn open_addresses(seen: &View<'_>) -> Vec<String> {
    seen.slots.iter().map(|slot| slot.address.clone()).collect()
}

/// **The one placement.** `moving` is the local id of the entry a move is
/// placing, left out of the neighbours; a new entry has none. The entry lands in
/// a bucket and between two keys of it.
fn place(
    seen: &View<'_>,
    facts: &[Fact],
    list: &super::EntityId,
    moving: Option<&str>,
    placement: &Placement,
) -> Result<Moved, PlaceError> {
    let others: Vec<&Slot> = seen
        .slots
        .iter()
        .filter(|slot| Some(slot.id.as_str()) != moving)
        .collect();
    // The bucket the entry goes in, and the keys it falls between inside it.
    let (bucket, lower, upper) = match placement {
        Placement::First => match others.first() {
            Some(first) => (first.bucket, None, Some(first.order.as_str())),
            None => (0, None, None),
        },
        Placement::End => match others.last() {
            Some(last) => (last.bucket, Some(last.order.as_str()), None),
            None => (0, None, None),
        },
        Placement::After(target) => {
            if moving.is_some_and(|moving| target.home == *list && target.local.0 == moving) {
                return Err(PlaceError::FollowsItself(format!(
                    "{}#{}",
                    target.home.as_str(),
                    target.local.0
                )));
            }
            let at = others
                .iter()
                .position(|slot| slot.id == target.local.0 && target.home == *list);
            let Some(at) = at else {
                let target_named = format!("{}#{}", target.home.as_str(), target.local.0);
                return Err(match seen.states.get(target.local.0.as_str()) {
                    Some(&state) if target.home == *list => PlaceError::NotAPlaceToFollow {
                        named: target_named,
                        state,
                    },
                    _ => PlaceError::NotAnEntry {
                        named: target_named,
                        entries: open_addresses(seen),
                    },
                });
            };
            let behind = others[at];
            // The next entry bounds the key only when it reads in the same bucket;
            // one in a later bucket reads after whatever this bucket holds.
            let next = others
                .get(at + 1)
                .filter(|next| next.bucket == behind.bucket)
                .map(|next| next.order.as_str());
            (behind.bucket, Some(behind.order.as_str()), next)
        }
    };
    let order =
        super::order::between(lower, upper).map_err(|e| PlaceError::NoRoom(e.to_string()))?;
    Ok(Moved {
        order,
        held_turn: bucket,
        held_since: facts.iter().filter_map(claim_number).max().unwrap_or(0),
    })
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
    if named == LAST {
        return Ok(Placement::End);
    }
    FactAddress::parse(named)
        .map(Placement::After)
        .map_err(|_| PlaceError::NotAPlace(named.to_string()))
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

    fn list() -> EntityId {
        EntityId("list:packing".into())
    }

    fn after(local: &str) -> Placement {
        Placement::After(FactAddress::new(list(), FactId(local.into())))
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
        let key = |placement: &Placement| {
            place_new(&facts, Setting::Sink, &list(), placement)
                .unwrap()
                .order
        };
        let end = key(&Placement::End);
        assert!(end.as_str() > "8", "{end}");
        let first = key(&Placement::First);
        assert!(first.as_str() < "4", "{first}");
        let after_first = key(&after("f1"));
        assert!(
            after_first.as_str() > "4" && after_first.as_str() < "8",
            "{after_first}"
        );
        let after_last = key(&after("f2"));
        assert!(after_last.as_str() > "8", "{after_last}");
        assert_eq!(
            place_new(&[], Setting::Sink, &list(), &Placement::End)
                .unwrap()
                .order,
            order_of_an_empty_list()
        );
    }

    fn order_of_an_empty_list() -> String {
        crate::memory::order::between(None, None).unwrap()
    }

    /// **A new entry is placed against the order the list reads, not the order
    /// of the keys.** Homer took his turn on a rotation, so the list reads Bart,
    /// Lisa, Homer; a new entry at the end or behind Homer reads after Homer, and
    /// one behind Bart reads second.
    #[test]
    fn a_new_entry_is_placed_against_the_order_the_list_reads() {
        let mut facts = vec![
            entry("f1", "4", FactStatus::Active),
            entry("f2", "6", FactStatus::Active),
            entry("f3", "8", FactStatus::Active),
        ];
        facts.push(tick("f4", "f1", "done"));
        let read = |placement: &Placement| {
            let placed = place_new(&facts, Setting::Back, &list(), placement).unwrap();
            let mut with = facts.clone();
            let mut new = entry("f5", &placed.order, FactStatus::Active);
            new.fields
                .insert(HELD_TURN.into(), placed.held_turn.to_string());
            new.fields
                .insert(HELD_SINCE.into(), placed.held_since.to_string());
            with.push(new);
            view(&with, Setting::Back)
                .entries
                .iter()
                .map(|shown| shown.fact.id.0.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(read(&Placement::End), ["f2", "f3", "f1", "f5"]);
        assert_eq!(read(&after("f1")), ["f2", "f3", "f1", "f5"]);
        assert_eq!(read(&after("f2")), ["f2", "f5", "f3", "f1"]);
        assert_eq!(read(&Placement::First), ["f5", "f2", "f3", "f1"]);
    }

    /// **A new entry cannot follow a done entry**, as a move cannot.
    #[test]
    fn a_new_entry_cannot_follow_a_done_entry() {
        let mut facts = vec![
            entry("f1", "4", FactStatus::Active),
            entry("f2", "6", FactStatus::Active),
        ];
        facts.push(tick("f3", "f1", "done"));
        assert!(matches!(
            place_new(&facts, Setting::Sink, &list(), &after("f1")),
            Err(PlaceError::NotAPlaceToFollow { state: DONE, .. })
        ));
        assert!(place_new(&facts, Setting::Sink, &list(), &after("f2")).is_ok());
    }

    /// **`after` must name an open entry of this list, and the refusal lists the
    /// entries there are.**
    #[test]
    fn after_naming_something_else_is_refused_with_the_entries() {
        let facts = vec![entry("f1", "4", FactStatus::Active)];
        match place_new(&facts, Setting::Sink, &list(), &after("f9")) {
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

    /// **A merge points each folded tick at the new id of its entry.** A tick
    /// taken back is moved too. A tick whose entry the merge did not give a new id
    /// is left as it is, and so is a record that is no tick.
    #[test]
    fn a_merge_points_each_folded_tick_at_the_new_id_of_its_entry() {
        use crate::memory::FactId;
        let tick = |local: &str, entry_id: &str, status: FactStatus| {
            let mut tick = entry(local, "unused", status);
            tick.fields.clear();
            tick.fields.insert(TICK_ENTRY.into(), entry_id.into());
            tick.fields.insert(TICK_ACT.into(), "done".into());
            tick
        };
        let folded = vec![
            entry("f2", "a", FactStatus::Active),
            tick("f9", "f2", FactStatus::Active),
            tick("f8", " f2 ", FactStatus::Archived),
            tick("f7", "f77", FactStatus::Active),
        ];
        let claims = vec![
            (FactId("f2".into()), FactId("f12".into())),
            (FactId("f9".into()), FactId("f13".into())),
        ];

        let moved = ticks_following(&folded, &claims);

        let pairs: Vec<(&str, &str)> = moved
            .iter()
            .map(|tick| (tick.fact.0.as_str(), tick.entry.as_str()))
            .collect();
        assert_eq!(pairs, [("f9", "f12"), ("f8", "f12")]);
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
        let ids: Vec<&str> = document::placed_in_order(&facts)
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
