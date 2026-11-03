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
    cell.get_or_try_init(|| async {
        let seed = make()?;
        Room::snapshot_after_stating(
            &server_binary().expect("a jojobot binary"),
            day,
            |surface| async move {
                seed.furnish(&surface).await?;
                Ok(())
            },
        )
        .await
    })
    .await
    .expect("the furnished seed builds")
    .clone()
}

/// **Every entity id a room holds, by kind**, read through the surface and
/// sorted, so a furnish made live and one copied from the cache can be compared
/// on what a case can find.
pub async fn held_ids(surface: &Surface) -> Vec<String> {
    let mut ids = Vec::new();
    for kind in [
        "person", "place", "work", "event", "thing", "project", "list", "thread", "topic",
        "rhythm", "org", "machine", "document", "pet",
    ] {
        let read = surface
            .call(
                "recall",
                json!({"kind": kind, "limit": 500, "keys": ["name"]}),
            )
            .await;
        let read: serde_json::Value = serde_json::from_str(&read).expect("a JSON read");
        for object in read["objects"].as_array().into_iter().flatten() {
            if let Some(id) = object["id"].as_str() {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids
}
