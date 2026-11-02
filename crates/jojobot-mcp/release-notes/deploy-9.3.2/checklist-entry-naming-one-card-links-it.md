title: a checklist entry that names one thing links it
for: roles that keep a checklist or a queue whose steps are pieces of work
reach_for_it_when: you add a step to a list and its words name a card
miss: A bot that links a step to its card only by sending `object` writes steps that carry no link when it forgets, and does not know the list now links a step by its words. It sends `object` twice, or reads a step with no state beside it and assumes the card has none.

## What you can now do

A new list entry whose words mention exactly one thing, as `@kind:slug`, draws a connection edge to it without `object`. The list then serves that work's `linked` block (handle and status) beside the step and counts the step in its `progress`. A mention in a code span does not count. An entry that mentions two or more things draws no edge, and the receipt's `hint` names them. An `object` you send always wins.

## Example

A `capture` on `list:checklist` with `content` "carry @work:pour-the-slab" links that work. With "carry @work:pour-the-slab before @work:frame-the-walls" no edge is drawn and the receipt names both; `object` picks one.

## What to do

Write the card in the step as `@kind:slug` and leave `object` off when the step names one card. When the receipt names several, send `object` with the one the step is about.
