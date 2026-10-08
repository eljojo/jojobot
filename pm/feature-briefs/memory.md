# Memory — entities, facts and edges

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-15

**jojobot stores three things: entities, events and edges. It reads back a fourth: the fact.**

## Events and facts

**An event is the atom.** An event has a date. It hangs off one entity. It carries fields: keys and values. Its kind names which keys. **An event is stored in two layers: a permissive record that keeps every key as written, and a typed projection over it that the kind's declarations supply.**

Events accumulate. jojobot never updates an event in place. jojobot never overwrites one.

**A fact is the projection of those events.** A fact holds one value per key. It states what is true now.

The operator's own comparison: **events are UDP, and facts are TCP over them.**

Many events attach to one thing. **Only the kinds that fold feed the projection.** The other kinds are chronology. They happened. You can read them. They do not change what the thing is now.

**A kind declares whether it folds. It declares this once.** The writer does not mark an event as folding. The reader does not choose it per query. A kind that folded on one day and not the next would mean two things under one name.

Bart eats a donut. Each donut is an event. Ask how many, and the number is the answer. Ask once in a hundred times, and the question is which donuts, when, and where. Both answers come from the same events.

## The three parts

* **Entity.** Anything jojobot knows about. It carries one kind at a time. The kind is what it is. Entities form a tree, so an entity can have children. A handle is a path: a child is named inside its parent.
* **Event.** A dated occurrence on one entity. It carries fields. Each key declares what it holds.
* **Edge.** A relation to another entity. It has one of five shapes: location, membership, attendance, about, and connection. Connection says a link exists and that nobody recorded how it relates.

**There is no fourth part, and that is a decision.** A fact is the projection you read, not a stored part. Every richer thing is one of these three or a query over them. This includes a portrait, a journal, an inbox and a bot's whole history. More primitives would make the agent choose between them, and the agent chooses wrong.

**An occurrence has several participants. A claim has one.** An edge draws one relation, which suits a claim about one subject. A mechanic visit has a bike, a mechanic, a shop and a part. An event kind declares a key for each, and each key holds a reference. **This is why an event kind carries its own schema.**

## Who backs a claim, and how sure

**Every claim carries two answers. They are independent.**

**Provenance** says who backs the claim. The operator said it, an agent derived it, or an agent read it out of a system of record — and that third one is refused unless it names what it read, because *who confirmed this, and on what* is the whole question a later reader asks.

**Standing** says how settled the claim is.

One answer does not imply the other. The operator can say something while thinking aloud: that is testimony, and it is still open. An agent can derive something checkable that the operator has never confirmed. **jojobot must show the difference between a decision and a musing.** An inference reads back as a hypothesis until the operator confirms it.

**A value nobody asserted must not read as one somebody did.** Where nobody recorded standing, jojobot says so.

## Reading, and what you query

**A read gives you the fact.** Ask for a thing, and you get one dense row of folded keys. Ask for one key, or for one claim's own address, and you get everything behind it, oldest first — including the wordings a claim has been through, which is why correcting a claim loses nothing. The projection is the default. The history is on request. The upper layer exists to keep a reader out of the lower one.

**Filters, comparisons and walks all read the projection.** *Which friends have eaten three or more donuts* asks about the folded total. It does not ask about any single event. A query that read raw events would answer a different question, and the answer would look correct.

**A key's declared value type licenses the comparison.** Dates compare as before and after. Numbers compare as less and greater.

**A reference key makes a join into a walk.** You name the relation. You do not compose it.

**A key declares how it folds.** Most keys keep the newest value, because the last thing said about a colour is the colour. **A counter adds its events together**, because the last donut is not the total. A filter on a counter compares the total.

## Editing, correcting and taking back

**Editing is the surface. Appending is the substrate.** An agent edits a claim and sees it change. Underneath, jojobot writes a new event. The agent reads back the projection.

**A correction takes one of two shapes, and who wrote the claim decides which.** A claim the current session wrote is rewritten in place. A claim an earlier session wrote is archived, and the corrected claim stands beside it, so the correction stays visible — somebody may already have read the old one. **Nothing stores a sentence saying what a thing is not.**

**Retracting is a different move.** A correction says the world turned out otherwise. Retracting says nobody should have written this. It is one way. It is not a delete, and the record stays readable. Taking something back is itself something that happened, and a system that hid it would lie about its own past.

