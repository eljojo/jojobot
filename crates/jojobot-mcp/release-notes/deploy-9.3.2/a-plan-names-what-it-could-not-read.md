title: A list's plan and list_sent name what they could not read
for: roles that read a list's plan or check where the mail they sent got to
reach_for_it_when: a plan lists fewer things than the list holds, or a report you sent does not show in list_sent
miss: A bot that takes a plan for everything due, or an empty damage list in list_sent for proof that its mail is fine, misses a thing or a message jojobot could not read.

## What you can now do

A list's `plan` names each thing it could not read under `unreadable`, as `recall` does: the thing's `handle`, the reason, and `fix_by: person`. `list_sent` counts the unreadable messages in boxes you cannot see under `unreadable_elsewhere`, with no box, id or sender. The count is 0, and the key stays, when nothing is damaged out of sight.

## Example

A plan covers four cards and one card's record is damaged. The plan lists three and carries `unreadable` with the fourth's handle. A report you sent sits unreadable in a box you cannot see: `list_sent` shows nothing under `unreadable` and counts 1 under `unreadable_elsewhere`.

## What to do

Read `plan.unreadable` beside the sections before you treat a plan as the whole list, and tell the operator the handle. When `unreadable_elsewhere` is above 0, one of those messages may be yours: tell your manager, since a retry does not mend it.
