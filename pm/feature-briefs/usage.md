# Usage — what each bot's runs add up to

> **jojobot** · kind: feature-brief · boot: on-demand

**Usage is what each bot's runs add up to, week by week, read like any other object.** It answers the questions a manager asks about the bots below it — how often each one compacted, how much it was served, which verbs it called and which refused it — from one ordinary read.

## Computed when read, held nowhere

**A usage record is worked out from the history of runs at the moment it is read.** Nothing stores it, so nothing about it can go stale, and no verb can write one: a write to a usage handle is refused with the read that answers it. It is runtime state shown as an object, the way a system shows its processes as files.

**One record answers one bot for one ISO week.** The week is the unit because it is the span a person compares — this week beside last — and a day is too noisy to compare and a month too slow to notice.

## Counted where it happened

**Everything is counted on the day its own run was in**: the day the run stated, else the server's clock in the run's zone, never the machine's. A run acting out an earlier stretch of time is counted in that stretch, so a story that covers months in minutes reads back the months.

**A week carries** the runs begun in it, the compactions its runs reported and what they set back, the characters served on each of its days, and the calls to each verb with the refusals among them by who fixes them. **A thing that never happened has no key**, so a bare key asks whether it ever did.

**A count starts on a day, and the record says which.** Each bot's week names the first day its serving was counted and the first day its calls were. A week before that day reads as not counted, never as zero, because "nobody measured" and "nothing happened" are different answers.

## Who may read it

**A bot reads its own weeks and those of every bot below it on the chart, and the head of the chart reads all.** The rest are counted as withheld rather than hidden, so a reader knows there is more it may not see. This is the chart's ordinary meaning — a manager sees its reports — and it needs no grant of its own.

## What it is not

**Not a log of calls.** It keeps counts, never the calls themselves: no arguments, no text, no times. A record of each occurrence is a separate stream of events.

**Not a limit.** Nothing here refuses, slows or warns a bot. A threshold that tells somebody is a tripwire over these numbers, built apart from them.

**Not pushed at anybody.** It answers when asked.
