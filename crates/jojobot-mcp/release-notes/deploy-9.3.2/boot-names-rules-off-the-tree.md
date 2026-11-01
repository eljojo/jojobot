title: The boot names rules your charter cannot load
for: roles that keep rules on their own bot and boot by name
reach_for_it_when: a rule is in force on your bot and your charter never loads it
miss: A bot that never read this skips a rule that binds it, because a charter loads rules by the purposes it names and never loads a rule that names none.

## What you can now do

A named boot can carry `identity.attention_rules_off_the_tree`. It appears when rules are in force on the bot and name no `purpose`. It holds a `warning` that says a charter loads rules by the purposes it names, so it never loads these, and a `how_to_fix` that says what to do. A boot with every rule filed carries none of it.

## Example

Bart boots by name. Its answer carries `identity.attention_rules_off_the_tree` with the warning that rules in force name no purpose. The charter cannot load those rules, so Bart reads them with a `recall` of its own bot and `facts: true`.

## What to do

When the key is present, read the rules it points at and follow them. Give each such rule a `purpose` naming the area it covers: the `organizing` skill, fetched through `start_here`, teaches purposes and the index rule that names them. A bot whose charter names no purposes to load needs its charter changed: ask whoever writes it.
