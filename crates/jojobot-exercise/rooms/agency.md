# The agency room

**A probe, not a proof.** One employee at an agency records everything as nine
months pass, across four cold sittings that are each dense, and is asked
questions that only a store that kept the right thing, and handed it back, can
answer. **It is built to fail on the product.** Its reds are the deliverable,
because they decide which project-management keys jojobot ships on `work` and
`project`. It is not built to fail on the harness, and it is not built to be
won.

## Two projects, one employee

The agency's client campaign, and beside it the agency's own ordering app for
that client, which the same employee also runs. Every sitting carries both and
stays dense, at about fifteen items. There are no extra sittings.

jojobot stores and retrieves. **The agent reasons**, so it does its own
arithmetic and no lock expects jojobot to sum, count or subtract.

## What each sitting is for

* **January** — the brief. A budget that is hearsay, deliverables with
  owners, an ordering, four dated milestones, one market, a legal rule, what
  done means, and a contact report that is never answered. For the app: four
  phases that each depend on the one before, a decision in quotable words with
  the rejected option beside it, and one question drafted and never asked.
* **May** — slips and a derived requirement. Two slips on the film, markets
  added by word of mouth, a requirement the employee worked out from a quote,
  and the first app phase landed as a named commit and verified by a named
  suite.
* **September** — what was misread and what was never written. The budget is
  corrected, an approval is conditional, a decision made in a meeting was never
  recorded, a third slip, and the quote the requirement rested on was misread.
* **December** — the retrospective. Every question names the slot its answer
  goes in, and the lock reads that slot.

## What a red can be blamed on, fixed before any run

**Every lock has a verdict taxonomy.** A red is one of three, and the room
says which is possible before anyone has run it:

* **PRODUCT** — the store could not hold it, or could not hand it back.
* **OCCUPANT** — it was stored, and the model got it wrong.
* **SUITE** — the room asked an unfair question.

| Slot | Read from | What a red may be blamed on |
| --- | --- | --- |
| `budget_truth` | the key the entry names | OCCUPANT only. A key and a number are both storable. |
| `owners` | five locks, one per pairing: the film and its owner, the film's copy and its owner, print, social, the launch event | PRODUCT: no edge shape says owns, so a model that writes a name into a sentence, or a list into one value, has nothing a walk can follow. OCCUPANT if a key holding the handle was available and it wrote text. **A red is expected.** |
| `approval` | the key the entry names | PRODUCT if a conditional approval has no place to be stored. OCCUPANT if it was stored as unconditional. |
| `spot_done` | the key the entry names | OCCUPANT. The definition of done was said in January; deciding is reasoning. PRODUCT only if the three conditions had no home at all. |
| `slips` | the key the entry names | PRODUCT if a milestone's re-dating left no history to count. OCCUPANT if the history is there and the count is wrong. |
| `undelivered` | the key the entry names | OCCUPANT if the commitments are stored. PRODUCT if a dropped-but-never-withdrawn item has no way to be stored as still owed. |
| `app_next` | the key the entry names | PRODUCT if neither dependency nor waiting could be expressed. |
| `menu_first` | the key the entry names | OCCUPANT. The words and the rejected option are both prose that can be kept. |
| `rests_on_misread` | the key the entry names | PRODUCT if lineage could not be stored. OCCUPANT if the derivation was stored as the operator's word. May's lock reads the pointer to the quote; September's reads the requirement as taken back. |
| `never_asked` | the key the entry names | OCCUPANT. The drafted question and the asked one are both stored. The lock reads that the provider question is there; whether the slot also names the offline question is a transcript read. |
| `app_landed` | the key the entry names | OCCUPANT. A commit and a suite are plain values. |
| Radio | nobody | **Never mentioned, so there was none.** A transcript read and unlockable: no lock can ask about what nobody said. |

Each sitting's own locks are floors. **They say what the sitting itself stored**,
so December's reds read against a column of greens.

## The identity rule — binding, and the same as every room's

**The model boots as the shipped `assistant` and stays there.** No charter is
written for it, and no hand-authored orientation reaches it. The entry names no
verb and no place to look.

## What the room cannot measure

**Whether the radio never came up for the right reason.** The question is asked
in December and the answer is in the transcript.

