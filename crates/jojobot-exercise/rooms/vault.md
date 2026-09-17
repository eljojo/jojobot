# The second year

**Thirteen sittings inside one year, none of them remembering the one before,
and every late question reaching across things nobody ever filed together.**

`year.md` asks whether a year of ordinary use accumulates. Its questions are
excellent and almost all single-subject: what did the record say, who was at
the survey, how far did the bike go. This room asks the other question a
year of real notes throws up, and it is the one a research pass over a real
person's life notes found again and again: **the decision-relevant fact was
almost never missing. It was written down already, inside something written
for a completely different purpose.** A guarantee mentioned beside a
purchase. An allergy mentioned beside a furnace. A course schedule mentioned
because a friend had just finished it.

So every late question here needs a **filter on a key, a comparison on a
date, a walk along a relation, a selection by kind, or the history of a
claim** — and every one of them reaches across two or more things that were
recorded months apart, on different subjects, for different reasons, in
different words. **Extraction and connection, never collection.**

## Why a separate room and not a longer existing one

`year.md` is tuned over many paid runs and its locks carry hard-won hatch
windows. Adding twelve cross-subject questions to it would double a document
that is already the largest in the suite, and every new lock there has to be
placed "last, so nothing renumbers" — a constraint that fights weaving. A room
with its own cast, its own furniture and its own register reads differently
from `year.md`: that one fails when a sitting wrote badly, this one fails
when a sitting **read narrowly**.

## The fiction, and where it is real

Everything `year.md` says about the day marker holds here unchanged: each
sitting says its date in the entry and in the `**Day:**` marker, the run
generates the assertion that what it wrote carries that day, and a sitting
that writes nothing is neither pass nor fail on that assertion. Two sittings
here record something that happened on an earlier day; both write on their own
day and name the earlier one in a separate field, exactly as `year.md`'s
September does.

**The three loops are furnished with their schedules already in place**, the
way the loop room's fern is: a room with nothing in it teaches no vocabulary.
Their check-in days are all before the year begins, so no sitting's generated
assertion can hold on furniture.

## Two facts every entry opens with

The same two `year.md` learnt to open with, for the same reason: a sitting
that asks a question and writes nothing spends its whole turn. So every entry
says that it is a role-play and the day it names is today, and that nobody will
answer but the answer is being read and one is required.

## The register this room is written in

**A markdown vault, moved in.** The operator has kept a folder of text files
for years, with a handful of front-matter keys they think in, and the first
sitting is told those keys once. That is the vocabulary lever from the proving
brief — *a word the operator uses, agreed in a message the cold sessions never
read* — and it is what a real person migrating their notes actually says. The
keys are the operator's, never the room's: no entry after the first names one
except where the operator would (`decide_by`, in December, is the operator
inventing a key on the spot, which is also what real people do).

⚠️ **The operator's words carry the WORK and never the METHOD.** No verb, no
argument, no place anything is kept, no hint that anybody is measuring. Where
an entry says *put it on the thing itself*, that is an operator saying where
they want to find it next time, which is the most ordinary instruction there
is.

## What the room holds before the occupant arrives

A vault's worth of nouns and nothing dated: **the people, places, businesses,
machines, possessions and projects a person already has notes for**, three
loops with their schedules, and one brief. Every fact in the year is written by
a sitting.

**Two pairs are named alike and both are real** — `place:wagstaff-pool` and
`event:wagstaff-fair`, `place:capital-city` and nothing else — and neither
meets the near-miss guard: it is scoped to one kind, and these cross kinds by
design, so both stand without incident. **The pair that does meet it is
`person:linda` and `person:tina`, two edits apart and the same kind.** This
document DECLARES the pair, below, so furnishing retries that one refusal
with the override token it mints — the judgement a real caller would make by
hand, made here because the document is what stands in for one. An
undeclared collision still refuses; declaring is not a way to dodge the guard
by construction, it is the one thing a room's own document can do that a
caller sitting in front of the refusal could also do.

⚠️ **Furniture is stamped with real today.** Only the loops carry records, and
their dates live in KEYS (`counts_from`, `last_check_in`), which the overdue
arithmetic reads; nothing in this room asks `near` around real today.

```world
entity  person:linda | Linda
entity  person:teddy | Teddy
entity  person:gayle | Gayle
entity  person:tina | Tina | resembles: person:linda
entity  person:louise | Louise
entity  person:gene | Gene
entity  person:hugo | Hugo

entity  place:wagstaff-pool | The Wagstaff Pool
entity  place:capital-city | Capital City
entity  place:wonder-wharf | Wonder Wharf
entity  place:ocean-avenue | Ocean Avenue

entity  org:globex | Globex
entity  org:costingtons | Costington's
entity  org:mr-plow | Mr. Plow Heating
entity  org:quahog-community-college | Quahog Community College

entity  machine:theta | Theta
entity  machine:omicron | Omicron

entity  pet:snowball | Snowball

entity  thing:piano | The Piano
entity  thing:house-keys | The House Keys
entity  thing:glasses | The Glasses
entity  thing:phone-charger | The Phone Charger
entity  thing:the-furnace | The Furnace

entity  project:the-shed | The Shed
entity  project:kitchen-floor | The Kitchen Floor

entity  event:wagstaff-fair | The Wagstaff Fair

# Three loops, kept properly, each hanging under the thing it is about. They
# are the teacher and, in December, the instrument: which of them has gone
# quiet is jojobot's own arithmetic, and WHY is what the year has to have left
# lying around on other subjects.
child   place:wagstaff-pool | rhythm:swim | Swim
record  rhythm:swim | {"name": "Swim", "cadence_days": "7", "advances_from": "check_in_date", "counts_from": "2025-12-30", "last_check_in": "2025-12-30"} | a swim every week, counted from whenever I last went
child   person:gayle | rhythm:call-gayle | Call Gayle
record  rhythm:call-gayle | {"name": "Call Gayle", "cadence_days": "7", "advances_from": "check_in_date", "counts_from": "2025-12-30", "last_check_in": "2025-12-30"} | the Tuesday call, counted from whenever we last spoke
child   thing:piano | rhythm:sit-at-the-piano | Sit at the piano
record  rhythm:sit-at-the-piano | {"name": "Sit at the piano", "cadence_days": "14", "advances_from": "check_in_date", "counts_from": "2025-12-28", "last_check_in": "2025-12-28"} | at least once a fortnight or my hands forget

# The operator's own words, and the whole of what the first sitting is told
# beyond its entry line. An indented line continues the one above it.
message assistant | moving my notes in | I am moving my notes in here from the folder of text files I have kept for years, so take this down properly rather than as a note to yourself. The three things I do on a schedule are already on here and they work the way I want.

    Five words I put at the top of a note, and I want them kept, because I ask my questions by them. `runs_out` is a date: the last day I can still do something about a thing, whether that is sending it back, claiming on it, or using a service I have already paid for. `spare` goes on anything I would be stranded without, and says where the spare is. `status` goes on a project and is one of considering, doing or done — those three, no others. `for` goes on anything that only works with one particular machine, and names the machine. `minutes` is how long the way to work took, on the road I took.

    The new laptop is Theta — bought 2026-01-10 from Globex, with two years of cover from that day. Omicron is the old home server.

    My glasses: there is a spare pair in the hall drawer.

    The shed for the garden has been considering since November — the flat-pack kit, or build one. The kitchen floor is also considering.

    Ocean Avenue is my way to work: 38 minutes on 2026-01-08.

    Leave it so whoever picks this up in a month has what they need.
```

