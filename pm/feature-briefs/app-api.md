# The app API — an outside program with a login of its own and the objects it owns

> **jojobot** · kind: feature-brief · boot: on-demand

**An app is an outside program that uses jojobot as itself: it logs in with a token of its own, does what the operator granted it, and owns the objects it brings.** It is how the lab keeps its machines, how a forge reports what was pushed and tested, and how a watcher reads the build and pages the operator, without any of them borrowing a bot's session or the operator's login.

## One kind of actor

**The operator, a bot, an app and jojobot itself are all actors**, each with an identity and permissions, and **one permission check judges every read and write**, whichever door it came through. An app differs from a bot in two things only: its credential and what it owns. It has no session, charter, mailbox, lease or run because it needs none, not because a second rulebook forbids them.

## Identity and grants

**An app's identity is a token, and the server keeps only its hash.** The operator mints a token and its hash with a command that touches no store, puts the hash where the server's configuration reads it, and gives the token to the app. A token has a fixed prefix, so it is told from a bot's key or a login token without a lookup.

**Over anything that already exists, an app does what the operator granted in its entry in the server's configuration, and no live call widens it**: which operations it may call; for writes, which kinds and keys; for reads, whether it reads beyond its own. An operation outside the grant is refused, naming it.

**A configuration with anything wrong in it does not start the server**: a repeated name, two apps sharing one hash, an app with no hash, a key no app may be granted. A grant nobody checked is never served.

**Some keys can never be granted**: a key that binds an identity, a key jojobot writes itself, a key that labels who read what, and a key that makes a thing fall due. The last one matters most: it would put an outside program's words into what the operator is owed.

**Revoking an app is a flag on its entry.** A revoked app is refused at the door and at the check, and the keys it owned stay locked: a revoked owner does not hand them to whoever writes next. Every refusal logs its reason and the app's name when known, never a body or a token.

## Discovery: what an app brings, it declares

**When it connects, an app declares what it brings**: its kinds, their keys, which of its fields are live, the keys it owns, and the declaration's version, the way a home-automation device announces itself on its network. **Discovery adds and never takes.** Anything new is the app's own. Anything that already exists (a kind jojobot ships, a key a bot declared, another app's object) is reached only by the operator's grant, and a declaration that reaches for one without it is refused whole, naming what it reached for.

**A no-op never appends.** A reconnect with the same declaration writes nothing; a changed one writes only what changed, and a changed meaning is a new version. Something an app stops declaring is marked no longer offered, never deleted.

## Two tiers over the same operations

**Apps do not speak the tool protocol bots use.** That surface is a rendering for models, with teaching and budgets, and an app's token is refused there. Each verb's decision and write lives in one function beneath its tool handler, taking the actor as data, and apps reach those same functions over typed HTTP, in two tiers:

- **The unstable tier** serves every operation that exists, generated from each one's typed input and output and never hand-written, so it is complete and cannot drift from what bots get. It is always unstable and promised to no one: it is for prototyping.
- **The stable tier** is versioned and designed object by object, in the terms an app thinks in: its objects, its live fields, its channel. Each endpoint is a curated shape over the same operations and adds no behaviour of its own.

Both tiers pass through the same check, so a guard written once binds both.

## What an app owns

**One app owns an object, and the binding survives a rename.** It alone writes the keys it owns; bots read them and add notes beside them, never change them. It keeps its objects as snapshots: an unchanged snapshot writes nothing, a changed key adds one observation, and an object that left the app's source is archived with its history and restored, never re-created, if it comes back.

**A claim an app wrote carries the app's mark**, a typed field and never a key among its fields, so a bot that writes an app's name into a key gets no mark, and a reader tells "the app recorded this" from "a bot said so" by the mark alone.

**A retry is safe.** The app names each attempt. The same attempt with the same body answers the first receipt and writes nothing; the same attempt with a different body is a conflict.

## The app stays connected

**An app keeps one long request open to jojobot, and jojobot never dials out.** The request answers with whichever comes first: the changes since the app's position on objects it owns or follows, or a request for one of its live fields. The position is the app's to keep, so a restart resumes: duplicates allowed, gaps never. An app needs only jojobot's address and its token, so it runs behind any firewall.

**A live field is asked of its app when a bot reads it**, within the field's deadline, and served marked volatile with its time. Late or offline, the bot gets the cached value with its age, or unavailable, and is never held past the deadline. **A reading is never history and never followed.** A change the app calls meaningful (a run finished, a machine went down) is an ordinary owned-key write, so it lands in history and wakes whoever follows the object.

## What it is not

**Not a bot's session.** An app holds none, so nothing it holds opens the surface where sessions act.

**Not a widening an app asks for.** Over what already exists, its reach is what the operator wrote; discovery only adds the app's own.

**Not a reader of the operator's life by default.** A read beyond its own objects is a grant, never a given.

**Not jojobot calling out.** An app comes to jojobot; jojobot never dials an app.