**A lock that reads a slot reads the value the model wrote there.** A model
that answers under the right key with the wrong words is an OCCUPANT red. A
model that answers in its reply and writes nothing is the same red as a
model that never knew.

**The refused calls are the instrument for where a write failed to store.**
They are marked in the per-sitting log. Read them beside every red.

## What the room holds before the occupant arrives

Almost nothing. The people the agency already has, the client and the other
team, each under the one name the entries use for it. Everything else is built
by the sittings.

```world
entity  person:milhouse | Milhouse
entity  person:nelson | Nelson
entity  person:ralph | Ralph
entity  person:homer | Homer
entity  person:lisa | Lisa
entity  person:martin | Martin
entity  person:krusty | Krusty
entity  person:burns | Burns
entity  org:krusty-burger | Krusty Burger
entity  org:moes-tavern | Moe's Tavern
```

## Phase 1 — January, the brief

**Session: fresh.** **Day: 2026-01-15.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 15 January 2026 and I am starting two things at the agency. The Krusty Burger launch campaign launches on 12 October and that date never moves. Krusty says Burns approved about 200k for it, but nothing is in writing. The deliverables: the TV spot, 30 seconds, owned by Milhouse with the copy by Nelson; print ads, Milhouse; social, Nelson; the launch event, Ralph. The order is copy, then design, then production, then the media buy, with copy due 1 March, design 15 April, production 1 July, and the media booked by 15 August. One market to start, Alpha. Lisa approves everything legal and needs six weeks for any product claim. The TV spot is done when Lisa has approved it, legal has cleared it, and it has gone to the stations in three formats. I will send Krusty a contact report tomorrow, 16 January. The second thing is our own ordering app for Krusty Burger, with Martin as the engineer: four phases, each depending on the one before, login, then menu sync, then ordering, then payments. Homer, who runs the agency, has decided menu sync goes before ordering, and said "we sync the menu first: an order for a menu we have not synced is an order for nothing". Martin wanted ordering first on a hard-coded menu and that is turned down. I have one question drafted that I am not asking yet: does ordering need an offline mode? It blocks ordering.

```locks
# January's floors. Each asks, at the end of this sitting, what the sitting
# itself stored, and each is the question the later sittings depend on. A date
# is asked as a VALUE some record holds under any key: the answer then lists
# only records holding it, so a date written into a sentence and nowhere else
# is absent, and nothing else in this room carries these days.
#
# A positive floor reading green is what lets December's reds read as the
# product's and not the harness's.
#
# 🚨 **A needle in prose drops its first letter when a sentence might begin
# with it.** `raft 3` is in "draft 3" and "Draft 3", `ix weeks` is in "six
# weeks" and "Six weeks". A needle that was only the lowercase word would make
# a model that capitalised at the start of a claim a PRODUCT red, which is the
# harness blaming the product. The fragment can match inside a longer word, and
# every one below was read for that.
#
# **The same goes for a name the model may link instead of typing**: a handle
# is stored in lower case, so `avern` is in "Tavern" and in `org:moes-tavern`,
# and `amma` is in "Gamma" and in `place:gamma`.

recall {"fields": [{"value": "2026-10-12"}]}
carries 2026-10-12
say     January: no record holds the launch day as a value, so the one date that never moves is in nothing a later question can select on
window  phase-end

recall {"fields": [{"value": "2026-03-01"}]}
carries 2026-03-01
say     January: no record holds the copy milestone as a value, so a slip against it has nothing to be a slip from
window  phase-end

recall {"fields": [{"value": "2026-04-15"}]}
carries 2026-04-15
say     January: no record holds the design milestone as a value, so a slip against it has nothing to be a slip from
window  phase-end

recall {"fields": [{"value": "2026-07-01"}]}
carries 2026-07-01
say     January: no record holds the production milestone as a value, so a slip against it has nothing to be a slip from
window  phase-end

recall {"fields": [{"value": "2026-08-15"}]}
carries 2026-08-15
say     January: no record holds the media-booking milestone as a value, so nothing says when the media has to be bought by
window  phase-end

# The contact report. A thing owed by a day is a thing that falls due, so the
# owed answer reads it: `due_on` is the key every shipped type that owes by a
# day leaves, whichever type the model chose.
recall {"fields": [{"key": "due_on", "value": "2026-01-16"}]}
carries "due_on":"2026-01-16"
say     January: nothing falls due on 16 January, so the contact report to Krusty that is never answered is not something an owed question can find
window  phase-end

# The budget is hearsay. The only thing this asks is that it was kept as a claim
# that is open, not as a settled one: the question is asked with the standing in
# the filter, so a claim recorded as settled is not in the answer.
search  {"query": "budget approved 200k", "standing": "open"}
carries 200k
carries "status":"active"
say     January: the 200k is not on record as an open claim, so hearsay is either missing or filed as if it were settled
window  phase-end

# The head's decision in the head's own words, which are quoted in the entry.
search  {"query": "menu we have not synced an order for nothing"}
carries menu we have not synced
carries "status":"active"
say     January: the head's decision about the menu is not on record in the head's own words
window  phase-end

# What done means for the film, which is three conditions and not one.
search  {"query": "stations three formats approved legal cleared"}
carries hree formats
carries "status":"active"
say     January: what done means for the spot is not on record, so a later sitting cannot tell finished from nearly finished
window  phase-end

# The legal rule.
search  {"query": "legal six weeks product claim"}
carries ix weeks
carries "status":"active"
say     January: the six weeks legal needs for a product claim is not on record
window  phase-end

# The option that was turned down.
search  {"query": "ordering first hard-coded menu"}
carries ard-coded
carries "status":"active"
say     January: the option that was turned down is not on record, so December cannot say what was rejected
window  phase-end

# The question that was drafted and is not asked yet.
search  {"query": "offline mode ordering"}
carries ffline
carries "status":"active"
say     January: the drafted offline question is not on record, so December cannot say it was drafted and never asked
window  phase-end
```

