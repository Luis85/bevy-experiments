# Diagnostics native UI independent review

Read-only review of the root operator's candidate and subsequent final-run screenshots, supplied at 1100 × 700 or 1440 × 900 with controls set to 150%. Candidate and final evidence are separated below. This note describes visible pixels, not inferred interaction results. Named JSON evidence is assessed separately. No implementation edits, native input, or Cargo run were performed for this review.

## Settings candidate screenshots

1. `/tmp/terra-native-017/v17-candidate-settings-top.png` — general health: readable and ordered. The Settings heading, graphics controls, supporting copy, View controls action, and Playtest diagnostics heading establish a clear sequence. Back, Save defaults, and keyboard instructions remain fully visible outside the scrollable content. Text wraps within the panel without visible overlap. The diagnostics toggle is partly below the content viewport; the scrollbar signals more content.
2. `/tmp/terra-native-017/v17-candidate-settings-diagnostics.png` — general health: readable consent and empty status. The visible copy explains application to the next world/host session, saving the default, the 10-second cadence, bounded retained captures, final-exit attempt, retention when turned Off, local-only reports, excluded private content, and reviewing before sharing. The complete report path is readable on one line in this capture. “No local playtest report yet” clearly distinguishes absence from a successful current capture. Refresh report status, Back, Save defaults, and keyboard instructions fit fully.
3. `/tmp/terra-native-017/v17-candidate-settings-choice.png` — general health: clear opt-in choice. The full Off toggle, next-session/persistence/retention copy, and privacy scope are legible together. This additional scroll position resolves the continuity question raised by the first two stills.
4. `/tmp/terra-native-017/v17-candidate-settings-enabled.png` — general health: clear selected state. The complete On toggle has a visible focus outline; fixed Back and Save defaults remain fully visible.
5. `/tmp/terra-native-017/v17-candidate-settings-saved.png` — general health: readable return to Home. New world, Your worlds, Multiplayer, Settings, Controls, and Quit all fit. Supporting empty-world text continues below the scroll viewport. The image alone does not establish preference persistence.

No material visual blocker is established by this expanded Settings set. The visible scrollbar, complete choice at an intermediate scroll position, and fixed footer support small-window use. Keyboard traversal, continuous scrolling behavior, activation, assistive technology semantics, and measured contrast are not independently verified by these stills. The root operator separately reported that the toggle was reachable with one scroll, Save defaults returned to Home, and a preference read showed `playtest_reports: true`; those are attributed runtime observations, not conclusions from pixels.

The screenshots show explanatory text asserting privacy and timing behavior. This visual review establishes that the explanation is displayed and legible; backend behavior is covered by the separate implementation and test records.

Candidate evidence SHA-256:

| Screenshot | Dimensions | SHA-256 |
| --- | --- | --- |
| settings-top | 1100 × 700 | `ad0462503e932ddcb09124a6e71be95eed89d7366e2de1c82b66f9c1fd47472e` |
| settings-diagnostics | 1100 × 700 | `4083f03299e91c538850af2ba9322648f729da7e1284665f8c9a8c33d9df1362` |
| settings-choice | 1100 × 700 | `d9668d0e4f398c4fbae5f65095af319f1cadcdfb0f1d35ebd17b5b595e34ceb2` |
| settings-enabled | 1100 × 700 | `536f783c751c51e8b6bc8b3831bb4f3f98b824ebfc1c9ff815608d034b8f7aec` |
| settings-saved | 1100 × 700 | `47cec5eeef22ecc879b7da110bd09a5163f931974dbc92332985ed2843f3b292` |

## Pause and performance candidates

6. `/tmp/terra-native-017/v17-candidate-pause-diagnostics.png` — general health: readable at 1440 × 900. “World paused” and the explanatory sentence distinguish paused local simulation from its retained clock settings. The saved checkpoint, diagnostic capture age, 10-real-second cadence, and complete wrapped report path fit without overlap. Resume, Checkpoint now, Settings, Controls, Restore latest checkpoint with its discard warning, and Save and return to main menu are readable. Save and quit continues below this scroll position.
7. `/tmp/terra-native-017/v17-candidate-pause-small.png` — general health: status remains readable at 1100 × 700. The full report path still wraps within the panel. Resume, Checkpoint now, Settings, and Controls fit. Restore and exit controls are below the current viewport; the scrollbar and fixed Mouse wheel to scroll instruction are visible, but this image alone does not establish the lower controls' reachability.
8. `/tmp/terra-native-017/v17-candidate-performance-top.png` — general health: readable performance panel at 1100 × 700. Close [F12] and Reset timings fit. The text distinguishes frame interval from CPU stages and explicitly says GPU time is not measured. The capture shows performance in this software-rendering session; it is not evidence of physical-device performance.
9. `/tmp/terra-native-017/v17-candidate-performance-local-report.png` — general health: readable report status at 1100 × 700. “Last captured 11 s ago” and “A diagnostic capture is queued or publishing” separate the last completed publication from current work. Cadence and bounded retention remain readable. The report path continues below the panel viewport in this capture, so its complete visibility is not yet established here.
10. `/tmp/terra-native-017/v17-candidate-session-keyboard-save.png` — general health: readable lower pause actions at 1100 × 700. Restore latest checkpoint and its discard warning fit fully. Save and return to main menu is fully visible with a clear focus outline. The root operator separately reported reaching this state with five held Tab steps from Resume and then activating it with Enter. The screenshot establishes the resulting visible focus and complete action label; the key sequence and resulting save/return are attributed runtime observations. Background F12 text is partly occluded by the modal and is not accepted as independent proof of the full F12 destination.