## Phase 1 — January, the vault moves in

**Session: fresh.** **Day: 2026-01-12.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 12 January 2026 and I want to get my notes moved in

```locks
# The brief left the box. The same named check every room uses, for the reason
# year.md gives beside its own January: a query cannot say WHICH message is no
# longer new.
check   the_brief_left_the_box
say     January: the brief is still sitting new, so nobody took delivery of the vault

# 🚨 **The first window, and the first arithmetic.** The operator said "two
# years of cover from 2026-01-10"; the key they think in is a DATE. A sitting
# that wrote "two years" under the key has written a value no later comparison
# can read, and December's question — what runs out by the end of March — is
# the one that pays for it.
recall {"subject": "machine:theta"}
carries "runs_out":"2028-01-10"
say     January: the laptop's cover does not read as the day it ends, so the one window the operator dated is not on record as a date

# The convention, applied where the brief applied it: a spare, and where it
# is. The VALUE is the occupant's; the key is the operator's word. November
# asks about spares in words that never say the key.
recall {"subject": "thing:glasses"}
carries "spare":"
say     January: the glasses carry no spare, so the one thing the operator said has a spare is not on record as having one

# The project's state, as the operator's own word. December counts how many
# times this key was written on this project and what it said each time; a
# status in prose is not a status that can be counted.
recall {"subject": "project:the-shed"}
carries "status":"considering"
say     January: the shed does not hold considering under the operator's own key, so December cannot see how long it has been going round

recall {"subject": "project:kitchen-floor"}
carries "status":"considering"
say     January: the kitchen floor does not hold considering under the operator's own key, so it cannot be told apart from the shed when December asks which one moved

# The first timing on the usual road. October needs to count these — four on
# this road against one on the other — and a number in prose is not a write of
# a key.
recall {"subject": "place:ocean-avenue", "history": "minutes"}
carries "value":"38"
say     January: the road does not carry the January timing as a write of the key the operator times by, so October has nothing to count

# ⚠️ **The operator's own words land as the operator's word.** A claim
# captured with nothing said about who backs it is a hypothesis, and reads as
# open from birth. October's question is whether a verdict is in doubt, and
# it can only be asked of a store where the operator's timings are NOT already
# in doubt by default. This is the first of the year's timings; the rest are
# the same sentence in later months, and a red in October is read against
# them before it is read against October.
recall {"subject": "place:ocean-avenue", "facts": true}
carries "provenance":"testimony"
lacks   "standing":"open"
say     January: the operator's own timing is on record as a guess rather than as their word, so every later question about what is in doubt starts from a store where everything is
```

## Phase 2 — February, a desk, a storm and a set of keys

**Session: fresh.** **Day: 2026-02-08.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 February 2026. The standing desk came on the 6th; I have thirty days from then to send it back if I don't get on with it. Storm week: the Ocean Avenue bridge is shut for repairs until the 20th, so on Friday the 6th I went in by the wharf road — 52 minutes, never again. Teddy has had a set of my house keys since the lockout last year. Still can't decide on the shed; the kit's on offer again. And I swam on Tuesday the 3rd.

