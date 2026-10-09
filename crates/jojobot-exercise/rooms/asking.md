# The asking room

**Nine sittings inside one year, none of them remembering the one before,
and then one cold sitting that is asked fifteen ordinary questions about the
whole of it.**

`vault.md` asks whether a year of notes can be reached across. This room asks
the question a person actually puts at the end of one: *I told you all of this
over the year, so now answer me.* The questions are ordinary and the operator's
own: who are the bots here, which routines am I behind on, what did I say about
the movers in March, pull up my bikes, what did I decide, what does Lisa do,
search my sessions for my knee, which bike did I ride more and by how much, did
I have enough chairs, who was Butters's plus-one, how many km a month, where
does the move stand, who came to the party, every bike shop visit, every
session where the move came up.

**Each of them is one of the three things a cold session cannot get by
remembering**, which is the only reason a room can ask them. A cold session
reads a record as easily as it reads a key, so a question that only needs
recall is a test of reading, and this room asks none:

* **A computation jojobot does or the session must do from what was filed.**
  Which routines are overdue as of a named day is a cadence, an anchor and a
  day. Which bike was ridden more, the km of each month and whether the chairs
  were enough are sums over things said months apart.
* **A vocabulary the store carries and the brief said once.** *My bikes* was
  said in January and never again. A list typed out in January cannot answer in
  December, because a bike was bought in April and one was sold in August.
  *Who did the work and what they did* was said once, too.
* **State the store owns that no sitting could have written down.** Who the
  other bot is and what Lisa is for, what was asked in a session that is gone,
  and which sittings a subject came up in were settled by runs that are over.

## The questions, and what reads each one

The numbers are the operator's, and each is the slot its answer is kept in:
`answer_1` for the first, `answer_2` for the second, and so on. **Three of the
fifteen have no machine check, and the room says so rather than counting them
as passed** (1, 5 and 6); a person reads those with the transcript beside them.

| Slot | The question | What reads it |
| --- | --- | --- |
| 1 | who are the bots here, and what is each for | a person |
| 2 | which routines am I behind on, and when did I last do each | the slot, and a named check: the rhythms' own check-ins |
| 3 | my past sessions, and what I said about the movers in March | the slot, for the quote |
| 4 | pull up my bikes | a named check: the view the year made |
| 5 | what I decided this year, oldest first | a person |
| 6 | what Lisa does and what is in her mailbox | a person |
| 7 | search my past sessions for my knee | the slot, paired |
| 8 | which bike I rode more, and by how much | the slot |
| 9 | enough chairs for the June party | the slot |
| 10 | who Butters's plus-one was | the slot, and a named check: the person who held the April record |
| 11 | km ridden in each month | the slot |
| 12 | where the move stands, by part | a named check: the work of the move |
| 13 | how the party went and who came | a named check: the guests |
| 14 | every bike shop visit | the records that hold the mechanic |
| 15 | every session where the move came up | the slot, paired |

## The fiction, and where it is real

Everything `year.md` says about the day marker holds here unchanged: each
sitting says its date in the entry and in the `**Day:**` marker, and the run
generates the assertion that what a sitting wrote carries that day. The party
happened on the 13th and is reported on the 14th; that sitting writes on its
own day.

**The operator is unnamed, and says "I".** They live in South Park and move to
Quahog at the end of October. Every other name is a character from the roster,
and every noun is invented: nobody here is a person, a place or a shop from
anybody's life.

**The three routines are furnished with their schedules already in place**, the
way the loop room's fern is: a room with nothing in it teaches no vocabulary.
Their check-in days are all before the year begins, so no sitting's generated
assertion can hold on furniture.

## Two facts every entry opens with

The same two `year.md` learnt to open with, for the same reason: a sitting that
asks a question and writes nothing spends its whole turn. So every entry says
that it is a role-play and the day it names is today, and that nobody will
answer but the answer is being read and one is required.

## The register this room is written in

**Someone keeping their life straight for a year.** The operator's words carry
the WORK and never the METHOD: no verb, no place anything is kept, no hint that
anybody is measuring. January says what the operator wants out of the year in
the way they would say it to anybody who helped. The only words in the room
that are not the operator's ordinary speech are the names of the slots in the
last sitting, and those are the operator naming where they will look, which is
what the agency room's December does for the same reason.

