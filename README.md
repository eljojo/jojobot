# jojobot

A server that holds one person's context — the people, the projects, the
commitments, the things that come round again — and hands it to an AI assistant
over a single [Model Context Protocol](https://modelcontextprotocol.io)
endpoint.

**jojobot does not think.** It stores things and answers questions about them.
The assistant does the thinking.

**This repo is an experiment.** It is one person's assistant, built in the open
to find out whether the idea holds up. A fair amount of it is not built, and
some of what is built will turn out to be wrong.

## The problem

An assistant is only useful if it knows things, and at the moment that knowledge
lives in prose: rules files, skills, notes carried from one session to the next.
Prose drifts. Nothing tests it and nothing enforces it.

It fails in two directions. Given too little, a model fills the gap by inventing.
Given too much, it skims and misses the part that mattered. In both cases the
person is the one who notices, which means the work has come back to them.

The bet is that an assistant's method belongs in software instead — versioned,
tested, enforced. Whether that is the right bet is still open.

## What it actually is

**Everything jojobot holds is one graph.** Entities are nouns: a person, a
project, a place, a loop that comes round again. A fact is a single dated claim
about one of them, and a fact can draw a typed edge at another entity. That is
the whole shape. Questions that cross several things — who was at that, what has
gone quiet, which of these is missing a detail — are answerable because the
edges are there, rather than because somebody wrote a query for each one.

**The method ships with the software.** Skills are procedures held in the
binary: what to do when a particular sort of job is in front of you. A bot's
charter is composed the same way, part from the build and part from the
instance. So the discipline about how to work is not configured onto jojobot
afterwards. It arrives with the verbs, versioned and tested beside them. The
surface and the practice are designed against each other, and neither is much
use on its own.

## Three ideas the design rests on

**A claim records who backs it and, separately, how sure anyone is.** Something
the person said and something an agent worked out do not read alike. Keeping the
two apart is what stops a confident guess settling into fact.

**What a bot carries between runs has a hard ceiling, and nothing can raise its
own.** Being made to drop something is meant to be the point. Context runs out
somewhere in any case; when the limit is not explicit, a model skims instead.

**A refusal should say what to do next** — candidates, the verb to call instead,
the piece that is missing. The aim is a surface an agent can find its way around
without being told what exists. It does not always manage that.

## Where it is

Measured against the code on `main`. A running instance may be behind it.
Each feature has a design page in [`pm/feature-briefs/`](./pm/feature-briefs)
covering what it is for and what it deliberately is not.

| | feature | |
|---|---|---|
| ● | [Memory](./pm/feature-briefs/memory.md) | Entities, events and edges in a typed graph. |
| ● | [Provenance and standing](./pm/feature-briefs/provenance-and-standing.md) | Who backs a claim, and how sure — two separate questions. |
| ● | [Finding things](./pm/feature-briefs/finding-things.md) | `recall` for a precise question, `search` for when you do not know where to look. |
| ● | [Kinds](./pm/feature-briefs/kinds.md) | What a thing is and the keys it carries, treated as one idea. |
| ● | [Mailboxes](./pm/feature-briefs/mailboxes.md) | Leaving word for someone who is not in this conversation. |
| ● | [Bots and identity](./pm/feature-briefs/bots-and-identity.md) | An AI identity with a charter, its own rules and one mailbox. |
| ● | [Sessions](./pm/feature-briefs/sessions.md) | One run of one bot, and the key it works through. |
| ● | [The journal](./pm/feature-briefs/the-journal.md) | What a bot did, tagged with the run that did it. |
| ● | [Skills](./pm/feature-briefs/skills.md) | Procedures that ship in the binary, fetched by name. |
| ● | [The status bar](./pm/feature-briefs/the-status-bar.md) | A small block on each answer for what the caller did not ask about. |
| ● | [The store](./pm/feature-briefs/the-store.md) | One versioned SQL store underneath, which no agent hears about. |
| ● | [Rhythms](./pm/feature-briefs/rhythms.md) | A loop in a life, held as the loop rather than as a date. |
| ● | [What is owed and late](./pm/feature-briefs/what-is-owed-and-late.md) | What has a due moment that has passed, as of a named day. |
| ● | [Entitlements](./pm/feature-briefs/entitlements.md) | Tickets, passes, visas, memberships. One shape under all of them. |
| ● | [Budgets](./pm/feature-briefs/budgets.md) | What is live has a ceiling, and something gives when it is reached. |
| ● | [Carried state](./pm/feature-briefs/carried-state.md) | The few thoughts a bot keeps between runs. |
| ● | [The web UI](./pm/feature-briefs/the-web-ui.md) | A directory listing over the graph. An entity path is a URL path. |
| ● | [How jojobot is proven](./pm/feature-briefs/how-jojobot-is-proven.md) | Unit tests, user stories, and a real model driven through a live instance. |
| ◐ | [Finding your way](./pm/feature-briefs/finding-your-way.md) | Refusals point onward. The query vocabulary is taught nowhere. |
| ◐ | [What you have already been shown](./pm/feature-briefs/what-you-have-already-been-shown.md) | Writes answer with receipts. Tracking what a session has seen is not built. |
| ◐ | [Synthesis](./pm/feature-briefs/synthesis.md) | A record can carry what it stands for. Nothing produces one on its own. |
| ○ | [Lines](./pm/feature-briefs/lines.md) | A bot that is running, started and stopped through jojobot. |
| ○ | [Fronting a layer](./pm/feature-briefs/fronting-a-layer.md) | Standing in front of a task board or calendar without owning it. |

● works · ◐ partly · ○ not built

## Reading further

[`pm/feature-briefs/`](./pm/feature-briefs) carries the reasoning.
[CLAUDE.md](./CLAUDE.md) is for AI sessions working in this repo.
[CONTRIBUTING.md](./CONTRIBUTING.md) is how the work gets done.

## License

[AGPL-3.0-or-later](./LICENSE).
