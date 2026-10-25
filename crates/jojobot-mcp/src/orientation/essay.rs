//! **The world-model a fresh agent reads** — engine prose, and nothing else.
//!
//! It explains the method in role language only ("the operator"), and every
//! example identity in it is fictional. It is a file of its own because it is
//! text rather than code, and because the one-door test counts how many places
//! reach for it: defined once, read once.

/// The core as written, with the kind sentence left as [`KINDS_MARK`].
const CORE_TEMPLATE: &str = r#"# jojobot — start here

jojobot is a personal-assistant server: the durable memory and message rail behind an assistant serving one person, the operator. You are one of possibly many AI sessions connected to it — jojobot itself never thinks; it stores, guards, and serves. What you write here outlives this conversation and will be read back as truth by sessions that cannot ask you what you meant.

## The two worlds

**MEMORY** is a typed graph of the operator's life. An **entity** is a noun — {kinds} — with a handle like `person:milhouse`, the name it is addressed by and the only name a caller ever sends. A **machine** is a computer — a server, a laptop, a router, a guest running on another machine. The kind names the OBJECT, not a role it plays, so a guest is a machine exactly as its host is, and which hosts which is a link between them. A **rhythm** is a recurring loop and it is one of the two kinds that REQUIRE a `parent` — the other is a **promise**, something somebody has to do by the day in `promised_by` — which says whose job it is: a maintenance loop under the thing maintained, a review loop under the bot that carries it. Two loops on one object are two rhythms, which is why they are entities, not a label. A **view** is a question asked by name: a query over this graph, held as a record, so asking for it is naming it. Some ship with the software and you can declare your own. A **thread** is an ongoing line in somebody's life — a situation, not a task — and it ends when it stops being TRUE, never when it is done: a `project` moves toward completion and ends there, a `topic` is the glue noun for a world-fact belonging to no person, place or project, and a thread is not the anchor of last resort. A **list** is ordered entries on a thing, a checklist or a queue. A **fact** is one dated claim about an entity, addressed `person:milhouse#f3`, carrying a **provenance**: `testimony` (the operator said or confirmed it), `observation` (an AI read it out of a system of record and says which one) or `inference` (an AI derived it). Inference is the default and reads back as a hypothesis; only the operator's explicit confirmation makes a claim testimony. **A claim also carries a `standing`, which answers a different question: `settled` or open.** Provenance says who backs it; standing says how sure anybody is. A claim nobody declared one on is stored with the standing its provenance implies. **An `observation` reads back as a finding rather than a hypothesis, and must name where it was read** — the field `read_from`, plus `read_ref` for what was read there if you have it — because who confirmed something and on what is one question, and a claim answering half of it is refused. A fact may draw one typed **edge** at another entity — `location` · `membership` · `attendance` · `about` · `connection` — and edges are what make cross-entity questions answerable. `connection` is the one to reach for when a link is there and how it relates was not recorded: filing that as `about` would state something nobody said. **`search` is the front door** to all of it — and to the messages in mailboxes too, when you ask for them with `include_mail: true`: one ranked list, one call.

**`sets` is what makes a key a property of a thing.** A write carries two bags. **A thing's fields are every `sets` write on it, folded, the newest write of each key winning (a counter sums)**; a write that takes a key off takes it off the thing, and a thing is found and filtered by those keys. A claim's `fields` are its own labels: they stay on that claim, and a filter on the thing will not find one. Six keys only label a claim (`read_from`, `read_ref`, `starred`, `subject`, `purpose`, `recorded_by`). Nothing need be declared; a key you invent is kept as written. A handle, or one in a comma list, is a link and must exist. **Carrying keys is what makes a thing ANSWER a type**, and answering one is not being held to one. Declare a type to say which keys it names; a thing holding all of them FITS it. Its keys refuse no write. **What a write may take off a thing is its KIND's question**: a kind may name keys that the things of that kind have to keep, and a write that would drop one of those is blocked, naming the key it would lose. A `rhythm` has to keep its `name` and `last_check_in`, and a `promise` its `promised_by`. A kind may also name a key that holds one of a named set: the `status` of a `project` or `work` holds someday, next, now, waiting or done and the words `columns` adds, and any other word lands, counts as not finished and comes back with the write that binds it. The closed lists on a loop, a promise and a view still refuse. **The floor is what a thing already holds**: a record written a piece at a time is never blocked on the way up. Declaring a type buys write-time help, the vocabulary search asks with, and ordering and traversal on its keys. `answers_type` selects things carrying SOME of a type's keys and says which each one lacks; `fits_type` keeps only the things with no gaps. Ask `answers_type` which of these are described like a pet, and `fits_type` which ARE pets.

## Writing it down, by example

