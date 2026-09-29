//! "I filed somebody who turns out not to belong in here at all. I don't
//! want them cluttering the list every time I browse, but if I ever come
//! back to their handle I want the record still there."
//!
//! Archiving is not the repair for a duplicate — that is `merge_entities`,
//! folding two handles back into one thing. This is for a handle that was
//! never a duplicate of anything: it should simply stop turning up by
//! default. One way, and it never touches what was actually said — a
//! session naming the exact handle still reads the whole record, reason and
//! all.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_mistaken_entry_drops_out_of_the_default_list_and_stays_reachable_by_name() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // Two people on file — one turns out not to belong here, one is
    // ordinary and stays exactly as it was.
    s.add("person:apu", "Apu").await;
    s.fact("person:apu", "came up once in passing").await;
    s.add("person:skinner", "Skinner").await;
    s.fact("person:skinner", "runs the school").await;

    // ── the positive: browsing lists both, before anything is archived ───────
    s.list("person")
        .await
        .says("person:apu")
        .says("person:skinner");

    // ── archiving names the handle and the reason, and the receipt echoes
    //    both back ───────────────────────────────────────────────────────────
    let archived = s
        .call(
            "archive_entity",
            json!({
                "handle": "person:apu",
                "reason": "came up once, never actually part of anything I'm tracking",
            }),
        )
        .await;
    archived.says("person:apu");
    archived.says("came up once, never actually part of anything I'm tracking");

    // ── the positive `handle` rests on: only the named handle drops out ──────
    //
    // Skinner was never named in the call, so Skinner is exactly where he
    // was — a build that archived the wrong entity, or archived everything
    // in reach, fails this rather than the assertion on Apu alone.
    let after = s.list("person").await;
    after.never_says("person:apu");
    after.says("person:skinner");

    // ── the positive `reason` rests on: the claims stand, only the subject
    //    is out of scope ─────────────────────────────────────────────────────
    //
    // Archiving the entity never touches what was said about it — the
    // fact from before the archive is still there, and the reason the
    // archive itself carried reads back beside it.
    let apu = s.recall("person:apu").await;
    apu.says("came up once in passing");
    apu.says("came up once, never actually part of anything I'm tracking");

    // Skinner's own record is untouched throughout.
    s.recall("person:skinner")
        .await
        .says("runs the school")
        .never_says("came up once");

    s.wrap("archived the one who never belonged, left the other alone")
        .await;

    story.finish().await;
}