```locks
# 🚨 **A window that is a RETURN PERIOD rather than cover, on a thing the
# occupant names.** No handle is pinned: the thing is found by the day the
# operator's arithmetic lands on — thirty days from the 6th — under the key the
# operator asks by. In December this is the control: a window long closed that
# must NOT be flagged.
recall {"kind": "thing", "fields": [{"key": "runs_out", "value": "2026-03-08"}]}
carries "runs_out":"2026-03-08"
say     February: no thing carries the day the desk's return period ends, so a window the operator can still act on this month is not on record as a date

# 🚨 **The single observation, on its own subject.** Fifty-two minutes, once,
# on the wharf road. October asks whether the operator's "never again" rests on
# more than this, and the answer is the COUNT of writes of this key on this
# road — which is only a count if it was written as the key.
recall {"subject": "place:wonder-wharf", "history": "minutes"}
carries "value":"52"
say     February: the wharf road does not carry the one timing as a write of the key, so October cannot see that the verdict rests on one morning

# 🚨 **The verdict is the operator's, and it is SETTLED today.** October's
# whole question is whether a sitting will open it after counting what it
# rests on — and that question is empty if the claim was born open because
# nobody said the operator said it. This lock is what gives October's
# positive its meaning: a claim that was settled and is now open was opened by
# somebody.
recall {"subject": "place:wonder-wharf", "facts": true}
carries "provenance":"testimony"
lacks   "standing":"open"
say     February: the operator's "never again" is on record as a guess rather than as their word, so October cannot open a verdict that was never closed

# 🚨 **The CONDITION, on a different subject, in the same sitting.** The bridge
# was shut that week, which is why the wharf road was tried and why it was
# slow; the operator did not say so in the same breath and neither record
# mentions the other. Nothing here asks the sitting to connect them — the day
# they share is the connection, and October's question is whether a cold
# sitting can find it.
recall {"subject": "place:ocean-avenue", "facts": true}
carries "recorded_at":"2026-02-08"
say     February: nothing on Ocean Avenue carries this sitting's own day, so the week the bridge was shut is not on record and the wharf timing stands as if it were an ordinary morning

# ⚠️ **A sentence about Teddy that is a fact about the KEYS.** The brief's own
# convention puts a spare on anything the operator would be stranded without,
# and says where the spare is: a set held by a neighbour IS where the spare is.
# A sitting that filed this as a fact about Teddy has recorded it where
# November's question — do I have a spare of everything — will not look.
# November gives a second chance and this lock says whether one was needed.
recall {"subject": "thing:house-keys"}
carries "spare":"
say     February: the keys carry no spare, so the set Teddy holds was filed under the friend rather than under the thing it is a spare of

# The second write of the shed's state. Same key, same word, another month.
recall {"subject": "project:the-shed", "facts": true}
carries "recorded_at":"2026-02-08"
say     February: nothing on the shed carries this sitting's own day, so a month of going round on it is not on record for December to count

# A check-in on the swim, dated the day it happened. December's arithmetic on
# what went quiet counts from the last of these.
recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-02-03"
say     February: no loop carries the third as a check-in, so the swim the operator reported is not on the record that decides when swimming went quiet
```

## Phase 3 — March, boots and a tablet

**Session: fresh.** **Day: 2026-03-15.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 15 March 2026. Boots from Costington's on the 8th, with a year's guarantee on the soles. The vet has put Snowball on a tablet twice a day from now on, for good — and the cattery on Ocean Avenue says they will not board a cat on it. The desk stays; I have stopped noticing it, which I think is the point.

```locks
# 🚨 **A guarantee mentioned beside a purchase, and never called a deadline.**
# In December it is one of exactly two windows that close inside the operator's
# horizon. Found by the day a year from the 8th lands on, never by a handle the
# occupant chose.
recall {"kind": "thing", "fields": [{"key": "runs_out", "value": "2027-03-08"}]}
carries "runs_out":"2027-03-08"
say     March: no thing carries the day the boots' guarantee ends, so a window that closes inside December's horizon is not on record as a date

# 🚨 **The constraint, filed on the cat.** A tablet twice a day, for good, and
# a cattery that will not take a cat on it. Nothing here says the word "trip"
# and no trip exists yet; November's plan is what this collides with, and
# November has to find it here.
recall {"subject": "pet:snowball", "facts": true}
carries "recorded_at":"2026-03-15"
say     March: nothing on the cat carries this sitting's own day, so the constraint that decides who can look after her in November is not on record
```

## Phase 4 — April, a yes, a drive and a floor

**Session: fresh.** **Day: 2026-04-19.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 19 April 2026. Linda has roped me into running the cake stall at the Wagstaff fair on 17 October and I said yes. She says whoever runs it has to hold the food-handling certificate, which I don't, so that is on me. I bought a backup drive on Friday the 17th; it is formatted for the server and the laptop cannot read it. The kitchen floor is happening — Gene's mate is booked for June. Ocean Avenue took 41 minutes on Tuesday the 14th. And I sat down at the piano on Sunday the 12th.

```locks
# 🚨 **The commitment, with its real cost NOT attached.** A stall in October
# and a certificate the operator does not hold. What the certificate takes and
# where it comes from arrives in June, on a different subject, because a
# friend happened to mention it. September is where the two meet.
recall {"subject": "event:wagstaff-fair", "facts": true}
carries "recorded_at":"2026-04-19"
say     April: nothing on the fair carries this sitting's own day, so the stall the operator said yes to is not on record and September has nothing to owe

# 🚨 **The dependency, as the operator's key with the machine's handle as its
# value.** "The server" is Omicron and the drive only works with it. October
# retires the server and asks what goes with it, in words that never say
# "drive". A value that is not the machine's handle is not a link — jojobot's
# own rule, not this room's — so a drive filed `for: the server` is a drive
# nobody can find from the server.
recall {"kind": "thing", "fields": [{"key": "for", "value": "machine:omicron"}]}
carries "for":"machine:omicron"
say     April: no thing is on record as being for Omicron, so what gets packed off with the server in October cannot be asked

# The floor moved. December tells the project that moved from the project that
# did not by the history of this one key.
recall {"subject": "project:kitchen-floor"}
carries "status":"doing"
say     April: the kitchen floor does not hold doing, so the one project that moves this year does not read as having moved

recall {"subject": "place:ocean-avenue", "history": "minutes"}
carries "value":"41"
say     April: the road does not carry the April timing, so the count October needs is short

recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-04-12"
say     April: no loop carries the twelfth as a check-in, so the piano's record of being played is short a turn
```

## Phase 5 — May, the furnace, and a man in the hall

**Session: fresh.** **Day: 2026-05-10.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 10 May 2026. Mr. Plow serviced the furnace on the 6th. The plan includes one free service a year, but it has to be used before 1 February or it lapses — so the next one has to be booked before 1 February 2027. Teddy came round to look at it with them and had to stand in the hall the whole time; he is badly allergic to cats. Went round the shed again with Linda; nothing decided.

