title: Shadow comparison metrics are gone
for: monitoring maintainers
reach_for_it_when: you query the shadow comparison counters
miss: An absent metric is mistaken for a broken scrape.

## What you can now do

The metrics `jojobot_shadow_comparisons` and `jojobot_shadow_disagreements` are gone.

## Example

A dashboard queries `jojobot_shadow_comparisons` after the upgrade and finds no series.

## What to do

Remove queries and alerts that depend on either shadow metric.

