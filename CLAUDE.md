# CLAUDE.md — working in this repo

## What jojobot is

jojobot is a personal-assistant **domain server**. It fronts a user's existing
life-layer services (kanban, notes wiki, calendar, link library) and exposes
itself to an AI assistant through one MCP endpoint. It is not an MCP proxy: it
is the assistant's *method*, made into software — it feeds the assistant
context, guards writes behind invariants, remembers everything, and never
pretends to think. The assistant is the only mind.

**The through-line:** the assistant's behaviour today lives as prose — markdown
rules, skills, per-session caches, boot hooks. The mission is to migrate it
into software: versioned, tested, enforced. A fuzzy rule in a markdown file
becomes a typed invariant the compiler and tests hold. The end state is
sessions that boot *from jojobot* instead of from files.

## ⚠️ THE BRIGHT LINE — zero user PII, zero life specifics. Ever.

**This repo is public. Nothing that identifies a user's life enters it — no
real people, places, events, organizations, festivals, trips — anywhere:**
code, tests, fixtures, docs, commit messages, branch names, error strings.
The line binds text *bound for* the repo, not just files inside it: a
hand-off task, an example, a report that will become a commit message is
covered the moment it is written. Fixture names come from the fictional
roster (Simpsons universe + greek letters) —
`crates/jojobot-domain/tests/fixture_roster.rs` is the allowlist and CI
enforces it. An example quoted from the user's private docs gets a roster
substitution BEFORE it crosses; that quote path is the standing leak vector
and has burned this project three times. When in doubt, it doesn't cross.

## Where the design lives

- The **product roadmap & vision** and the **architecture doc** (bounded
  contexts, data model, decisions, as-built records) live in the user's private
  wiki, not in this repo. The coordinator session owns them and reconciles them
  after every slice.
- **Migration note:** while behaviour migrates, most operating context still
  lives in the user's private `~/code/life` repo. Sessions on the user's
  machine may read it for orientation. **Nothing life-specific may cross back
  into this repo** — see the bright line below.

## The build model — two sessions, never one

- **Coordinator (`pm`)** — holds the vision, scopes each slice, writes
  hand-off tasks, reviews adversarially, reconciles the docs. Does not
  implement.
- **Implementer (`dev`)** — boots in this repo, builds ONE scoped slice
  test-first, reports back, and is disposed. If that's you: read your task
  from the **`dev` mailbox** over the jojobot MCP (`read_mailbox`). Work that
  one task. When done, `mark_processed(message, notes)` and post your report
  as a message to the **`pm`** mailbox. A failure the coordinator must know
  about is also a message to `pm` — failure is data, not silence.
- Merging advances `main` only. **Deploy and push are the user's verbs** —
  never deploy; never push unless asked.

## The capability ladder

Milestones are **capabilities, never infrastructure** — each is named "after
this, I can ___".

> **This file and the README carry the roadmap status, so they are versioned
> with the code: any slice that changes what's true here — a milestone ships,
> a verb lands, config changes — updates both in the same round.** A stale
> "Status" section is a bug, not a nice-to-have.

Shipped and live:

- **M0** — skeleton: MCP over streamable HTTP behind OAuth2 resource-server auth.
- **M1** — Memory: typed entities (`kind:slug` handles) + dated facts with
  provenance (testimony vs inference); the write guard (nothing is created as
  a side effect; near-misses come back blocked-with-candidates); read-back on
  every write.
- **M2/M2.5/M2.8** — `search` across facts, entities and prose; structured
  edges (location · membership · attendance · about); aliases; orienteering
  retrieval (every hit arrives with its surroundings).
- **M3** — Mailboxes: kanban-backed message boxes (`new → read → processed`;
  read ≠ processed; processed is a terminal archive; no delete verbs — the
  tool surface is pinned by test).

Ahead: **M4** bots (an AI identity = handle · charter · rules · memory ·
owned mailbox; `boot_bot` as the one orienting door) → **M5** sessions →
**M6** trace (claim → fact → receipt → evidence) → **M7** portraits →
**M8** attention (rules, rhythms, nags) → **M9** boot bundles (sessions boot
from jojobot) → **M10** seeding (user-agnostic defaults, "batteries included,
overrides win").

**Layering: engine + bot.** The engine (this repo) is user-agnostic code; a
bot and its rules are *data* in the user's own store. Nothing about any
particular person is compiled in.

## Engineering rules (non-negotiable)

- **Strict TDD.** A feature is proven by an automated test; every bug fix
  starts from a failing test you watched fail FIRST. A manual run proves
  nothing.
- **Commits: one per coherent problem.** A milestone lands as a handful of
  commits — never one per file or checklist item, never dozens.
- **Zero user PII, zero life specifics** — the bright line at the top of this
  file. It outranks every other rule here.
- **The real-dependency gate.** A slice that touches an adapter does not merge
  on fakes alone: run the real-dependency integration suites
  (`crates/jojobot-adapters/tests/`, against disposable stores; credentials
  come from `.env`) and show the output. A suite that exists but was never run
  is a blocker, not a footnote. Sourcing `.env` to *run* the suites is
  sanctioned; never print or copy its values.
- **Fakes must be hostile where reality is.** When a real store's quirk is
  discovered (normalization, silent drops, clamped pagination), bake it into
  the fake or a golden fixture — a polite fake that stores bytes verbatim is
  how green tests ship broken adapters.
- **Hexagonal, domain-driven.** `jojobot-domain` stays pure (no I/O, no MCP);
  each fronted service's quirks live in its adapter, quarantined.
- **Green bar before DONE:** `cargo test` green and `cargo clippy` clean, run
  through the flake (`nix develop -c cargo test`).
- **Don't over-engineer.** Reach for the simplest model that fits the stated
  design; the tell is a pass that keeps getting bigger. When the design and
  the code disagree, say so — don't silently deviate, and don't silently
  comply either.
