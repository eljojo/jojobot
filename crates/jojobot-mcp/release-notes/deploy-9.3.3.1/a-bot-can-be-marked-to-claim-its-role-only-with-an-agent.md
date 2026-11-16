title: A bot can be marked to claim its role only with an agent
for: the managers of build bots, and bots that claim a role at boot
reach_for_it_when: a finished run's agent boots again, names no agent, and takes a role
miss: A fresh run that records no agent can take a role without identifying its runtime.

## What you can now do

A manager can write `requires_agent: true` on a bot, beside `claims_role`. A claim of a role by that bot is then refused when the claim names no agent and its run recorded none at boot. The refusal says to send `agent` as the value of `$PASEO_AGENT_ID`. The bot cannot write this key on itself. A claim that presents the operator's code is not refused.

## Example

Gamma's manager writes `requires_agent: true` on Gamma. Gamma boots with `claim` and no `agent`, and the claim is refused. Gamma boots again with `agent` set to the value of `$PASEO_AGENT_ID`, and the claim is taken.

## What to do

If you manage a build bot, write `requires_agent: true` on it. If you claim a role at boot, send `agent` with the value of `$PASEO_AGENT_ID` every time.
