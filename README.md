# jojobot

A server that holds one person's context — the people, the projects, the
commitments, the things that come round again — and hands it to an AI assistant
over a single [Model Context Protocol](https://modelcontextprotocol.io)
endpoint.

**jojobot does not think.** It stores things and answers questions about them.
The assistant does the thinking.

**This repo is an experiment**: one person's assistant, built in the open to
find out whether the idea holds up. Some of it is not built, and some of what is
built will turn out to be wrong.

## The problem

An assistant is only as useful as what it knows, and today that knowledge lives
in prose that drifts and that nothing tests. Given too little, a model invents.
Given too much, it skims. Either way the person is the one who notices. The bet
is that an assistant's method belongs in software instead: versioned, tested,
enforced.

## What it is

**One graph.** Entities are nouns — a person, a project, a place, a loop. A fact
is one dated claim about one of them, and it can draw a typed edge at another.
Questions that cross several things are answerable because the edges are there.

**The method ships with the software.** Skills and charters arrive with the
verbs, versioned and tested beside them.

**Three ideas it rests on:** a claim records who backs it and, separately, how
sure anyone is · what a bot carries between runs has a hard ceiling it cannot
raise · a refusal says what to do next.

## Where it is

Measured against `main`; a running instance may be behind it. Each feature has a
design page in [`pm/feature-briefs/`](./pm/feature-briefs).

● works ·
[Memory](./pm/feature-briefs/memory.md) ·
[Provenance and standing](./pm/feature-briefs/provenance-and-standing.md) ·
[Finding things](./pm/feature-briefs/finding-things.md) ·
[Kinds](./pm/feature-briefs/kinds.md) ·
[Mailboxes](./pm/feature-briefs/mailboxes.md) ·
[Bots and identity](./pm/feature-briefs/bots-and-identity.md) ·
[Sessions](./pm/feature-briefs/sessions.md) ·
[The journal](./pm/feature-briefs/the-journal.md) ·
[Skills](./pm/feature-briefs/skills.md) ·
[The status bar](./pm/feature-briefs/the-status-bar.md) ·
[The store](./pm/feature-briefs/the-store.md) ·
[Rhythms](./pm/feature-briefs/rhythms.md) ·
[What is owed and late](./pm/feature-briefs/what-is-owed-and-late.md) ·
[Entitlements](./pm/feature-briefs/entitlements.md) ·
[Budgets](./pm/feature-briefs/budgets.md) ·
[Carried state](./pm/feature-briefs/carried-state.md) ·
[The web UI](./pm/feature-briefs/the-web-ui.md) ·
[How jojobot is proven](./pm/feature-briefs/how-jojobot-is-proven.md)

◐ partly ·
[Finding your way](./pm/feature-briefs/finding-your-way.md) ·
[Role claims](./pm/feature-briefs/role-claims.md) ·
[What you have already been shown](./pm/feature-briefs/what-you-have-already-been-shown.md) ·
[Synthesis](./pm/feature-briefs/synthesis.md)

○ not built ·
[Lines](./pm/feature-briefs/lines.md) ·
[Fronting a layer](./pm/feature-briefs/fronting-a-layer.md)

## Reading further

[`pm/feature-briefs/`](./pm/feature-briefs) carries the reasoning.
[CLAUDE.md](./CLAUDE.md) is for AI sessions working in this repo.
[CONTRIBUTING.md](./CONTRIBUTING.md) is how the work gets done.

## License

[AGPL-3.0-or-later](./LICENSE).