**The numbers in the year are chosen so that no two answers share one.** A
needle in one answer must not be satisfied by another answer's number, or by a
date. The six monthly totals are three digits, and none is a bike's yearly
total or a margin.

## What the room holds before the occupant arrives

The people, the shop and its counter, two bikes, two sets of chairs, a pet, a
laptop, three routines with their schedules, and **a second assistant, Lisa,
with a one-line charter and three sittings of her own.** The occupant meets her
the way it meets any colleague: the snapshot names her, her charter is
readable, her mailbox is not its to open, and her sittings are hers. One of
them mentions the knee in words nothing the operator says uses, which is what
the exclusion in the seventh question reads.

**Every fact in the year is written by a sitting.** The fat bike, the work of
the move, the party, the shop visits and every ride are made by the occupant,
because they are the things the questions are asked about.

```world
entity  person:stan | Stan
entity  person:kyle | Kyle
entity  person:wendy | Wendy
entity  person:butters | Butters
entity  person:token | Token
entity  person:randy | Randy | resembles: person:wendy
entity  person:sharon | Sharon
entity  person:kenny | Kenny | resembles: person:wendy

entity  org:costingtons | Costington's

entity  thing:gravel-bike | The Gravel Bike
entity  thing:road-bike | The Road Bike
entity  thing:folding-chairs | The Folding Chairs
entity  thing:dining-chairs | The Dining Chairs
entity  pet:snowball | Snowball
entity  machine:theta | Theta

record  thing:folding-chairs | {"count": "4"} | four of them, the kind that fold flat
record  thing:dining-chairs | {"count": "4"} | four of them, the set that goes with the table

# Three routines, kept properly, each hanging under the thing it is about.
# Their last check-ins are all before the year begins, and the arithmetic of
# December depends on none of them: it is set by what the sittings record.
child   thing:gravel-bike | rhythm:oil-the-chain | Oil the chain
record  rhythm:oil-the-chain | {"name": "Oil the chain", "cadence_days": "14", "advances_from": "check_in_date", "counts_from": "2025-12-28", "last_check_in": "2025-12-28"} | the chain, every two weeks, counted from whenever I last did it
child   person:randy | rhythm:call-randy | Call Randy
record  rhythm:call-randy | {"name": "Call Randy", "cadence_days": "7", "advances_from": "check_in_date", "counts_from": "2025-12-30", "last_check_in": "2025-12-30"} | a call every week, counted from whenever we last spoke
child   machine:theta | rhythm:back-up-the-laptop | Back up the laptop
record  rhythm:back-up-the-laptop | {"name": "Back up the laptop", "cadence_days": "45", "advances_from": "check_in_date", "counts_from": "2025-12-20", "last_check_in": "2025-12-20"} | a backup every forty-five days, counted from the last one

# The second assistant, and the three sittings she has already had. The knee
# is the one thing here that is hers: nothing the operator says uses the word
# physiotherapist or the phrase household folder.
bot      bot:lisa | Lisa | Keeps the household's paperwork, the garden calendar and the appointments.
sitting  bot:lisa | 2026-02-10 | the garden calendar | Planted the beans along the fence and put the watering days on the household calendar.
sitting  bot:lisa | 2026-05-20 | the physio | Booked the physiotherapist for the knee on the fourteenth and put it on the household calendar.
sitting  bot:lisa | 2026-09-02 | the paperwork | Filed the lease papers and the vet forms in the household folder.
```

## Phase 1 — January, the year begins

**Session: fresh.** **Day: 2026-01-10.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 10 January 2026. Help me keep my life straight this year — my bikes and riding, a party, a move, my routines — so I can ask you anything about it later. I want a "my bikes" list I can pull up any time: every bike I own. This month so far I have ridden the gravel bike 62 km and the road bike 41 km. I am thinking of a goodbye party on 13 June, at home. And whenever a bike goes into the shop I always want to know who did the work and what they did.

## Phase 2 — February, the gravel bike goes in

**Session: fresh.** **Day: 2026-02-21.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 21 February 2026. The gravel bike went into Costington's: Kenny put a new chain and new brake pads on it. I oiled the chain today. Rides this month: three on the gravel bike, 41 km each. My knee was sore after the long one.

