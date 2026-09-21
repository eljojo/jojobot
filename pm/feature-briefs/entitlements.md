# Entitlements — something somebody holds that opens a door

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**Somebody HOLDS something, scoped to an event, a place or an organisation, valid over a window, which PERMITS an action that is otherwise closed.** Tickets, passes, memberships, registrations, visas, licences, reservations, insurance, accreditations. One shape underneath all of them.

## The failure it exists to end

**An agent asks whether the operator holds the thing — and the answer was already recorded.** That is the shape every time: not missing data, unreachable data. A pass written down months earlier, accurate, sitting in the description of something else, never read at the moment it decided anything.

⭐ **So the measure of this feature is not whether an entitlement can be stored. It is whether the session that needed it was handed it without knowing to ask.**

## Two facts, and collapsing them is the error

**"The forum offers a full-pass tier" is a fact about the world.** True for everybody, belongs on the event.

**"Lisa holds one" is a fact about Lisa.** That is the one a session needs, and it is not an attribute of the event at all.

⛔️ **Collapsing them produces a record that reads correct and does not answer the question anybody asked.** Filing the holding under the event means a read about Lisa never surfaces it.

## It is a relation, and attendance is the wrong one

**Holding is not attending.** The pass is the thing that *gates* attendance. Somebody can hold one and not go; somebody can go to the free outdoor part holding nothing at all. **The question being asked is *will they let me in*, and that is about entitlement.**

Using a generic link instead would record that we knew a connection existed and did not know what we meant by it — which is honest, and useless to the one question this exists to answer.

## What comes back, and why it arrives unasked

**A read about a person or an event carries what they hold, ranked: what is live as of the day asked about, what has lapsed, and what is merely related.**

⭐ **It attaches to a read the caller already made.** That is the whole design. A read somebody has to remember to issue is a read that will not be issued at the moment it mattered — which is the failure above, rebuilt.

**The window is compared against the day the caller names, never a clock**, so the same question about the same day always gives the same answer.

## Certainty is asymmetric here, and the design turns on it

**A pass the operator SAID they hold is their word. A pass inferred from the fact that they are going is a guess.**

🚨 **A guess that reads back as settled means somebody shows up somewhere and is turned away.** The reverse mistake — being told to check something they already have — costs a moment. **Those two are not symmetric, and the ranking treats them differently rather than averaging them.**

**A claim that has been taken back is not held.** Whatever the window says, a withdrawn claim answers nothing about now, and a ranking of what is current must not carry it.

⭐ **AND THE ANSWER SHOWS ITS WORK RATHER THAN LEAVING THE RANKING TO CARRY IT ALONE.** Each thing somebody holds comes back with **who backs the claim and how sure they were** — so a pass the operator only thinks they have is legible as that, instead of arriving in the same shape as one they confirmed. **The ranking decides the order; the certainty travels with the item.**

⚠️ **Two different questions live here and they are spelled differently on purpose.** **Whether the record is in force on the day asked about** is one. **How sure anybody was that it exists** is the other. **One word for both is a reader who cannot tell which question an answer answered** — and a date computation reading as a person's confidence is exactly the confusion this feature cannot afford.

## What it is not

**Not a calendar entry.** When something happens is the calendar's. Whether somebody may get in is this.

**Not attendance, and not a plan to go.** Holding, going, and wanting to go are three different claims and each can be true without the others.

**Not a confidence score.** jojobot performs no inference, so a number here would be a model's self-report wearing the authority of a measurement.

**Not a gate.** It refuses nothing and blocks nothing. It puts what somebody holds in front of the session that is about to need it, and the deciding stays with a mind.

**Not a store of the thing itself.** A booking reference or a barcode lives where it was issued. What jojobot holds is that the entitlement exists, what it covers, and until when.