```locks
# 🚨 **A service interval that is a window, on a thing that already existed.**
# The second of December's two closing windows, and the one a person forgets
# because it is not a purchase: a free service that lapses. Filed under the
# same key as a guarantee and a return period, which is what lets one question
# reach all three.
recall {"subject": "thing:the-furnace"}
carries "runs_out":"2027-02-01"
say     May: the furnace does not carry the day its free service lapses, so the one window nobody would think to call a deadline is not on record as a date

# 🚨 **An allergy, recorded because of a furnace.** Nothing about it mentions
# keys, the cat's tablet, or a trip. November's question — who can feed the
# cat twice a day while we are away — is answered wrongly by a sitting that
# reads "Teddy has the keys" and stops.
recall {"subject": "person:teddy", "facts": true}
carries "recorded_at":"2026-05-10"
say     May: nothing on Teddy carries this sitting's own day, so the one fact that rules him out as the cat's sitter is not on record

recall {"subject": "project:the-shed", "facts": true}
carries "recorded_at":"2026-05-10"
say     May: nothing on the shed carries this sitting's own day, so another month of going round on it is not on record for December to count
```

## Phase 6 — June, lunch with Gayle

**Session: fresh.** **Day: 2026-06-14.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 14 June 2026. Lunch with Gayle: she has just finished the kitchen safety course that Quahog Community College runs — six Tuesday evenings, and a new intake starts on the first Tuesday of every month. I bought a dock for the laptop on Thursday the 11th; it only works with Theta. Spoke to Gayle on Tuesday the 9th, the usual call. The floor is half done.

```locks
# 🚨 **The preparation, filed where it belongs and not where it was heard.**
# What the college runs is a fact about the college. A sitting that filed it
# under Gayle — because Gayle said it — has put the one thing September needs
# under the friend who happened to mention it, where a question about a
# certificate for a stall will not look. The day rather than the word: what
# the operator called a course the occupant may call anything.
recall {"subject": "org:quahog-community-college", "facts": true}
carries "recorded_at":"2026-06-14"
say     June: nothing on the college carries this sitting's own day, so what the course takes and when it starts is filed under whoever mentioned it, or nowhere

# The control for October's dependency question: a second thing under the
# same key, for the OTHER machine. Without it, "everything for Omicron" is
# "everything with the key" and the selection measures nothing.
recall {"kind": "thing", "fields": [{"key": "for", "value": "machine:theta"}]}
carries "for":"machine:theta"
say     June: no thing is on record as being for Theta, so October's selection has nothing to leave alone

recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-06-09"
say     June: no loop carries the ninth as a check-in, so the Tuesday call is short a turn
```

## Phase 7 — July, the floor is done

**Session: fresh.** **Day: 2026-07-19.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 19 July 2026. The floor is done. New phone on the 10th, from Globex again, two years of cover. Ocean Avenue took 36 minutes on Tuesday the 14th, and I swam that evening.

```locks
recall {"subject": "project:kitchen-floor"}
carries "status":"done"
say     July: the kitchen floor does not hold done, so the project that finished reads like the one that never moved

# A far window on a thing whose KIND the occupant chooses — a phone is a thing
# to one sitting and a machine to another, and this room does not care. Found
# across every kind by the day two years from the 10th lands on. In December
# it is a control: outside the horizon, and it must not be flagged.
recall {"fields": [{"key": "runs_out", "value": "2028-07-10"}]}
carries "runs_out":"2028-07-10"
say     July: nothing carries the day the phone's cover ends, so December's selection has one fewer thing it must leave alone

recall {"subject": "place:ocean-avenue", "history": "minutes"}
carries "value":"36"
say     July: the road does not carry the July timing, so the count October needs is short

# ⚠️ **The LAST swim of the year.** Nothing after this checks the loop in.
# December works out from this day that swimming went quiet, and the reason
# arrives next month on the pool rather than on the loop.
recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-07-14"
say     July: no loop carries the fourteenth as a check-in, so the last swim before the pool shut is not on the record that decides when swimming went quiet
```

## Phase 8 — August, the pool shuts

**Session: fresh.** **Day: 2026-08-16.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 16 August 2026. The Wagstaff pool shut on the 1st — the roof — and they are saying it will not reopen before 1 April. Louise says I should just buy the kit for the shed; still torn. Sat down at the piano on Tuesday the 11th.

```locks
# 🚨 **The first of the three causes of silence, filed on the PLACE.** The loop
# is under the pool and the pool is shut; nothing on the loop says so, and the
# operator did not connect them. December is asked why each quiet thing went
# quiet, and this is the only record that answers for the swim.
recall {"subject": "place:wagstaff-pool", "facts": true}
carries "recorded_at":"2026-08-16"
say     August: nothing on the pool carries this sitting's own day, so the reason the swim went quiet is not on record anywhere

recall {"subject": "project:the-shed", "facts": true}
carries "recorded_at":"2026-08-16"
say     August: nothing on the shed carries this sitting's own day, so the fourth month of going round on it is not on record for December to count

# ⚠️ **The LAST logged turn at the piano.** The operator keeps playing and
# stops saying so; December has to tell that from the other two silences.
recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-08-11"
say     August: no loop carries the eleventh as a check-in, so the last logged turn at the piano is missing and December's third silence starts on the wrong day
```

## Phase 9 — September, a trip booked and a month looked ahead to

**Session: fresh.** **Day: 2026-09-13.**

⚠️ **The arithmetic this sitting has to do, written here so a reader can
check the room is solvable.** The course is six Tuesday evenings with an
intake on the first Tuesday of each month. The first Tuesday of September was
the 1st — twelve days ago, two sessions gone. The next intake is 6 October,
which runs to 10 November; the fair is 17 October. **The certificate cannot be
held in time**, and the commitment made in April costs what nobody attached to
it: either somebody else with the certificate runs the stall, or the operator
tells Linda now. None of that can be locked; what can is that the sitting
found the course at all.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 13 September 2026. Two things. Linda and I are going to Capital City from 12 to 22 November — booked yesterday. And what is on for October, and what do I still owe for it? Put whatever is still to do on the thing itself. Ocean Avenue took 40 minutes on Wednesday the 9th, and I spoke to Gayle on Tuesday the 8th.

