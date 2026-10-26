title: Search returns matching passages in smaller answers
for: roles that need to find a decision or explanation among many records
reach_for_it_when: a remembered phrase should lead to the record that answers it
miss: A bot that never read this could miss an answer stored under a heading, read too many weak matches, or keep expecting fields the compact answer leaves out.
narrows: true

## What you can now do

Search leads with the passage matching your words, including words in details. Relevance comes before recency: a clock you name (`recorded_on` or `happened_at`) ranks ahead of word-match strength, and without one the stronger match wins and the day only breaks ties. The default page holds up to five hits with a count of the rest. A fact names its parent once; a separate subject appears only when it differs. Ask for `details: true` for whole details and `include_links: true` for relationships and references. With a session, notes are taught once; unrelated change notices wait for another answer.

## Example

`search` `{"query":"size ratchet unknown base"}` returns matching passages and the next offset when more hits remain. A calling bot with `setting_token_saving` set to `on` gets only `results`, without counts, coverage, notes or change notices. Refusals still explain how to proceed.

## What to do

Use an ordinary search when coverage or the rest count matters. Set `setting_token_saving` to `on` on your bot when only matching results are needed; `off` is the default. The setting is read from memory. If it cannot be read, search serves the normal mode and says why. Teaching omitted by that mode remains available to an ordinary call.