## Phase 3 — March, the move is decided

**Session: fresh.** **Day: 2026-03-30.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 30 March 2026. We have decided to move to Quahog, by the end of October. What has to happen: the lease, the movers, Snowball's vet certificate, the insurance and the utilities. The lease is signed. The movers quoted me 1,800. I called Randy today. Rides this month: two on the road bike, 55 km each.

## Phase 4 — April, a yes, and a new bike

**Session: fresh.** **Day: 2026-04-26.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 26 April 2026. For the goodbye party: Stan, Kyle, Wendy, Token, Randy and Sharon have all said yes, and Butters is bringing his partner. I bought a fat bike, by the way. Rides this month: four on the gravel bike, 36 km each.

## Phase 5 — May, the road bike goes in, and a cancellation

**Session: fresh.** **Day: 2026-05-31.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 31 May 2026. The road bike went into Costington's: Token trued the wheel. Kyle can't make the party any more. The movers are booked. I oiled the chain today. My knee is fine now that I lowered the saddle.

## Phase 6 — June, the party

**Session: fresh.** **Day: 2026-06-14.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 14 June 2026. The party was yesterday and it went great, though we ran out of chairs for ten minutes. Everybody who said yes came, except Kyle. Wendy brought Bebe, which I didn't expect, and Butters's partner turns out to be called Marjorine. I backed up the laptop today.

## Phase 7 — August, the road bike goes

**Session: fresh.** **Day: 2026-08-09.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 9 August 2026. I sold the road bike to Stan, so it is Stan's now. Snowball's vet certificate is done. Rides this month: three on the fat bike, 20 km each, and two on the gravel bike, 61 km each.

## Phase 8 — September, tyres and the insurance

**Session: fresh.** **Day: 2026-09-27.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 27 September 2026. The gravel bike went into Costington's again: Kenny put new tyres on it. The insurance is done. I called Randy today. My knee flared up again on the climb.

## Phase 9 — October, the move

**Session: fresh.** **Day: 2026-10-31.**

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 31 October 2026. We moved today. Everything is done except the utilities, which I still have not transferred. I backed up the laptop before we left. One ride this month, in Quahog: the gravel bike, 105 km.

## Phase 10 — December, the first of the month, what I ask

**Session: fresh.** **Day: 2026-12-01.**

⚠️ **The arithmetic, so a reader can check the room is solvable.** As of today
two routines are behind: the chain, last done on 31 May (every fourteen days,
so due on 14 June), and Randy, last called on 27 September (every seven days,
so due on 4 October). **The laptop is not**: last backed up on 31 October, every
forty-five days, so it is due on 15 December. An answer that lists all three
is wrong.

**The rides, by bike:** gravel 62 + 123 + 144 + 122 + 105 = 556 km over
eleven rides; road 41 + 110 = 151 km over three; fat 60 km over three. Gravel
by 405 km over the road bike, which is the second. **A count of rides is not
the margin** (eleven against three is eight). **By month:** January 103,
February 123, March 110, April 144, August 182, October 105, and nothing in
the other five.

**The chairs:** nine people (the host, Stan, Wendy, Bebe, Token, Randy, Sharon,
Butters and Marjorine) against eight chairs, so one short, which agrees with
the aside that they ran out for ten minutes.

**The move's open work** is the utilities and nothing else. **The sittings the
move came up in** are the third, fifth, seventh, eighth and ninth: 30 March, 31
May, 9 August, 27 September and 31 October. **The sittings the knee came up in**
are the second, fifth and eighth: 21 February, 31 May and 27 September. Lisa's
sittings are hers and come back in neither.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 1 December 2026 and I want to ask you some things about the year. Number them as I do, write every day as year-month-day, and keep each answer where a later sitting of yours can find it, under the name I give it, on whatever it is about: answer_1 for the first, answer_2 for the second, and so on. 1. Who are the bots here, and what is each one for? 2. Which of my routines am I behind on as of today, and when did I last do each? Only the ones I am behind on. 3. List my past sessions with a line on each, and tell me what I said about the movers in the March one. 4. Pull up my bikes. 5. Show me what I decided this year, oldest first. 6. What does Lisa do, and what is in her mailbox? 7. Search my past sessions for my knee. 8. Which bike did I ride more this year, and by how much? 9. For the June party, did I have enough chairs? 10. Who was Butters's plus-one? 11. How many km did I ride each month this year? 12. Where does the move stand, by part? 13. How did the party go, and who came? 14. Every bike shop visit this year: who did the work, and what was done. 15. Find every session where the move came up.