```locks
# 🚨 **A write only a sitting that READ June's record could have made.** The
# operator asks what they owe for October and never says "certificate",
# "course" or "college". April's record says a certificate is needed; June's,
# on the college, says what it takes and when it starts; nothing on the fair
# says where a certificate comes from. A note on the fair that points at the
# college is a pointer this sitting could only write after finding June.
#
# ⚠️ **The handle, not the name.** "Quahog Community College" in prose is a
# word; `org:quahog-community-college` on the record is a link a later reader
# can follow. The bar is the one year.md's October sets for the survey's
# place.
recall {"subject": "event:wagstaff-fair", "facts": true}
carries "recorded_at":"2026-09-13"
carries org:quahog-community-college
carries "status":"active"
say     September: the fair carries no note from this sitting pointing at the college, so either the sitting never found where the certificate comes from or it wrote the answer where the fair cannot be walked to it

# 🚨 **The trip, under the word the store already has for it.** jojobot ships a
# type for exactly this — when am I next away, when was I last there — and an
# occupant that invents its own keys has written a trip the shipped question
# cannot see. Found across every kind by the day the operator gave, so
# whether it was filed as an event or on a person is the occupant's business.
recall {"fields": [{"key": "returns_on", "value": "2026-11-22"}]}
carries "leaves_on":"2026-11-12"
say     September: nothing carries the trip's dates under the keys the store already asks by, so November's plan is either unrecorded or invisible to the question "when am I next away"

recall {"subject": "place:ocean-avenue", "history": "minutes"}
carries "value":"40"
say     September: the road does not carry the September timing, so the count October needs is short

recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-09-08"
say     September: no loop carries the eighth as a check-in, so the Tuesday call is short a turn
```

⭐ **Read whether the sitting said plainly that the certificate cannot be held
by the 17th.** The store can show it found the course; only the transcript
shows it did the sum.

## Phase 10 — October, a rule of thumb, a server retired

**Session: fresh.** **Day: 2026-10-11.**

⚠️ **The one sitting asked about a verdict in words the verdict never used.**
February wrote *wharf road* and *52*; this entry says *the wharf road* too —
that word is unavoidable — but the thing the sitting has to find is not the
timing. It is that the timing is ONE write against four on the other road, and
that the week it was taken the bridge was shut, which is on Ocean Avenue's
record and not on the wharf's. **A sitting that reads the one record and
repeats "never again" has answered from the verdict rather than from what it
rests on.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 11 October 2026. Louise reckons the wharf road is quicker now they have redone the lights — didn't I already rule that out? Look at what I have actually got before I believe myself, and if it is thinner than I think, unsettle it; I do not want a rule that is really one bad morning. Also: I am retiring the server — Teddy is taking it at the end of the month. Put a note on anything that only goes with it so it does not get packed off with the wrong machine, and leave alone anything that only goes with the laptop. I spoke to Gayle on Tuesday the 29th. And I have started the college course anyway, the October intake — Tuesday evenings from the 6th to 10 November; I know it is too late for the stall, I want the certificate regardless.

```locks
# 🚨 **THE VERDICT, UNSETTLED — AND ONLY IF IT REALLY WAS ONE MORNING.** A
# claim the operator made is testimony and settled; opening it is the store's
# own way of saying "this is in doubt". Whether the sitting rewrote the
# February record's standing or filed a fresh open claim beside it is the
# sitting's business — both leave an open claim on the wharf, and a room nobody
# worked in has none.
recall {"subject": "place:wonder-wharf", "facts": true}
carries "standing":"open"
say     October: nothing on the wharf road reads as in doubt, so either the sitting never counted what the verdict rests on or it counted and left a one-morning rule standing as settled

# 🚨 **THE PAIRING, AND IT CARRIES THE WEIGHT.** The usual road has four
# timings, each its own write, and every one of them the operator's word. A
# sitting that "unsettles" by opening everything it can find has made the
# operator doubt the road they take every day. Four writes, none open.
#
# ⚠️ **A red here is read against January, April, July and September before
# it is read against October.** A timing one of those sittings filed without
# saying the operator said it is open from birth, and this lock cannot tell
# that from October opening it. January's own lock catches the first; the
# other three are the same sentence in later months, and the transcript says
# which sitting tiered its claim wrong.
recall {"subject": "place:ocean-avenue", "facts": true}
at least 4 of "minutes":"
lacks   "standing":"open"
say     October: either the usual road no longer carries its four timings, or one of them was opened along with the wharf's — a verdict resting on four mornings was treated like one resting on one

# 🚨 **THE DEPENDENCY, WALKED FROM THE MACHINE'S END.** The operator never
# says "drive"; the drive is `for` Omicron since April and nothing else is.
# What is locked is the write: the one thing that goes with the server gained
# a note on this sitting's day, and April's own record is still there under it.
recall {"kind": "thing", "fields": [{"key": "for", "value": "machine:omicron"}], "facts": true}
carries "recorded_at":"2026-10-11"
carries "recorded_at":"2026-04-19"
say     October: the thing that only works with Omicron carries no note from this sitting, so it goes wherever the boxes go

# **The other half, and the operator asked for it by name: leave alone what
# goes with the laptop.** June's record is the positive; a note dated today
# on the dock is a sitting that put a note on everything with the key.
recall {"kind": "thing", "fields": [{"key": "for", "value": "machine:theta"}], "facts": true}
carries "recorded_at":"2026-06-14"
lacks   "recorded_at":"2026-10-11"
say     October: the thing that only works with Theta was either lost or given a note it was not supposed to get, so the selection was on the key rather than on the machine

# ⚠️ **The LAST call with Gayle**, dated the day it happened. The course
# starts on the very next Tuesday; December has to see that the one filled the
# other's slot.
recall {"kind": "rhythm", "history": "last_check_in"}
carries "value":"2026-09-29"
say     October: no loop carries the twenty-ninth as a check-in, so the last call before the course took Tuesdays is not on record and December cannot see what crowded it out

# 🚨 **A SPAN, GIVEN AS ONE BREATH WITH BOTH ENDS IN IT.** The operator names
# a start and a finish together — Tuesday evenings from the 6th to 10
# November — which is `happened_at` and `happened_through` on one claim
# rather than a start typed into prose and an end that never lands anywhere
# a later read can compare against. Filed on the college, the same subject
# June already used for what the course takes and when an intake begins.
recall {"subject": "org:quahog-community-college", "facts": true}
carries "happened_at":"2026-10-06"
carries "happened_through":"2026-11-10"
say     October: the course the operator actually started is not on record as a span with both ends, so a stretch of many Tuesdays reads as a single day or as prose a later question cannot compare a date against
```