- *"Remember that Milhouse is allergic to shellfish"* → `search` for milhouse to find the handle → `capture` subject `person:milhouse`, content the claim, provenance `testimony`, `observation` or `inference`. The gate is on promotion, not assertion — a first capture declares its own provenance on honour, so declare `testimony` only for the operator's words, and capture what a later session would need: a passing mention is not a fact.
- *"The diner's Thursday special now costs 12"* → `capture` subject `place:leftorium`, `sets` `{special_price: "12"}`, so a filter on `special_price` finds the diner.
- *A person, place, org or event the operator named that jojobot doesn't know* → `add_entity`, then the write: two deliberate steps, nothing created as a side effect. This is the normal, welcome move — the graph is meant to grow with the operator's life.
- *"The diner's booking page says they do a Thursday special"* → you read that yourself, in a system of record. `capture` subject `place:leftorium`, content the claim, provenance `observation`, fields `{read_from: "the diner's booking page", read_ref: "the specials section"}`. **The source rides on the claim**, so the claim's one edge stays free to say where it is or who it is about; a claim with no `read_from` is refused.
- *"Remember the diner does a Thursday special — I read it on their posted menu"* → **this one is the operator's word, and the menu is a thing in its own right.** `add_entity` the source: `thing:leftorium-menu`. Then `capture` subject `place:leftorium`, content the claim, provenance `testimony`, with `shape: about` and `object: thing:leftorium-menu`. **The edge is right here and wrong above:** the operator is telling you about a thing the store should know, and later claims will point at that same menu. **Reach for it when the source is an entity in the operator's life; reach for `observation` when it is a system you read.**
- *"I think the diner closes early on Sundays, but don't hold me to that"* → **the operator's own word, and undecided.** That is `capture` with provenance `testimony` and standing open — not `inference`, which would say an AI worked it out and quietly take the claim away from the operator. **The pairing nobody reaches for on their own is the operator's word, unsettled**: the only way to record somebody thinking aloud without overstating or misattributing it.
- *"Where did that come from?"* → `recall` the subject with `facts: true`. Each claim carries its own edge, so the source comes back beside the claim in one read. A claim you worked out from ANOTHER claim names that one in `derived_from` instead — a fact address like `place:leftorium#f1`, never an edge, because an edge points at an entity.

## Your session's frame

**Your session carries its own timezone, and a boot omits it.** The instance holds the operator's zone, written once as `timezone` on `topic:instance`, and answers every boot that sends none in it (UTC when there is none); the boot says which zone and who named it. Everything day-grained is answered in it: the date a `capture` gets when you name none, and whether a recurring loop has fallen due. Send `timezone` — an IANA name — only when the run really works in another zone: it wins, and the boot names both. A resume that sends none keeps its zone.

**And your session carries its own DAY, when it is not the day the server is having.** Pass `today` to `start_here` — a calendar day like `2026-03-15` — and everything in that run is in that day: a `capture` that names no date gets it, your beats are stamped with it, and the sweep that decides whether your other runs went quiet reads it. The zone says how to name a day; this says WHICH DAY YOU ARE IN, and no zone can tell the server that. **jojobot never derives it and never advances it.** **A write that names its own date still wins.**

Send it when your run is not happening now: a session catching up on last week, an instance restored from a backup. Send none and days come off the clock in your zone.

⚠️ **Two sessions in different zones WILL disagree about what today is for one stored claim, and that is correct.** A claim captured at nine in the evening in one zone is the 18th there and the 19th in a zone further east; a loop due today has arrived for the run whose day it already is and has not for the run still on yesterday. **It is not a fault and there is nothing to work around** — if you meet it and start compensating, you will write a wrong date into the store to fix a right one.

**Every answer may carry a `status_bar`**: `mail_waiting` (the count of new mail for you), `charter_changed` (the call that reads a charter another identity rewrote) and `mail_pile` (twenty read, not closed), each present only when there is something to say. **No block means none or unknown.** Beside it, `held_changed` (a key of the answer) names by `handle` or claim `address` what you were served that another session has since written to, with `claims_new` and `claims_edited` under a handle. Recall it before editing and send `expected_revision`.

**This is the short form.** The rest — mailboxes, more worked examples, the six gates, bots and sessions — comes from `start_here` naming no bot, which ships the sections that fit and names the rest; `section` returns one whole.
"#;

/// The mark in the core's text that stands for the kind sentence.
const KINDS_MARK: &str = "{kinds}";

/// **The kinds the core names as examples.** Each is a kind the store accepts,
/// which a test holds against [`EntityKind::ALL`].
pub(crate) const KIND_EXAMPLES: [&str; 4] = ["person", "place", "work", "bot"];

/// **The sentence the core carries about kinds, for `count` of them**: how many
/// there are, a few by name, the one call that lists those you can create and the
/// two you only read. Its length does not depend on the count, so a kind added
/// to the store costs the boot nothing. The essay and its tests call this one function.
pub(crate) fn kinds_sentence_for(count: usize) -> String {
    let examples = KIND_EXAMPLES
        .iter()
        .map(|kind| format!("`{kind}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "one of {count} kinds, for example {examples}; the `kind` argument of `add_entity` \
         lists the kinds you create; read `session` and `usage` with `recall`'s `kind`"
    )
}

use jojobot_domain::memory::EntityKind;

/// **The core of what `start_here` hands a fresh agent** — always served
/// whole, the same way a bot's charter is: the kinds, what a claim carries,
/// that a question can be asked structurally, the verbs a cold session
/// writes with, its own zone and day, and the call that reaches the rest.
/// Engine prose: the method, in role language only — no operator specifics,
/// fictional example identities.
///
/// **The boundary is drawn by what a cold session cannot act without, not
/// by what teaches the model best.** A first pass drew it at the essay's own
/// paragraph seam — kinds and fields, nothing past it — and three
/// pre-existing user stories (`unprompted`, `backdated`, `timezones`, in
/// `crates/jojobot/tests/user_stories/`) caught what that missed: a named
/// boot that never learns `add_entity`/`capture`/`recall` exist cannot write
/// anything, and one that never learns `timezone`/`today` cannot date a
/// claim correctly. Those stories are the acceptance criteria for this
/// boundary — they are pre-existing, realistic, already-approved assertions
/// about first contact, not this file's own taste.
pub(crate) static ORIENTATION_CORE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| core_for(EntityKind::ALL.len()));