**A claim carries two dates, and they answer different questions.** One says when the claim was made. The other says when the thing happened, and it is absent when nobody said — which is the honest record of *she came back over the summer*, rather than a day nobody chose. **A date meaning anything else gets its own key rather than its own column**, so a query still reaches it and no column carries two meanings. **So does any other per-claim marker that does not earn a column:** it rides the same key-value bag an event already carries.

**Something somebody intends is its own entity, not a date on a claim.** It accumulates its own events, and two of them can stand about one subject.

## A kind carries the schema

**A thing's fields are every event on it, folded.** No single event holds them. Everything recorded about Santa's Little Helper, taken together, is the object that dog presents.

**A kind is what a thing is and the keys it holds. These do not come apart.** The kind is the namespace in the handle, as in `pet:santas-little-helper`. **Changing it is a deliberate act rather than something that happens to a thing** — and the handle is the only thing that changes, because a link underneath holds an opaque id.

Declare `bike`, and a bike has a year, a brand, a colour and a model. Nobody sets these up per bike. Nobody invents a spelling for them. **This is what makes a key mean the same thing everywhere, and it is the whole return on declaring anything.**

**A thing earns what jojobot can do with it as it accumulates what is known.** It does not fill in a form before it may write anything down.

**The operator declares kinds.** A new kind is based on an existing kind and inherits it. **Each key declares what it holds.** A key that holds a reference names the kind it points at. A bike's brand is a company, and that company must already exist. **jojobot keeps a key no kind mentions exactly as written.**

Full detail → *Kinds — what a thing is, and the keys it carries*.

## Strictness

**Strictness applies to one thing at a time. When it is on, the write path validates and rejects.** That is the point of it.

**jojobot offers strictness. It does not assume it.** When a thing starts to hold every key its kind names, jojobot offers to turn strictness on, and an agent confirms. A thing that holds every key when somebody creates it can be strict from the start. **A half-described thing has nothing to protect, so jojobot refuses it nothing.** This keeps strictness from becoming a gate on the way in.

**A value must hold what its key declared.** Set a bike's brand to a pet, and the write fails. **Holding a key badly is not holding it.**

**A write that would remove a key the kind names also fails.** Adding keys always succeeds. Strictness is a floor, not a ceiling.

**A refusal names the key and what it wanted, so the caller knows the next move.** A refusal never leaves a thing unrepairable, because jojobot checks the shape the write produces and not the change alone.

**Best-effort is the escalation, and it is permanent.** Some things stay loose on purpose. Life arrives in the wrong order, and a lossy record you can correct later beats storing nothing.

## The tree

**A claim lives on the most specific entity it is about.** Reading is zooming, not loading. Fetch a parent for the map. Then descend into the part you need. jojobot loads no subtree you did not ask for.

**A thing with a life of its own gets its own entity.** An occurrence in that life is an event on it. A bike's servicing is a rhythm that hangs off the bike and holds its own cadence. Each visit is an event on that rhythm. **The test is whether the thing accumulates events of its own.**

This prevents one failure: a claim put on the parent because that was easier. A page that collects everything silts up. A source file of ten thousand lines has the same disease.

## Addresses and links

**A claim carries the address a caller edits it through.** Another claim links to that address when it says where a claim came from. A claim's local number is unique inside its home. The number says how many claims sit on one entity. It does not say how many jojobot holds.

**A reference points at the thing, not at its name.** A person and an agent read a handle. Underneath, a link holds an opaque id. So somebody can rename or move a thing, and nothing that points at it breaks. **A field that holds several pointers holds their permanent ids, never a text list.** The readable form is composed on the way out and is never what is stored, so a read cannot rewrite what the writer typed.

**A caller chooses a link when jojobot captures the event.** jojobot never infers one later. This makes a question across the graph a walk: you name the relation once, and every later question follows it.

## What it is not

**Not a document store.** An agent cannot see how jojobot writes a record down or which service holds it. An agent that knew the store would reach around jojobot to it.

**Not a surface that asks the caller to choose.** No verb makes a caller pick a depth, a container or a flavour of record. You write what happened. The kind decides what it feeds.

**Not the reader's question.** A reader asks for some of the keys or for all of them, and that decides what comes back. It never decides what a write may do. These are two questions, and collapsing them makes the design toothless.

**Not a place anything is created by accident.** Everything a write names must already exist. jojobot brings nothing into being as a side effect. **Before a write brings a new name in, the write guard checks it against the entities jojobot already knows, the way a phone's autocomplete checks the contacts book.** The one exception is the operator's mailbox, which opens inside the first post to them.