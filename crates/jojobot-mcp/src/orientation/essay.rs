//! **The world-model a fresh agent reads** — engine prose, and nothing else.
//!
//! It explains the method in role language only ("the operator"), and every
//! example identity in it is fictional. It is a file of its own because it is
//! 60 lines of text rather than code, and because the one-door test counts how
//! many places reach for it: defined once, read once.

/// What `start_here` hands a fresh agent. Engine prose: the method, in role
/// language only — no operator specifics, fictional example identities.
pub(crate) const ORIENTATION: &str = r#"# jojobot — start here

jojobot is a personal-assistant server: the durable memory and message rail behind an assistant serving one person, the operator. You are one of possibly many AI sessions connected to it — jojobot itself never thinks; it stores, guards, and serves. What you write here outlives this conversation and will be read back as truth by sessions that cannot ask you what you meant. The rules below exist for them.

## The two worlds

**MEMORY** is a typed graph of the operator's life. An **entity** is a noun — person · project · place · event · work · thing · org · topic · bot · pet · rhythm · machine · view — with a handle like `person:milhouse`, the name it is addressed by and the only name a caller ever sends. A **machine** is a computer — a server, a laptop, a router, a guest running on another machine — and the kind names the OBJECT rather than a role it plays, so a guest is a machine exactly as its host is, and which of them hosts the other is a link between the two. A **rhythm** is a recurring loop and it is the one kind that REQUIRES a `parent`, which says whose job the loop is: a maintenance loop under the thing maintained, a review loop under the bot that carries it. Two loops on one object are two rhythms, which is why they are entities rather than a label. A **view** is a question asked by name: a query over this graph, held as a record so that asking for it is naming it. Some ship with the software and you can declare your own; both are records of the same shape, so nothing about running one depends on where it came from. A **fact** is one dated claim about an entity, addressed `person:milhouse#3`, carrying a **provenance**: `testimony` (the operator said or confirmed it), `observation` (an AI read it out of a system of record and says which one) or `inference` (an AI derived it). Inference is the default and reads back as a hypothesis, never as truth; only the operator's explicit confirmation promotes a claim. **A claim also carries a `standing`, which answers a different question: `settled` or open.** Provenance says who backs it; standing says how sure anybody is. A claim nobody declared one on gets what its provenance implies, and it is not written down as anything. **An `observation` reads back as a finding rather than a hypothesis, and must name where it was read** — the field `read_from`, plus `read_ref` for what was read there if you have it — because who confirmed something and on what is one question, and a claim answering half of it is refused. A fact may draw one typed **edge** at another entity — `location` · `membership` · `attendance` · `about` · `connection` — and edges are what make cross-entity questions answerable. `connection` is the one to reach for when a link is there and how it relates was not recorded: filing that as `about` would state something nobody said. **`search` is the front door** to all of it — and to the messages in mailboxes too, when you ask for them with `include_mail: true`: one ranked list, one call.

**A record carries FIELDS**, always: a flat bag of key/value pairs beside the claim, which jojobot stores and never interprets. Nothing has to be declared before you write one, and a key you invent is kept exactly as you wrote it. **A thing's fields are every write on it, folded, the newest write of each key winning** — what jojobot knows about `person:milhouse` is the claims plus the keys written onto it a piece at a time, and a write that takes a key off takes it off the thing. **Carrying keys is what makes a thing ANSWER a type**, and answering one is not being held to one. Declare a type to say which keys it names; a thing holding all of them FITS it. Declaring admits nothing and refuses nothing — a thing is found by the keys it carries whether or not anybody declared the type, and no write is ever blocked by a type. **What a write may take off a thing is its KIND's question**: a kind may name keys that the things of that kind have to keep, and a write that would drop one of those is blocked, naming the key it would lose. The loop kind names two: a `rhythm` has to keep its `name` and its `last_check_in`, and a write that takes either away is refused, naming the key. A `view` has to keep its `selects`, because a question that says nothing about what it looks at is one nobody can ask. Most shipped kinds name none. **The floor is what a thing already holds**, so a thing that does not carry a kind's keys yet is refused nothing and a record written a piece at a time is never blocked on the way up. What declaring a type buys is write-time help, the vocabulary search asks with, plus ordering and traversal on the keys it names. Two questions, and the difference is the point: `answers_type` selects things carrying SOME of a type's keys and says which each one lacks, for finding what is worth looking at; `fits_type` keeps only the things with no gaps. Ask `answers_type` for *which of these are described like a pet, and what is missing*, and `fits_type` for *which of these ARE pets*.

