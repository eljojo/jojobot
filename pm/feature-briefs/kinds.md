# Kinds — what a thing is, and the keys it carries

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-11

**A kind is what a thing is and the schema it carries. These are one thing, not two.**

A kind names the keys a thing holds. Each key says what it holds. Once a thing is bound to its kind, the schema governs what anybody may write to it.

**A thing has one kind at a time, and it governs what may be written to the thing.** The kind is the namespace in the handle, as in `pet:santas-little-helper`. A second way to say what something is would be a way to disagree with yourself. The schema therefore hangs on the kind and nowhere else. **Changing a thing's kind is a deliberate act rather than a drift**: the handle changes, the thing does not, and nothing pointing at it breaks — because a reference holds an opaque id and never a name.

## You discover a kind before you declare one

Keys accumulate on things one at a time. The same keys keep appearing. A bike keeps showing a year, a brand, a colour and a model. That shape is the schema.

**Declaring it makes it real.** After that, a bike arrives with those keys. Nobody sets them up per bike.

**A key means the same thing everywhere inside its kind.** This is what makes a question about many things answerable.

**A new kind is based on an existing kind and inherits it.** A figurine collector declares `figurine` on top of `thing`. A figurine then has what a thing has, plus what a figurine has. Inheritance keeps the set from becoming a flat list of everything anybody owns.

**A thing may carry a key its kind never named.** That key can have a declaration of its own. The kind is what a thing arrives with. Anything past it is deliberate.

## What a key holds

A key declares one of: text, a number, a date, **a span between two dates, held as one value**, yes-or-no, or **a reference to a thing of a named kind**. A span is one key rather than two, because two keys cannot say they belong together: a thing carrying a start and no end reads as a thing with a key missing rather than as a span nobody finished. **A key may also hold a LIST rather than one value** — *who came along* is nought or more people, and a single reference cannot hold it.

**Beside the value type, a key may be NARROWED.** Two narrowings exist and neither is a value type of its own: the kind a reference points at, and **one of a named set of values**, checked on the write. ⭐ **A set is checked against what somebody named. The open kind is checked against what the store already holds** — nobody maintains that list, and a caller picks a value in use or adds one.

⚠️ **A schema shaped by what the type system can carry, rather than by what a life keeps, is the failure these close.**

**A reference names the kind. It does not say "an entity".** A bike's brand is a company, and that company exists in the graph. This turns a field into a relation you can walk by the key's name. **A value that names a handle is a link whether or not any key was declared.** The declaration adds the kind the value must point at, and the walk by name. It never decides whether the link exists.

**A reference points at the thing, not at its name.** A person and an agent read a handle. Underneath, the link holds an opaque id. So somebody can rename or move the target, and the reference still resolves.

**A key declares how it folds.** Most keys keep the newest value, because the last thing said about a colour is the colour. **A counter adds its writes together**, because the last donut is not the total. The key declares this once. The reader never chooses it.

**Numbers and dates order. Everything else compares as equal or not equal.** The declaration licenses the operator, and nothing else does. This keeps a question from becoming an expression. *Expired* is a date compared against today. You can ask it only because a kind said that key holds a date.

**A reference key declares a relation, and both directions come free.** From the record, the key names the relation: a lens reaches the camera it fits. From the other end, the same key names the reverse: every record that points at that camera through it. **That reverse is the has-many. Nobody declares an inverse.** jojobot derives both names from the declaration.

**A key belongs to the kind that declares it.** `owner` on `pet` is the pet's owner and points at a person. Another kind may declare a key of the same name for its own purpose. Those are two keys.

## A kind also declares whether it folds

**Some kinds feed the projection. Others are chronology.** A kind that folds contributes its writes to what a thing currently is. A kind that does not fold records that something happened and leaves the thing unchanged.

**The kind declares this once, with its keys.** No writer marks it per record. No reader chooses it per query.

⚠️ **Two declarations use the word fold and they are different.** A *kind* declares whether it folds at all. A *key* declares how its values fold together. Read the level before you read the rule.

## Event kinds

**An event kind and an entity kind use the same machinery.** They declare keys the same way. Their keys hold the same range of values. Their references walk the same way.

**The schemas differ in practice**, because an event describes what happened to a noun rather than a noun. A mechanic visit declares a bike, a mechanic, a shop and a part. A pet declares a name, an owner and a vet.

**An event carries a handle, exactly as a noun does.** A thing that happened is something you may need to name, point at and correct later, and a record with no address is a record nobody can reach twice. So an event is addressable, references walk to it, and renaming it breaks nothing that points at it.

**Expect shared behaviour rather than a second mechanism.** The schemas differ because the subjects differ; nothing below them does.

## A key or a child, and the test is events

**A key holds a value. A child entity has a life.** A bike's servicing is not a date on the bike. It is a `rhythm` that hangs off the bike, with its own cadence, its own check-ins and its own history.

**The test is whether the thing accumulates events of its own.** A key cannot. An entity can.

**The parent's schema still names it.** A bike declares a key that holds a reference to its servicing rhythm. The link then walks from either end. The relation is declared. The life is separate.

⚠️ **Getting this wrong flattens something that needed room.** Fold a cadence onto the bike, and that bike has exactly one cadence forever, with nowhere to record what happened each time.

## The two questions

**Which of these carry ANY of a kind's keys** is the tolerant question. A partial answer is useful, and it names the missing keys.

**Which of these carry ALL of them** is the strict question.

Choosing among things asks which are *described like* a pet. Narrowing a walk asks which *are* pets. A pet with no name belongs in the first answer. It does not belong in a job that needs the name.

**Both questions are real. The reader picks one, and the choice is the reader's.** Which question a reader asks is never a property of the thing.