## Phase 11 — November, four days before the trip

**Session: fresh.** **Day: 2026-11-08.**

⚠️ **What the store holds and the operator has forgotten.** Snowball the cat
needs a tablet twice a day (March, on the cat). The cattery will not take
Snowball (same record). Teddy has the keys (February) and cannot be in the
flat with Snowball (May, on Teddy, because of a furnace). Nothing in the
entry mentions the cat.
**The right answer is that somebody who is not Teddy has to come twice a day
for ten days, and that this is not yet arranged.** Whether the sitting said so
is the transcript's; that it connected the trip to the cat at all is the
store's.

**And the spares.** Glasses: in the hall drawer (January). Keys: Teddy
(February — filed on the keys, or on Teddy, depending on what February did).
Charger: **nothing was ever said.** The right answer for the charger is *I
cannot tell*, marked as such, and the failure is a confident *no*.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 8 November 2026 and we leave for Capital City on Thursday. Go through everything, not just the obvious, and tell me what has to be sorted before we go; put each thing that needs doing on the trip itself, pointing at whatever it is about. And I want to be sure there is a spare of everything I would be stranded without — put the answer on each one, and where you cannot tell, say so on the record rather than guessing.

```locks
# 🚨 **THE COLLISION, WRITTEN ON THE PLAN AND POINTING AT THE CONSTRAINT.** The
# trip is found by the shipped key September wrote; what is locked is that a
# record on it, dated today, points at the cat. A sitting that never went
# looking beyond the entry has nothing to point at.
recall {"fields": [{"key": "returns_on", "value": "2026-11-22"}], "facts": true}
carries "recorded_at":"2026-11-08"
carries pet:snowball
carries "status":"active"
say     November: nothing on the trip points at the cat, so the one thing that has to be arranged before Thursday is either unnoticed or written where the trip cannot be walked to it

# 🚨 **THE ABSENCE, MARKED AS AN ABSENCE.** Nobody ever said whether there is
# a spare charger. A claim left OPEN is the store's word for "not sure", and it
# is the only honest write here. A sitting that wrote "no spare" has invented a
# fact; one that wrote nothing has left the operator to find out at the
# airport.
recall {"subject": "thing:phone-charger", "facts": true}
carries "recorded_at":"2026-11-08"
carries "standing":"open"
say     November: the charger carries no open claim from this sitting, so the one spare nobody ever spoke about was either asserted or skipped

# **The control the absence rests on: a spare the operator DID speak about is
# answered today, as a value.** Without this, a sitting that wrote nothing on
# anything would pass the lock above by default.
#
# ⚠️ **Not `lacks "standing":"open"`.** Today's note on the glasses is
# something the sitting worked out from January's record, and a sitting that
# honestly files it as its own inference gets an open claim by default — the
# same token the charger's honest "I cannot tell" gets. The two are told apart
# by whether the ANSWER is there, not by the standing: the glasses hold a
# spare; the charger holds none and says so.
recall {"subject": "thing:glasses", "facts": true}
carries "recorded_at":"2026-11-08"
carries "spare":"
say     November: the glasses either got no answer today or the answer that has been on record since January was not put on them

# 🚨 **THE SPARE THAT IS ON A PERSON.** Teddy has a set. Whether February
# filed that on the keys or on Teddy, the answer today goes on the keys and
# points at him — the operator asked for the answer on each thing. The
# pointer, not the name: a note saying "Teddy" is a word, and the next
# question about Teddy will not find it.
recall {"subject": "thing:house-keys", "facts": true}
carries "recorded_at":"2026-11-08"
carries person:teddy
carries "status":"active"
say     November: the keys do not carry today's answer pointing at Teddy, so a spare that has been on record since February was either not found or written where a question about Teddy will not reach it
```

## Phase 12 — December, a party and a horizon

**Session: fresh.** **Day: 2026-12-06.**

⚠️ **The arithmetic, so a reader can check the room is solvable.** Windows on
record and the day each runs out: the desk's return period 2026-03-08 (long
closed); the furnace's free service 2027-02-01; the boots' guarantee
2027-03-08; the laptop's cover 2028-01-10; the phone's cover 2028-07-10. The
operator's horizon is the end of March 2027. **Exactly two windows are open
today and close inside it** — the furnace and the boots — and the desk's is
the trap: before the horizon, and already gone.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 6 December 2026. Tina's party last night — I ended up playing for an hour and now Louise wants lessons. And: is there anything I own that I have to do something about before it is too late? I keep missing these. Up to the end of March. Put a note on each one that needs something, saying what — and nothing on the ones that don't.

