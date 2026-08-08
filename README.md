# jojobot

A personal-assistant server that fronts your existing life-layer services
(task/kanban, notes, calendar, link library) and exposes itself to an AI
assistant through a single [Model Context Protocol](https://modelcontextprotocol.io)
(MCP) endpoint.

jojobot is **not** an MCP proxy. It is software with a domain: it feeds the
assistant context, guards writes behind invariants, remembers what matters, and
never runs inference of its own — the assistant is the only mind.

> **Status: two domains live, one front door over both, sessions can boot as an
> identity, and the method ships with the binary.** The server ships **Memory** — typed entities and dated
> facts carrying **two independent answers: who backs a claim** (testimony vs
> inference) **and how settled it is** (open vs settled), because the operator's
> own hedge is their claim and still a hypothesis — a write guard that blocks
> near-miss creations with candidates, structured edges in five shapes, aliases,
> and orienteering retrieval (every hit arrives with its surroundings) — and
> **Mailboxes** — message boxes with `new → read → processed`
> state (read ≠ processed; processed is a terminal archive; no delete verbs —
> the tool surface is pinned by test), each message optionally carrying a
> `subject` and an `in_reply_to` link to what it answers, `read_message` taking
> delivery of one without draining its box, and `list_sent` showing a sender
> where their own mail got to without moving any of it. **What a caller already
> has is not shipped back**: the two verbs that echoed a body now answer with a
> receipt, a poll can ask for news only, and orientation can be skipped by a
> session that has read it — always with a marker saying what was left out and
> how to get it. **A queue belongs to whoever drains it**: every box is listed
> by name, and its per-state counts go to the bot that owns it (or to anyone,
> when nobody does), so a sender sees that a box exists without being handed
> somebody else's workload — while a quarantined message is reported to
> everyone, because that is a fault in storage rather than a queue.
> **`search` spans both**: one ranked list over entities, facts, prose and
> messages, mail **opt-in** through `include_mail` and searchable in every
> state (`processed` archives too), each mail hit carrying its box, state,
> sender and id. **Every answer is backed by a read taken for it** — each half
> re-reads its own store before answering, so a record the store has since lost
> stops being served — and **each half states its own coverage**: complete,
> blind, or behind, and when it is behind it says which way — `unscanned` when
> no read has filled it yet, `stale` when the index holds an older version than
> the store. An answer that could not read a store says so rather than reading
> as "nothing matched". On top of both, **bots**: an AI identity is an entity of kind `bot`
> with a charter, rules, memory and one owned mailbox. **`start_here` is the one
> door**: it takes an optional bot name, and naming one **starts or resumes that
> bot's session**, because a bot is a role and a session is one mortal run of it.
> Booting anonymously is legal and deliberately useless — orientation, and no
> handle behind it. **It is also skill zero.** The essay is the world model and
> arrives unasked; a **skill** is a procedure and is fetched by name through the
> same door. Every boot lists the skills by name and when-to-use and never their
> bodies, so a session learns what exists without paying for what it does not
> need. jojobot decides nothing about when one applies — the caller asks. **Sessions** are the store's own records, one per run of a bot:
> a focus that is current truth, an append-only chronology (the session's own
> beats plus jojobot's, marked apart), and `active` → `wrapped` | `abandoned` —
> **`wrapped` is the last word, because wrapping folds the still-open focus into
> the closing story as one last chronology entry, while `abandoned → active` is
> the one legal walk-back, since a run nobody wrapped up left nothing to fold.**
> A session begins lazily — no row
> until the first write — resumes across a disconnect, and is swept to
> `abandoned` after a day of silence. **The `sid` is the only address**: a short opaque handle the server
> mints at the door, saying nothing about the work, and it rides every verb —
> reads included — because real MCP clients open a fresh connection per tool call
> and a connection-held identity does not survive to the next one. It is written
> on the session's own record, so a restart does not orphan it. **Every record
> jojobot mints an id for is addressed the same way** — drawn from OS entropy
> over the same alphabet, saying nothing about what it names, and redrawn if the
> store already holds it. Records that predate this keep the ids they were
> minted with, so two shapes sit on one board: that seam is deliberate and
> permanent, not a migration that half-ran. All behind OAuth2
> resource-server auth, and a write is reported successful only once it has
> landed — read back through the read path, or carried by a transaction that
> either commits or does not.

## Vision & roadmap

jojobot's mission is to move an assistant's *method* out of prose (rules files,
skills, per-session caches) into software: versioned, tested, enforced. The
roadmap is a ladder of **capabilities** — each release is named "after this,
I can ___", never after infrastructure.

Shipped so far: memory, search and edges, mailboxes, bots (AI identities with a
handle, charter, rules, memory and an owned mailbox), every session on the
record, the redesigned surface, events as a first-class flavour of fact, and the
skills that ship the method in the binary. A fresh instance arrives holding one
identity, `assistant`, with its mailbox — which is what lets the next rule have
no hole in it: every memory write names the session behind it, with no exemption
for any kind.

Next: trace, portraits, attention, and sessions booting from jojobot.

Two layers hold it together: the **engine** (this repo — user-agnostic, golden
tests) and **bots** (data in the user's own store). The design docs live with
the user, not in-repo. AI sessions working here: read [CLAUDE.md](./CLAUDE.md)
first.

## The tool surface

This is the whole surface, shipped and planned. It is maintained by hand and it
is meant to be read: if you are wondering whether jojobot can do something, the
answer is on this page.

### How this surface is designed

Five rules, and they are the reason the list is short.

1. **Domain verbs, never CRUD.** A tool is named for something a caller wants to
   do, not for a row it touches. `update_fact` is a CRUD name; *"correct
   something I recorded wrong"* is the action it serves.
2. **The caller says what it is doing; jojobot decides where that lands.** No
   tool asks a caller to choose a container, a depth, or a flavour of record.
   That decision is the product — pushing it back onto the caller is the whole
   failure this server exists to prevent.
3. **Combinatorial power over more tools.** Flexibility gets packed onto an
   existing verb as a parameter with a domain meaning, rather than spawning a
   sibling. A new tool has to earn its place against widening an old one.
4. **The safe branch is the default.** A caller that passes nothing gets the
   conservative behaviour. Anything expensive or destructive is opt-in. A
   default that has to be corrected by prose is a bug.
5. **A misuse is an answer, not an error.** Wrong input comes back as a blocked
   result naming the way forward — candidates, the verb to call instead, the
   thing that is missing. Callers branch on status; they should never have to
   parse a failure.

Two consequences worth stating outright, because they surprise people:

- **Reading a mailbox takes delivery.** There is no peek. This is deliberate —
  it is what makes a crashed consumer's work resurface instead of vanishing.
- **Nothing is created as a side effect.** A verb that names something which
  does not exist is refused with candidates. Creation is always its own call.

### Shipped

**Orientation**

| tool | what a caller is doing |
|---|---|
| `start_here` | Coming online: getting the world model, the skills that exist by name and when-to-use, a live snapshot of what exists, and — when naming an identity — that identity's rules, mailbox counts, a session handle, and its charter on a first boot but not on a resume. Naming a skill returns that one procedure. The one door; there is deliberately no second. |
| `ping` | Checking jojobot is reachable at all, and which build answered. Identity, build, time, nothing else. |

**Memory — recording and reading what is known**

| tool | what a caller is doing |
|---|---|
| `capture` | Remembering a fact about something, with its provenance and optionally one typed edge to another entity. |
| `recall` | Reading back what is recorded about one subject. |
| `search` | Finding something across everything held — entities, facts, prose and messages in one ranked list, each hit arriving with its surroundings. |
| `add_entity` | Bringing a new thing into memory. Screened against near-misses first, so a typo never mints a duplicate. |
| `update_entity` | Maintaining what a thing is called, and its other metadata. |
| `update_fact` | Correcting something recorded wrong — rewritten in place, never as an addendum. Also how a claim gets confirmed, or a refutation recorded as standing truth. |
| `retract` | Taking back an EVENT — one way, never reversed, and never a flag on an edit. Nothing is removed: the record is marked, and a dated account of why lands beside it when one is given, so the two read as one story. Facts are not retracted; they are fixed. |
| `list_entities` | Seeing what exists, by kind. |
| `set_charter` | Writing an identity's charter — the orienting text that says what it is and where its work lives. |

**Mailboxes — leaving word and taking delivery**

| tool | what a caller is doing |
|---|---|
| `post_message` | Leaving word for someone not in this conversation. The one verb that reaches another box. |
| `read_mailbox` | Taking delivery of everything waiting in its own channel. Leftovers from an interrupted read come back too, flagged and still owed. With `counts_only`, checking what is waiting and taking delivery of none of it — so a poll that finds nothing costs nothing and owes nothing. |
| `read_message` | Taking delivery of exactly one message from its own box, without draining the rest. A live message in somebody else's box is refused; the `processed` archive is readable from anywhere, because reading history moves nothing. |
| `mark_processed` | Retiring a message once it has actually been acted on, with a note recording the outcome — including failure. Terminal; an archive, never a deletion. |
| `list_sent` | Checking whether what it sent arrived, and what became of it, without moving anything. |

**Sessions — keeping the run's own record**

| tool | what a caller is doing |
|---|---|
| `journal` | Recording what happened, at the altitude of a journal rather than a log. |
| `amend_journal` | Fixing the last thing it wrote, rather than appending a correction. |
| `wrap_session` | Ending this run and telling its story. |

### Planned

Derived from a catalogue of the domain actions actually observed over a month of
real use — around eighty of them across thirteen families. Most are already
served by the verbs above. What follows is what is genuinely not served yet.
Nothing here is scheduled; the list exists so the gaps are visible rather than
discovered one at a time.

| the action | why there is no verb yet |
|---|---|
| **Compose a walk across several hops** | A single typed edge is filtered on and traversed today, which answers "which people are in X" in one call. What is not served is several edge filters combined, a walk of more than one hop, or a named query kept and re-run. |
| **Read a thing's history as distinct from its current truth** | The two flavours of record are being separated; until then, chronology and truth read alike. |
| **Get a synthesised portrait of a subject** | A projection over the graph rather than a read of one record. Waits on graph traversal. |
| **Surface what has gone quiet, or is decaying** | Rules and rhythms are stored as ordinary facts and nothing fires on them. This is the largest single gap. |
| **Verify a write by reading it back** | Every write already does this internally; no caller can ask for it as its own step. |
| **Take a whole boot bundle in one call** | Orientation is one call today, but the standing rules and context a session needs still come from outside jojobot. |
| **Hand off continuity to whoever picks up next** | A resuming run has to reconstruct from the chronology rather than being handed a note written for it. |

### Deliberately absent

Recorded so nobody proposes them again as fresh ideas.

- **No delete.** Removed from the port and pinned by test. True deletion is a
  rare human act, taken outside this server.
- **No peek at a mailbox.** Reading is delivery; a peek would silently strip the
  guarantee that unfinished work resurfaces.
- **No `create_mailbox`.** A box is not minted by a call of its own — it opens
  with the bot that owns it, in the same act that creates the bot, and is named
  for its handle. An unowned mailbox cannot exist, so there is nothing such a
  verb would have jurisdiction over.
- **No second orientation door.** There is one, and only one.
- **No version marker on the orientation payload.**
- **No per-identity tool surface.** Every identity sees the same tools. A
  charter shapes what an identity considers appropriate, never what it is
  permitted to call. This server aligns; it does not police.
- **No inference of any kind.** jojobot never guesses, summarises, or decides.
  It holds, guards and serves; the assistant is the only mind.

## Design

- **User-agnostic.** Nothing about any particular person is compiled in. Every
  instance-specific value — issuer, audience, hostnames, preferences — is
  runtime configuration or data the server reads, never code.
- **Domain-driven, ports-and-adapters.** A pure `jojobot-domain` crate owns the
  ubiquitous language; adapters and the MCP surface depend on it, never the
  reverse.
- **Fronts what it does not own.** Where a service is the source of truth,
  jojobot reads through it rather than copying it, and an anti-corruption layer
  quarantines that service's quirks. What no service owns, jojobot keeps in a
  SQL store it starts and supervises itself.
- **Resource server only.** jojobot validates bearer tokens; it never issues
  them. Bring your own OpenID Connect authorization server.

## Layout

```
crates/
  jojobot-domain     pure domain — bounded contexts as modules, no I/O, no MCP
  jojobot-adapters   the fronted service's anti-corruption layer, and the SQL store jojobot runs itself
  jojobot-mcp        the MCP adapter — the only crate that maps MCP calls to the domain
  jojobot            the binary: HTTP transport + resource-server auth
```

## Build & run

Development uses a Nix flake (a Rust toolchain, `pkg-config`, and OpenSSL):

```sh
nix develop            # drops you in a shell with the pinned toolchain
cargo build            # build the workspace
cargo test             # run the tests, including the auth golden tests

# Mail and sessions are rows in a SQL store the server starts and supervises,
# so it needs somewhere to keep them. The service manager hands that path over
# in production; supply one by hand for a local run.
STATE_DIRECTORY=$PWD/.state cargo run -p jojobot
```

The store's server binary comes from the flake, so run from inside `nix develop`.
It is the one package taken from `nixpkgs-unstable` rather than the release
channel the rest of the toolchain tracks: dolt moves faster than a channel does,
and the tests run against the newer version a deploying host tracks.

Or build the package directly:

```sh
nix build              # produces ./result/bin/jojobot
```

## Deploying

The flake exposes an overlay and a NixOS module:

```nix
{
  nixpkgs.overlays = [ inputs.jojobot.overlays.default ];
  imports = [ inputs.jojobot.nixosModules.default ];

  services.jojobot = {
    enable = true;
    issuer = "https://id.example.org";       # enables resource-server auth
    resource = "https://jojobot.example.org/mcp";
    # audience defaults to the resource id — set it if the issuer's `aud` differs
  };
}
```

The service binds localhost by default; front it with a TLS-terminating reverse
proxy or tunnel. With `issuer` unset it runs open (development only).

## Configuration

All configuration is environment-driven.

| Variable | Meaning | Default |
| --- | --- | --- |
| `JOJOBOT_BIND` | Listen address | `127.0.0.1:8080` |
| `JOJOBOT_RESOURCE` | This server's resource identifier (RFC 9728) | derived from bind |
| `JOJOBOT_ISSUER` | OIDC issuer URL — **set this to enable auth** | unset |
| `JOJOBOT_AUDIENCE` | Required token audience (RFC 8707) | the resource id |
| `JOJOBOT_JWKS_URI` | Explicit JWKS URI | discovered from issuer |
| `JOJOBOT_ALLOW_NO_AUTH` | Set to `1` to run **without auth** (dev only) | unset |
| `JOJOBOT_ALLOWED_SUBJECTS` | Optional comma-separated `sub` allowlist (requires auth) | unset = any valid token |
| `JOJOBOT_OUTLINE_URL` / `JOJOBOT_OUTLINE_TOKEN` | The Outline instance Memory fronts | unset |
| `STATE_DIRECTORY` | Where the SQL store keeps its data — **required**; the service manager sets it | unset |
| `JOJOBOT_STORE_PORT` | Loopback port the SQL store serves on | `3307` |

Everything the store keeps sits under `$STATE_DIRECTORY/db`: `jojobot` is the
database, and `dolt-home` is the server's own configuration. jojobot names that
second path instead of taking it from the environment, so the store also comes
up where the process runs as an account with no home directory of its own.

The server **fails closed**, and there are two kinds of it.

*Misconfiguration.* With `JOJOBOT_ISSUER` unset it refuses to start unless
`JOJOBOT_ALLOW_NO_AUTH=1` is set explicitly, and even then it refuses a
non-loopback bind. This turns a dropped-secret misconfiguration into a startup
error rather than a silently unauthenticated `/mcp`.

*No store to serve from.* Everything jojobot holds — mail, sessions, entities,
facts and prose — lives in the SQL store, so the server refuses to start without
a state directory, or when the store does not come up. A server that started
anyway would answer as if it held nothing and present that emptiness as the
truth. Each refusal names the condition it found.

A missing `JOJOBOT_OUTLINE_URL` is **not** one of these. Outline is only the
source a first boot carries records out of, so with none wired there is simply
nothing to carry and the server starts on an empty store.

*A carry that did not finish.* The first boot after an upgrade moves the records
in and then serves from them, in that order. If the read-back fails the boot
dies rather than serving — the store is then holding rows nothing has checked,
and **every later boot refuses with that state until a person looks.** That is
deliberate: the alternative is a server treating "there are rows here" as "this
is everything."

The way out is to make the target empty again and let the boot redo it: clear
the carried tables and the row recording the carry, then start the server. **Do
that only after reading why the first one failed** — the journal names the
condition, and a repair that skips the diagnosis restores the same failure.
Nothing is lost by clearing: the carry reads its source and never writes to it,
so the records it came from are still there.

## Auth model

jojobot is an OAuth2 protected resource per the MCP authorization spec:

- `GET /.well-known/oauth-protected-resource` advertises the authorization
  server(s), unauthenticated (RFC 9728).
- `/mcp` requires a valid bearer JWT. Tokens are verified against the issuer's
  JWKS with an **RS256-only** allowlist, and checked for issuer, audience
  (RFC 8707), and expiry. A 401 carries a `WWW-Authenticate` challenge pointing
  back at the metadata endpoint.
- A path jojobot does not serve answers **404**, not 401. A challenge on an
  unmounted path tells a client its credentials are wrong when the truth is
  that the path is not there.

## License

[AGPL-3.0-or-later](./LICENSE).
