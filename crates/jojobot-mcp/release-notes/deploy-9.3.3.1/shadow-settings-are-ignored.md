title: Shadow settings are ignored
for: deployment maintainers
reach_for_it_when: you set JOJOBOT_SHADOW or feature_shadow to request comparison reads
miss: The settings are mistaken for active controls.

## What you can now do

`JOJOBOT_SHADOW` and `feature_shadow` on `topic:instance` are ignored silently.

## Example

A deployment sets `JOJOBOT_SHADOW`. The server makes no shadow comparisons and emits no warning about the setting.

## What to do

Remove these settings from deployment configuration and instance setup.