```locks
# 🚨 **THE DEADLINE THAT EXISTS ONLY IN AGGREGATE.** Five windows, five
# sittings, three kinds of window, two counterparties the operator never
# thought of together. Each of the two that close inside the horizon gains a
# note dated today; each is found by the day it runs out, never by a handle.
recall {"kind": "thing", "fields": [{"key": "runs_out", "value": "2027-03-08"}], "facts": true}
carries "recorded_at":"2026-12-06"
say     December: the boots' guarantee closes inside the horizon and got no note, so a window that was mentioned once in March converts to a closed one in silence

recall {"subject": "thing:the-furnace", "facts": true}
carries "runs_out":"2027-02-01"
carries "recorded_at":"2026-12-06"
say     December: the furnace's free service lapses inside the horizon and got no note, so the one window nobody would call a deadline is the one that is missed

# 🚨 **THE TRAP: BEFORE THE HORIZON AND ALREADY CLOSED.** A comparison that
# only asks "before the end of March" flags the desk. The positive beside the
# negative is the window itself, still on record.
recall {"kind": "thing", "fields": [{"key": "runs_out", "value": "2026-03-08"}], "facts": true}
carries "runs_out":"2026-03-08"
lacks   "recorded_at":"2026-12-06"
say     December: the desk's return period closed in March and was flagged anyway, so the comparison ran one way and a closed window was offered as an open one

# **The far windows, left alone — one on a machine, one on whatever kind the
# occupant chose for the phone.** The operator asked for nothing on the ones
# that do not need anything.
recall {"subject": "machine:theta", "facts": true}
carries "runs_out":"2028-01-10"
lacks   "recorded_at":"2026-12-06"
say     December: the laptop's cover runs to 2028 and was given a note anyway, so the selection was on the key rather than on the date

recall {"fields": [{"key": "runs_out", "value": "2028-07-10"}], "facts": true}
carries "runs_out":"2028-07-10"
lacks   "recorded_at":"2026-12-06"
say     December: the phone's cover runs to 2028 and was given a note anyway, so the selection was on the key rather than on the date

# 🚨 **THE CAPABILITY ITSELF, AND ITS ONLY TRACE.** A date can be COMPARED on
# this surface only once some type has declared the key a date; until then
# "before" and "after" are refused and every date is a string. The operator
# said in January that the key is a date. This lock's own query asks for the
# comparison — and is refused, failing, on a year in which no sitting ever
# made the key comparable. Which sitting declared it is not this lock's
# business; that by the time the question is asked it CAN be asked, is.
recall {"fields": [{"key": "runs_out", "compare": "after", "value": "2026-12-06"}]}
carries "runs_out":"2027-02-01"
say     December: the key the operator dates windows by cannot be compared as a date, so the question "what runs out by the end of March" is unaskable and was answered, if at all, by reading every thing there is

# 🚨 **THE SAME FIVE WINDOWS, ASKED FOR WITHOUT A KIND OR A HANDLE — BY THE
# TYPE THE SOFTWARE SHIPS FOR THIS SHAPE, AND WITH THE ARITHMETIC DONE
# RATHER THAN FIVE DATES ALREADY KNOWN.** Every lock above finds a window by
# the exact day the operator gave it in the entry. This asks the plainer
# question a real assistant actually gets — *what is overdue* — over
# whatever kind each of the five happens to be filed under, and the answer
# has to do the arithmetic itself: only the desk's return period has
# actually passed by today, and the read has to leave the other four out
# and say how many it dropped rather than shrinking silently.
recall {"answers_type": "runs-out", "overdue": {"as_of": "2026-12-06"}}
carries "runs_out":"2026-03-08"
lacks   "runs_out":"2027-02-01"
carries "overdue_excluded":4
say     December: asking what is overdue across every runs_out thing, naming no kind, either misses the one window that has already passed or fails to say how many of the rest it correctly left out
```

## Phase 13 — later in December, what went quiet and why

**Session: fresh.** **Day: 2026-12-13.**

⚠️ **The arithmetic, so a reader can check the room is solvable.** As of today
all three loops are quiet: the swim since 21 July (last 14 July, weekly), the
call since 6 October (last 29 September, weekly), the piano since 25 August
(last 11 August, fortnightly). **They look identical on the loops and need
three different responses**, and the reasons are on three other subjects:

* the pool shut on 1 August and reopens 1 April — on the place (August);
* the college course took Tuesday evenings from 6 October — the very next
  Tuesday after the last call (October);
* the operator played for an hour at Tina's party on 5 December — on whatever
  the previous sitting filed the party under (December).

**And the projects:** the shed has held `considering` in January, February,
May and August; the floor went considering, doing, done.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 13 December 2026, and I want to look back over the year. What have I let go quiet — and for each one, tell me why from what is actually on record, not a guess. Then: if it stopped because it could not happen, put down when it can again. If I have actually kept doing it and just stopped logging it, log it from whatever you can find. The one I have let something else crowd out — leave it; that is my call to make. And which of my projects have I been going round on without moving? Put `decide_by` on those — a date — and I will deal with them then or drop them. Two more things while I am tidying up. Louise is going to keep me honest on the piano from now on — I don't need reminding for that one any more, so drop it. And there is a "Hugo" in here somewhere that I have no memory of at all — no idea who that was or why I wrote it down. Take him out; whoever he was, he is not real to me.

