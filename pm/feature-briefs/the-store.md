# The store — where everything lives, and why an agent never hears about it

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**One versioned SQL store holds what jojobot holds:** entities, events, edges, kinds, mailboxes, messages and sessions. An entity's prose is a field on an object, not a page somewhere else.

**Only one thing stays outside the store: a page of markdown a person reads as a page.** That is a real capability, not a leftover. The test is the whole scope line. For most of what jojobot holds, the answer is machinery.

## Invisible on purpose

**An agent may know how jojobot is arranged.** It may know that jojobot is a graph, that things nest, and that a claim draws typed edges.

**An agent must never receive anything that is true only of the storage product.** This includes its identifiers, its table names and its furniture.

**An agent that knows the store reaches around jojobot to it.** The discipline about where things get stored is the product. Nothing that reaches past it survives.

## One writer, and the fold in memory

**The store only appends. What is true now is a fold over those writes, and jojobot holds that fold in memory.** A read answers from memory instead of scanning, and the fold is rebuilt when the server starts.

**jojobot is one server with one application worker, so every write passes through a single process.** That is what makes an in-memory fold authoritative rather than a cache: nothing reaches the store without the same process folding it in.

⚠️ **THIS ASSUMPTION IS LOAD-BEARING, and it is written here because a later reader will not infer it from the code.** A second writer of any kind — another worker, a restored backup, a migration tool, or a person running SQL against the store directly — leaves the memory copy silently wrong, and nothing running would notice. **Anything that introduces a second writer rebuilds the fold or stops the server. There is no third option.**

**"Current" is not one rule, so the fold is not one map.** The newest write wins for an ordinary field. A counter's value is the sum of its writes. A mark that hides a record changes what counts at all, and what a search should surface is a different answer from what a field read returns, over the same rows. **Each rule is named for what it folds, so a new one joins the set instead of rewriting it.**

## Versioning, and the bound on it

**jojobot uses the store's own history at a session's two boundaries.** It marks one when a session opens and one when it closes, so a sitting can be diffed and an operator can roll one back by hand in an emergency. **A boundary with nothing to mark leaves no mark** — a run that only read has nothing to roll back, and marking it anyway would fill the history with sittings that changed nothing.

**No commit per write. No diff standing in for an audit trail. No branches standing in for migrations.**

**The purpose is human forensics and break-glass recovery. It is never the data model.** A session is far coarser than a write. **jojobot derives nothing it serves from the store's history.** What jojobot keeps, it keeps in its own ordinary rows, where it answers per key rather than per sitting.

**A rollback is an operator's tool, not an agent's.** It reaches across everything written since the commit it returns to, including another session's work, and it reports no error to anybody. A person may pull that handle deliberately. A verb within an agent's reach must not offer it.

## Schema

**Migrations are ordered and versioned. The server applies them. The database records what has run.** Nobody applies one by hand. jojobot infers nothing from the shape it finds.

**A migration that somebody has pushed has run somewhere, and it is frozen.** Editing one creates a new migration. *Pushed* is the test because it is the question anybody can answer. The repository does not know which instances are live.

**The deploying machine supplies the engine as a parameter.** It pins the version there, not here, so one tool has one source.

## Moving

**Replacing a store is a one-time migration. It is never a sync.** Two live stores are two sources of truth, and the second one starts to lie immediately.

**A migration that replaces a store reads every record and writes it through the new path.** It derives explicitly what it cannot carry across. Anything a page's position implied must become a column, or it is lost and nobody sees it go.

⚠️ **Reshaping data already in this store is a different act, and it carries no such duty.** It may give the new shape its own tables. It may leave the old rows unread. It leaves every table it does not own alone.

## What it is not

**Not a version-control system for what jojobot knows.** The store keeps history. That is not where a thing's history comes from.

**Not durable by courier.** jojobot records a durable fact in the store. Local scratch carries something between two points, and it is never a home.

**Not deletable through the surface.** True deletion is a rare, deliberate act by a person, outside jojobot.