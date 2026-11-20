# Terra World 0.24 — native journey and confirmation

## Evidence scope

These are actual native X11 windows captured on Linux with software Vulkan, using the pinned Rust toolchain and the repository's original runtime assets. They establish observed interactions and pixels under the recorded conditions. They do not establish physical GPU performance, Windows execution, physical controller behavior, assistive-technology support, 200% reflow or uncoached usability.

`run_024.py` records the direct executable PID, command, source-input map digest, executable hash, process lifetime, complete log hash and exit. `ui_024.py` publishes each complete PNG atomically with its dimensions and the window PID observed before and after capture. The integrator inspected every image used as accepted visual evidence. Candidate and superseded confirmation images remain useful observations, but only the explicit final registry and accepted audit may bind images to the executable subsequently accepted by the canonical gates.

The original baseline and actual user-created checkpoints were never semantically edited to manufacture a scenario. Private checkpoint copies were made byte-for-byte for restoration through the ordinary F9 review. Passing `--save` selects the manual JSON path; it does not automatically load that file.

## Fresh delivered 0.23 baseline

The pinned 0.23 executable ran as `baseline-game`, PID 26267, from 11:16:05 to 11:20:02 UTC on 30 October 2026 and saved/quit with exit 0. Its exact SHA-256 is `432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.

Seven complete images, 01 and 03–08, were inspected. Activities exposed the four-wood request; Industry used a combined generic pause/resume label; Build exposed unnamed damage even with no structures; Fleet and the normal Save and quit menu were visible. [The product review](../../PRODUCT_REVIEW.md#fresh-native-baseline) records the baseline findings.

Capture 02 was incomplete and rejected. Its original bytes and receipt remain retained. Capture 03 supplies the complete Industry observation. The original baseline process helper also mislabeled its release field as 0.24. The pinned binary and rendered 0.23 title identify the actual baseline, and the original receipt remains unchanged. The parallel implementation checkout changed before baseline exit, so its ending source map is not final-source evidence.

## Candidate: complete connected journey

`candidate-game`, PID 59077, ran from **11:48:52.858 to 12:32:09.294 UTC**, exited 0 after Save and quit, and retained all 390 implementation inputs unchanged. Candidate executable SHA-256: `b269b894d00f311ea66200f04b1dfff1d0863353e00b263d42761a40bd34ef8a`. This is the executable before the final two-label correction.

The journey used ordinary mouse/keyboard controls at 1440×900 and 100% text, then the actual Controls setting to save 150% text, and a 1100×700 minimum window. Company setup continued at 1440×900 with 150% text. Eighty candidate images, 10–89, retain the actual sequence.

| Area | Executed behavior | Selected images |
|---|---|---|
| Supply | Accepted the four-wood Riverside request, travelled to it and delivered; wallet reached 256.13 credits and progression reached 40 XP / one completed contract. | [Accept](11-candidate-supply-accepted.png), [arrival](13-candidate-supply-arrival.png), [delivery](14-candidate-supply-delivered.png) |
| Industry | Observed the named facility and out-of-reach blocker, approached it, paused and resumed its named operation. | [Initial blocker](15-candidate-industry.png), [ready](20-candidate-industry-ready.png), [paused](21-candidate-industry-paused.png), [resumed](22-candidate-industry-resumed.png) |
| Building | Empty damage action was unavailable; claimed a plot, crafted planks and built fence #2 at 40/40 health. | [Empty state](19-candidate-build-empty.png), [plot](25-candidate-plot-claimed.png), [craft](26-candidate-planks-crafted.png), [fence](29-candidate-fence-built.png) |
| Damage consent | Reviewed the exact named fence and health effect, entered safe Cancel focus, cancelled with Enter, then separately reviewed and confirmed damage. | [Review](30-candidate-damage-review.png), [Cancel focus](31-candidate-damage-cancel-focus.png), [cancelled](32-candidate-damage-cancelled.png), [minimum review](44-candidate-minimum-damage-review.png), [confirmed](45-candidate-damage-confirmed.png) |
| Scale | Set and saved 150% text, then inspected 1100×700 Build and Fleet. The old long damage labels were shortened after this candidate. | [Controls](38-candidate-text-150.png), [minimum property](43-candidate-minimum-property.png), [minimum Fleet](47-candidate-minimum-fleet.png) |
| Company | Approached the garage using RTS camera movement and a real move order; registered Field Service, transferred 50 credits and hired Mira. | [Garage arrival](58-candidate-fleet-arrival.png), [registered](61-candidate-company-registered.png), [funded](65-candidate-fleet-company-summary.png), [hired](72-candidate-hired-summary.png) |
| Route and service | Saved Town service with two stops and one passenger, assigned the Trail utility pickup, started service and paused it safely. | [Saved route](78-candidate-route-saved.png), [assignment](82-candidate-driver-assigned.png), [started](85-candidate-service-started.png), [paused](86-candidate-service-paused.png) |
| Persistence | Used F5 during the journey and ordinary Save and quit; selected checkpoint values were independently read back. | [Saved career](87-candidate-career-saved.png), [Save and quit](89-candidate-save-quit.png), [checkpoint audit](candidate-checkpoint-audit.md) |

The company goal's “Review setup” opens Fleet; it does not automatically move the actor to the garage. The candidate required manual RTS navigation. This remains a concrete discovery/wayfinding item in G04/G05, not evidence of an automatic travel feature.

### Independently audited saved results

The exact building checkpoint is 1,769,411 bytes, SHA-256 `412f4e8ff1369137e3f459bdd4e94532598a5c742cfdcffa785992f1cbdeacfe`. It contains 256.13 personal credits, 40 XP, one completed supply contract, no accepted supply claims, 24 wood / 2 planks / 20 stone / 8 wheat seeds, one plot and fence #2 at 15/40 health.

The final candidate save SHA-256 is `932413c61bd2ee1cff5b5847eb9597afe5e5af946979e2d4b41d15b16484e516`. It retains the inventory, progression and fence. Personal cash is 186.13; Field Service cash is 39.34; Mira is paused with no loaded trip; Town service and pickup assignment remain. The ledger reconciles 20.00 registration, 50.00 owner deposit, 10.00 recruitment and 0.66 payroll. The pickup odometer is 36.0806974835 m and it is stopped/idle/parked. All 20 selected independent checks passed.

Both audited checkpoints are after the supply/crafting/damage actions. They prove resulting saved state and retention, not an independently reconstructed earlier transaction sequence. The actual captures supply that sequence. Full private saves were not copied into the source artifact.

## Final building confirmation and corrected visual assessment

The next build shortened the two controls to **Review damage** and **Confirm damage**, executable SHA-256 `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`. Its build succeeded from 390 unchanged inputs.

`confirmation-building`, PID 76362, restored the actual building checkpoint through F9. Images 200–223 show restored money/progression, fence #2 at 15/40, the minimum window at saved 150% text, exact destruction/no-refund consequences, safe keyboard cancellation and retained 15/40 health. It saved/quit with exit 0 and unchanged source inputs.

An initial visual assessment incorrectly reported missing prose in 209, 211 and 220. Exact pixel comparison disproved the asserted difference: the named-consequence area in 208/209 is identical, and the complete Build-prose area in 219/220 is identical. Direct inspection of the extracted regions shows the text. The original finding and inferred renderer problem were withdrawn. A narrowly scoped experimental refresh and regression were removed exactly; it is not a shipped change. [The independent comparison](../v0.24-review/pixel-comparison.md) retains file hashes, coordinates, raw-region hashes and zero differing pixels.

The experimental `confirmation-polish` run, PID 78977, used executable `092484f4ffbfc9f89b4950f7b55a935864117e5a7c96e83744224e07f53cb87d` and exited 0. Its images 300–318 and build/regression attempts remain experimental evidence. They are excluded from final executable acceptance. The exact final source is restored to the input map used by the original building confirmation; no source or evidence is relabeled.

`confirmation-fleet`, PID 77552, restored the real final candidate checkpoint through F9. Images 230–237 show 186.13 personal credits, 40 XP, one completed contract, Field Service with 39.34 credits, Mira paused, the saved pickup/route assignment and explicit departure information. It saved/quit with exit 0, but its source map changed during the subsequently withdrawn rendering experiment, so this process cannot supply final-source acceptance. That earlier run remains excluded from final-source acceptance; the fresh completed process is recorded below.

## Final executable confirmations

Final build 03 restored **all 390 input hashes** and the exact executable from build 01, SHA-256 `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`. The canonical input-map digest is `37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`. This establishes that the original clean building process belongs to the final source/executable; no historical receipt was modified.

The fresh `confirmation-final-fleet` process, PID **79904**, ran from **13:10:39.173 to 13:13:44.634 UTC**, saved/quit with exit 0 and retained the same 390 source inputs unchanged. The actual F9 restore and subsequent Company/Drivers screens show 186.13 personal credits, 40 XP, one completed contract, Field Service cash 39.34, Mira paused without a load, Trail utility pickup assigned to Town service, and the explicit funded-departure/payroll explanation. Images **330–337** were individually inspected. Independent review also inspected the exact body regions and checked file/receipt hashes and process lifetime.

The [approved registry](APPROVED-REGISTRY.json) explicitly selects **18 inspected images from two clean processes**, `confirmation-building` and `confirmation-final-fleet`. Candidate images, the source-drift Fleet run and the withdrawn experimental process are excluded from final provenance acceptance. [Final checkpoint readback](final-checkpoint-audit.md) separately checks saved gameplay values; it does not replace image or command-path evidence.

## Canonical binding status

The canonical 44-gate run started at **13:13:54.011 UTC**, after both registered native processes completed, and passed at **13:37:40.813 UTC**. It records 1,099 distinct Rust tests, 1,200 Rust test executions, 390 unchanged implementation inputs and the same final executable.

The [accepted native provenance audit](confirmation-provenance-accepted.json) **passed and authorizes release**, validating the actual terminal receipt, its 36 evidence artifacts, both registered processes and all 18 selected captures. Terminal receipt SHA-256: `8c386ac3ef5f1bead91f808db90b1301a6b6b834096a36c2cc654abd8f776d81`. The prior pending audit remains unchanged and non-authorizing; it is not substituted for this accepted result.
