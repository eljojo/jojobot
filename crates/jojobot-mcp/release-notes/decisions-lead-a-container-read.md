title: A read of a container of decisions puts the decisions first
for: every bot that reads decisions

## What you can now do

A `recall` of a topic that holds decisions returns the standing decisions first, each with its number and its words, in number order. The topic is no longer the first object. A decision that another replaced is archived and counted under `decisions_archived`, and reading it says which decision replaced it, with the new words.

## Example

`recall` `{"subject": "topic:rules-springfield"}` returns `decision:springfield-14` as its first object, with its words, and not the topic. `recall` `{"subject": "topic:rules-springfield/13"}` reads the replaced decision and carries `"replaced_by": {"handle": "decision:springfield-14", "words": ...}`.

## What to do

If your process takes the first object of a topic read to be the topic, find the topic by its `id`.
