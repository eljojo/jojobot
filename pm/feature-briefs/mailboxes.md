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

⚠️ **The asymmetry worth knowing:** the contract pulls hard against marking something processed too early and not at all against marking it too late. So `read` is the state that looks handled while owing everything.

## Posting takes delivery

**When a bot posts, it receives whatever is waiting for it, in the same call.**

It is aimed at two bots corresponding, each holding a reply it has not seen (*The status bar — telling a caller what it did not ask about* carries why). **The moment a bot posts is precisely when a reply is most likely to be waiting**, because posting is what it does at the end of a piece of work.

⚠️ **A sender's view has to stay honest about this.** A message that left `new` because its recipient posted something unrelated is not a message somebody picked up, and the two are reported apart.

## Two sessions, one box

**Where a bot has two live sessions they share its box and sort it out between themselves.** Why two workers are two bots is *Sessions — one mortal run of a bot*.

## What it is not

**Not a chat.** A message is left for somebody who is not here; nobody waits on it. A bot reaches the operator only through the operator's own box, which no bot reads.

**Not a store of record for decisions.** A ruling that stays in a mailbox is a ruling that gets lost — it belongs in the decision log.

**Not deletable.** A message sent in error stays in the archive, which is the cost of an archive that can be trusted.