/// **The core as it reads when the store accepts `count` kinds.** The one place the
/// kind sentence is put into the template.
fn core_for(count: usize) -> String {
    CORE_TEMPLATE.replace(KINDS_MARK, &kinds_sentence_for(count))
}

/// **The rest of the essay** — everything past the core: the small-answer
/// discipline, mailboxes, worked examples, the refusal shapes, bots and
/// sessions. **A named boot never gets this at all** — it is not a ranking
/// candidate for one, only a rule. An anonymous boot gets it joined with the
/// core, as the one candidate ranked against the boot's declared ceiling —
/// see `essay_for_boot` in `jojobot-mcp`'s `orient` module.
const REMAINDER_TEMPLATE: &str = r#"
**jojobot hands back the small answer and keeps the large one reachable.** A write returns a receipt rather than the thing you wrote; a message body is not echoed to its author; a delivery leaves out what it handed you once; prose is off by default on a read, because a page is bigger than a claim. The reason is always the same: context is the scarce thing in this conversation, and an answer that ships everything spends it on what you already have. **Eliding is never silent** — whenever less comes back, the answer says what was left out and which call returns it.

**MAILBOXES** are the async rail between sessions: named boxes where one session leaves a message another will find. A message is `new` → `read` → `processed`. Reading IS taking delivery (no peek); anything read but not yet processed comes back under `leftovers` (counted, five ids;`new_only: false` returns the bodies), so crashed work resurfaces on its own. `processed` means acted-on, and it is a terminal archive: nothing here is ever deleted. **A bot's box belongs to exactly one bot**, is named for it, and comes into being with it: a box states its owner, so whose it is is a fact you can read rather than an arrangement you have to be told. Only `add_entity` opens a bot's box, a boot repairs a missing one, and the operator's opens at the first `post_message` to them; no bot reads it back. **Yours is yours by construction**: booting as your identity is what tells you which box you drain. **Messages are searchable, on request**: `search` with `include_mail: true` finds them beside the memory hits, in every state, `processed` archives included (the operator's box alone is never searched). It is opt-in because a hit carries somebody's box, sender and a snippet, and `search` is the verb you reach for first; a finding filed for another session is reachable by anyone who asks the right question. `read_message` takes that one message without making the rest of the box yours — **from your own box**, because taking delivery of somebody else's mail moves it out of `new` and it never looks fresh to them again. A `processed` hit is the exception and is readable from any bot's box: that one is history, and reading it moves nothing.

## Working here, by example

In a mention, `@<collection>/<N>` links the member holding N in a numbered collection. A number nobody holds is refused with the numbers held.

- *"Milhouse and Nelson were both at the gorge stunt show"* → the operator named three things in one breath; write them as handles rather than as words so a later session can follow them: `capture` subject `event:gorge-stunt-show`, content `"@person:milhouse and @person:nelson were at it"`. A handle written this way is stored as the name that does not move, served back as whatever that thing is called today — the same three nouns written as plain prose leave words a later session has nothing to follow. Works the same in a claim's `details` and in a bot's charter prose.
- *"These three claims are really saying one thing now — mark the newest as standing for the others"* → `update_fact` the newest, with `stands_for` naming the older claims' addresses. This is a synthesis, not a citation: the named claims stay as they are, active and readable. `recall`'s facts listing leaves them out once the mark lands; `stood_for: true` returns them. The shape is inference, not testimony. `clear_stands_for` takes the mark off. A mark naming an address that does not exist, the record's own address, one already a shape, or an empty set is refused.
- *No mailbox fits what you want to leave* → write to the bot it concerns, or say plainly there is nowhere fitting and let the operator decide: **only a new bot opens a box**.
- *"Which people are in Shelbyville?"* → `search` with kind `person` and edge `{shape: location, object: place:shelbyville}` — an edge walk, not a text match.
- *"I mistyped that just now"* → `recall` the subject with `facts: true`, then `update_fact` rewrites the claim in place — the record becomes what it should have said, and the wrong wording stays readable through `history_record`. *"That changed"* or *"that was never true"* is a different move: the old claim was so once, or never was — `update_fact` the old claim with `status: archived` and `details` saying why ("confirmed by the operator: not allergic after all"), then `capture` the new one, naming the archived claim as its `derived_from` when there is a direct replacement. Archive it, never negate it: rewriting a claim into its own denial ("the club does NOT meet on Tuesdays" replacing "the club meets on Tuesdays") leaves a sentence about what is not so where a claim about what is true belongs. A negative is still an ordinary fact when it is not correcting anything — "he did not attend" stands on its own. A claim about a past event that turned out never to have happened is a third move: `retract` it, which leaves a dated record of the retraction.
- *Leave word for another session* → **you write to a BOT, not to a box.** The snapshot names the bots and `list_entities` with kind `bot` lists them all, so you can find out who is there; `post_message` to one of them, with a body written for a reader with none of your context. jojobot records the sender from the `sid` you pass.
- *Handle mail* → `read_mailbox`, which opens YOUR box — the `sid` you pass says which one, so there is no name to give and no way to reach into somebody else's. Reading takes delivery of every message in it; act, then `mark_processed`, ONLY after acting, with the outcome in notes. A failure is data to record, not a state to park in.
- *Is anything waiting?* → `read_mailbox` with `counts_only: true`. It reports your box's per-state counts and anything on it jojobot cannot read, and takes delivery of NOTHING: a poll that finds an empty box costs nothing and owes nothing. Poll with this; deliver with the call above.
- *One message, not a whole box* → `search` for it, then `read_message` on the id the hit carries. Draining your whole box makes every message in it owed work; `read_message` takes on the one you meant. A live message in somebody else's box comes back blocked.

