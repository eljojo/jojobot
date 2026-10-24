# The app API — an outside program with a login of its own and one grant

> **jojobot** · kind: feature-brief · boot: on-demand

**An app is an outside program that writes to jojobot under a login of its own and one grant it cannot widen.** It is how a build pipeline can say that a deploy happened without being a bot and without holding the operator's life.

## A service principal, not a colleague

**An app is not a bot.** It has no session, no charter, no mailbox, no lease and no run, and nothing it sends can start, stop, poke or message a session or write a role. A bot's identity is declared and then guarded by vouchers; an app's is a token that proves which app is writing, and nothing more. *The chain of trust — vouchers passed down the org chart* is the other kind of grant: delegated down the chart, bound to a session, issued by the store. **An app's grant is neither delegated nor issued: the operator writes it in the server's configuration, and only the operator can change it.**

## The grant

**Each app has a name, an opaque token, and one grant: one operation, one thread, one kind and the keys it may write.** The only operation is recording a deploy event, the only kind is an event, and the event lands as a child of the grant's thread. Alpha's pipeline can write a `commit` and a `build` under the deploys thread, and nothing else anywhere.

**The server keeps the hash of the token, never the token.** The operator mints a token and its hash with a command that touches no store, puts the hash in the secret the configuration reads, and gives the token to the app. A token has a fixed prefix, so a bearer that is an app token is told from a bot's key or a login token without looking anything up.

**A configuration with anything wrong in it does not start the server.** A repeated name, two apps sharing one hash, an app with no hash, a thread that is no thread handle, and a key no app may be granted each refuse the whole file. A grant nobody checked is never served.

**Some keys can never be granted**: a key that binds an identity, a key jojobot writes itself, a key that labels who read what, and a key that makes a thing fall due. The last one matters most: it would put an outside program's words into what the operator is owed.

## What a write is

**The app sends keys with values of a fixed shape, never free text.** A commit is seven to forty hex digits; every other value is a short token of letters, digits, dots, underscores and hyphens. The server composes the claim's words from the keys, so an app's values appear as values and nowhere else.

**The claim is an observation, and it carries a mark naming the app.** The mark is a typed field on the claim and never a key among its fields, so a bot that writes an app's name into a key gets no mark, and a reader tells "the app recorded this" from "a bot said so" by the mark alone.

**One check judges every request**, and only what it returns reaches the store. A key outside the grant is refused as outside it; a value of the wrong shape is refused as invalid; nothing is written either way.

**A retry is safe.** The app names each attempt. The same attempt with the same body answers the receipt of the first and writes nothing; the same attempt with a different body is a conflict.

## One door, and it opens one way

**There is one route, and it exists only when an app is registered.** An instance with no app has no app route, and no app token opens anything. **An app's token is refused at the tool surface**, whether or not a login check is mounted there, because an app has no session and nothing it holds may open the surface bots use.

**Revoking an app is a flag on its entry.** A revoked app is refused at the door and again at the check, so a request that somehow reached the check would still write nothing. Every refusal logs its reason and the app's name when known, and never a body or a token.

## What it is not

**Not a bot, and not a way to become one.** No grant here reaches a session, a role or a charter.

**Not a voucher.** There is no delegation and no session to bind to; the operator is the only issuer.

**Not a general write API.** One operation, one thread, one kind. A second operation is a second grant the operator writes, never a widening an app asks for.

**Not a read path.** An app writes and learns whether it landed; it reads nothing back.

**Not free text.** If the words need to be an app's own, the app is not the right writer.
