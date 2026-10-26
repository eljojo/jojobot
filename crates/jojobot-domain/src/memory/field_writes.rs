use super::*;

/// **One write of one key on one thing — the substrate a field read projects
/// from.**
///
/// A key is written a piece at a time: every capture carrying it and every edit
/// reaching it is a write of its own, kept. The current value of the key is the
/// newest of them, which is what an ordinary read answers with; the writes
/// themselves are what [`Memory::history`] answers with, and how many there are
/// is the answer to "how many times".
///
/// **The address is the thing and the key**, never the record's id. A hundred
/// sittings that each record one donut are a hundred records, so a history
/// hanging off a record's id would be a hundred histories of length one and
/// could count nothing.
/// **Everything the key-trail reads answer for one thing, read at once.**
///
/// [`Memory::history`] is asked of a thing and a key, and
/// [`Memory::keys_kept_off_the_thing`] of a record. A reader that holds the
/// whole store in memory asks both of every thing and every record, so this is
/// the same two answers in one round trip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyTrails {
    /// `history(thing, key)` for every key any write on the thing ever named.
    pub history: BTreeMap<String, Vec<FieldWrite>>,
    /// `keys_kept_off_the_thing` for every record on the thing, by its local id.
    /// A record with nothing kept off it has no entry.
    pub kept_off: BTreeMap<FactId, std::collections::BTreeSet<String>>,
    /// **Every write of a key that stays on its record**, oldest first within a
    /// key — the writes [`Memory::history`] leaves out because they do not reach
    /// the thing. A section of a document keeps its place and its heading depth
    /// this way, so a read of the document as it stood on a day needs them with
    /// their moments.
    pub record_writes: Vec<RecordFieldWrite>,
}

/// **One write of a key that stays on the record that carried it**, with the
/// moment the store took it in.
///
/// Where [`FieldWrite`] answers a key on a thing, this answers a key on a
/// record: the same row of the substrate, read for a reader that asks what a
/// record held on a day rather than what a thing holds now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFieldWrite {
    /// The record that carried the write, local to the thing.
    pub fact: FactId,
    /// The key.
    pub key: String,
    /// Its place among the writes of this key on this thing, from one.
    pub ordinal: u64,
    /// What the write put there; `None` says it took the key off.
    pub value: Option<String>,
    /// **When the store took this write in**, `None` for a write an older build
    /// appended before the store stamped its writes. It is never filled in, for
    /// the reason [`KeyWrite::written_at`] is not.
    pub written_at: Option<jiff::Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldWrite {
    /// **When the store took this write in**, when it stamped one: see
    /// [`KeyWrite::written_at`].
    pub written_at: Option<jiff::Timestamp>,
    /// **What this write put there, and `None` says it took the key off.**
    ///
    /// A clear is a write like any other: nothing is removed from the
    /// substrate, so the key stops being current while the writes that put it
    /// there stay readable.
    pub value: Option<String>,
    /// The record that carried the write — its address, so a reader can go and
    /// read what else that sitting said.
    pub fact: FactAddress,
    /// **The day the record this write arrived in was made** — see
    /// [`Fact::recorded_at`].
    pub recorded_at: Date,
    /// That record's status. A write inside a record that is now archived
    /// still happened, so it is reported rather than dropped — and reported
    /// as archived, so a count can leave it out.
    pub status: FactStatus,
    /// **Who backs the record it came from**, so a reader of a key's history
    /// can tell a value somebody stated from one somebody guessed.
    pub provenance: Provenance,
    /// **How sure anyone was of that record.**
    pub standing: Standing,
    /// **The note that record carries**, if it carries one — see
    /// [`KeyWrite::note`].
    pub note: Option<String>,
    /// **The app that wrote that record**, if one did — see
    /// [`KeyWrite::recorded_by`].
    pub recorded_by: Option<crate::app::AppStamp>,
}
