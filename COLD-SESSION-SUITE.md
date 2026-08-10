# The cold-session suite

**A model with no prior context, given a real task, judged on what it reaches
for unprompted.** That is the whole instrument, and it is the only one this
project has that measures whether the served surface teaches itself.

## Why this is not a user story

A user story is written by somebody who already knows the answer. That is what
writing the assertion means. So a story can prove a capability WORKS and can
never measure whether a session that did not know it existed would have found
it. The failure this suite hunts — a session concluding that a shipped
capability is absent — is invisible from inside a session that knows where to
look.

Both bars stand. A unit test proves the code. A story proves the capability is
reachable through the served surface. This suite proves the surface says so to
somebody who arrives blind.

## What this file gives the harness

A make task spawns a disposable instance against its own database, seeds a
known starting state, drives a model through the phases below, and asserts over
the store afterwards. **The expectations are Rust and they live in the
harness's crate, keyed to these phases by name.** This file stays prose and
grows no syntax.

It hands over three things and nothing else:

1. **An ordered list of phases**, named.
2. **One block of prose per phase**, addressed to the model. Everything outside
   those blocks is for whoever maintains the suite.
3. **A per-phase session marker** — whether the phase runs on the session
   before it, or starts a fresh one with no memory of anything earlier.

**A phase marked fresh gets a self-contained block**, because a session with no
memory has not read the standing instructions either. That is why the two
watches and the no-working-around rule appear more than once: the repetition is
the contract holding, not drift.

## The identity rule — binding

**The model boots as the shipped `assistant` and stays there.** jojobot ships
with strong identities by default, and a fresh instance arrives holding that
one. The instrument measures what a session meets on arrival.

* **No charter is written for the identity the model wears** — not by the seed,
  not by a phase, not to help it along.
* **No hand-authored orientation of any kind.** If the model needs to be told
  something the shipped charter does not say, that IS the finding.
* A session is bound to its bot, so the model does not switch identity mid-run
  and no phase may suggest it can. The bot it creates in phase 8 is an object it
  makes, never an identity it wears.

A suite that hands the model a bespoke charter measures the charter somebody
wrote for the test. The batteries are the product, and this is the claim under
test.

## The measurement bar

**A phase earns an assertion when it leaves a visible side effect** — something
in the store afterwards that was not there before, reachable through the
surface. Not that the model said the right thing: that it left something.

Some phases cannot leave one, because what they measure is what the model
understood. Those are legitimate and they are the reason a person reads the
report. What is not legitimate is a phase that quietly does neither, so every
phase below is marked one way or the other in the last section.

---

## Phase 1 — the door, cold

**Session: fresh.** This is the first contact with the server.

> You are testing a live MCP server called jojobot. You have no prior context
> about it and that is deliberate: you are standing in for a fresh session,
> which is the case that has broken before.
>
> Write one line at the end of this phase: **PASS**, **PARTIAL** or **FAIL**,
> and what you actually saw.
>
> **Do not fix anything. Do not retry creatively. Do not work around a
> failure.** A workaround hides the defect this suite exists to find. If a step
> fails, record what happened and go on.
>
> **Two watches run through every phase. Report every instance:** any mention
> of where data is STORED — a service name, a document, a page, a row, a card,
> a table; you should never learn this. And any error that arrives as a raw
> failure rather than as a refusal naming a way forward.
>
> 1. Call `ping` with no arguments. Record the `build` string verbatim. Report
>    anything it tells you that is not liveness.
> 2. Call `start_here` with no arguments at all. Expected: an explanation of
>    how the system works, plus a snapshot of what exists, and **no session
>    handle** — you have not said who you are. Report whether it hands you a
>    handle it should not.
> 3. From that answer alone, and without calling anything else, write down:
>    every verb you believe you are allowed to call, how you would get an
>    identity, and what you think this server is for. **This is the
>    measurement.** Do not go back and correct it later — mark it PARTIAL or
>    FAIL yourself if a later phase proves it wrong, and say which part.

## Phase 2 — the identity you arrive holding

**Session: continues phase 1.**

> 4. Call `start_here` naming the bot `assistant`. Expected: its charter, its
>    rules, the counts of the mailbox it owns, and either a session handle or
>    an offer to resume an earlier run. If you are offered the choice, answer
>    it and take a **new** session. Record the handle. Report FAIL if you
>    answered and still hold none.
> 5. **In your own words, before going further: what did that charter tell you
>    you are for?** Name one thing you now know you must not do, and one thing
>    you now know you are expected to do. Report FAIL if the charter left you
>    unable to answer either — you are the identity this instance ships with,
>    and being unable to say what it is for is the finding.
> 6. **This is the only identity you use.** Do not boot as another bot at any
>    point, including one you create later.
> 7. Call `start_here` naming a bot that certainly does not exist — use
>    `zzz-not-a-bot`. Expected: a refusal that names the bots that do exist and
>    tells you what to do. Not a crash, not an empty answer, and it must create
>    nothing. Report the exact shape.
> 8. Confirm step 7 created nothing: list the entities of kind `bot`.
>    Expected: the names the snapshot gave you, and no `zzz-not-a-bot`. Report
>    both halves — the real names present AND the invented one absent.

