title: A field value that names a handle is a link, and one that names nothing is refused
for: roles that write the handle of one thing into a field of another
reach_for_it_when: you record which thing a record points at, or waits on, in a field you chose yourself
miss: A bot that never read this would write a handle for a thing it has not created yet and have the write refused, or expect a list that mixes handles and words to stay plain text.

## What you can now do

A field value whose whole text is a handle, such as `person:lisa`, is a link, whatever the key is called and whether or not any type declared the key. In a comma-joined list, every item that is a handle is a link and the other items stay text. A handle inside a longer sentence is text, and it is not asked about. The link is stored on the permanent id, so a rename of the thing changes what the field reads as and rewrites no stored text. Declaring a type afterwards neither makes a link nor removes one.

Every handle that is a link must name a thing that exists. A write with a field that names nothing is refused with `status: blocked` and `wrote: false`, the answer names the handle, and nothing is written. The write lands when the handle names a thing that exists.

## Example

Marge calls `capture` with `{"subject": "work:phi", "content": "who this waits on", "provenance": "testimony", "fields": {"blocks": "person:lisa,person:ralph"}}`. If `person:ralph` does not exist, the write is refused and names `person:ralph`. Marge creates it with `add_entity`, sends the same call again, and both `person:lisa` and `person:ralph` are links. A value such as `read person:ralph before friday` is text, and the write lands with no check.

## What to do

Create a thing with `add_entity` before you write its handle into a field. Put one handle in a field, or a comma-joined list of handles, when you mean a link. Write a sentence when you mean text.
