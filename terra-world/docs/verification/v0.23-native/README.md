# Native review — Terra World 0.23

This directory retains actual Linux/X11 screenshots and process records from
the 30 October 2026 review. The runtime uses Vulkan llvmpipe / LLVM 20.1.2,
Mesa 25.2.8, a software CPU renderer. Native evidence does not certify physical
GPUs, Windows, controllers/wheels, assistive technology or uncoached users.

**Current status:** direct native confirmation and its accepted provenance
audit **passed**. The actual terminal receipt completes **44 canonical gates**,
with **1,070 distinct Rust tests**, **1,171 Rust test executions** and **50 Python
tests**. Its accepted executable and 381 frozen implementation inputs match the
direct confirmation. The separately retained candidate journey is not relabeled
as final-binary execution.

## Method and association

Screenshots are captured from the actual Terra window. Each sidecar records
window dimensions, PID before/after capture and exact image hash/size. The saved
image is inspected before acceptance. The helpers refuse to overwrite prior
captures/process receipts. All worlds are isolated test data under
`/tmp/terra-native023`; user worlds are not used.

The baseline executable came from the exact saved 0.22 desktop ZIP, with SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
Candidate/final processes use the newly built executable. Source inputs tracked
by a baseline process describe the evolving current checkout, not its historical
binary: implementation edits during that run are reported honestly.

The first four capture receipts use `release: 0.23.0` for the review increment;
their baseline names, visible 0.22 label and PID 8340 bind the actual historical
executable. They are not current-release acceptance. Process receipts supply
the executable's release and identity.

## Fresh baseline

| Step | Capture | Observed state and health |
|---|---|---|
| 1 | [Activities](01-baseline-activities.png) | Current request, carried goods, reserved reward and next action are visible at 1440×900. Healthy entry; capacity/timing correctness requires source/functional checks. |
| 2 | [Nature](02-baseline-nature.png) | Resource stock/reach and food/water controls are visible. Generic consuming-actor wording is confirmed and motivates explicit identity. |
| 3 | [Territory](03-baseline-territory.png) | Empty-state explanation, founding costs and controls are visible. Existing layout remains the design baseline. |
| 4 | [Pause/save](04-baseline-pause.png) | Save/restore/settings/exit controls are reachable. Isolated baseline Save and quit completed with exit 0. |

Accepted baseline process: [baseline-ready-process.json](baseline-ready-process.json),
PID 8340. The checkout changed during independent implementation, so its record
correctly reports source-input change. The exact historical binary did not change.

## Runtime setup attempt retained

`baseline-play.log` and its process/start records retain an initial failed launch
before the fresh environment had a Vulkan driver. The log explicitly reports
that no driver was available. After installing the isolated software driver,
the new baseline-ready process above rendered and exited normally.

That first failed process record also has a helper-generation release-label
error (`0.23.0` despite the exact 0.22 binary path/hash). The helper was corrected
before the successful baseline process; the failed record remains unchanged and
cannot establish native acceptance. It is an environment setup attempt, not a
gameplay pass or a relabeled successful release.

## Candidate journey: create, earn, build, save and recover

The candidate was operated through its actual native controls. Captures 10–61
cover a continuous isolated journey, including **150% interface text**, the
1440×900 view and a reduced **1100×700** gameplay view.

| Steps | Representative captures | Observed outcome |
| --- | --- | --- |
| 10–21: launcher defaults and controls | [150% text](17-candidate-text150.png), [saved defaults](21-candidate-defaults-saved.png) | Graphics and controls were inspected, larger interface text selected and saved, and settings returned to the launcher flow. |
| 22–29: create a world | [character](22-candidate-character.png), [world review](28-candidate-world-review.png), [created world](29-candidate-create.png) | A local synthetic frontier world was created for Alex through the native setup flow. |
| 30–35: accept and travel | [accepted supply](34-candidate-accepted.png), [travel](35-candidate-travel.png) | Suggested guidance retained the accepted Riverside sawmill delivery and offered travel for the named actor. |
| 36–41: reduced view and delivery | [named consumption target](38-candidate-consumption-target.png), [delivery ready](40-candidate-delivery-ready.png), [completed supply](41-candidate-supply-complete.png) | At 1100×700, the guidance reached delivery of **4 wood**. Completion changed the wallet from **250.00 to 256.13 cr**, added **40 XP** and recorded **1 completed contract**. |
| 42–49: found and fund | [founded neighborhood](44-candidate-community-founded.png), [funding target](47-candidate-fund-target.png), [funded result](49-candidate-funded.png) | Founding **Review23** cost **50 cr** and added **10 XP**. Funding transferred a further **25 cr** to its treasury, leaving **181.13 cr**, **50 XP** and a **75.00 cr** treasury. |
| 50–56: save, return and rename | [save/return](51-candidate-save-exit.png), [renamed world](56-candidate-renamed.png) | The saved world returned to Your worlds and was renamed **My frontier Review23**. |
| 57–61: backup, verify and recover | [backup ready](57-candidate-backup.png), [separate-copy review](59-candidate-backup-verified.png), [both worlds listed](60-candidate-recovered.png) | A native backup at **checkpoint 23** was verified, then recovered as a separately named world while the original remained listed. |

The independent [candidate evidence audit](candidate-evidence-audit.md) and
[machine-readable audit](candidate-evidence-audit.json) verify all 52 PNG hashes,
byte lengths, formats, dimensions and capture chronology. There are 36 captures
at 1440×900 and 16 at 1100×700.