## Phase 2 — May, slips and a quote

**Session: fresh.** **Day: 2026-05-20.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 20 May 2026. The copy for the film landed on 15 March, two weeks late. Milhouse was pulled over to Moe's Tavern from 6 to 26 April, so the design is now due 6 May. Krusty has added two markets, Beta and Gamma, on the phone and nothing is written down; Beta needs a local-language disclaimer that Lisa has to clear. On the app, login landed as commit a1b2c3d and the LoginFlowSuite verified it, and menu sync is in progress. I put the offline question to Krusty's contact plainly. Krusty said "the basement location has terrible reception", so I worked out that orders must work offline. Who owns what, and what is blocked on whom?

```locks
# May's floors. The same shape as January's: what this sitting itself stored,
# asked at the end of it, and each one a thing a later question depends on.

recall {"fields": [{"value": "2026-03-15"}]}
carries 2026-03-15
say     May: no record holds the day the copy landed as a value, so the first slip is a sentence and not a date
window  phase-end

recall {"fields": [{"value": "2026-05-06"}]}
carries 2026-05-06
say     May: no record holds the new design day as a value, so the second slip is a sentence and not a date
window  phase-end

search  {"query": "Milhouse Moe Tavern April design"}
carries avern
carries "status":"active"
say     May: why the design slipped is not on record
window  phase-end

search  {"query": "markets Beta Gamma Krusty phone"}
carries amma
carries "status":"active"
say     May: the markets added by phone are not on record, so a written brief and an unwritten one cannot be told apart
window  phase-end

search  {"query": "Beta local-language disclaimer Lisa clear"}
carries isclaimer
carries "status":"active"
say     May: the disclaimer Lisa has to clear is not on record
window  phase-end

search  {"query": "login commit a1b2c3d"}
carries a1b2c3d
carries "status":"active"
say     May: the commit login landed as is not on record by name
window  phase-end

search  {"query": "login suite verified LoginFlowSuite"}
carries LoginFlowSuite
carries "status":"active"
say     May: the suite that verified login is not on record by name
window  phase-end

search  {"query": "basement location terrible reception", "provenance": "testimony"}
carries errible reception
carries "status":"active"
say     May: Krusty's quote is not on record in his own words
window  phase-end

# The requirement the employee worked out. It is the employee's inference, and
# it rests on a quote, so it is asked as an inference whose own record points at
# the claim it was worked out from. A needle of `"derived_from":"` can only be
# present when the pointer is there: an absent one prints `"derived_from":null`.
search  {"query": "orders must work offline basement reception", "provenance": "inference"}
carries "derived_from":"
carries "status":"active"
say     May: the offline requirement is not on record as the employee's own inference pointing at the quote it was worked out from
window  phase-end
```

## Phase 3 — September, what was misread and what was never written

**Session: fresh.** **Day: 2026-09-08.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 September 2026. Burns told me directly that the 200k was never approved, the budget is 150k. Lisa approved draft 3 of the film on 28 August, on condition that the legal claim clears; draft 4 has existed since 2 September, Krusty prefers it, and nobody has approved it. We dropped the print ads at a meeting on 22 July and it was never written down; Ralph had booked print inventory with a cancellation deadline of 30 September, and Milhouse is still designing the print ads. Production on the film slipped from 1 July to 21 September, and the stations need the materials by 25 September; so far they have two formats and legal has not cleared the Beta disclaimer. Krusty has handed the account to a new contact. On the app, the quote was misread: Krusty meant the staff radios, not orders, so orders do not need to work offline after all, and the offline half of ordering rested on it. Menu sync landed as commit c0ffee1. I have drafted a second question that I am not asking yet: which payment provider. It blocks payments. Ordering now waits on Homer's sign-off. What is at risk, and what did we commit to and not do?

```locks
# September's floors.

