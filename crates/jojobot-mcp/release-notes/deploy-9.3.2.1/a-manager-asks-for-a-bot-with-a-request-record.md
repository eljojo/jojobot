title: A manager asks for a bot to run by writing a request record
for: managers who want a bot below them running or stopped, and any role that reads such a record
reach_for_it_when: you want a bot below you to run or to stop, or a write to a record that asks for one is refused and you do not know why
miss: A manager has no record to ask with, and a bot that writes the target, a status key or the generation of a request gets a refusal it cannot explain.

## What you can now do

Make a record of the kind `bot-request` to ask for a bot to run. It has two halves.

The spec says what should run. A bot above the target on the chart writes it, any manager up the target's reporting chain and not only the direct one: `target_bot` (the bot), `desired` (whether it should run: write `running` or `stopped`, jojobot does not check the value), `machine`, `model` and `bound_app` (the name of the one app that reports on the request). A key you leave off is not in the spec. `target_bot` is fixed once it is set, and nobody moves it. The target does not write its own request.

jojobot works out `generation`. It goes up by one each time the spec changes, and a resend of the same spec does not raise it. Nobody sends it.

The status says what runs. Only the app named in `bound_app` writes it: `observed_state`, `observed_generation`, `runtime_ref` and `last_result`. No bot writes these, a manager included. Read them with `recall` of the request. The app has acted on your latest spec when `observed_generation` equals `generation`.

A write to the wrong half is refused with the code `KeyNotYours`. For a spec key the refusal names the bots that may write it. For a status key it says no bot does, and for `generation` it says no one does. Other claims about the request are still yours to add: a claim that sets none of these keys is not refused, so you can leave a note beside the request, and you cannot change what the app wrote.

## Example

Smithers manages Milhouse. Smithers calls `add_entity` with kind `bot-request`, handle `bot-request:gamma`, and `sets` of `target_bot: bot:milhouse`, `desired: running` and `bound_app: watchbot`. The request holds `generation` 1. The app watchbot later writes `observed_state` and `observed_generation: 1` on it.

Smithers sets `desired: stopped`, and `generation` becomes 2. Until watchbot writes `observed_generation: 2`, Smithers knows the app has not acted on the stop yet. Milhouse tries to set `desired: running` on his own request and gets `KeyNotYours` that names `bot:smithers`. Marge's bot tries to write `observed_state` and gets `KeyNotYours` that says only the bound app writes it. Homer's bot sends `generation: 9` and gets `KeyNotYours` that says no one writes it, because jojobot works it out.

## What to do

To ask for a bot below you, make the request with `add_entity` and set only spec keys. Never send `generation`. Compare `observed_generation` with `generation` before you take the bot to be running or stopped. If you are the target of a request, ask your manager to change it. Do not write the status keys, whoever you are.

When a spec write comes back `KeyNotYours`, read the bots it names and ask one of them. If it names none, the request already holds its `target_bot`, which is fixed, or no manager is recorded above the target: place the target under a manager with its `reports_to`, and that manager can write the spec.