⚠️ **The word strict names two things on this page, and they never decide each other.** Here it names the reader's filter. Further down it names the write-time switch, which IS a property of the thing. A reader's filter changes what comes back. It never changes what a write may do.

**The tolerant reading is the default**, because a thing that vanishes from an answer is worse than a thing that arrives with its gaps stated.

**A walk narrows on the strict question.** The key being walked is already one of the kind's, so the tolerant filter would pass everything the walk found.

*(Two reference keys of one kind onto the same kind stay distinct. A trip's start and its end are different keys.)*

## Interfaces — a schema a thing merely carries, and what it unlocks

**A set of keys may be declared without being a kind, and that is an INTERFACE** — a shape a thing carries alongside whatever it already is. `concert` over things of kind `event` gathers the concerts without inventing a kind for them. **Every kind is a schema; not every schema is a kind.**

**A kind is the schema you can only have one of, that sits in the handle, and that can refuse a write** — and changing it is a deliberate act rather than something that happens to a thing. **An interface has none of those three.** A thing carries any number, it qualifies the moment it holds the keys and stops the moment it does not, nothing about it shows in the handle, and it gates nothing. **Nobody declares one on a thing. Carrying the keys is the whole of it.**

**What carrying one buys is behaviour.** Carry a due moment and the read for what is owed and late reaches you. Carry a key that adds its writes together and the question of a total reaches you. **That behaviour belongs to jojobot and is written once, for the schema rather than for each carrier** — which is what keeps a new carrier free, because it arrives as data and costs no code.

**This is the only way a question can span kinds.** A thing has one kind, and a new kind inherits one existing kind, so anything cross-cutting — *is due*, *has a chronology*, *belongs to somebody* — has nowhere to live inside the kind. A read that instead kept a list of the kinds it knows about would stop being true the day somebody declares a kind. **The proof that one of these generalised is that a carrier the read has never seen appears in its answer without the read changing.**

**One matcher serves both.** Whether a thing holds a set of keys is one question with one implementation, kind or not. **The two stay separate words on the surface**, because they answer *what is this* and *what can I do with it*, and a reader who cannot tell those apart is how a distinction nobody asked for gets drawn.

## Batteries, and then the operator's own

**Kinds arrive in the software. An instance declares its own beside them.** A solid shipped set is the deliverable. A fresh instance that knows nothing is the blank store that batteries-included exists against.

**A shipped kind belongs to the software, and an instance EXTENDS it rather than editing it.** The software ships a base; what an instance adds sits in its own layer, and an upgrade replaces the shipped half underneath while leaving the instance's untouched. **So a customization survives every upgrade and nobody reconciles anything by hand.**

⭐ **Separate in storage, and COMPOSED in the answer.** A read returns one kind, not two halves — the layers exist so an upgrade can tell them apart, never so a caller has to.

**Keeping them separately addressable is what makes that safe.** A shipped kind is a promise the software makes about what it will do with a thing; if an instance could edit that promise in place, an upgrade would have no way to tell a deliberate change from a leftover of an older build. So adding is welcome, and shrinking or replacing the shipped half is refused — with the refusal pointing at the instance's own layer, which is where that want belongs.

## Declaring makes it real, and then it governs

**jojobot finds a thing by the keys it carries, declared or not.** Discovery never waits for a declaration. That is how you find the schema, and it is why declaring later is an improvement rather than a migration.

**Declaring buys a reader reach.** An undeclared key answers to equality only. A declared key answers to ordering, and to a walk by the key's name, because only a declaration says which keys hold dates and which hold other things. A value that names a handle is a link under either.

**Declaring buys the operator a promise that holds.** When strictness is on for a thing, its kind governs every write to it. A value must hold what its key declared. A write that would remove a key the kind names fails. A refusal names the key and what it wanted.

**jojobot offers strictness when a thing starts to hold every key its kind names, and an agent confirms it.** A thing that holds every key at creation can be strict from the start. **A half-described thing has nothing to protect, so jojobot refuses it nothing.**

**jojobot never refuses a key no kind mentions.** The schema is a floor. Its keys must survive. Anything else a caller wants to say is welcome beside them. **jojobot checks the shape the write produces, never the change alone**, so anybody can always repair a thing that is already broken.

## What it is not

**Not a registry of legal kinds.** A principle is not an enumeration, and a system that lists what may exist is one. The operator declares what their life has.

**Not a toll on the way in.** jojobot refuses nothing to a thing with strictness off, and this stays true permanently. A lossy record you can correct later beats storing nothing.

**Not a ceiling.** A kind is a floor. Its keys must survive, and jojobot keeps a key it never mentions exactly as written. Enforcement protects what a thing has earned. It does not police what anybody may say about it.

**Not a shape a thing takes on by accident.** A kind is never derived from whatever keys a thing happens to be carrying. Retyping is possible and it is somebody's deliberate act, never something that happens to a thing on its own.

**Not a second way to say what a thing IS.** A schema that is not a kind says what a shape is called and what carrying it unlocks. It never competes with the kind for identity, and a caller may name one after a kind without it governing anything.

**Not a set of keys any carrier implements for itself.** What a schema unlocks is written once, in jojobot. A carrier that supplied its own code behind a shared shape would make every new carrier a change to the software, which is a kind wearing a different word.

**Not a catalogue of every kind anybody might want.** The shipped set covers what a life has. The rest is one instance's data.

**Not a gate on whether a link exists.** A value that names a handle is a link whatever its key, and an unscoped walk reaches it. jojobot infers nothing: the caller wrote the handle. A declaration adds the kind it must point at and a walk by the key's name.