### Candidate association limit

[candidate-menu-process.json](candidate-menu-process.json) records launcher
PID **11234**, exit **0**, and executable SHA-256
`700120e44255afe4c482b3416bdcedaad4200d930012418aa0c36cda0281c7f0`.
Captures 10–28 and 52–61 match that PID directly.

Gameplay captures 29–51 independently record stable window PID **12147**. There
is no retained child process/binary/exit receipt or parent-log PID association.
The child's parentage, executable identity and exit status are therefore not
independently established; the launcher's zero exit is not substituted for the
child's exit. The candidate record reports 381 unchanged inputs but has no
retained source-map digest. These limits remain attached to the candidate even
after stronger final confirmation.

## Direct confirmation after the copy correction

The final candidate executable was launched directly for two confirmation
processes. Both records bind their launch and finish to the same **381-input**
canonical map digest and the same executable bytes:

- Source-map SHA-256, using the recorded canonical JSON encoding:
  `dd6d40911a1defed04d39a3a4287ae01ced1cfde27fdcbf0575caa404c39c5d5`.
- Executable SHA-256:
  `432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.

| Direct process | Captures | Recorded result and scope |
| --- | --- | --- |
| [Recovered-world confirmation](confirmation-recovered-process.json), PID **14073** | [70: reopened state](70-confirmation-recovered.png), [71: 1100×700](71-confirmation-minimum.png), [72: territory](72-confirmation-territory.png), [73: pause](73-confirmation-pause.png), [74: sky settings](74-confirmation-sky.png), [75: return to pause](75-confirmation-pause-return.png), [76: save/quit](76-confirmation-save-quit.png) | The actual recovered managed profile was opened directly, inspected, saved and closed with **exit 0**. Every capture matches that same process PID within its recorded lifetime. |
| [Menu confirmation](confirmation-menu-process.json), PID **14392** | [80: menu](80-confirmation-menu.png), [81: sky settings](81-confirmation-menu-sky.png), [82: return/exit](82-confirmation-menu-exit.png) | The final menu was launched directly, its settings inspected, and it closed with **exit 0**. All three captures match its recorded PID. |

The recovered world visibly retains one completed supply contract, 50 XP,
181.13 cr and the Review23 territory with 75.00 cr. The final settings copy
distinguishes the **effective sky state, Off under Performance**, from the
retained **On preference**. The small gameplay viewport keeps the activity and
territory panels usable, and the pause controls scroll to Save and quit.

The [native provenance validator](audit_confirmation_023.py) rereads every final
capture and sidecar, both start/terminal records and logs, the current native
executable, the current implementation map and the canonical run's frozen
`source-hashes.json`. The [accepted result](confirmation-provenance-accepted.json)
additionally rereads the actual completed canonical receipt, its identical alias,
all 44 gate logs and 36 accepted artifacts. The final receipt SHA-256 is
`12b4a3f72591c7431bbc0aeda6fe189c5650b81da29d29c7fa8ef955c903cfa0`.

The earlier [pending result](confirmation-provenance-pending.json) and
[missing-receipt refusal](confirmation-admission-refusal.json) remain intact as
their original observations. The [provenance narrative](confirmation-provenance.md)
describes the final association and its limits. Canonical source/log/test details
remain in the [actual terminal receipt](../v0.23-release-acceptance/completed-gates.json).

## Durable state and recovery checks

The independent [saved-state inspection](state-inspection.md) checks the
persisted journey separately from screenshots:

- [Original versus backup](candidate-original-backup-state.json): every logical
  schema tuple and table column matches at checkpoint **23**.
- [Deferred live inspection](candidate-state-inspection-deferred.json): a
  nonempty WAL caused refusal to claim a quiescent immutable read.
- [After direct recovered-world confirmation](confirmation-state-inspection.json):
  all three SQLite integrity checks passed; the recovered world advanced to
  checkpoint **35**, retaining one completed supply contract, 50 XP, 181.13 cr,
  one founded settlement and its 75.00 cr treasury.

The recovered simulation advanced during actual play, so exact full-state
equality with checkpoint 23 is intentionally not claimed after reopening. No
full private save payload was copied into this evidence directory.

## Reproduce the provenance check

The evidence-only validator requires Python 3.11+ and Pillow. It never launches
the game, rewrites a capture or replaces a prior report.

Rerunning it also requires the recorded native executable and matching checkout
to remain available at their recorded paths. An archived report remains evidence
of its observed run; extracting the source alone does not recreate those runtime
paths or authorize a new passed result. Use a new output path:

```sh
python3 -B docs/verification/v0.23-native/audit_confirmation_023.py \
  --phase accepted --output /tmp/terra-native-confirmation-accepted.json
```

Accepted mode requires the exact ordered **44 gates**, matching receipt/alias
bytes, every accepted artifact and log hash, pinned compiler, executed-test
identity counts and the same frozen inputs and native binary. Failure publishes
no passed report. The separate `--phase pending` mode always withholds canonical
acceptance and was used before the real terminal receipt existed.

Local hashes and process observations establish consistency, not publisher
authentication, power-loss guarantees or universal platform compatibility. The
candidate route and the final direct confirmation have different scopes; neither
is represented as an uncoached human study or a replay of every product system.
