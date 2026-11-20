# Native guidance click investigation — 0.15 candidate

Read-only investigation by content_increment_015 at parent request. No display control, source changes or Cargo invocations.

## Observation and resolution

The initial captured Activities screen showed an enabled `Accept this supply request` action for four wood at Riverside sawmill. Parent reported two 0.65-second xdotool click holds, first in RTS and then in management focus, followed by screenshots that still showed the unaccepted goal. Screenshots examined:
- `/tmp/terra-native/v15-candidate-world-ready.png`
- `/tmp/terra-native/v15-candidate-supply-accepted-focused.png`

Parent subsequently observed `/tmp/terra-native/v15-candidate-guidance-keyboard-focus.png` with accepted guidance at step 3: four wood ready and a target about 176 metres away. The click therefore did apply. Parent also observed five rapid Down presses coalescing to one focus step on this software-rendering host.

No product defect was demonstrated. Treat the initial lack of visible response as a host/input-timing and settled-frame limitation. Parent will allow more rendered frames and use a smaller native window for follow-on review. Do not classify the initial screenshot as a failed supply-acceptance result.

## Source trace

- Native button observes `Activate`, enqueues the exact action into the bounded `UiQueue`, and the ordinary local interaction dispatcher submits the returned command.
- `FrontierAction::ContinueJourney` derives fresh guidance and compares the command intent and player/restore context against the displayed cache. For this stable request the intent is only `AcceptContract { contract_id }`; ordinary clock/carry-count text changes cannot themselves invalidate it.
- The UI cache updates at a 0.2-second presentation cadence; resulting visible pixels also depend on the host rendering frames.
- Local `SandboxQueue` processing retains intents while a regional persistence worker owns a revision. It requeues commands requiring hydration until hydration completes; it does not silently discard them.
- Regional autosave and command results share `sandbox.last_message`. Autosave could replace an earlier toast, but this investigation did not reproduce an obscured rejection, and the accepted-contract result disproves rejection for this click. No new toast design or broad polish scope was introduced.

Reviewed files: `terra-demo/src/{frontier,interface,interaction,command_input,control}.rs`, `terra-bevy/src/sandbox.rs`, `terra-sandbox/src/guidance.rs`, and the installed Bevy 0.20 headless Button widget source.