## Phase 3 — the handle that rides every call

**Session: continues phase 2.** This is the load-bearing phase: no real client
holds a connection open, so identity travels on the handle or it does not
travel. This failed wholesale in production once, on two clients at once.

> 9. Call `list_entities`, passing the handle from phase 2.
> 10. Call `search` for a common word, passing the same handle.
> 11. Call `read_mailbox` with counts only, passing the same handle.
> 12. Call `ping` passing the same handle. Expected: it says the handle still
>     addresses your session.
> 13. Call `ping` passing a handle that is not yours: take your own and change
>     one character. Expected: it says that handle is unknown or unreadable —
>     **not** that it is held. Report the exact word, and report FAIL if a
>     string that is not your handle came back held.
>
> If any call in 9–12 does not know who you are, that is a **BLOCKER**: record
> it and stop.

## Phase 4 — reading

**Session: continues phase 3.**

> 14. `list_entities`. Report which kinds exist and roughly how many of each.
> 15. `search` for something you saw in step 14. Report whether each hit
>     arrives with its surroundings — what it is about, where it sits, the text
>     around the match — or as a bare match.
> 16. `search` again for the same word, this time asking for mail as well.
>     Expected: messages arrive beside the memory hits, each carrying its box,
>     its state and the id that would take delivery. Report whether asking was
>     needed, and whether you would have known to ask.
> 17. Read the coverage on those answers: does each half say whether it
>     searched everything, nothing, or part of it? Report what it said and
>     whether you could act on it.
> 18. `recall` one handle from step 14. Report what a subject's record looks
>     like: what a claim carries, and whether you can tell a claim the operator
>     made from one an AI derived.
> 19. Call `search` with neither a query nor any filter. Expected: a refusal
>     naming the rule. Report FAIL if it errors rawly or returns a happy empty
>     answer.
> 20. `recall` a handle that does not exist — use `person:zzz-nobody`.
>     Expected: a refusal, with anything that resembles it, rather than an
>     error. Report whether it read as "no such thing" or as "broken".

## Phase 5 — the procedures

**Session: continues phase 4.**

> 21. From the phase 2 answer, list the skills this build ships. Expected:
>     names and when-to-use, and **no bodies**. Report FAIL if a body arrived
>     unasked.
> 22. **Which of them did you learn existed without being told to look?**
>     Answer from the boot alone. A skill you only found because this step
>     named it is a finding about the index, and saying so is the point.
> 23. Fetch one skill by name, through the same door. Report whether it reads
>     as a procedure you could follow.
> 24. Ask for a skill that does not exist — use `zzz-not-a-skill`. Expected: a
>     refusal naming the real ones.

## Phase 6 — the session's own record

**Session: continues phase 5.**

> 25. `journal` one entry, and set what you are working on.
> 26. `journal` a second, different entry.
> 27. `amend_journal`, correcting the second entry.
> 28. Read your own chronology back: call `start_here` naming `assistant`
>     again and resume the session you are already in. Expected: the amended
>     entry reads as you corrected it, the first entry is untouched, and both
>     are still there. **Report FAIL if you cannot tell** — a correction you
>     cannot verify is the finding, not a pass.
> 29. In that chronology, look for entries jojobot wrote about you rather than
>     entries you wrote. Report whether you can tell the two apart, and how.
> 30. `search` for the exact words you journalled in step 25. Expected: you
>     find it, as a hit that says it is a session — your own run, carrying that
>     run's handle and a snippet, and sitting below the other kinds of hit in
>     the list. A run is only ever returned to the bot that owns it, so the one
>     you get back is yours. Report what came back and where in the list it
>     sat, and report whether anything told you sessions were searchable at all
>     before you tried.

## Phase 7 — mail, without taking on work

**Session: continues phase 6.**

> 31. Call `read_mailbox` with counts only, twice. Expected: the same counts
>     both times and nothing moved out of `new`. Report FAIL if the second call
>     disagrees with the first.
> 32. Go back to the snapshot the step 4 boot gave you and look at the mail
>     hanging off each bot on it. Report, bot by bot, whether you were shown
>     counts and what stood there instead when you were not. Expected: counts on
>     the box your own bot owns, and on anybody else's an entry saying its counts
>     were withheld — neither the numbers nor the name of the box. Report whether
>     you could tell "withheld" from "empty", and **if your own bot is the only
>     one on that board, say so** rather than reasoning about boxes you cannot
>     see.
> 33. Find a way to read another bot's live mail. Expected: there is none — the
>     verb that takes delivery has no box argument. Report what you found when
>     you looked, and whether anything told you why.
> 34. Call `list_sent` for yourself. Expected: where your own messages got to,
>     read-only. Report whether it moved anything.

