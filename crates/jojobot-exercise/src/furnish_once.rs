//! **A room's furnish, paid once per test process and copied for each case.**
//!
//! A room suite whose every case opens a room and writes the room's whole seed
//! through the served surface pays the seed again for each case. This module
//! builds the furnished store directory once, on the first call, and hands back
//! its path: a caller opens each case's room from a private copy of it
//! (`Room::open_with_client_from_stating`), so no case sees another's writes.
//!
//! **The first caller builds and a caller that arrives meanwhile awaits the same
//! build**, which is what `tokio::sync::OnceCell::get_or_try_init` gives. The
//! directory is never persisted past the process.

use std::path::PathBuf;

use serde_json::json;

use crate::expectations;
use crate::room::{Room, server_binary};
use crate::surface::Surface;

/// **The furnished store of `room_document`, built on first use.** `cell` is
/// the suite's own, so each suite builds its own room's seed and no other.
pub async fn seed_dir(
    cell: &'static tokio::sync::OnceCell<PathBuf>,
    room_document: &'static str,
    day: &str,
) -> PathBuf {
    seed_dir_of(cell, day, || expectations::seed_for(room_document)).await
}

/// **The furnished store of a seed the caller makes**, for a suite that furnishes
/// a room's seed in more than one shape. `make` is called once, on the first use.
pub async fn seed_dir_of(
    cell: &'static tokio::sync::OnceCell<PathBuf>,
    day: &str,
    make: impl FnOnce() -> anyhow::Result<crate::surface::Seed>,
) -> PathBuf {
    seed_dir_after(cell, day, |surface| async move {
        make()?.furnish(&surface).await?;
        Ok(())
    })
    .await
}

/// **The store of a room a caller filled in its own way**, built once per process:
/// `fill` gets the live room's surface and may furnish it and then work it. A suite
/// whose cases all start from the same worked room pays the work once.
pub async fn seed_dir_after<F, Fut>(
    cell: &'static tokio::sync::OnceCell<PathBuf>,
    day: &str,
    fill: F,
) -> PathBuf
where
    F: FnOnce(Surface) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    cell.get_or_try_init(|| async {
        let binary = server_binary().expect("a jojobot binary");
        Room::snapshot_after_stating(&binary, day, fill).await
    })
    .await
    .expect("the filled seed builds")
    .clone()
}

/// **The kinds a room holds entities of**, as the room's own boot names them under
/// `snapshot.entities.by_kind`. `None` is a boot that names none, which a
/// comparison must not take for a room that holds nothing.
pub fn kinds_held(boot: &serde_json::Value) -> Option<Vec<String>> {
    let kinds = boot["snapshot"]["entities"]["by_kind"].as_object()?;
    Some(kinds.keys().cloned().collect())
}

/// **Every entity id a room holds, by kind**, read through the surface and
/// sorted, so a furnish made live and one copied from the cache can be compared
/// on what a case can find. The kinds are the ones the room's boot names, so a
/// kind the build gains or a room fills (a bot, a role) is compared as well. Each
/// kind is read in pages of 500 objects, and goes on where the answer says it
/// stopped, so a kind the answer ceiling cut is compared whole.
pub async fn held_ids(surface: &Surface) -> Vec<String> {
    let boot = surface.call("start_here", json!({"brief": true})).await;
    let boot: serde_json::Value = serde_json::from_str(&boot).expect("a JSON boot");
    let kinds = kinds_held(&boot).expect("the boot names the kinds the room holds");
    let mut ids = Vec::new();
    for kind in kinds {
        let mut offset = 0;
        loop {
            let read = surface
                .call(
                    "recall",
                    json!({"kind": kind, "limit": 500, "offset": offset, "keys": ["name"]}),
                )
                .await;
            let read: serde_json::Value = serde_json::from_str(&read).expect("a JSON read");
            ids.extend(ids_read(&kind, &read));
            match next_offset(&kind, offset, &read) {
                Some(next) => offset = next,
                None => break,
            }
        }
    }
    ids.sort();
    ids
}

/// **Where a read that was cut goes on**, or `None` for a read that said what it
/// left out and left out nothing. An answer stops at its ceiling as well as at its
/// limit, and names the offset that reads on under `not_shown`.
pub fn next_offset(kind: &str, offset: u64, read: &serde_json::Value) -> Option<u64> {
    let cut = read.get("not_shown")?;
    match cut["offset"].as_u64() {
        Some(next) if next > offset => Some(next),
        _ => panic!("the read of the kind {kind} was cut and does not go on past {offset}: {cut}"),
    }
}

