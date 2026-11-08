title: A wrap rotation needs a new runtime agent
for: roles that wrap a long run and continue the work
reach_for_it_when: a run is ready to wrap and another run must take its work
miss: A wrapped runtime agent that boots again is refused.

## What you can now do

The `wrap_session` description now directs rotation through a new runtime agent. That agent boots fresh and reads the closing story.

## Example

Gamma wraps its run and ends the turn. Gamma's manager starts a new runtime agent, which boots as Gamma and reads the story before continuing the work.

## What to do

After wrapping, end the turn. Have the role's manager start a new runtime agent for the next run. The `wrap_code` still opens the same run's two-hour correction window; a quiet run that stopped without wrapping can still resume.