When the right write is not obvious, ask the operator — an unasked write outlives the conversation that guessed it.

## The answers that are not errors

A **blocked** result is a SUCCESS whose body says `status: "blocked"`, `wrote: false`: nothing was written by the step refused (a several-step call under a `request_id` can land first parts: `how_to_proceed` then counts them and says to resend the same call with the same `request_id`); `how_to_proceed` says what to do next and `fix_by` who does it: `retry` (resend it unchanged; only then), `change` (change the call), `person` (a person has to repair it; neither a resend nor a change helps). Six gates produce it, with different ways out: **resemblance** (creating or renaming something that looks like what exists — pick the candidate you meant, or hand back the `override_token` that refusal carries, only when you can say how the two differ — a token lifts the one refusal that minted it and no other; an exact handle or box name is never overridable), **absence** (you named something that is not there — the subject of a capture, an edge's object, the box of a post, a handle to read, an address to edit, a message id to retire; empty `candidates` means nothing even resembles it, not that your call was malformed; for an entity, creating it and retrying is usually right — for a mailbox it usually is not), **ownership** (a mailbox has exactly one owner, and a second claim on one is refused naming the holder; an `override_token` does not clear this — it answers a question about names), **unreadable** (`mark_processed` reached an item jojobot cannot read — no retry helps, a person must repair it; treat what it carried as unhandled and say so), **shape** (a field is validated and what you sent is not what it can carry — a message's `subject` is one plain line of unformatted text; the answer names the rule; send the same call with that field fixed or left out), and **malformed** (a validator refused the arguments themselves — a claim with no content, a name that is no name, an id that is no id, a query that narrows nothing; the answer carries the validator's reason; send the same call with that fixed).

A plain **error** is a narrow thing: **a token this server cannot parse at all** — one naming no kind, no status, no provenance, no edge shape, a date that is no date, a string that is no fact address — or the store itself failing. Everything else a caller can get wrong comes back blocked. **You cannot always tell the two apart**: both are your call to fix, so read `status` first and treat an error on a malformed argument as the instruction the blocked answer would have given. **Absence is never an error here**: naming something that does not exist is an answer with candidates, not a broken server. The guards catch resemblance, absence and ownership, never judgement — a wholly novel name sails through, and nothing will stop you standing up an entity nobody needed. That call is yours, and the store keeps whatever you decide.

## Bots

An **identity** is an entity of kind `bot`: a handle like `bot:gamma`, a **charter** (its prose — what this identity is, its hard lines, where its work lives), **rules** as ordinary facts about it (so each one carries its own provenance: an inferred rule is a hypothesis, not a policy), and **one owned mailbox**, named for it and opened with it — not optional, and not separately created: an identity that cannot be written to is not one. If you were told which identity you are, pass that name to `start_here` — the one door — and it hands over everything here plus that identity. If you were NOT told, the snapshot names the bots and counts the rest under `bots_left_out` with the calls that list them all, so you need not guess a name; a refusal for a name that is no bot lists them the same way. Two bots ship, the assistant and defrag; every other bot is data somebody wrote.

## Sessions

A **session is one mortal run of a bot** — the unit of work, not the unit of connection. It outlives a disconnect and a device hop: what makes two connections one session is the `sid` you carry, so pass it on every call.

**Booting an identity starts or resumes its session; there is no separate verb.** `start_here` with your bot name sweeps that bot's stale sessions to `abandoned` (a day without a beat on the clock; two stated days for a run that states its own). If a resumable session remains you get the choice — what each was working on, and whether it is still running or stopped — and NO sid until you answer: `resume` takes the run's name (its `run`), `new` or a `wrap_code`: resume inherits its chronology and hands back a new `sid`, new mints a fresh `sid` beside it, closing nothing; a fresh role lease blocks a new claim even from another run of the same bot, until its holder wraps, the lease expires or a code the operator typed for that bot authorizes takeover. With nothing to resume the sid comes back straight away. Either way the record is written **lazily**, on your first real write; a boot that does nothing keeps only a note of its key.

A session has two halves. Its **focus** is what it is working on NOW, one line, rewritten in place. Its **chronology** is what happened: append-only, oldest first; you amend only your own newest entry.

- *Record a beat* → `journal` — **a literal journal, not a log.** What you did, found, and what went wrong; NOT every tool call or file: a reader months from now wants the story, and a firehose buries it. Pass `focus` when what you are working on changes.
- *Fix the beat you just wrote* → `amend_journal`. Only the most recent one; everything older is what it was.
- *End* → `wrap_session` with the story, written for somebody with none of your context. It becomes your final entry and the session goes `wrapped` — terminal both ways, except through the `wrap_code` its wrap hands back. It is published NOWHERE: your chronology is the record.

jojobot also writes **its own beats** into your chronology: one per class of WRITE you make, its count kept current. Reads are not journalled. They are marked apart (`beat` names the class) because what you said you did and what jojobot saw you do are different evidence.

**Claim a role** — the seat one session holds, kind `role` — by naming `claim` on `start_here`, to refuse a second claimant while yours is fresh: `taken`, `refused`, `unavailable` or `conflict` (a collision with another write; retry the same call). Every write and your mailbox polls renew it, at most every {{renewal}}; the lease is {{lease}}; wrapping releases it.

### The two endings, and they are not interchangeable

**WRAP when the work is over, or must move to another agent now.** The story is the handover: the wrap frees your role at once, and the next agent boots fresh and reads it. Never pass your `sid` on; it is your session's secret.

**CLEAR AND RESUME when the run may go quiet without a wrap** — a crash, a quota wall. **Journal a resume note as you go**: once the run has been quiet for the lease, the next boot of this identity is offered it by name and by what it says it is working on, and whoever picks it up reads your chronology. **A compaction is not a quiet run**: it is the same run without what it was taught and shown. Call `start_here` with `compacted: true` and your `sid`, and jojobot forgets what it believes you hold. The run, its key and its chronology stay.

The resume note is **the one sanctioned exception to journal leanness**. Everywhere else a beat is high-level; here, be dense and specific — where you got to, what you already ruled out, the exact next step, the thing that will bite whoever picks this up.

`abandoned` is neither of these, and it is **not a failure**: it means the run was never wrapped up. A session stops without telling its story — a disconnect, a closed laptop, an agent that moved on — and the next boot marks it so. Its chronology survives and is worth reading; **resuming it is ordinary, not recovery**. The difference between `wrapped` and `abandoned` is whether a run ended or merely stopped.

### Your box is yours; the others are not

**You read your OWN mailbox, and the surface offers no other**: `read_mailbox` opens the box your `sid` owns, with no name to pass, because looking in somebody else's would move their mail out of `new` and its real reader would never see it fresh.

The snapshot names up to twenty identities when you boot with no bot, and only yours when you boot as one: that is a fact about the board and **not an invitation**. Only your own comes back with counts — somebody else's queue is not yours to weigh. If you need something from a colleague, ask them — and know that `post_message` is not a pure write: it also takes delivery of YOUR box. Whatever was waiting rides back with the receipt under your_mail, out of `new` and yours to finish, because posting is the moment a reply is most likely to be waiting. A post lists only mail no read had handed you; mail you were already working is counted under `your_mail.leftovers`. Posting into your own box delivers nothing.
"#;

/// **The rest of the essay as it is served**: the template with the windows it
/// states written from their constants (see [`crate::windows`]).
pub(crate) static ORIENTATION_REMAINDER: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| crate::windows::fill(REMAINDER_TEMPLATE));

