# jojobot

A personal-assistant server that fronts your existing life-layer services
(task/kanban, notes, calendar, link library) and exposes itself to an AI
assistant through a single [Model Context Protocol](https://modelcontextprotocol.io)
(MCP) endpoint.

jojobot is **not** an MCP proxy. It is software with a domain: it feeds the
assistant context, guards writes behind invariants, remembers what matters, and
never runs inference of its own — the assistant is the only mind.

> **Status: two domains live, one front door over both, and sessions can boot
> as an identity.** The server ships **Memory** — typed entities and dated
> facts with provenance (testimony vs inference), a write guard that blocks
> near-miss creations with candidates, structured edges, aliases, and
> orienteering retrieval (every hit arrives with its surroundings) — and
> **Mailboxes** — kanban-backed message boxes with `new → read → processed`
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
> somebody else's workload — while an unreadable card is reported to everyone,
> because that is a fault on the board rather than a queue.
> **`search` spans both**: one ranked list over entities, facts, prose and
> messages, mail included by default and in every state (`processed` archives
> too), each mail hit carrying its box, state, sender and id — and an answer
> that could not see the board says so rather than reading as "nothing
> matched". On top of both, **bots**: an AI identity is an entity of kind `bot`
> with a charter, rules, memory and one owned mailbox. **`start_here` is the one
> door**: it takes an optional bot name, and naming one **starts or resumes that
> bot's session**, because a bot is a role and a session is one mortal run of it.
> Booting anonymously is legal and deliberately useless — orientation, and no
> handle behind it. **Sessions** live on their own board: a focus that is current
> truth, an append-only chronology (the session's own beats plus jojobot's,
> marked apart), and `active` → `wrapped` | `abandoned` — **`wrapped` is the last
> word, because wrapping publishes the story to the operator's Journal, while
> `abandoned → active` is the one legal walk-back, since a run nobody wrapped up
> published nothing.** A session begins lazily — no card until the first write —
> resumes across a disconnect, and is swept to `abandoned` after a day of
> silence. **The `sid` is the only address**: a short opaque handle the server
> mints at the door, saying nothing about the work, and it rides every verb —
> reads included — because real MCP clients open a fresh connection per tool call
> and a connection-held identity does not survive to the next one. It is written
> on the session's own card, so a restart does not orphan it. All behind OAuth2
> resource-server auth, every write verified by read-back.

## Vision & roadmap

jojobot's mission is to move an assistant's *method* out of prose (rules files,
skills, per-session caches) into software: versioned, tested, enforced. The
roadmap is a ladder of **capabilities** — each release is named "after this,
I can ___", never after infrastructure.

Shipped so far: memory, search and edges, mailboxes, bots (AI identities with a
handle, charter, rules, memory and an owned mailbox), and every session on the
record.

**In progress: an alignment release — cleanup only, no new capability.** A
design revision moved where sessions and mailboxes live and how a bot's journal
works, so part of what runs today reflects the previous model. Nothing ships
until the repo matches the current one. Then: the surface redesign, events as a
first-class flavour of fact, trace, portraits, attention, sessions booting from
jojobot, and seeding.

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
| `start_here` | Coming online: getting the world model, a live snapshot of what exists, and — when naming an identity — that identity's charter, rules, mailbox counts, and a session handle. The one door; there is deliberately no second. |
| `ping` | Checking jojobot is reachable at all. Identity, version, time, nothing else. |

**Memory — recording and reading what is known**

| tool | what a caller is doing |
|---|---|
| `capture` | Remembering a fact about something, with its provenance and optionally one typed edge to another entity. |
| `recall` | Reading back what is recorded about one subject. |
| `search` | Finding something across everything held — entities, facts, prose and messages in one ranked list, each hit arriving with its surroundings. |
| `add_entity` | Bringing a new thing into memory. Screened against near-misses first, so a typo never mints a duplicate. |
| `update_entity` | Maintaining what a thing is called, and its other metadata. |
| `update_fact` | Correcting something recorded wrong — rewritten in place, never as an addendum. Also how a claim gets confirmed, or a refutation recorded as standing truth. |
| `list_entities` | Seeing what exists, by kind. |
| `set_charter` | Writing an identity's charter — the orienting text that says what it is and where its work lives. |

**Mailboxes — leaving word and taking delivery**

| tool | what a caller is doing |
|---|---|
| `post_message` | Leaving word for someone not in this conversation. The one verb that reaches another box. |
| `read_mailbox` | Taking delivery of everything waiting in its own channel. Leftovers from an interrupted read come back too, flagged and still owed. |
| `read_message` | Taking delivery of exactly one message, without draining the box. |
| `mark_processed` | Retiring a message once it has actually been acted on, with a note recording the outcome — including failure. Terminal; an archive, never a deletion. |
| `list_mailboxes` | Checking what is waiting without taking delivery, so a poll that finds nothing costs nothing. |
| `list_sent` | Checking whether what it sent arrived, and what became of it, without moving anything. |
| `create_mailbox` | Bringing a channel into being. The only mint, with its full near-miss screen. |

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
| **Walk the graph from a subject** | Retrieval reads one subject's own record. The typed edges exist and nothing traverses them, which is what would make this a graph rather than a table. |
| **Read a thing's history as distinct from its current truth** | The two flavours of record are being separated; until then, chronology and truth read alike. |
| **Ask why a claim is believed** | Records carry addresses, so the evidence chain exists; nothing follows it end to end. |
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
- **No second orientation door.** One existed, drifted from the first, and was
  deleted. The rule that replaced it: a constraint that matters is written as a
  claim a test can fail, never as a metaphor.
- **No version marker on the orientation payload.** Rejected outright, every
  variant — over-engineering for a problem that does not occur.
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
- **Fronts, never owns.** The underlying services stay the source of truth. An
  anti-corruption layer per service quarantines each one's quirks.
- **Resource server only.** jojobot validates bearer tokens; it never issues
  them. Bring your own OpenID Connect authorization server.

## Layout

```
crates/
  jojobot-domain     pure domain — bounded contexts as modules, no I/O, no MCP
  jojobot-adapters   anti-corruption layer per fronted service (Outline, Vikunja)
  jojobot-mcp        the MCP adapter — the only crate that imports rmcp
  jojobot            the binary: HTTP transport + resource-server auth
```

## Build & run

Development uses a Nix flake (a Rust toolchain, `pkg-config`, and OpenSSL):

```sh
nix develop            # drops you in a shell with the pinned toolchain
cargo build            # build the workspace
cargo test             # run the tests, including the auth golden tests
cargo run -p jojobot   # start the server
```

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
| `JOJOBOT_VIKUNJA_URL` / `JOJOBOT_VIKUNJA_TOKEN` | The Vikunja instance Mailboxes front | unset |

The server **fails closed**: with `JOJOBOT_ISSUER` unset it refuses to start
unless `JOJOBOT_ALLOW_NO_AUTH=1` is set explicitly, and even then it refuses a
non-loopback bind. This turns a dropped-secret misconfiguration into a startup
error rather than a silently unauthenticated `/mcp`.

## Auth model

jojobot is an OAuth2 protected resource per the MCP authorization spec:

- `GET /.well-known/oauth-protected-resource` advertises the authorization
  server(s), unauthenticated (RFC 9728).
- `/mcp` requires a valid bearer JWT. Tokens are verified against the issuer's
  JWKS with an **RS256-only** allowlist, and checked for issuer, audience
  (RFC 8707), and expiry. A 401 carries a `WWW-Authenticate` challenge pointing
  back at the metadata endpoint.

## License

[AGPL-3.0-or-later](./LICENSE).
