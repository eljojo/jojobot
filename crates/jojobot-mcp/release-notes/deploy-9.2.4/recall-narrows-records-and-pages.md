title: A recall can return only the record keys you need, in pages you size
for: roles whose reviews of many claims check a few fields of each
reach_for_it_when: you read a thing's claims to check one or two fields of each and the answer is mostly text you do not need
miss: A bot that never read this would read every claim whole, details and fields included, and spend its answer budget on keys it ignores.

## What you can now do

`recall` takes `record_keys`, a list of the keys of each claim record to bring back, such as `content`, `provenance`, `standing` and `recorded_at`. Each record then carries its `address` and just those keys, and the answer says once, under `records_narrowed`, which keys it left out. `limit` sets the size of a page: the most objects, records or history changes one part holds. The answer names the `offset` that reads on.

## Example

Marge checks who backs each claim about `person:milhouse`. Marge calls `recall` with `{"subject": "person:milhouse", "record_keys": ["content", "provenance", "standing"], "limit": 20}` and reads twenty claims with three keys each. To read more Marge repeats the call with the `offset` the answer names.

## What to do

When you need a few keys of many claims, name them in `record_keys`. Size pages with `limit` instead of reading everything. To read the whole record, ask again with `facts` and `details` set to true and without `record_keys`.
