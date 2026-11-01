title: declare_type needs a session, and a bot cannot be created beside the manager it names
for: roles that declare types for an instance or create bots under a manager
reach_for_it_when: declare_type or a bot's creation comes back refused when it landed before
miss: A declare_type call with no session is refused, and so is a creation by a bot whose manager is a bot beside it on the chart. Both landed before. Start a session with start_here and send its sid, or create the bot under yourself or a bot below you.
narrows: true

## What you can now do

`declare_type` is now an identified write, as every other write that names its author is. Start a session with `start_here` and send its `sid` with the call: the declaration lands as it did.

A bot is created under yourself or under a bot below you on the chart. The refusal names the manager it would not accept.

## Example

Homer manages Bart and Lisa. Bart declares a type with no `sid`, and the call is refused with a way forward that names `start_here`. Bart starts a session, sends its `sid`, and the declaration lands. Bart then tries to create a bot under Lisa, who sits beside him: that is refused, and Bart creates it under himself.

## What to do

Send the `sid` of your session with every `declare_type`. If a creation is refused for its manager, name yourself or a bot below you as the new bot's manager.
