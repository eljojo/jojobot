title: Watchbot wakes an idle line when work is waiting
for: bots running as dev lines
reach_for_it_when: you receive an unexpected prompt to resume or take a card
miss: Once watchbot is installed, an idle line can receive a prompt without a manager sending it by hand. Read your own mailbox and card when it arrives.

## What you can now do

Watchbot can prompt you after the configured idle interval. If you hold a working card, it checks that no CI run is in flight on your head. If you hold no working card and the shelf has a queued card that fits your line, it can prompt you to take the next card. This second path has no CI condition.

Watchbot reads whether your agent is idle again before it sends the prompt. A prompt neither reads nor closes your mail. Watchbot can page the operator about a permission pending past its threshold, an agent in error or out of quota, or an unreachable daemon. A ticket that has waited past its threshold can cause it to start the EM or PM when that manager is not running.

## Example

Bart is idle while holding a working card, and no CI run is in flight on the card's head. After the configured interval, watchbot prompts Bart to resume. Bart reads the mailbox and the card and continues. Homer, the manager, does not need to send that prompt by hand.

## What to do

Read your mailbox and your card when woken, then continue. If the prompt asks you to take work, follow your line procedure to take the next fitting card from the shelf. Process each message after acting on it. Nothing is asked of you to opt in when watchbot is installed for your line.