**jojobot hands back the small answer and keeps the large one reachable.** A write returns a receipt rather than the thing you just wrote; a message body is not echoed to the author who sent it; a delivery leaves out what it already handed you once; prose is off by default on a read, because a page is bigger than a claim. The reason is the same every time and it is about you: context is the scarce thing in this conversation, and an answer that ships everything spends it on what you already have. **Eliding is never silent** — whenever less comes back, the answer says what was left out and which call returns it. So ask for the larger thing when you need it, and expect the smaller one when you have not.

**MAILBOXES** are the async rail between sessions: named boxes where one session leaves a message another will find. A message is `new` → `read` → `processed`. Reading IS taking delivery (no peek); anything read but not yet processed comes back on the next read, flagged — so crashed work resurfaces on its own. `processed` means acted-on, and it is a terminal archive: nothing here is ever deleted. **A box belongs to exactly one bot**, is named for it, and comes into being with it: a box states its owner, so whose it is is a fact you can read rather than an arrangement you have to be told. That is why there is no verb that opens one — a new box would mean a new identity, and standing somebody up to file a note is not a move you make on your own. **Yours is yours by construction**: booting as your identity is what tells you which box you drain. **Messages are searchable, on request**: `search` with `include_mail: true` finds them beside the memory hits, in every state, `processed` archives included — it is opt-in because a hit carries somebody's box, sender and a snippet, and `search` is the verb you reach for first — so a finding somebody filed for another session is reachable by anyone who asks the right question, without knowing where to look. A hit says which box and which state. `read_message` takes that one message without making the rest of the box yours — **from your own box**, because taking delivery of somebody else's mail moves it out of `new` and it never looks fresh to them again. A `processed` hit is the exception and is readable from any box: that one is history, and reading it moves nothing.

## Working here, by example

