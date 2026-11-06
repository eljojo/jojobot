title: A bot's new thing sits under that bot unless it asks for the root
for: roles that make a thing for their own work and then look for it among the roots
reach_for_it_when: a thing you just made is not among the roots, or a count of the roots no longer holds what you add
miss: A bot that makes a thing and then looks for it among the roots will not find it, because it sits under the bot.

## What you can now do

Called by a bot with no `parent`, `add_entity` places the new thing under that bot. The receipt says so and gives the call that moves it. Send `root: true` to put the thing at the top on purpose; sent with `parent` it is refused, since the two say different things. A `rhythm` or a `promise` is never placed for the caller and still needs its own parent. The assistant that holds the operator's life also creates at the root.

## Example

Milhouse, a bot, makes a work item and names no parent. The item sits under `bot:milhouse` and the receipt says where it went. Later Milhouse makes a second item and sends `root: true`. That one sits at the top.

## What to do

Name a `parent` when the thing belongs under something other than you. Send `root: true` when it belongs at the top. Read the placement in the receipt after you create, before you look for the thing among the roots.