search  {"query": "Burns budget 150k never approved"}
carries 150k
carries "status":"active"
say     September: the budget Burns gave is not on record
window  phase-end

search  {"query": "Lisa approved draft 3 conditional legal claim"}
carries raft 3
carries "status":"active"
say     September: the approval of draft 3 is not on record
window  phase-end

search  {"query": "draft 4 Krusty prefers never approved"}
carries raft 4
carries "status":"active"
say     September: draft 4 is not on record
window  phase-end

# A decision made in a meeting and never written. The claim has to carry the day
# it happened, which is not the day it was typed.
search  {"query": "print ads dropped meeting"}
carries "happened_at":"2026-07-22"
carries "status":"active"
say     September: the print ads being dropped is not on record on the day of the meeting, so the day it happened is lost
window  phase-end

recall {"fields": [{"value": "2026-09-30"}]}
carries 2026-09-30
say     September: no record holds the print cancellation deadline as a value, so what dropping print would still cost has no day
window  phase-end

recall {"fields": [{"value": "2026-09-21"}]}
carries 2026-09-21
say     September: no record holds the new production day as a value, so the third slip is a sentence and not a date
window  phase-end

recall {"fields": [{"value": "2026-09-25"}]}
carries 2026-09-25
say     September: no record holds the stations' materials deadline as a value
window  phase-end

# The requirement that was withdrawn. A taken-back claim is asked for by its own
# status, so a requirement that is still standing is not in the answer.
search  {"query": "offline requirement orders", "status": "archived"}
carries ffline
carries "status":"archived"
say     September: the offline requirement is not on record as taken back, so it still stands or never existed
window  phase-end

search  {"query": "which payment provider question"}
carries ayment provider
carries "status":"active"
say     September: the second drafted question is not on record
window  phase-end

search  {"query": "menu sync commit c0ffee1"}
carries c0ffee1
carries "status":"active"
say     September: the commit menu sync landed as is not on record by name
window  phase-end

search  {"query": "ordering waits Homer sign-off"}
carries ign-off
carries "status":"active"
say     September: what ordering is now waiting on is not on record
window  phase-end
```

## Phase 4 — December, the retrospective

**Session: fresh.** **Day: 2026-12-10.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 10 December 2026 and I need the retrospective on the Krusty Burger launch and the ordering app. Keep each answer where a later sitting of yours can find it, under the name I give it, on whatever it is about. budget_truth: what the real budget was, as a number in thousands. owners: who owned each part of the launch. approval: which draft of the film Lisa approved, as the draft number. spot_done: whether the 30 was done, yes or no. slips: how many times a milestone on the film's critical path slipped, as a number. undelivered: what we committed to and did not deliver. For the app, app_next: what is next and what it is waiting on. menu_first: who decided what goes first, in their words, and what was turned down. rests_on_misread: what we believed that rested on something misread. never_asked: which question we drafted and never asked. app_landed: what has landed, with the commits. And was there ever any radio?

```locks
# December's slots. Each is the key the entry names. The answer lists only
# records holding that key, so an answer given in the reply and written nowhere
# is absent, and a needle in the slot's value is the model's own words.
#
# A value is read by a needle anchored in the slot's own key where the answer
# has a fixed shape (a number, a word) and by a fragment where it is free text:
# `eta disclaimer` is in "Beta disclaimer" however it starts, `rint ads` is in
# "print ads" and "Print ads".
#
# ⚠️ **The answer to a key is the whole record the key sits on**, so a slot's
# neighbours on the same record are in it, and so are the NAMES of the record's
# other keys. A fragment can therefore be satisfied by a neighbour or by a key
# name. Every fragment was chosen so that it is not in any other slot's right
# answer, and carries a space where the natural words do, because a key name
# does not. The room cannot rule out a model that names a key after the thing
# it is asked about.
#
# **never_asked has no absence assertion**, for the same reason: the offline
# question was asked, and the neighbouring slot that says what rested on it is
# entitled to say so on the same record. Whether the slot names both is in the
# transcript.