No material code correction is established by these captures. The lower pause capture resolves the small-window return-to-menu visibility question. An unobscured further scroll position is still needed to verify the complete F12 destination. The pause captures show different checkpoint/capture ages and are separate observations; no exact-frame correspondence to the retained JSON is claimed.

| Screenshot | Dimensions | SHA-256 |
| --- | --- | --- |
| pause-diagnostics | 1440 × 900 | `b8d69f0f7b91cf950a00321c6bf64eff368cfedc3513e3d16a2b856d31ee9c65` |
| pause-small | 1100 × 700 | `1e2b8911cdcc7d15278f3cde646b6557a6016f18c98328e0a991cce7b348c539` |
| performance-top | 1100 × 700 | `2191bf8dd062331a125ad9bb577c4ef880b8668269d45e4ac38c45360ea615a5` |
| performance-local-report | 1100 × 700 | `2b7b32bcf1e37cde06b76e2d4ce19e77aa832c5d1cb6d220b2ed0897e01c422f` |
| session-keyboard-save | 1100 × 700 | `7aa29566e4265e39dc23a9454881f25d8f14bf5dcb4934263119c58336679058` |

### Separately inspected candidate JSON

`/tmp/terra-native-017/candidate-menu-handoff.json` records parent/observed-parent 80307 and game 80823, diagnostics enabled with the menu path and 10-second interval, and a report session containing that game PID. This is explicitly a candidate receipt, not final acceptance. Its SHA-256 is `c0d5982816fc78fba7cd29b58120cb346a3bb343f410c56c3b97565df6a9a85c`.

`/tmp/terra-native-017/v17-candidate-paused-telemetry.json` has the matching session `1793264306814-80823-0`, sequence 11, status running, phase paused, virtual time paused, world clock paused false, input blocked, managed world true, no unsaved changes, no queued checkpoint/restore, checkpoint generation 25, no pending/blocked regional write or reported regional error, and 49 rendered chunks. This corroborates the distinction between a paused local session and its unchanged world-clock setting; it does not turn the screenshot review into an independent process execution. Its SHA-256 is `bf507b49e9dae75e40748385e88a997a1c6be88b0d3c54d2316da76c5eeabba6`.

## Candidate review completion

The candidate review is complete for the supplied evidence with no material correction requested. At candidate handoff, an unobscured F12-bottom view and final-run screenshots remained pending. Candidate images do not inherit the subsequent final process/build provenance.

## Final-run diagnostic captures

The following three images were supplied after the root operator reported completion of all 43 final gates. They are reviewed independently of the preceding candidate set. The inspected schema-1 capture receipts all record release 0.17.0, available stable window PID 86533 before/after capture, no PID query error, and image names/hashes/dimensions matching the actual inspected files. Completed process interval, accepted executable/source binding, and exit remain responsibilities of the separate final provenance receipts and independent archive audit.

1. `/tmp/terra-native-017/v17-confirm-managed-performance-report.png` — general health: readable and complete at 1100 × 700. The unobscured F12 panel shows last captured 9 seconds ago, every 10 real seconds, latest 16 captures, and the full wrapped destination `/tmp/terra-native-017/menu-player-data/diagnostics/latest.json`. It resolves the prior complete-path visual evidence gap. No error or pending publication message is displayed in this particular frame; that observation does not establish later health.
2. `/tmp/terra-native-017/v17-confirm-managed-pause.png` — general health: readable at 1440 × 900. Local simulation pause and retained clock settings are explicit. Last saved checkpoint 64, diagnostic capture age 5 seconds, cadence, and complete wrapped report destination are visually distinct. Resume, checkpoint, settings, controls, restore with discard warning, and return-to-menu action are readable; Save and quit continues below the current scroll position.
3. `/tmp/terra-native-017/v17-confirm-managed-pause-small.png` — general health: readable at 1100 × 700. The same pause explanation, checkpoint 64, capture age 6 seconds, cadence, and complete report destination fit without overlap. Resume, Checkpoint now, Settings, and Controls remain visible with the scroll instruction and scrollbar. This final still does not itself repeat the candidate lower-action keyboard journey.

No material visual correction is requested from this final diagnostic set. The finding is limited to these observed states and sizes, with no Windows, physical GPU/input device, assistive-technology, measured-contrast, or uncoached-usability qualification.

| Final image | Dimensions | SHA-256 |
| --- | --- | --- |
| managed-performance-report | 1100 × 700 | `445d48f5232eb57c66498fdf4f3818d78a67b9051ae2954783c0454bec1e64f9` |
| managed-pause | 1440 × 900 | `36a7837c84f0bce4f5875c20395ede321403897827d0f199c98274fb10fa635c` |
| managed-pause-small | 1100 × 700 | `8b2fee4b4210f298d56e52d2d238abbb957cf5b625b827b150ce3c0c68845d1c` |

Capture-receipt SHA-256 values, in the same order: `5182322470083a57d6c29517424c291581a7908695f73b02317993c918c70eae`, `07c9809319f0456eb7e01ffc03a5231d316b863619c5184b4e4a3e8c21d7580e`, and `d951ecb60f76e3c34b6cae01f07e64bff203e833a9ff03303c3aa46069f5b886`.