## Phase 8 — writes

**Session: continues phase 7.** What this phase leaves is what phase 10 has to
find without being told where to look, so leave it as you would leave it for
somebody else.

> 35. `add_entity` for a person with handle `smoke-alpha`. Expected: created,
>     and it says so.
> 36. `add_entity` again for `smoke-alfa` — deliberately a near-miss of the
>     first. Expected: **blocked**, offering the existing one, and creating
>     nothing. Report the exact refusal and whether it told you how to proceed
>     if you really meant a second one.
> 37. `capture` a clearly fictional fact about `smoke-alpha`, without saying
>     where it came from. Report the address it came back with and what
>     provenance it defaulted to. **The default should be the cautious one.**
> 38. `recall` `smoke-alpha` and confirm the fact is there.
> 39. `update_fact` at that address, changing the wording.
> 40. `recall` again. Expected: the claim is **rewritten** — not duplicated,
>     not appended to.
> 41. `capture` a second fact drawing an edge at an entity that does not exist.
>     Expected: blocked, naming what is missing. Nothing is created as a side
>     effect of anything else here.
> 42. `add_entity` for a bot with handle `smoke-gamma`. Expected: created, and
>     a mailbox named for it opens in the same act — no second call. Report
>     whether anything told you the box exists, or whether you had to go
>     looking.
> 43. `set_charter` on `smoke-gamma` with a short piece of prose. Report
>     whether you can read it back, and how.
> 44. **You do not boot as `smoke-gamma`.** It is an object you made, not an
>     identity you wear. Report FAIL if any part of the surface invited you to
>     switch to it.
> 45. `post_message` into your own bot's box, with a subject, saying what you
>     found out about `smoke-alpha`. Write it for a reader who was not here.
> 46. `list_sent`. Expected: you can see that message and where it got to, and
>     it did not move.
> 47. Look for a verb that creates a mailbox. Expected: **there is none.**
>     Report the five mail verbs you did find, and what you concluded from the
>     absence.

## Phase 9 — stopping without finishing

**Session: continues phase 8.** Deliberately does NOT wrap: phase 10 needs a
run left open, which is the ordinary case rather than a failure.

> 48. You are stopping, and the work is not done. Journal one entry for
>     whoever picks this up: where you got to, what you already ruled out, and
>     the exact next step. Set what you are working on to something a later
>     session could recognise.
> 49. Do **not** wrap this session. Report what you would have expected to
>     happen if you had.

## Phase 10 — the reader who was not here

**Session: fresh.** No memory of any earlier phase. **This is the product
claim**: a later session finds what an earlier one left, without being told
where to look. Nothing in this block names a verb on purpose.

> You are a fresh session of an assistant, and you have just come on shift. You
> have no memory of anything that happened before now.
>
> Write one line at the end: **PASS**, **PARTIAL** or **FAIL**, and what you
> actually saw. **Do not work around a failure** — record it and go on. **Two
> watches, and report every instance:** any mention of where data is STORED,
> and any error arriving as a raw failure rather than a refusal naming a way
> forward.
>
> Boot as the bot `assistant`. Then:
>
> 50. **Before anything else, report what the boot offered you** about work
>     already in flight, and whether you could tell from it what the earlier
>     run had been doing. Take that earlier run rather than starting a new one.
> 51. Read what that run recorded. Report whether somebody with none of its
>     context — you — could tell what it had done and what to do next, and name
>     the part that was missing if any was.
> 52. **Somebody left something for you, and it may not be the only thing
>     waiting.** Find what is there and act on it: take delivery, and mark each
>     one handled with a note saying what you did. Report how you found them and
>     whether anything told you they were waiting.
> 53. **Find out what this server knows about `smoke-alpha`.** Report what you
>     found, how you found it, and whether the claim read as something somebody
>     confirmed or as something an AI worked out.
> 54. Now finish the run properly, with a closing story written for somebody
>     who was not here.
> 55. Try to add one more entry to the run you just finished. Report what
>     happened.
> 56. Ask the server about the handle you have been carrying, and read that
>     answer beside step 55. Say whether the two together could leave a session
>     believing it can still write. Report FAIL if they could — the handle and
>     the run are different things, and a caller has to be able to tell which
>     one ended.

## Phase 11 — the ending, from cold

**Session: fresh.** No memory of any earlier phase.