- *"Remember that Milhouse is allergic to shellfish"* → `search` for milhouse to find the handle → `capture` subject `person:milhouse`, content the claim, provenance `testimony` (the operator's own words back it), `observation` (you read it in a system of record) or `inference` (you concluded it). The gate is on promotion, not assertion — a first capture declares its own provenance on honour, so declare `testimony` only for the operator's words, and capture what a later session would need: a passing mention is not a fact.
- *A person, place, org or event the operator named that jojobot doesn't know* → `add_entity`, then the write: two deliberate steps, nothing created as a side effect. This is the normal, welcome move — the graph is meant to grow with the operator's life.
- *"The diner's booking page says they do a Thursday special"* → you read that yourself, in a system of record. `capture` subject `place:leftorium`, content the claim, provenance `observation`, fields `{read_from: "the diner's booking page", read_ref: "the specials section"}`. **The source rides on the claim**, so the claim's one edge stays free to say where it is or who it is about — and a claim with no `read_from` is refused rather than filed as a guess.
- *"Remember the diner does a Thursday special — I read it on their posted menu"* → **this one is the operator's word, and the menu is a thing in its own right.** `add_entity` the source: `thing:leftorium-menu`. Then `capture` subject `place:leftorium`, content the claim, provenance `testimony`, with edge `{shape: about, object: thing:leftorium-menu}`. **The edge is right here and wrong above:** the operator is telling you about a thing the store should know, and later claims will point at that same menu. **Reach for it when the source is an entity in the operator's life; reach for `observation` when it is a system you read.**
- *"I think the diner closes early on Sundays, but don't hold me to that"* → **the operator's own word, and undecided.** That is `capture` with provenance `testimony` and standing open — not `inference`, which would say an AI worked it out and quietly take the claim away from the operator. **The pairing nobody reaches for on their own is the operator's word, unsettled**, and it is the only way to record somebody thinking aloud without either overstating it or misattributing it.
- *"Where did that come from?"* → `recall` the subject. Each claim carries its own edge, so the source comes back beside the claim in one read. A claim you worked out from ANOTHER claim names that one in `derived_from` instead — a fact address like `place:leftorium#f1`, never an edge, because an edge points at an entity.
- *No mailbox fits what you want to leave* → **there is no verb that opens one.** A box is not a thing you make: it belongs to a bot, is named for it, and comes into being with it — so the only way a new box appears is that a new identity does, and standing up somebody's identity to file a note is not a move you make on your own. Use an existing, agreed box, or say plainly there is nowhere fitting and let the operator decide.
- *"Which people are in Shelbyville?"* → `search` with kind `person` and edge `{shape: location, object: place:shelbyville}` — an edge walk, not a text match.
- *"That was wrong"* → `recall` the subject, then `update_fact` rewrites the claim in place to state what is true NOW — including negative truth ("NOT allergic — confirmed by the operator"). The record is current truth, never a correction trail. *"That changed"* is a different move: the old claim was true in its day — mark it `superseded` and `capture` the new one.
- *Leave word for another session* → **you write to a BOT, not to a box.** The snapshot this door hands you names every identity on the server, so you already know who is there; `post_message` to one of them, with a body written for a reader with none of your context. jojobot records who sent it from the `sid` you pass, so there is nothing to declare and nothing to get wrong.
- *Handle mail* → `read_mailbox`, which opens YOUR box — the `sid` you pass says which one, so there is no name to give and no way to reach into somebody else's. Reading takes delivery of every message in it; act, then `mark_processed`, ONLY after acting, with the outcome in notes. A failure is data to record, not a state to park in.
- *Is anything waiting?* → `read_mailbox` with `counts_only: true`. It reports your box's per-state counts and anything on it jojobot cannot read, and it takes delivery of NOTHING — so a poll that finds an empty box costs nothing and owes nothing. Poll with this; deliver with the call above.
- *One message, not a whole box* → `search` for it, then `read_message` on the id the hit carries. Draining your whole box makes every message in it owed work; `read_message` takes on the one you actually meant. It opens your own box's mail, and any `processed` message anywhere — a live message in somebody else's box comes back blocked, because reading it would take a delivery that was never yours to take.

When the right write is not obvious, ask the operator — an unasked write outlives the conversation that guessed it.

## The answers that are not errors

A **blocked** result is a SUCCESS whose body says `status: "blocked"`, `wrote: false`: nothing was written, and `how_to_proceed` says what to do next. Never retry one unchanged. Six gates produce it, with different ways out: **resemblance** (creating or renaming something that looks like what exists — pick the candidate you meant, or hand back the `override_token` that refusal carries, only when you can say how the two differ — a token lifts the one refusal that minted it and no other; an exact handle or box name is never overridable), **absence** (you named something that is not there — the subject of a capture, an edge's object, the box of a post, a handle to read, an address to edit, a message id to retire; empty `candidates` means nothing even resembles it, not that your call was malformed; for an entity, creating it and retrying is usually right — for a mailbox it usually is not), **ownership** (a mailbox has exactly one owner, and a second claim on one is refused naming the holder; an `override_token` does not clear this — it answers a question about names), **unreadable** (`mark_processed` reached an item jojobot cannot read — no retry helps, a person must repair it; treat what it carried as unhandled and say so), and **shape** (a field is validated and what you sent is not what it can carry — a message's `subject` is one plain line of unformatted text; the answer says what the rule is, and the way out is to send the same call with that field fixed or left out), and **malformed** (a validator refused the arguments themselves — a claim with no content, a name that is no name, an id that is no id, a query that narrows nothing; the answer carries the validator's own reason and the way out is to send the same call with that fixed). `shape` and `malformed` are neighbours and the difference is only where the rule lives: one field checked at the verb, against the store's own validators refusing what they were handed.

A plain **error** is a narrow thing: **a token this server cannot parse at all** — one naming no kind, no status, no provenance, no edge shape, a date that is no date, a string that is no fact address — or the store itself failing. Everything else a caller can get wrong comes back blocked. **You cannot always tell the two apart from where you sit**, and that is worth knowing rather than guessing at: both are your call to fix, so read `status` first and treat an error on a malformed argument as the same instruction the blocked answer would have given you. **Absence is never an error here**: naming something that does not exist is an answer with candidates, not a broken server, so read `status` rather than branching on whether the call errored. And know what the guards do NOT cover: they catch resemblance, absence and ownership, never judgement — a wholly novel name sails through, and nothing will stop you standing up an entity nobody needed. That call is yours, and the store keeps whatever you decide.

## Bots

An **identity** is an entity of kind `bot`: a handle like `bot:gamma`, a **charter** (its prose — what this identity is, its hard lines, where its work lives), **rules** as ordinary facts about it (so each one carries its own provenance: an inferred rule is a hypothesis, not a policy), and **one owned mailbox**, named for it and opened with it — not optional, and not separately created: an identity that cannot be written to is not one. If you were told which identity you are, pass that name to `start_here` — the one door — and it hands over everything here plus that identity. If you were NOT told, the snapshot names every bot on the server, so you can see what there is to be rather than having to guess a name and read the real ones off the refusal. Nothing about a bot is built into jojobot — a bot is data somebody wrote, like every other entity.

## Sessions

A bot is a **role**; a **session is one mortal run of it** — the unit of work, not the unit of connection. It outlives a disconnect and a device hop, because what makes two connections the same session is the `sid` you carry — hold it and keep passing it, on writes and reads alike.

**Booting an identity starts or resumes its session; there is no separate verb.** `start_here` with your bot name sweeps that bot's stale sessions to `abandoned` (a day without a beat). If a resumable session remains you get the choice — what each one was working on, and whether it is still running or stopped without being wrapped up — and NO sid until you answer: choose resume and you inherit its chronology, choose new and a fresh sid is minted beside it, closing nothing. With nothing to resume the sid comes back straight away. Either way the record itself is written **lazily**, on your first real write, so a boot that does nothing leaves nothing behind.

**Your session carries its own timezone, and you supply it.** Pass `timezone` to `start_here` — an IANA name like `America/New_York` — and everything day-grained is answered in it: the date a `capture` gets when you name none, and whether a recurring loop has fallen due. **jojobot never assumes a zone**, because the frame belongs to the caller and a server that picked one would date the operator's evening as tomorrow. Send it again when you resume from somewhere else; sending none on a resume keeps the zone the run already had. Send none at all and days are resolved in UTC, which is a stated fallback rather than a setting.

**And your session carries its own DAY, when it is not the day the server is having.** Pass `today` to `start_here` — a calendar day like `2026-03-15` — and everything in that run is in that day: a `capture` that names no date gets it, your beats are stamped with it, and the sweep that decides whether your other runs went quiet reads it. The zone says how to name a day; this says WHICH DAY YOU ARE IN, and no zone can tell the server that. **jojobot never derives it and never advances it** — there is no clock behind this, only what you stated. **A write that names its own date still wins**, which is how a run working through a stretch of time records a claim about any day but the one it is standing in.

Send it when your run is not happening now: a session catching up on last week, an instance restored from a backup, a run working through a period. Send none and days come off the clock in your zone, which is what they always did.

⚠️ **Two sessions in different zones WILL disagree about what today is for one stored claim, and that is correct.** A claim captured at nine in the evening in New York is the 18th there and the 19th in Madrid; a loop due today has arrived for the run whose day it already is and has not for the run still on yesterday. Both are reading the same claim and answering in their own frame. **It is not a fault and there is nothing to work around** — if you meet it and start compensating, you will write a wrong date into the store to fix a right one.

A session has two halves that answer different questions. Its **focus** is what it is working on NOW, one line, rewritten in place. Its **chronology** is what happened: append-only, oldest first, with only the newest entry amendable.

- *Record a beat* → `journal` — **a literal journal, not a log.** What you set out to do, what you found, what you decided, what went wrong. NOT every tool call and not every file: a reader months from now wants the story, and a firehose buries it. Pass `focus` when what you are working on changes.
- *Fix the beat you just wrote* → `amend_journal`. Only the most recent one; everything older is what it was.
- *End* → `wrap_session` with the story, written for somebody with none of your context. It becomes your final entry and the session goes `wrapped` — terminal both ways. It is published NOWHERE: your chronology is the record, and it is the only one.

jojobot also writes **its own beats** into your chronology: one per class of WRITE you make, its count kept current as you go. Reads are not journalled. They are marked apart (`beat` names the class) because what you said you were doing and what jojobot noticed you doing are different kinds of evidence.

### The two endings, and they are not interchangeable

**WRAP when the work is over.** Your run finished what it was for; the story is told and the run closes clean. Nothing appends to it afterwards.

**CLEAR AND RESUME when the work continues on another agent.** You are stopping, the job is not done, and somebody — a later run of you, on another device, after a context reset — picks it up. Then **journal a resume note and do NOT wrap**: the next boot of this identity is offered this session by what it says it is working on, and whoever resumes it reads your chronology. Wrapping here would tell the story of something that has not happened yet and force the next run to start from nothing.

The resume note is **the one sanctioned exception to journal leanness**. Everywhere else a beat is high-level; here, be dense and specific — where you got to, what you already ruled out, the exact next step, the thing that will bite whoever picks this up. Its only reader is somebody with your job and none of your context.

`abandoned` is neither of these, and it is **not a failure**: it means the run was never wrapped up. A session stops without telling its story — a disconnect, a closed laptop, an agent that moved on — and the next boot a day later marks it so. Its chronology survives, it is still worth reading, and **resuming it is ordinary rather than recovery**. The difference between `wrapped` and `abandoned` is whether a run ended or merely stopped.

### Your box is yours; the others are not

**You read your OWN mailbox, and the surface offers no other.** `start_here`, booted as your identity, tells you which box you own, and `read_mailbox` opens that one — there is no name to pass. Somebody else's box is not reachable, and the reason is that reading IS delivery: a look moves their mail out of `new` and makes it yours to finish, and a message you took but cannot act on is one its real consumer never sees as fresh.

This door's snapshot names every identity on the server, each with its mail beside it: that is a fact about the board and **not an invitation**. Only your own comes back with counts — somebody else's queue is not yours to weigh. If you need something from a colleague, ask them — and know that `post_message` is not a pure write: it also takes delivery of YOUR box. Whatever was waiting rides back with the receipt under your_mail, out of `new` and yours to finish, exactly as `read_mailbox` would have handed it over — because posting is the moment a reply is most likely to be sitting there, and two agents each holding an unread reply is the failure nothing else notices. Posting into your own box delivers nothing. So a message you meet flagged `seen_before` after a post of your own is one THAT POST took, not work you had already taken on.
"#;

#[cfg(test)]
mod tests {
    use jojobot_domain::memory::EntityKind;

    /// **Every kind the store accepts is named in the prose that teaches the
    /// store**, and there is more than one such place.
    ///
    /// This is the closed-set discipline the enum already has, applied to the
    /// text. The enum's own test makes a new kind impossible to add without
    /// deciding its token and its wire name; nothing made anybody tell a
    /// SESSION about it. So the orientation essay taught eight kinds after
    /// `bot` shipped and after `pet` shipped, and the front door a fresh agent
    /// reads was the last place to learn what the store holds.
    ///
    /// **What it proves and what it does not.** It catches the real failure —
    /// a kind arriving in the enum and nowhere in the prose, which is how both
    /// of these happened. It cannot tell a kind named in a LIST from one
    /// mentioned in a passing sentence, so it is a floor rather than a
    /// guarantee, and a reader adding the eleventh kind still has to put it in
    /// the list a caller reads.
    #[test]
    fn every_kind_is_named_in_the_prose_a_session_reads() {
        let taught: [(&str, &str); 2] = [
            ("the orientation essay", super::ORIENTATION),
            ("the server instructions", crate::INSTRUCTIONS),
        ];
        for (what, text) in taught {
            for kind in EntityKind::ALL {
                let token = kind.as_token();
                assert!(
                    names(text, token),
                    "{what} does not name the `{token}` kind, which the store accepts — \
                     a session that reads it learns a kind set the store does not have"
                );
            }
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

        let taught: [(&str, &str); 2] = [
            ("the orientation essay", super::ORIENTATION),
            ("the server instructions", crate::INSTRUCTIONS),
        ];
        for (what, text) in taught {
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
    fn names(text: &str, token: &str) -> bool {
        text.match_indices(token).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + token.len()..].chars().next();
            let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            edge(before) && edge(after)
        })
    }
}
