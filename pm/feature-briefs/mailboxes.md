# Mailboxes — how one bot reaches another

> **jojobot** · kind: feature-brief · boot: on-demand

**A place to leave a message for somebody who is not in this conversation.** Bots are colleagues (*Bots and identity — colleagues in an organisation*), and a box is how one reaches another.

## You address the bot, not the box

**A message's recipient is a bot handle.** A box exists underneath it, and it is not a noun a caller holds a model of.

**A bot has exactly one box, always** — the unit of correspondence is the bot, and you scale by adding bots rather than by splitting a box. So a box name carries no addressing power the handle does not already carry, and asking a caller for one teaches them a second vocabulary for the same thing.

**Identity does the addressing on both sides.** The session handle says whose box a read takes from; the recipient handle says whose box a write lands in. Neither takes a box name.

**An unknown handle comes back blocked with candidates, and nothing is written.** The candidates are names a caller already knows from the directory.

## The operator's box

**The operator is a person, and theirs is the one box a person owns.** A bot writes to the operator by the person handle the instance names as its operator. The box opens inside the first such post, which makes it the one box that does not open with its owner. **No bot reads it, in any state:** no bot takes delivery of it, retires or quarantines its mail, or finds it by search, and a search over mail says it never reaches that box. A writer sees where its mail got to by id, time and subject only. **A post to any other person is refused**, saying that only the operator has a box. The operator reads and finishes their mail on a surface of their own.

**A bot quotes the operator by address, never from memory.** Writing `@kind:slug#fN` in a post to the operator cites a record of their words. When the post is accepted, the address must exist, be readable to the writer and be testimony, or the post is refused, naming the address and the valid form. The words are expanded once and stored with the citation, so a later correction of the source never changes what an old message says was sent, and the body is measured against the ceiling after expansion. **Plain quotation marks are left alone**, because punctuation is not attribution: it wraps titles and other people's speech too. The convention is taught once, on a session's first post into the operator's box. **The guarantee is narrow:** an inference cannot be handed back as the operator's testimony. A record's content is a claim, not a transcript, so a quote by address does not prove verbatim speech.

## Three states, and the middle one is not the last one

`new` → `read` → `processed`.

* **Reading is taking delivery.** A read moves messages out of `new` and they become yours to finish, so no caller sees a body without owing it. **Counting is the separate, free question:** a caller may ask what is waiting and take delivery of none of it, which is what makes checking a box cheaper than deciding whether to check.
* `**read**` **is not** `**processed**`**.** Reading takes delivery; processing means you acted.
* `**processed**` **is terminal — an archive, never a deletion.** Nothing on the surface deletes.

**A delivery that was interrupted comes back.** Messages a previous read already handed over return flagged, still counted and still owed, with their bodies left out because they were shipped once already. That is what makes the box trustworthy to a consumer that crashed.

**A message jojobot cannot read is reported apart, never dropped.** A box that quietly omits what it cannot parse hands back a count that is a lie, and being countable is the one property a mailbox cannot do without.

⚠️ **The asymmetry worth knowing:** the contract pulls hard against marking something processed too early and not at all against marking it too late. So `read` is the state that looks handled while owing everything. The surface does not let that stand: the stance that processing means you acted is kept, and what stops is making late closing free and closing at the right moment costly.

## Closing mail costs what reading it does

**Reading is free, so closing is made as cheap, and the act that already settles a message closes it.** None of this closes mail by age: a message nobody acted on stays open, which is the point of the state.

* **Many at once.** `mark_processed` takes `message_ids` for up to 200 messages in one call. A refusal is all or nothing: one id that names nothing, is in another box, is unreadable or is in a person's box refuses the call, names that id and closes none. A store failure part-way is not a refusal: the messages already closed stay closed and the same call sent again closes the rest. A message already processed is counted and left alone, so a resend is safe.
* **A reply closes what it answers.** A message that names `in_reply_to` a message in the poster's own box processes that message, with a note naming the reply. A reply to a message in another box changes nothing there. A close a post could not make is named in its receipt under `closed.failed`, never passed over as nothing to close.
* **A sender's newer word closes its older ones the reader has read.** A message answering into a thread processes the same sender's earlier messages on that thread in the box it lands in that are in state `read`. A message still `new` is left open: it was never handed over, and `supersedes` replaces an unread message by name.
* **Leftovers are a count and the oldest five ids**, with how many ids were left out, because naming every id charged the whole pile on every poll.
* **A pile is said once.** At twenty messages read and not closed the status bar says so, once per session and again after a compaction: close what you acted on with one batch call, and if the pile is real work, tell your manager.

One function does the processed transition for all of them, so a closed message reads the same whichever act closed it. A person's box is moved by that person alone: no post closes anything there.

## Posting takes delivery

**When a bot posts, it receives whatever is waiting for it, in the same call.**

It is aimed at two bots corresponding, each holding a reply it has not seen (*The status bar — telling a caller what it did not ask about* carries why). **The moment a bot posts is precisely when a reply is most likely to be waiting**, because posting is what it does at the end of a piece of work.

⚠️ **A sender's view has to stay honest about this.** A message that left `new` because its recipient posted something unrelated is not a message somebody picked up, and the two are reported apart.

## Mail wakes a managed bot

**A post to a bot whose session is managed wakes it.** The hypervisor service sees the post on the mailbox's change feed and its plugin wakes the agent at its next idle moment (*Lines — a bot asked to run, and the software that keeps it running*). **The feed and the wake carry no body and close nothing**: the bot still reads its own box and decides what each message asks of it. A plain session is not woken, and a sender can see which kind its recipient runs, so it knows whether a nudge by hand is still owed.

## One reader per box

**Where a bot has two live sessions, the newest takes delivery and closes mail; the older is fenced from both and told so.** Two readers of one box would each owe what the other took. Why two workers are two bots is *Sessions — one mortal run of a bot*.

## What it is not

**Not a chat.** A message is left for somebody who is not here; nobody waits on it. A bot reaches the operator only through the operator's own box, which no bot reads.

**Not a store of record for decisions.** A ruling that stays in a mailbox is a ruling that gets lost — it belongs in the decision log.

**Not deletable.** A message sent in error stays in the archive, which is the cost of an archive that can be trusted.