> You are a fresh session of an assistant. You have no memory of anything
> earlier. Write one line at the end: **PASS**, **PARTIAL** or **FAIL**, and
> what you saw. **Two watches, and report every instance:** any mention of
> where data is STORED, and any error arriving as a raw failure rather than a
> refusal naming a way forward.
>
> 57. Boot as the bot `assistant`. Expected: a fresh run, and the run that was
>     finished earlier is **not** offered back to you. Report what you were
>     offered.
> 58. Report everything this instance now holds that looks like it was made by
>     a test rather than by a person, and say how you can tell. You have no
>     stake in the answer being tidy.

---

## The report

The harness collects, per phase: the PASS / PARTIAL / FAIL line and what the
model saw. Beyond that, four things are wanted whole:

* The phase 1 step 3 answer, verbatim, and what it got wrong.
* The phase 2 step 5 and phase 5 step 22 answers in full.
* Every place a tool revealed where data is stored.
* Every error that arrived as a raw failure instead of a refusal with a way
  forward.

## Reading the report

The model has no context by design, so some of what it reports as surprising is
settled design. Check a finding against this list before it becomes work.

**Behaviour that is correct and will be re-reported every run:**

* **The closing story opens with the focus line.** Wrapping folds the session's
  still-open focus into the story as one entry, deliberately.
* **A session hit comes back last, and only to the bot whose run it is.**
  Journalled text is findable by `search`; it is demoted rather than filtered,
  so a run is reachable when it is what you are after and never crowds out what
  a search is usually for. What stays outside the index is anything the memory
  scan finds marked as jojobot's own machinery.
* **Mail is opt-in in `search`.** A model that does not pass the flag sees no
  messages and is right not to. Phase 4 asks whether it would have known to
  ask; "no" there is a finding about the surface, not about the default.
* **Nothing deletes.** There is no delete verb over MCP, by rule. What a run
  creates stays for the life of that instance, which is the life of the run.
* **A blocked answer is a success.** Refusals across a bad bot name, a
  near-miss handle, an unknown edge object and a closed run are the design
  working. What is a finding is a refusal that names no way forward.

**How to read the two watches.** They are permanent: there is no end state
where the report stops mentioning them, because a clean run reports zero
instances rather than nothing at all. A run that omits them has not run them.

**How to read the judgement steps.** Phase 1 step 3, phase 2 step 5 and phase 5
step 22 have their whole value in the first attempt. A wrong answer there is
worth more than a right one: it names something the surface failed to teach,
and no later phase can recover it once the model has learned the answer another
way. They are self-judged, which is weaker than an assertion — and no assertion
can reach what they measure.

## What each phase leaves, and what it does not

For whoever writes the expectations. **Assertable** means the phase leaves a
visible side effect in the store. **Runner-reported** means what it measures
exists only in the model's answer, by design.

| Phase | Kind | What is there afterwards |
| --- | --- | --- |
| 1 · the door, cold | runner-reported | Nothing. A boot that does nothing writes nothing. |
| 2 · the identity | assertable, as absence | No `zzz-not-a-bot`, and the seeded bots unchanged. Pair with the positive: the seeded roster is present. Step 5 is runner-reported. |
| 3 · the handle | runner-reported | Nothing. Reads only. |
| 4 · reading | runner-reported | Nothing. The refusals in 19 and 20 leave nothing either. |
| 5 · the procedures | runner-reported | Nothing. Skills are shipped, not stored. |
| 6 · the session record | **assertable** | One run for `assistant` with two entries, the second carrying the amended text and the first unchanged, plus jojobot's own beats. |
| 7 · mail, read-only | **assertable, as no-change** | Every mailbox in the same state as before the phase. This is the phase whose whole claim is that nothing moved. |
| 8 · writes | **assertable, richest** | `person:smoke-alpha` with exactly one active fact, carrying the rewritten wording and `inference` provenance. No `person:smoke-alfa`. `bot:smoke-gamma` with its charter and a mailbox named for it. Two messages in the `assistant` box, both `new`: the one the room was furnished with and the one step 45 posts. |
| 9 · stopping | **assertable** | The run still open — not wrapped — with a further entry and a focus. |
| 10 · the reader | **assertable** | The run from phases 2–9 now `wrapped`, its final entry carrying the focus; the message step 45 posted now `processed` with a note — the furniture sits beside it and what the reader does with that is not pinned; a second run for `assistant`. Steps 50, 51 and 53 are runner-reported on top of that. |
| 11 · the ending | **assertable** | A third run for `assistant`, and the wrapped one still wrapped. |

**The trap in this table.** Phases 2 and 7 are assertable only as absence or
as no-change, and an absence passes on a run where nothing happened at all.
Each needs its positive beside it or it proves nothing.

**Why the runner-reported phases stay.** They are the ones that do what the
instrument exists for. Everything assertable here, a user story could have
covered; what a story cannot do is measure what a session that did not know
would have found, and that lives entirely in phases 1, 5 and 10.

## Run history

Each entry: the build, and what came out. A run against a build nobody recorded
cannot be read later.