/// **The ids one page of one kind's read answered.** An answer that refused the
/// read says so naming the kind, because a kind that contributes nothing on both
/// sides would compare equal over ids nobody looked at.
pub fn ids_read(kind: &str, read: &serde_json::Value) -> Vec<String> {
    assert!(
        read["status"] != "blocked" && read.get("transport_error").is_none(),
        "the read of the kind {kind} was refused: {read}"
    );
    let objects = read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the read of the kind {kind} had no list of objects: {read}"));
    objects
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A boot as a room's server answers it with no bot named: the kinds it
    /// holds entities of, with how many of each. The shape is the served one.
    fn a_boot() -> serde_json::Value {
        json!({
            "snapshot": {"entities": {
                "available": true,
                "by_kind": {
                    "bot": 3, "challenge": 1, "decision": 12, "document": 4, "event": 2,
                    "list": 1, "machine": 1, "person": 1, "project": 2, "promise": 1,
                    "rhythm": 1, "role": 3, "thing": 5, "thread": 2, "topic": 3,
                    "view": 14, "voucher": 1, "work": 9
                },
                "count": 66
            }}
        })
    }

    /// **Every kind the boot names is read, bots and roles included.** The
    /// comparison of a cached room with a live one is only as wide as this list,
    /// and a list that left out `bot` let a cache that dropped a bot compare
    /// equal. The control is a kind the old list named: it stays.
    #[test]
    fn every_kind_the_boot_names_is_read() {
        let kinds = kinds_held(&a_boot()).expect("the boot names kinds");
        for kind in [
            "bot", "role", "view", "voucher", "decision", "person", "work",
        ] {
            assert!(
                kinds.iter().any(|held| held == kind),
                "{kind} is read: {kinds:?}"
            );
        }
        assert_eq!(
            kinds.len(),
            18,
            "exactly the kinds the boot names: {kinds:?}"
        );
    }

    /// **A read of a kind that was refused is a failure naming the kind.** Taking
    /// it for a kind with no ids would let two rooms compare equal over a kind
    /// nobody read. The controls beside it are the answers a room gives: ids, and
    /// none.
    #[test]
    #[should_panic(expected = "bot")]
    fn a_refused_read_of_a_kind_names_the_kind() {
        ids_read(
            "bot",
            &json!({"status": "blocked", "wrote": false, "how_to_proceed": "nothing"}),
        );
    }

    /// The same for a read the transport lost: `Surface::call` answers it as text.
    #[test]
    #[should_panic(expected = "role")]
    fn a_lost_read_of_a_kind_names_the_kind() {
        ids_read("role", &json!({"transport_error": "connection reset"}));
    }

    /// **An answer with no list of objects is a failure naming the kind, whatever
    /// else it says.** A read of a kind always carries one, empty or not, so an
    /// error of another shape or a malformed answer must not read as a kind with
    /// no ids.
    #[test]
    #[should_panic(expected = "had no list of objects")]
    fn an_answer_with_no_list_of_objects_names_the_kind() {
        ids_read("pet", &json!({"error": "something else"}));
    }

    #[test]
    fn a_read_that_answered_gives_its_ids_and_an_empty_one_gives_none() {
        let answered = json!({"objects": [{"id": "bot:otto"}, {"id": "bot:lisa"}], "count": 2});
        assert_eq!(ids_read("bot", &answered), ["bot:otto", "bot:lisa"]);
        assert!(ids_read("pet", &json!({"objects": [], "count": 0})).is_empty());
    }

    /// A read the answer ceiling cut, as a server answers it: the ids it gave, and
    /// under `not_shown` how many it left out and the offset that reads on. The
    /// shape is the served one, taken from an answer that was cut.
    fn a_cut_read(next: u64) -> serde_json::Value {
        json!({
            "count": 1,
            "objects": [{"id": "bot:otto"}],
            "not_shown": {
                "count": 5,
                "how_to_proceed": "these come after the ones above: repeat this call with the offset",
                "offset": next
            }
        })
    }

    /// **A read that was cut says where to go on, and a whole one says nothing.**
    #[test]
    fn a_cut_read_goes_on_where_it_stopped_and_a_whole_one_stops() {
        assert_eq!(next_offset("bot", 0, &a_cut_read(16)), Some(16));
        assert_eq!(
            next_offset("bot", 0, &json!({"objects": [], "count": 0})),
            None
        );
    }

    /// A cut that names no way on, or one that does not move, is a failure naming
    /// the kind: going on from the same place would read the same page for ever.
    #[test]
    #[should_panic(expected = "the read of the kind bot was cut and does not go on")]
    fn a_cut_that_does_not_advance_is_a_failure_naming_the_kind() {
        next_offset("bot", 16, &a_cut_read(16));
    }

    #[test]
    #[should_panic(expected = "the read of the kind role was cut and does not go on")]
    fn a_cut_that_names_no_offset_is_a_failure_naming_the_kind() {
        next_offset(
            "role",
            0,
            &json!({"objects": [], "not_shown": {"count": 3}}),
        );
    }

    /// **A boot that names no kinds is not a room that holds nothing.** Taking
    /// it for one would let two rooms compare equal over no ids at all.
    #[test]
    fn a_boot_that_names_no_kinds_is_not_an_empty_room() {
        assert_eq!(kinds_held(&json!({"snapshot": {"entities": {}}})), None);
        assert_eq!(kinds_held(&json!({"transport_error": "refused"})), None);
    }
}