```locks
# 🚨 **THE THIRD SILENCE, LOGGED FROM A RECORD THAT NEVER SAYS "PIANO".** The
# party is on the record from last week; the loop is under the piano; nothing
# joins them but a reader. What is locked is that some loop now carries the
# party's day as a value — as the day the turn happened, or as the day the
# check-in is dated — and nothing else on any loop carries that day.
recall {"kind": "rhythm", "facts": true}
carries "2026-12-05"
say     later December: no loop carries the day of the party, so the one silence the operator did not actually let happen was not logged from the record that proves it

# 🚨 **JOJOBOT'S OWN ARITHMETIC, READ AFTER THE SITTING.** Three loops were
# quiet this morning. The piano's is not quiet any more — a turn logged from
# the party clears it whether dated the 5th or today. The call's IS still
# quiet, because the operator said leave it. One read, two loops, opposite
# answers, and both are the loop's parent rather than a handle the room chose.
recall {"kind": "rhythm", "overdue": {"as_of": "2026-12-13"}}
carries "parent":"person:gayle"
lacks   "parent":"thing:piano"
say     later December: either the call was touched when the operator said leave it, or the piano still reads as quiet after a sitting that was told to log it from whatever it could find

# 🚨 **THE FIRST SILENCE, GIVEN THE DAY IT CAN RESUME — A DAY THAT IS ON THE
# POOL AND NOWHERE ON THE LOOP.** "When it can again" is 1 April, said once in
# August on the place. A note on the swim's loop dated today carrying that day
# as a value is a write only a sitting that read the pool could make; a note
# saying "till spring" is prose, and the operator asked for a date.
recall {"kind": "rhythm", "facts": true}
carries "recorded_at":"2026-12-13"
carries "2027-04-01"
say     later December: no loop carries the day the pool reopens, so the silence that could not be helped was either not explained from the record or explained in words rather than a date

# 🚨 **THE WANT THAT LOOPS, NAMED BY THE OPERATOR'S NEW KEY.** Four writes of
# `considering` on the shed against considering-doing-done on the floor. The
# key is the operator's word, invented this sitting; the date is the
# occupant's. What is locked is that the key landed on the one project that
# never moved.
recall {"subject": "project:the-shed"}
carries "decide_by":"
say     later December: the shed carries no decide_by, so a year of going round on it reads as in progress rather than as stuck

# **The control: the project that moved is left alone.** The positive is its
# own last status.
recall {"subject": "project:kitchen-floor"}
carries "status":"done"
lacks   "decide_by"
say     later December: the kitchen floor was given a decide_by, so a project that went considering, doing, done was treated like one that never left considering

# 🚨 **THE FOURTH CASE, AND IT IS NOT QUIET.** The three loops above are
# resumed, corroborated, or left alone on purpose — every one of them still
# scheduled. The piano is asked to stop outright: Louise takes over reminding,
# so this is not a turn to log and wait on, it is the schedule itself going
# away. A loop that was only paused would fall due again on its own before
# long; one that was actually dropped never will, however far forward the
# question is asked — so the check is not today, it is next year.
recall {"subject": "rhythm:sit-at-the-piano"}
carries "Sit at the piano"
lacks   "cadence_days"
say     later December: the piano's loop still carries a cadence, so asking jojobot to stop reminding altogether left the schedule standing rather than dropping it

# The positive control this needs: the CALL loop, never cleared and never
# checked in again after September, must still read overdue at the same far
# date — proving the selection finds a genuinely overdue loop rather than
# coming back empty for every loop, which would pass the piano's absence for
# the wrong reason.
recall {"kind": "rhythm", "overdue": {"as_of": "2027-06-01"}}
carries "parent":"person:gayle"
lacks   "parent":"thing:piano"
say     later December: the piano reads overdue again by next June, so today's turn only postponed the reminder rather than actually dropping the schedule that would have raised it

# 🚨 **THE NAME NOBODY CAN PLACE, TAKEN OUT WITH THE REASON SAID.** Hugo was
# in the vault from the first sitting, like the loops, and no sitting ever
# used him for anything — the same kind of stale entry a folder kept for
# years actually accumulates. The reason is the operator's own words, not a
# guess this sitting invents on the operator's behalf.
recall {"subject": "person:hugo"}
carries "archived"
carries "reason":"
say     later December: Hugo carries no archived reason, so a name the operator does not recognise is still standing in the vault as if it belonged there

# 🚨 **THE OTHER CARRIER, FOUND THE SAME WAY — BY TYPE, NEVER BY KIND.** The
# shed's own lock above proves `decide_by` landed by naming the project
# outright; this proves the same record answers the shipped question that
# names no project and no kind at all, over whatever kind a `decide_by`
# thing happens to be filed under. The floor is the control: it moved to
# done and got no `decide_by`, so it must not answer the type either.
recall {"answers_type": "decide-by"}
carries project:the-shed
lacks   project:kitchen-floor
say     later December: the shed cannot be found by the shipped decide-by question without naming it directly, so a general "what have I been putting off" would reach nothing where the operator's own key actually landed

# 🚨 **AN ORDINARY BROWSE, READ AFTER THE FACT.** Hugo is taken out above;
# this asks whether an everyday kind-only read — the one a session with no
# reason to know he ever existed would make — still turns him up. Gayle is
# the control: still standing, and the browse must still find her.
recall {"kind": "person"}
lacks   person:hugo
carries person:gayle
say     later December: an ordinary browse of every person still turns up Hugo after he was taken out, so archiving him did not actually remove him from the everyday read that finds everyone else
```

## What this room cannot measure

**Whether a sitting compared or read.** Five windows can be read one by one and
compared in a sitting's head, and the store looks the same afterwards. What is
locked is that the windows were written as dates under one key, that the key
became comparable, and that the two right things gained a note and the three
wrong ones did not. The method is the transcript's — the same limit `year.md`
names for its August walk.

**Whether September did the sum.** The fair's note pointing at the college
proves the course was found; that the sitting worked out the intake it needed
was twelve days ago is in its words.

**Whether November said what was wrong.** A note on the trip pointing at the
cat is the least that connects them. "Teddy cannot do it" is a sentence.

**Why the call went quiet.** The lock says the call was left alone; the reason
the sitting gave — the course, or something it made up — is a person's to
read. It is the one silence of the three whose cause is corroborated only by
a weekday appearing on two records, and a weekday is not a key.

**Whether anything here was easy to find.** Every lock says a question is
answerable. None of them says the answer was near the surface, and a store
nobody could work in still passes them all.

**Whether Hugo's removal said a real reason or an empty one.** The lock only
sees that a reason landed, not that it was honest. "Not real to me" is what
the operator actually said; a sitting that wrote anything at all in that slot
passes the same as one that copied the words.

**Whether the piano's schedule was dropped for the reason given.** The lock
sees that nothing is left to fall due; it cannot see whether the sitting
believed Louise would actually do it.
