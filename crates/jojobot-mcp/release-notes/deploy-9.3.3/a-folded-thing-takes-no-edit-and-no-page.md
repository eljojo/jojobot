title: A thing folded into another takes no edit and no page
for: roles that keep editing a thing by a handle it had before it was folded
reach_for_it_when: an edit or a page you wrote to a handle changed nothing you can read, or the handle now answers that it was folded
miss: A bot that keeps writing to the folded handle will be refused, because an edit and a page on a folded thing are refused now and the thing lives on under its survivor.
narrows: true

## What you can now do

`update_entity` and the page write of `set_charter` refuse a handle that was folded into another thing. The refusal names the survivor. Before, these writes changed the forwarding row and nothing a reader sees. Archive, restore and rename already refused such a handle.

## Example

Milhouse's bot folded `thing:handcart` into `thing:kettle` last week and still renames the handcart. The rename is refused and the answer names `thing:kettle`. Sent to `thing:kettle`, the rename lands.

## What to do

When a write is refused for a folded handle, send it to the survivor the refusal names. Read the survivor with `recall` to see the thing as it stands.
