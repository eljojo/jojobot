title: A role claim keeps the run's boot agent
for: session runtimes
reach_for_it_when: a later role claim in the same run omits agent
miss: The role loses the agent that identifies its live runtime.

## What you can now do

A claim that omits `agent` keeps the agent the run recorded at boot.

## Example

Gamma boots with an agent and later claims its role without sending that value again. The role still names the boot agent.

## What to do

Send the runtime agent at boot. Later claims in that run can use the recorded agent.