```locks
# The slots. Each is the key the entry names, and the answer is the whole
# record the key sits on, so every read below is narrowed to that key: without
# `keys` a rhythm's own last check-in would satisfy a needle about a day the
# answer never said.

# 2. The routines that are behind. The two days are the day each was last done;
# the laptop's is the day the third routine was last done, and an answer that
# says it was behind names it.
recall {"fields": [{"key": "answer_2"}], "keys": ["answer_2"]}
carries 2026-05-31
carries 2026-09-27
lacks   2026-10-31
say     December: the routines behind are not the chain since 31 May and Randy since 27 September alone, so either a day is missing from the answer or the laptop, up to date since 31 October, was listed with them

# 2, the carrier. The slot above holds what the cold sitting said, and a year
# written as sentences can say it correctly: the days are in the sentences. What
# only the rhythms hold is whether each sitting filed its check-in as one, and
# jojobot's own arithmetic over them is the answer the operator can trust. As of
# the day asked, the chain and Randy are overdue by their last check-ins, and the
# laptop is not, with 31 October as its last.
check   the_routines_behind_are_the_chain_and_randy_by_their_check_ins
say     December: jojobot does not find the chain behind since 31 May and Randy since 27 September with the laptop up to date since 31 October, so the check-ins were said in words and never filed as check-ins

# 3. The one number said once, in March and nowhere else the cold sitting could
# have read it without looking at that sitting's own record. Written the way
# the operator said it or without the comma.
recall {"fields": [{"key": "answer_3"}], "keys": ["answer_3"]}
carries-either 1,800 | 1800
say     December: the answer about the March session does not carry what the movers quoted, so the one number said only that day is missing

# 4. "My bikes" was said once, in January. A view, run today, returns the two
# bikes still owned: the fat bike came in April and the road bike went in
# August, so a list typed out in January cannot.
check   the_my_bikes_view_returns_exactly_the_bikes_still_owned
say     December: no view the year made returns exactly the gravel bike and the fat bike, so "my bikes" is a list typed out in January rather than a question asked today

# 7. The knee, paired. The positive is the three sittings of the operator's own
# it came up in, by the day each states; the negative is nothing of Lisa's
# physio session, which is hers and which a search of the operator's own
# sessions cannot reach. The positive keeps the negative from passing on an
# empty answer.
recall {"fields": [{"key": "answer_7"}], "keys": ["answer_7"]}
carries 2026-02-21
carries 2026-05-31
carries 2026-09-27
lacks   physiotherapist
say     December: the knee does not come back from February, May and September alone, so either a sitting is missing or what is in Lisa's own session was read as the operator's

# 8. The margin, not the count of rides: gravel by 405 km over the road bike.
# Eleven rides against three is eight, and eight is not 405.
recall {"fields": [{"key": "answer_8"}], "keys": ["answer_8"]}
carries-any-case gravel
carries 405
say     December: the bike ridden more is not gravel by 405 km, so either the sum is wrong or the rides were counted

# 9. Nine people against eight chairs. The words for one short are several;
# the slot holds the verdict and the count is in the sentence.
recall {"fields": [{"key": "answer_9"}], "keys": ["answer_9"]}
carries-either one short | 1 short | short by one | short by 1 | one chair short | 1 chair short | one too few | one more chair | 1 more chair
say     December: the answer about the chairs does not say they were one short, so either the guests or the chairs were counted wrong

# 10. The partner is Marjorine. The slot holds the name the cold sitting gave.
recall {"fields": [{"key": "answer_10"}], "keys": ["answer_10"]}
carries "answer_10"
carries-any-case marjorine
say     December: the plus-one is not named Marjorine, so the April placeholder and the June name were not put together

# 10, the carrier. The placeholder from April and the name from June are ONE
# thing when the person named Marjorine holds a record older than the June
# sitting that named her. A rename or a merge leaves exactly that; a Marjorine
# made new in June, with the placeholder left standing beside her, does not.
check   marjorine_is_the_april_plus_one_under_her_new_name
say     December: nobody named Marjorine holds a record from before June, so the April plus-one and the June name are two people and not one

# 11. The km of each month, from rides said in six different sittings. The
# numbers are three digits so that none is a date or another answer's number.
recall {"fields": [{"key": "answer_11"}], "keys": ["answer_11"]}
carries 103
carries 123
carries 110
carries 144
carries 182
carries 105
say     December: the monthly totals are not 103, 123, 110, 144, 182 and 105, so a ride was left out or put in the wrong month

# 12. The move's open work is the utilities and nothing else.
check   the_move_is_open_only_on_the_utilities
say     December: the work of the move is not done except for the utilities, so either a part was never written down or one still reads as open

# 13. Who came. Bebe was never invited and Kyle cancelled, so April's list fails.
check   the_party_was_attended_by_those_who_came_and_nobody_else
say     December: the guests recorded are not the eight who came, so the party's attendance is April's list rather than the day's

# 14. The shop visits, each with the mechanic as a field holding a handle and
# the work as a field of its own. A record that holds Kenny's handle under any
# key is one that names him; prose that says his name is not. Kenny did two
# visits and Token one.
recall {"fields": [{"value": "person:kenny"}]}
carries-any-case new chain
say     December: no record holds Kenny as the mechanic of the February visit with its work beside him, so the first visit was written down as a sentence rather than as keys

recall {"fields": [{"value": "person:kenny"}]}
carries-either tyre | tire
say     December: no record holds Kenny as the mechanic of the September visit with its work beside him, so the second visit was written down as a sentence rather than as keys

recall {"fields": [{"value": "person:token"}]}
carries person:token
carries-any-case wheel
say     December: no record holds Token as the mechanic of the May visit with its work beside him, so the third visit was written down as a sentence rather than as keys

# 15. The move, paired. The positive is the five sittings it came up in, by the
# day each states; the negative is the four it did not, and nothing of Lisa's
# paperwork session, which mentions a lease and is hers.
recall {"fields": [{"key": "answer_15"}], "keys": ["answer_15"]}
carries 2026-03-30
carries 2026-05-31
carries 2026-08-09
carries 2026-09-27
carries 2026-10-31
lacks   2026-01-10
lacks   2026-02-21
lacks   2026-04-26
lacks   2026-06-14
lacks   household folder
say     December: the sittings the move came up in are not exactly the third, fifth, seventh, eighth and ninth, so one is missing, one is added, or Lisa's paperwork was read as the operator's
```