/// **The whole essay, core and remainder joined** — test-only. Production
/// never wants this: the floor always serves the core, and the remainder is
/// ranked against what is left, so the two are never concatenated outside a
/// test that means to check the text a session reads when nothing cut it.
#[cfg(test)]
pub(crate) fn orientation() -> String {
    format!(
        "{}{}",
        ORIENTATION_CORE.as_str(),
        ORIENTATION_REMAINDER.as_str()
    )
}

/// One `##`-section of a markdown text: its heading line itself (e.g.
/// `"## Bots"`) and everything from that line up to, but not including, the
/// next `##` heading — including any `###` subheading nested inside it.
pub(crate) struct MarkdownSection<'a> {
    pub(crate) heading: &'a str,
    pub(crate) body: &'a str,
}

fn is_level_2_heading(line: &str) -> bool {
    line.starts_with("## ")
}

/// **The remainder's own ranking units**: the un-headed preamble, then each
/// `##` section whole — carrying every `###` subheading nested under it
/// rather than splitting on it. **Only `##` starts a unit**: a `###` block
/// ships or drops with the `##` section it sits under, and is never named
/// on its own in a left-out note.
pub(crate) fn remainder_units(text: &str) -> (&str, Vec<MarkdownSection<'_>>) {
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        if is_level_2_heading(line.trim_end_matches('\n')) {
            starts.push(pos);
        }
        pos += line.len();
    }

    let preamble_end = starts.first().copied().unwrap_or(text.len());
    let preamble = &text[..preamble_end];

    let sections = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = starts.get(i + 1).copied().unwrap_or(text.len());
            let body = &text[start..end];
            let heading = body.lines().next().unwrap_or("").trim();
            MarkdownSection { heading, body }
        })
        .collect();

    (preamble, sections)
}

/// **`remainder_units` of the one remainder this build ships**, so every
/// caller wanting the essay's real units — `orient`'s own ranking, and
/// `start_here`'s section fetch — goes through this one place rather than
/// each naming the essay's own remainder constant again. The one-door
/// guard (`there_is_exactly_one_orientation_verb`) counts literal readers
/// of that constant; this keeps the count at the two it already pins — the
/// constant's own definition and this function — however many callers
/// this function itself gains.
pub(crate) fn remainder() -> (&'static str, Vec<MarkdownSection<'static>>) {
    remainder_units(ORIENTATION_REMAINDER.as_str())
}

#[cfg(test)]
mod tests {
    use jojobot_domain::memory::EntityKind;

    /// **The essay names every bot the software ships**, read off the list the seed
    /// creates them from, so a bot added there and left out of the sentence that
    /// says which bots ship fails here. Only the slug is asked for; the words
    /// around it are free to change.
    #[test]
    fn the_essay_names_every_bot_the_software_ships() {
        let essay = super::orientation();
        for (handle, _) in crate::seed::SHIPPED_BOTS {
            let slug = handle.trim_start_matches("bot:");
            assert!(
                essay.contains(slug),
                "the essay does not name the shipped bot {slug}"
            );
        }
    }