recall {"fields": [{"key": "budget_truth"}]}
carries "budget_truth":"150
say     December: budget_truth does not say 150, so the real budget is not where the retrospective was told to put it
window  phase-end

# Who owns what is five pairings, one lock each. The query is every record that
# holds the owner's handle as a value under ANY key, and the second needle is a
# fragment of the deliverable's own name, so the record that holds the handle has
# to be the one about that deliverable. No edge shape is asked for: a key
# holding the handle counts, and a name written into a sentence does not.
#
# The fragments are `pot` in "spot", `rint` in "print", `ocial` in "social"
# and `vent` in "event", which are the operator's own words for them. A model
# that names a record unlike the operator's word reads as red here, and the
# room says so rather than pretending the fragment cannot miss.

recall {"fields": [{"value": "person:milhouse"}]}
carries person:milhouse
carries pot
say     December: nothing about the film holds Milhouse as a value, so who owns the film is prose and not a thing a walk can follow
window  phase-end

recall {"fields": [{"value": "person:nelson"}]}
carries person:nelson
carries pot
say     December: nothing about the film holds Nelson as a value, so who owns its copy is prose and not a thing a walk can follow
window  phase-end

recall {"fields": [{"value": "person:milhouse"}]}
carries person:milhouse
carries rint
say     December: nothing about the print ads holds Milhouse as a value, so who owns them is prose and not a thing a walk can follow
window  phase-end

recall {"fields": [{"value": "person:nelson"}]}
carries person:nelson
carries ocial
say     December: nothing about social holds Nelson as a value, so who owns it is prose and not a thing a walk can follow
window  phase-end

recall {"fields": [{"value": "person:ralph"}]}
carries person:ralph
carries vent
say     December: nothing about the launch event holds Ralph as a value, so who owns it is prose and not a thing a walk can follow
window  phase-end

recall {"fields": [{"key": "approval"}]}
carries "approval":"3
say     December: approval does not say draft 3
window  phase-end

recall {"fields": [{"key": "spot_done"}]}
carries "spot_done":"no
say     December: spot_done does not say no, so the film is recorded as done or not recorded as anything
window  phase-end

recall {"fields": [{"key": "slips"}]}
carries "slips":"3
say     December: slips does not say 3
window  phase-end

recall {"fields": [{"key": "undelivered"}]}
carries eta disclaimer
carries hird
carries rint ads
say     December: undelivered does not name the Beta disclaimer, the third station format and the print ads
window  phase-end

recall {"fields": [{"key": "app_next"}]}
carries ign-off
carries ayments
say     December: app_next does not say ordering is next, waiting on the head, with payments behind it and the provider question
window  phase-end

recall {"fields": [{"key": "menu_first"}]}
carries menu we have not synced
carries ard-coded
say     December: menu_first does not carry the head's words and the option that was turned down
window  phase-end

recall {"fields": [{"key": "rests_on_misread"}]}
carries ffline
say     December: rests_on_misread does not name the offline requirement
window  phase-end

recall {"fields": [{"key": "never_asked"}]}
carries ayment provider
say     December: never_asked does not name the payment provider question
window  phase-end

recall {"fields": [{"key": "app_landed"}]}
carries a1b2c3d
carries c0ffee1
carries LoginFlowSuite
say     December: app_landed does not carry both commits and the suite that verified the first
window  phase-end
```
