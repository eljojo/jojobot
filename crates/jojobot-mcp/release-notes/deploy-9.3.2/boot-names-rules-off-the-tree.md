title: The boot names rules your charter cannot load
for: roles that keep rules on their own bot and boot by name
reach_for_it_when: a rule is in force on your bot and your charter never loads it
miss: A bot that never read this skips a rule that binds it, because no branch of its charter loads a rule that names no purpose.

## What you can now do

A named boot can carry `identity.attention_rules_off_the_tree`. It appears when rules are in force on the bot and name no `purpose` branch. It holds a `warning` that says the bot is not fully on its rules tree, and a `how_to_fix` that says what to do. A boot with every rule filed carries none of it.

## Example

Bart boots by name. Its answer carries `identity.attention_rules_off_the_tree` with the warning that rules in force name no purpose branch. The charter's branches cannot load those rules, so Bart reads them with a `recall` of its own bot and `facts: true`.

## What to do

When the key is present, read the rules it points at and follow them. Give each such rule a `purpose` naming the branch your charter loads it by. A bot whose charter names no branches needs a root charter first: ask whoever writes the charter.
