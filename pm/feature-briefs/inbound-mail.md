# Inbound mail — an address with a meaning, delivered to one bot's box

> **jojobot** · kind: feature-brief · boot: on-demand

**The operator writes an email to an address that means something, and it arrives as a message in the one bot's box that address is for.** `reminders@`, `work@` and `assistant@` are not aliases for bots; each is a purpose the operator named, and the address decides which bot hears it.

## An address is a thing, and it names its bot

**Each address the instance receives on is its own object**, filed under the webhook that brings its mail in. It carries the address, the bot whose box it delivers to, and a line saying what it is for. **A bot has no address unless somebody gave it one**, because the address carries the meaning and the bot is only where that meaning is handled. Two addresses may deliver to one bot; an address never delivers to two.

**Mail to an address the instance does not hold is refused**, never routed by guessing from the part before the `@`. A guessed route turns a typo into a message in the wrong box.

## The webhook is data

**A webhook is an object too:** the path the instance listens on, the kind of notification it accepts (a mail relay's signed notification), the one topic it trusts, and the senders whose mail it accepts. The instance's own account, domain and topic live on that object, so the software names none of them and a second instance configures its own.

**The path is not the lock.** Anyone can learn a URL. **What makes a delivery real is the relay's signature**: every notification is verified against the relay's own signing certificate, fetched over HTTPS from the relay's domain, and a notification from any topic but the trusted one is refused. A subscription confirmation is verified the same way before it is confirmed.

## Only the people the webhook names, and only when the mail proves it

**The webhook lists who may write in, each as a sender address linked to a person jojobot knows.** At first that is the operator alone.

**A From line proves nothing; anyone can type one.** So a message counts as from a listed sender only when the relay reports that the sending domain authenticated it (DMARC passed). **Then it arrives as that person's own word**, the way anything the operator says is their testimony.

**Everything else is quarantined, never delivered:** an unlisted sender, a failed or missing authentication, a spam or virus verdict. A quarantined message is kept with the reason, visible to the operator, and handed to no bot. **A box is something a bot acts on, so the open internet must never reach one.**

## What a message becomes

**The subject stays the subject. The plain-text part becomes the body**, or the formatted part converted to text when there is none. The message records which address it came through, so the bot reading it knows which purpose it was sent to. **An attachment is named, never silently dropped**, and its content is not carried.

**The relay hands over a whole message only up to its own size limit**, and larger mail never reaches the instance. That limit belongs to the relay, and the way past it is the relay's storage, not a bigger message.

## What it is not

* **Not a mail client.** jojobot receives; it does not send or reply by mail.
* **Not an address per bot.** Addresses exist because the operator gave them a purpose.
* **Not open mail.** Nobody outside the webhook's list reaches a bot, however plausible the message.
* **Not a door for bots.** Bots still reach jojobot only through its own surface; this door is for a person writing in from outside.
