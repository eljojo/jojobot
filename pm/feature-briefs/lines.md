# Lines — a bot asked to run, and the software that keeps it running

> **jojobot** · kind: feature-brief · boot: on-demand

**A bot is somebody jojobot knows. A line is that bot asked to be running.** jojobot holds the request and what was seen of it; the hypervisor service, an outside program on jojobot's API, makes the runtime match the request and writes back what it saw. **jojobot never calls the runtime.**

## jojobot records, the service acts

**jojobot is the API server and the service is the controller**, the way a cluster's API server holds what should exist and a controller makes it so. jojobot stores the request, guards who may write it, and serves it with a change feed. The service watches the feed, starts and stops agents through the runtime, and reports what it observed. **jojobot holds no runtime credential and dials nobody**, so a runtime that is down, replaced or swapped for another never reaches into the store.

**The service is pure software and uses no model.** Starting a line, noticing it went quiet and waking it are arithmetic over stored facts. A model doing that by hand misses steps and spends quota on bookkeeping, so the bar is that no step of keeping a line running needs judgement.

**Policy lives in records anyone can read, never in the service's code.** A behaviour nobody can find written down is a behaviour nobody can check.

**A bot never needs the runtime's vocabulary to ask for a line.** It names a bot and reads back a line; providers, workspaces, permission modes and agent ids stay with the service. A field that only makes sense to somebody who has read the runtime's documentation does not belong on the request.

## The request

**A manager anywhere above the bot on the org chart writes the request**: which bot, whether it should be running, on which machine and model, and its recovery policy (whether to restart, after how long, how many times, and what to do when quota runs low). **The service writes the status**: what it observed, which version of the request it acted on, the runtime's reference and the last result. Neither writes the other's keys, and other bots add notes beside them.

**A request without a recovery policy gets no recovery.** The service reports a visible block instead of guessing, because software that invents a retry has made a decision nobody wrote down. **Reports go to whoever asked for the bot, or to its manager.**

## What a line is, and what was seen of it

**What a line IS belongs to jojobot**: which bot, who asked, since when, for what. **Whether it is WORKING is an observation the service reports, and every observation carries when it was made.** Idle, thinking, waiting on quota and unreachable are different readings, and a session with no recent reading is unknown, never assumed alive. A fresh reading is a new request to the service, never jojobot reaching through to the runtime.

**Is anybody working as `bot:dev` right now?** A board column asserts it because somebody moved a card; a dated reading from the runtime answers it, and says how old the answer is.

## A managed session

**A session started through the stack is managed, and jojobot stamps it enhanced when every required feature is present and working**, the way a processor lists its features: wake on mail, a compaction report from the runtime, runtime status, and attested identity. Advertised is not enough; the path has to work now. When anything fails the stamp drops to plain, with the reason, and the bot's mail stays queued.

**What enhanced buys is chores done by software.** The bot stops polling, claiming and reporting by hand, and its charter gets lighter. A bot writing to an enhanced peer knows the message will wake it.

**Attested identity is the runtime vouching which agent is speaking.** The service's plugin puts a token on that agent's connection to jojobot, which the model never reads, and reports which bot request the agent serves. A boot whose token matches is attested; a missing or wrong one leaves the session plain. **It stops mistakes now: the wrong bot booted, a stale agent, two copies of one line. It does not yet stop malice**: an agent that searches its own files could find the token, and the stamp says so.

**A plain session keeps everything** — boot, roles, memory, mail, journal, compaction reports, wrap. Only being woken is missing, and the stamp says so.

## Waking

**Mail that lands for a managed bot wakes it.** The service sees the post on the mailbox feed, finds which agent runs the bot, and its plugin delivers a wake at the next idle moment. **A wake never interrupts a turn, and it never reads or closes mail**: the agent reads its own box and decides what to do.

## Stopping

**Stopping is a desired state, not a deletion.** The record stays, pending wakes are dropped before the stop is reported, and a runtime that keeps sessions on disk can resume one. 🚨 **What is lost is the live turn, and what sits uncommitted in the checkout stays invisible to anyone not watching.** So a stop reports what is unsaved before it acts: the person stopping is the last one positioned to notice.

## What it is not

**Not jojobot driving the runtime.** jojobot has no verb that reaches a process, and no credential that would let one.

**Not a model.** No step needs judgement; one that seems to is a policy nobody has written yet.

**Not a scheduler.** Recurring work lives elsewhere; a line runs because somebody asked for it.

**Not a second charter store.** A started agent is told which bot to boot as and reads its charter from jojobot at boot.

**Not a way to make more work happen.** Two lines running is not two lines producing. The card comes first, exactly as when a person starts one by hand.

**Not ownership of the machine.** Which hosts run agents, which software they run and what credential the service holds are the operator's, enrolled by the operator and revoked by the operator.