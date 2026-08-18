//! "I write gigs down the same way I write everything else down. Which of
//! them were gigs, and which did I actually go to?"
//!
//! Nobody wrote these as gigs. They went in as events, because that is what
//! they are — the same handle shape as a birthday and a winter fest — and the
//! two things worth knowing about a gig got written beside the record as they
//! came up: who played, and whether the evening actually happened for me.
//!
//! **The question comes months later, and it is a question about a SHAPE
//! rather than about a sort of thing.** "Which of these were gigs" is not
//! answerable by kind: minting a `gig` kind would mean deciding, on the night,
//! that this event is a different species from a birthday — and nobody knows
//! that on the night. What is knowable on the night is what got written down.
//!
//! **So a schema is not a kind, and this story is where that stops being a
//! sentence and starts being usable.** The keys make a gig; the kind stays
//! `event`; and the two are separate questions asked in one call. The jukebox
//! is what makes that difference visible rather than decorative: it carries
//! `played` too, honestly and about something else entirely, and only the kind
//! keeps it out.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_schema_that_is_not_a_kind_finds_the_gigs_among_the_events() {
    let story = Story::begin("bot:otto").await;

    // ── over the months · things happen, and get written down as events ─────
    let s = story.session().await;
    s.add("event:moe-open-mic", "Open Mic Night at Moe's").await;
    s.add("event:lisa-quartet-night", "The Quartet at the School Hall")
        .await;
    s.add("event:otto-benefit-show", "The Benefit Show").await;

    // Whole: both of the things worth knowing, written on the night. Nothing
    // on the record says "gig" — there is no word for the class yet, so the
    // keys are the only thing a schema can be answered by later.
    s.event_with(
        "event:moe-open-mic",
        "went along straight from work",
        json!({ "played": "Barney Gumble", "went": "yes" }),
        &[],
    )
    .await;

    // Whole, and the other answer: knowing which ones I MISSED is half the
    // point of asking. A key is not a flag for the good ones.
    s.event_with(
        "event:lisa-quartet-night",
        "double booked, heard it was the best one all year",
        json!({ "played": "Lisa Simpson", "went": "no" }),
        &[],
    )
    .await;

    // Partial, the way notes actually are: somebody mentioned who played and
    // nobody ever wrote down whether I was there.
    s.event_with(
        "event:otto-benefit-show",
        "heard about it afterwards",
        json!({ "played": "Otto Mann" }),
        &[],
    )
    .await;

    // ── and the events that are not gigs, which is most of them ─────────────
    s.add("event:birthday-party", "Bart's Birthday").await;
    s.event_with(
        "event:birthday-party",
        "cake at the house, everybody came",
        json!({ "brought": "the good cups" }),
        &[],
    )
    .await;
    s.add("event:winter-fest", "The Winter Fest").await;
    s.event_with(
        "event:winter-fest",
        "stood behind a stall for four hours",
        json!({ "brought": "a folding table" }),
        &[],
    )
    .await;

    // **And the thing that is not an event at all, which is what makes the
    // kind a real filter rather than a decoration.** A jukebox honestly
    // carries a `played`: it is the word for what a jukebox does, and nobody
    // gets to reserve it for gigs. Nothing but the kind separates the two.
    s.add("thing:jukebox", "The Jukebox at Moe's").await;
    s.event_with(
        "thing:jukebox",
        "put a new record in it",
        json!({ "played": "the B side twice" }),
        &[],
    )
    .await;

    // ── today · the shape gets a name, months after the fact ────────────────
    //
    // A declaration is a name and a set of keys. **It says nothing about kind,
    // because it is not one**: no handle will ever read `gig:something`, and
    // none of the events below moves.
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "gig",
                "fields": [
                    { "key": "played", "holds": "text" },
                    { "key": "went", "holds": "text" },
                ],
            }),
        )
        .await;
    declared.says("\"name\":\"gig\"");
    declared.says("\"key\":\"played\"");
    declared.says("\"key\":\"went\"");

    // ── "which of my events were gigs?" ─────────────────────────────────────
    //
    // Two filters, one call: the kind says which things are even in the
    // question, and the schema says which of those answer it.
    let gigs = s
        .call(
            "search",
            json!({ "kind": "event", "answers_type": "gig", "limit": 50 }),
        )
        .await;

    // The two written whole, months before the word `gig` existed here.
    gigs.says("event:moe-open-mic");
    gigs.says("event:lisa-quartet-night");
    // The half-written one comes back saying what it lacks by name, rather
    // than being dropped: a gig I have not recorded going to is exactly the
    // one I want to be asked about.
    gigs.says("event:otto-benefit-show");
    gigs.says("\"lacking\":[\"went\"]");

    // …and the negatives, in the same answer that just proved it is not empty.
    // The birthday and the fest are events sharing no key with the shape.
    gigs.never_says("event:birthday-party");
    gigs.never_says("event:winter-fest");
    // **The one that carries the key and is not a gig.** The jukebox answers
    // `played` as honestly as any of them, and the kind is the only thing
    // keeping it out. Take the kind off this call and it comes back.
    gigs.never_says("thing:jukebox");

    // ── "and which did I actually go to?" ───────────────────────────────────
    //
    // The strict question over the same shape: only the evenings where both
    // halves got written down. The benefit show is out because nobody knows,
    // which is the honest answer rather than a missing "no".
    let settled = s
        .call(
            "search",
            json!({ "kind": "event", "fits_type": "gig", "limit": 50 }),
        )
        .await;
    settled.says("event:moe-open-mic");
    settled.says("event:lisa-quartet-night");
    settled.never_says("event:otto-benefit-show");
    settled.never_says("thing:jukebox");

    // ── the line this whole story rests on ──────────────────────────────────
    //
    // **A schema is not a kind, and the surface says so both ways.** `gig`
    // answers a question about things, and it is no more a kind today than it
    // was before it was declared: an entity cannot be created as one.
    s.refused(
        "add_entity",
        json!({
            "kind": "gig",
            "handle": "moe-open-mic",
            "name": "Open Mic Night at Moe's",
            "source": "user-named",
        }),
    )
    .await;

    // And the events are still events, which is what "no new kind is minted"
    // means when it is read back rather than asserted.
    let events = s.list("event").await;
    events.says("event:moe-open-mic");
    events.says("event:birthday-party");
}