    /// **The core states how many kinds the store accepts, and the one call that
    /// lists them.** The count is read off [`EntityKind::ALL`] by the function
    /// the essay is built with, so it cannot say a number the store does not
    /// have, and a kind added later costs the boot nothing. Each example the
    /// sentence gives is a kind the store has.
    #[test]
    fn the_core_states_the_kind_count_and_every_example_is_a_kind() {
        let count = EntityKind::ALL.len();
        let sentence = super::kinds_sentence_for(count);
        assert!(
            sentence.contains(&format!("one of {count} kinds")),
            "the sentence does not state the count: {sentence}"
        );
        assert!(
            super::ORIENTATION_CORE.contains(&sentence),
            "the essay's core does not carry the sentence: {sentence}"
        );
        assert!(
            !super::orientation().contains(super::KINDS_MARK),
            "the mark for the kind sentence is still in the served text"
        );
        for example in super::KIND_EXAMPLES {
            assert!(
                EntityKind::ALL
                    .iter()
                    .any(|kind| kind.as_token() == example),
                "`{example}` is not a kind the store accepts"
            );
            assert!(
                names(&sentence, example),
                "`{example}` is not in {sentence}"
            );
        }
    }

    /// **The core alone states the count and names the call that lists the
    /// kinds.** A boot heavy enough to cut the essay's remainder must still tell
    /// a session where the vocabulary is.
    #[test]
    fn the_core_alone_names_the_call_that_lists_the_kinds() {
        let core = super::ORIENTATION_CORE.as_str();
        assert!(
            core.contains(&format!("one of {} kinds", EntityKind::ALL.len())),
            "the essay's core does not state the count of kinds"
        );
        assert!(
            core.contains("`kind` argument of `add_entity`"),
            "the essay's core does not name the call that lists the kinds"
        );
    }

    /// **Every kind the store ships is reachable from what the core says**: the
    /// ones `add_entity` creates are listed in its `kind` argument (held against
    /// [`EntityKind::ALL`] by the surface's own case), and each projected one is
    /// named in the core as read with `recall`. Dropping a kind from either
    /// place reddens a case.
    #[test]
    fn the_core_names_every_projected_kind_as_read_not_created() {
        let sentence = super::kinds_sentence_for(EntityKind::ALL.len());
        assert!(
            super::ORIENTATION_CORE.contains(&sentence),
            "the core does not carry the kind sentence"
        );
        assert!(
            sentence.contains("`recall`'s `kind`"),
            "the sentence does not say a projected kind is read with recall's kind"
        );
        for kind in EntityKind::ALL.into_iter().filter(|k| k.is_projected()) {
            assert!(
                names(&sentence, kind.as_token()),
                "the sentence does not name the projected kind `{}` as read",
                kind.as_token()
            );
        }
    }

    /// **The core is the template and one sentence whose length does not depend
    /// on how many kinds exist**, so a kind added to the store never grows the
    /// core past the ceiling. A core that listed the kinds again would be longer
    /// than that.
    #[test]
    fn the_core_does_not_grow_with_the_number_of_kinds() {
        assert_eq!(
            super::kinds_sentence_for(10).len(),
            super::kinds_sentence_for(99).len()
        );
        assert_eq!(
            super::kinds_sentence_for(100).len(),
            super::kinds_sentence_for(999).len()
        );
        assert_eq!(
            super::ORIENTATION_CORE.len(),
            super::CORE_TEMPLATE.len() - super::KINDS_MARK.len()
                + super::kinds_sentence_for(EntityKind::ALL.len()).len(),
            "the core carries something beyond the kind sentence in the mark's place"
        );
    }

    /// **A kind added to the store changes nothing in the core but the count.**
    /// The core for one more kind, with that count put back, is the core as it
    /// is, byte for byte: no name of the new kind is in it, and nothing moved.
    #[test]
    fn a_kind_added_changes_only_the_count_in_the_core() {
        let now = EntityKind::ALL.len();
        let more = super::core_for(now + 1);
        assert_ne!(more, *super::ORIENTATION_CORE, "the count is in the core");
        assert_eq!(
            more.replace(
                &format!("one of {} kinds", now + 1),
                &format!("one of {now} kinds")
            ),
            *super::ORIENTATION_CORE
        );
    }

    /// 🚨 **The core teaches `sets` before `fields`, and shows it in use.** A
    /// key sent as one of a claim's own fields stays on the claim and the thing
    /// does not hold it; paid runs lost 31 writes to that, having met the word
    /// `fields` first. So in the paragraph that teaches what a thing holds, the
    /// first of the two write arguments named is `sets`, and a worked example
    /// sends a pair in `sets`. **The negatives are the order and the example
    /// together**: a paragraph that named `fields` first, or an essay with no
    /// example sending `sets`, fails.
    #[test]
    fn the_core_teaches_sets_first_and_an_example_sends_it() {
        let core = super::ORIENTATION_CORE.as_str();
        let paragraph = core
            .split("\n\n")
            .find(|paragraph| paragraph.contains("`fits_type`"))
            .expect("the paragraph that teaches what a thing holds");
        let at = |word: &str| {
            paragraph
                .find(word)
                .unwrap_or_else(|| panic!("the paragraph does not name {word}"))
        };
        assert!(
            at("`sets`") < at("`fields`"),
            "the paragraph names `fields` before `sets`: {paragraph}"
        );
        let examples = core
            .split("## Writing it down, by example")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .expect("the worked examples");
        assert!(
            examples
                .lines()
                .any(|line| line.starts_with("- ") && line.contains("`sets`")),
            "no worked example sends a pair in `sets`: {examples}"
        );
    }

