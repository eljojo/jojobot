//! **The packing list: two things packed and the camera still open, read off the
//! list's own entries.**
//!
//! The entry's words are the sitting's own: it may keep "camera" or "The camera",
//! and only its state says whether it is packed. A substring of the answer
//! (`"state":"open","text":"camera"`) holds for the first wording and fails the
//! second, though the list is right. This reads the state of the entry by
//! structure and takes the entry by a word in its text, in any case.

use serde_json::{Value, json};

use crate::run::Observed;

/// **The list that holds the camera shows it open, with at least two other
/// entries ticked done.** Lists that turn (`tick` is `back`) are left out of the
/// read, as the lock this replaces left them out: their turns are ticks of the
/// same kind and a count of every tick would count them.
pub(super) async fn the_packing_list_has_two_packed_and_the_camera_open(
    seen: &Observed<'_>,
) -> Result<(), String> {
    let read = super::read_json(
        &seen
            .room
            .call(
                "recall",
                json!({"kind": "list", "where": "!(tick = back)", "ticks": true,
                       "history_most": 500}),
            )
            .await,
    )?;
    let lists: &[Value] = read["objects"].as_array().map_or(&[], Vec::as_slice);
    let mut refusal = "no list holds an entry for the camera".to_string();
    for list in lists {
        let entries: &[Value] = list["entries"].as_array().map_or(&[], Vec::as_slice);
        let Some(camera) = entries.iter().find(|entry| {
            entry["text"]
                .as_str()
                .is_some_and(|said| said.to_lowercase().contains("camera"))
        }) else {
            continue;
        };
        match judge(list, camera) {
            Ok(()) => return Ok(()),
            Err(why) => refusal = why,
        }
    }
    Err(refusal)
}

/// Whether one list that holds the camera shows it open and two others packed.
///
/// **An entry is packed when its newest tick is `done`.** The ticks come newest
/// first, so the first tick seen for an entry is the one that decides: an entry
/// ticked done and then undone is not packed, however many done ticks it has in
/// its history. Counting ticks would call it packed.
fn judge(list: &Value, camera: &Value) -> Result<(), String> {
    let address = camera["address"].as_str().unwrap_or_default();
    if camera["state"] != "open" {
        return Err(format!(
            "the camera entry {address} reads {}, and it is the one thing not packed",
            camera["state"]
        ));
    }
    let mut newest: std::collections::BTreeMap<&str, &Value> = std::collections::BTreeMap::new();
    for tick in list["ticks"].as_array().map_or(&[][..], Vec::as_slice) {
        if let Some(entry) = tick["entry"].as_str() {
            newest.entry(entry).or_insert(&tick["act"]);
        }
    }
    let packed = newest
        .iter()
        .filter(|(entry, act)| **entry != address && **act == "done")
        .count();
    if packed < 2 {
        return Err(format!(
            "{packed} entries besides the camera stand packed, and two were packed"
        ));
    }
    Ok(())
}
