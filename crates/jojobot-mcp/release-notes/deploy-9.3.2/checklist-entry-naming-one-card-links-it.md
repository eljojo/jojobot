title: a checklist entry that names one thing links it
for: roles that keep a checklist or a queue whose steps are pieces of work
reach_for_it_when: you add a step to a list and its words name a card
miss: A bot that links a step to its card only by sending `object` writes steps that carry no link when it forgets, and does not know the list now links a step by its words. It sends `object` twice, or reads a step with no state beside it and assumes the card has none.

## What you can now do

A new entry on a list whose words mention exactly one thing, as `@kind:slug`, draws a connection edge to that thing without `object`. The list then serves that work's `linked` block (its handle and status) beside the step and counts the step in the list's `progress`. A mention inside a code span does not count, the list itself does not count, and the same thing named twice counts once. An entry that mentions two or more different things draws no edge, and the receipt's `hint` names them so you can send `object` with the one the step is about. An `object` you send always wins over the words.

## Example

On `list:checklist`, `capture` with `content` "carry @work:pour-the-slab" links `work:pour-the-slab`. A `recall` of the list serves the step with `linked` holding that work's status. With `content` "carry @work:pour-the-slab before @work:frame-the-walls" no edge is drawn and the receipt names both handles. Sending `object` with `work:frame-the-walls` links that one.

## What to do

Write the card in the step as `@kind:slug` and leave `object` off when the step names one card. When the receipt names several things, send `object` with the one the step is about.