    /// **The word `sets` means one thing in the essay: the argument.** A sentence
    /// about "the sets on a loop" meant closed lists of values, beside `sets` the
    /// argument that decides what a thing holds, and a reader took one for the
    /// other. So outside backticks the bare word does not appear.
    #[test]
    fn the_essay_uses_the_word_sets_only_for_the_argument() {
        for (what, text) in [
            ("the core", super::ORIENTATION_CORE.as_str()),
            ("the remainder", super::ORIENTATION_REMAINDER.as_str()),
        ] {
            let mut outside = String::new();
            for (n, piece) in text.split('`').enumerate() {
                if n % 2 == 0 {
                    outside.push_str(piece);
                    outside.push(' ');
                }
            }
            let bare = outside
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| word.eq_ignore_ascii_case("sets"))
                .count();
            assert_eq!(bare, 0, "{what} uses the bare word sets {bare} times");
        }
    }

    /// **The core alone still teaches the structural type questions** —
    /// `answers_type` and `fits_type` — the same reasoning as the kind test
    /// above, applied to the other half of what a cut boot must not lose.
    #[test]
    fn the_core_alone_teaches_the_structural_type_questions() {
        for filter in ["answers_type", "fits_type"] {
            assert!(
                names(super::ORIENTATION_CORE.as_str(), filter),
                "the essay's core does not name `{filter}` — a boot that cuts the remainder \
                 would leave a session unable to ask the question it answers",
            );
        }
    }

    /// **Every type filter the surface publishes is taught by the prose a
    /// session reads, and no filter it does not publish is.**
    ///
    /// The same discipline as the kind test above, applied to the argument
    /// names instead of the enum: the set is READ OFF THE SERVED SCHEMA rather
    /// than written down here, so it cannot drift from the structs and a third
    /// filter is covered the day it ships.
    ///
    /// **Both directions, because each catches a different rot.** A filter the
    /// surface gained and the prose never learned is a capability no session
    /// finds — the door taught eight kinds after ten shipped for exactly that
    /// reason. A filter the prose still teaches after the surface dropped it is
    /// worse: a session follows the instruction and the same binary refuses it.
    ///
    /// It is narrowed to the `_type` arguments deliberately. They are the
    /// vocabulary of the model this door exists to teach, and the narrowing is
    /// what keeps the negative direction free of false alarms — every `_type`
    /// token in this prose is a filter, where a bare word could be anything.
    ///
    /// **Checked against the orientation essay only**, for the same reason
    /// the kind test above is: the connection-time instructions deliberately
    /// stopped naming `answers_type`/`fits_type`, so this does not ask them
    /// to.
    #[test]
    fn every_type_filter_the_surface_publishes_is_taught_by_the_prose() {
        let published: Vec<String> = crate::arguments::published_argument_names()
            .into_iter()
            .filter(|name| name.ends_with("_type"))
            .collect();
        assert!(
            published.len() >= 2,
            "the surface publishes {published:?} — this case is reading the wrong thing if the \
             type filters are not among them",
        );

        let taught: [(&str, String); 1] = [("the orientation essay", super::orientation())];
        for (what, text) in &taught {
            let text = text.as_str();
            for filter in &published {
                assert!(
                    names(text, filter),
                    "{what} does not name `{filter}`, which the surface publishes — a session \
                     that reads it cannot ask the question that argument answers",
                );
            }
            // The other direction: a filter this prose teaches must be one a
            // caller can actually send.
            for token in type_tokens(text) {
                assert!(
                    published.contains(&token),
                    "{what} teaches `{token}`, which no verb publishes — a session that follows \
                     it is refused by the same build that told it to",
                );
            }
        }
    }

    /// 🚨 **The connection-time instructions carry shape, never the growing
    /// vocabulary — and this is the guard that would have caught the defect
    /// this design replaced.**
    ///
    /// `INSTRUCTIONS` (`lib.rs`) is served to every client at connection, no
    /// call required, so it used to restate the same vocabulary the
    /// orientation essay teaches — and drifted: it named only two of the
    /// three provenance values, missing `observation` entirely, because a
    /// growing list copied by hand is exactly what goes stale. The fix is not
    /// to sync the two documents — they will always differ in length, and
    /// keeping them in step means hand-recompressing the long one on every
    /// addition, which is the habit that produced the drift. Instead
    /// `INSTRUCTIONS` stops naming specific values and points at
    /// `start_here` and each tool's own description for them.
    ///
    /// **Checked against the closed sets that can grow**, not against a
    /// pinned sentence: a value this test does not yet know about is exactly
    /// the case a hand-maintained list would miss, so the sets are read from
    /// the domain's own enums, the same way [`EntityKind::ALL`] already
    /// drives the presence check above.
    ///
    /// **Backtick-quoted only.** A bare `about` or `new` is ordinary English
    /// this text cannot avoid; a backtick-quoted `` `about` `` or `` `new` ``
    /// is this codebase's own convention for citing a literal wire value —
    /// every example elsewhere in this file and in `INSTRUCTIONS` itself
    /// quotes a value that way. A plain word-boundary check, the kind
    /// [`names`] does for kinds, would flag ordinary prose as a violation on
    /// a text this size; this is the narrower, reliable half of the same
    /// question.
    ///
    /// **Two categories are deliberately left unchecked, on purpose rather
    /// than by oversight.** Entity kind names are not reliably
    /// backtick-quoted in either document (see the list `INSTRUCTIONS` used
    /// to carry, in plain prose) and several of them are ordinary English
    /// words on their own — `work`, `thing`, `view`, `place` — so a
    /// word-boundary absence check would false-positive on prose this text
    /// cannot avoid using. And the refusal categories named in the
    /// orientation essay's six gates (resemblance, absence, ownership,
    /// unreadable, shape, malformed) have no matching closed set in code —
    /// that taxonomy is prose the essay itself invented, not a wire
    /// vocabulary — so checking their absence would mean pinning words this
    /// codebase chose to write, which is refused on its own terms.
    #[test]
    fn the_connection_instructions_never_enumerate_a_growing_vocabulary() {
        use jojobot_domain::mailbox::MessageState;
        use jojobot_domain::memory::{EdgeShape, Provenance};

        let provenance_tokens = [
            Provenance::Testimony,
            Provenance::Observation,
            Provenance::Inference,
        ]
        .map(Provenance::as_token);
        let edge_tokens = EdgeShape::ALL.map(EdgeShape::as_token);
        let mailbox_state_tokens = MessageState::ALL.map(MessageState::as_token);

        let guarded = provenance_tokens
            .iter()
            .chain(edge_tokens.iter())
            .chain(mailbox_state_tokens.iter())
            .copied()
            .chain(["answers_type", "fits_type"]);

        for token in guarded {
            let quoted = format!("`{token}`");
            assert!(
                !crate::INSTRUCTIONS.contains(&quoted),
                "the connection instructions cite {quoted} — a specific value from a set that \
                 grows, which is exactly the pattern that let `observation` fall out of the \
                 provenance list: {}",
                crate::INSTRUCTIONS
            );
        }
    }

    /// Every `snake_case_type` token this text presents in backticks, which is
    /// how it writes an argument a caller sends.
    fn type_tokens(text: &str) -> Vec<String> {
        text.split('`')
            .skip(1)
            .step_by(2)
            .flat_map(|quoted| {
                quoted
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|token| token.ends_with("_type"))
            .collect()
    }

    /// Whether the text names this token as a word of its own, rather than
    /// inside a longer one: `pet` must not be satisfied by `appetite`, and
    /// `org` must not be satisfied by `organising`.
    /// 🚨 **Discoverability, at the door.** The verb's own description names
    /// `stands_for` (checked in `update_fact`'s own test module); this is
    /// the other half — the orientation essay every fresh session reads
    /// before it ever sees a tool schema.
    #[test]
    fn stands_for_is_named_in_the_orientation_essay() {
        assert!(
            names(&super::orientation(), "stands_for"),
            "a session that reads only the essay has no way to find the synthesis mark"
        );
    }

    /// **The essay's claim that a wrapped run is terminal carries its one
    /// exception.** A wrap hands back a `wrap_code` that reopens the run for one
    /// last change, so the sentence that calls the end terminal has to name it,
    /// in the same sentence, or a reader takes the end for absolute.
    #[test]
    fn the_essay_names_the_wrap_code_where_it_calls_a_wrap_terminal() {
        let essay = super::orientation();
        let at = essay
            .find("terminal both ways")
            .expect("the essay says a wrapped run is terminal");
        let sentence = essay[at..].split(". ").next().expect("a sentence");
        assert!(
            names(sentence, "wrap_code"),
            "the exception rides in the sentence that states the rule: {sentence}"
        );
    }

    /// **`remainder_units` splits on `##` headings only and separates the
    /// un-headed preamble from the headed chunks** — a `###` inside a `##`
    /// block folds into its parent rather than starting a unit of its own:
    /// two units here, not three, and the nested `###` line is inside the
    /// first unit's body.
    #[test]
    fn remainder_units_folds_a_nested_subheading_into_its_parent_section() {
        let text = "intro line\nmore intro\n## First\nfirst body\n### Nested\nnested body\n## Second\nsecond body\n";
        let (preamble, units) = super::remainder_units(text);
        assert_eq!(preamble, "intro line\nmore intro\n");
        let headings: Vec<&str> = units.iter().map(|s| s.heading).collect();
        assert_eq!(headings, vec!["## First", "## Second"]);
        assert_eq!(
            units[0].body,
            "## First\nfirst body\n### Nested\nnested body\n"
        );
        assert_eq!(units[1].body, "## Second\nsecond body\n");
    }

    /// **Pinned against the real remainder's actual `##`-level units** — the
    /// `###` subheadings under Sessions fold into it rather than appearing
    /// as units of their own.
    #[test]
    fn remainder_units_reads_the_real_remainder_as_four_top_level_units() {
        let (_, units) = super::remainder_units(super::ORIENTATION_REMAINDER.as_str());
        let headings: Vec<&str> = units.iter().map(|s| s.heading).collect();
        assert_eq!(
            headings,
            vec![
                "## Working here, by example",
                "## The answers that are not errors",
                "## Bots",
                "## Sessions",
            ]
        );
        assert!(
            units[3].body.contains("### The two endings"),
            "the Sessions unit must carry its nested subheadings whole: {}",
            units[3].body
        );
    }

    /// **The essay tells a boot to omit its timezone and names no zone.** The
    /// instance holds the operator's zone, and an example zone is the thing a
    /// session copies into a call, so the paragraph that teaches the argument
    /// carries none: no word of the form `Area/City`. The omit instruction is
    /// pinned by its structure, the same way the argument's own description is.
    #[test]
    fn the_essay_tells_a_boot_to_omit_its_timezone_and_names_no_zone() {
        let essay = super::orientation();
        let taught = essay
            .lines()
            .find(|line| line.contains("carries its own timezone"))
            .expect("the essay teaches the session's timezone");
        assert!(
            taught.to_lowercase().contains("omit"),
            "the essay does not tell a boot to omit the timezone: {taught}"
        );
        let chars: Vec<char> = taught.chars().collect();
        assert!(
            !chars
                .windows(3)
                .any(|w| w[0].is_alphabetic() && w[1] == '/' && w[2].is_alphabetic()),
            "the essay's timezone paragraph names a zone, which a session copies: {taught}"
        );
    }

    fn names(text: &str, token: &str) -> bool {
        text.match_indices(token).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + token.len()..].chars().next();
            let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            edge(before) && edge(after)
        })
    }
}