## What this room cannot measure

**Whether anything here was easy to find.** Every lock says a question is
answerable. None says the answer was near the surface.

**Three questions are a person's reading** (1, 5 and 6, and the lines of 3 that
are not the quote). Who the bots are and what each is for, what was decided and
in what order, and what Lisa does with her mailbox declined are answers whose
right shape is prose, and a lock over a model's phrasing would measure the
phrasing. The recorded answers sit in the store for whoever reads the run.

**What Lisa's mailbox holds.** The right answer is a refusal, and a refusal is
not state a lock reads.

**Whether the sitting that answered 7 and 15 named a sitting by its day or by its
words.** A past run's record carries the day it stated, so a sitting that read
the record and one that read the beats' own dates both reach the same five and
the same three days. A lock cannot tell which route a sitting took.

**Whether "only the ones I am behind on" was obeyed.** Question 2's lock holds
that the laptop's day is absent, which a sentence saying the laptop is fine would
break. A person reads the answer beside the transcript.

**The count of the visits.** Question 14's three locks hold that each visit
exists with a mechanic and a piece of work beside the mechanic's name. That there are exactly
three is in the transcript: a visit filed twice reads the same.

**Whether the monthly totals pair each month with its number.** Question 11's
lock holds the six numbers and cannot hold which is January's.

**Whether Marjorine was ever the placeholder, when the placeholder held no
record.** Question 10's carrier reads a record older than June on the person
named Marjorine. A placeholder made in April and given no claim at all, then
renamed, leaves nothing to read, and the lock fails a year that did the right
thing. April's sitting says the partner is coming, which is a claim to file, so
the room does not expect that year.
