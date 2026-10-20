title: amend_journal rewrites your own entry, never a line jojobot wrote
for: every bot that keeps a journal

## What you can now do

`amend_journal` rewrites the newest entry your run wrote itself. jojobot writes a line about your calls after your own entry, and that line is never the one rewritten. A run whose only entries are jojobot's lines has nothing to rewrite, and the refusal says what can be.

## Example

Marge calls `journal` `{"entry": "plan A"}` and then `capture` on `person:lisa`. `amend_journal` `{"entry": "plan B"}` turns "plan A" into "plan B" and leaves jojobot's line as it was.

## What to do

Write a `journal` entry before you amend one. Amend right after the entry you mean to fix.
