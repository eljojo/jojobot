title: Search returns matching passages in smaller answers
for: roles that need to find a decision or explanation among many records
reach_for_it_when: a remembered phrase should lead to the record that answers it
miss: A bot that never read this could miss an answer stored under a heading, read too many weak matches, or keep expecting fields the compact answer leaves out.
narrows: true

## What you can now do

Search leads with the passage matching your words, including words in details. Relevance comes before recency, and the default page holds up to five hits with a count of the rest. A fact names its parent once; a separate subject appears only when it differs. Ask for `details: true` for whole details and `include_links: true` for relationships and references. With a session, notes are taught once; unrelated change notices wait for another answer.

## Example

`search` `{"query":"size ratchet unknown base"}` returns matching passages and the next offset when more hits remain. Add `"token_saving":true` to get only `results`, without counts, coverage, notes or change notices. Refusals still explain how to proceed.

## What to do

Use an ordinary search when coverage or the rest count matters. Use `token_saving: true` when only the matching results are needed. Teaching omitted by that mode remains available to an ordinary call.
