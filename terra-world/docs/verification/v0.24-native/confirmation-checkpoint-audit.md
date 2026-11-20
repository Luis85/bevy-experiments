# Restored checkpoint audit — pre-final 0.24 binary

**Result: passed_selected_restore_invariants.** All 50 selected identity, state and readback checks passed. Every selected gameplay invariant matches its actual source checkpoint. Whole files differ; this report does not claim final acceptance.

Observed 2026-10-30T12:55:39.582199+00:00 through 2026-10-30T12:55:39.720544+00:00. All four checkpoints and referenced evidence retained stable regular-file identities and hashes through independent readback. No full save was copied into the repository.

## Actual files

| Pair / phase | Bytes | File modification time (UTC) | SHA-256 |
|---|---:|---|---|
| Building / original | 1,769,411 | 2026-10-30T12:05:35.617066+00:00 | `412f4e8ff1369137e3f459bdd4e94532598a5c742cfdcffa785992f1cbdeacfe` |
| Building / restored and saved | 1,768,720 | 2026-10-30T12:49:30.872909+00:00 | `f7a4a292f8cfeeca9ecced70698a3cf4dc85efe61865fb8fe97f9d717a460095` |
| Fleet / original | 1,776,513 | 2026-10-30T12:32:08.836796+00:00 | `932413c61bd2ee1cff5b5847eb9597afe5e5af946979e2d4b41d15b16484e516` |
| Fleet / restored and saved | 1,776,980 | 2026-10-30T12:53:06.280303+00:00 | `56999955add4a1a2ac9dea04244f4aba464701abd33301bcdf0bb0351cfd3dd5` |

The original files also match their hashes in the [earlier candidate audit](candidate-checkpoint-audit.json) and integrator's [copy record](confirmation-inputs.json).

## Preserved results

| Selected value | Building restore and Save and quit | Fleet restore and Save and quit |
|---|---|---|
| Personal money | 256.13 credits, unchanged | 186.13 credits, unchanged |
| Inventory | 24 wood, 2 planks, 20 stone, 8 wheat seeds, unchanged | Identical unchanged inventory |
| Progression | 40 XP, one completed supply contract, unchanged | Same |
| Accepted supply claims | None, unchanged | None, unchanged |
| Fence #2 | Exact structure unchanged, 15/40 health | Exact structure unchanged, 15/40 health |
| Company | Still absent | Field Service, exact selected ledger/driver/route fields unchanged |
| Company cash and payroll | Not applicable | 39.34 cash; 0.66 paid payroll, unchanged |
| Employee and assignment | Not applicable | Mira #1 remains paused, running=false, pickup #1000053, route #2, no loaded trip |
| Route | Not applicable | Town service, stops #2 and #3, one passenger, unchanged |

Player/source identity, saved player position and active/avatar identity also match within each pair. The Fleet accounting totals and retained receipt digest are unchanged; no extra company wage payment or delivery was introduced by the readback journey.

## Expected simulation differences

Saved world time advances by **554.549471 seconds** in the Building pair and **102.399902 seconds** in the Fleet pair. The saved avatar's food and water levels decrease while energy stays at 1.0. Exact time/needs values are recorded separately from the invariant fields in [the JSON audit](confirmation-checkpoint-audit.json). These are actual differing saves with normal simulation progression, not whole-file equality.

The integrator reports F9 restore and two cancellations of the damage review for Building, and restore/readback for Fleet. The unchanged 15-health fence and selected values support those retained outcomes. Parsing these files alone does not prove the order or number of UI actions.

## Executable and acceptance scope

Both [Building](confirmation-building-process.json) and [Fleet](confirmation-fleet-process.json) process receipts report exit 0 for executable SHA-256 `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`. These runs occurred **before the final rendering/focus correction**. The Fleet receipt explicitly reports that the source input map changed while that correction was prepared; the audit preserves this fact and does not bind the run to later source or the rebuilt executable.

This review made no checkpoint/source edits and launched no Cargo or native process. It is selected restore-state evidence, not final native/canonical/package acceptance, whole-world conservation or Windows/device qualification. The final rendering correction requires its own executable-bound confirmation.


## Correction — withdrawn rendering experiment (30 October 2026)

The integrator and reviewers subsequently checked the exact RGB pixels of the
relevant text regions in capture pairs 208/209 and 219/220. Those regions are
identical; the alleged disappearing-text defect was withdrawn. The experimental
rendering helper and test were removed, restoring the native implementation
inputs recorded for `native-final-build-01`. Rebuild 03 is intended to prove the
restored executable identity; this correction does not claim that result before
its receipt exists.

The earlier “pre-final rendering correction” wording above and in the immutable
JSON therefore describes a **withdrawn experiment**, not a required product fix.
The Building process retains its successful exit, unchanged source map and exact
`509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`
executable association. It may contribute to final native verification once the
final source map and executable are proven identical. Removing the experiment
does not by itself require replaying the completed broad gameplay journey.

The Fleet process still records `source_inputs_unchanged=false`. Its selected
checkpoint invariants remain valid observations, but that run does not satisfy
stable-source binding. A fresh Fleet run with the restored final inputs is still
required. The original audit JSON has not been rewritten; its historical scope
and source-change observation remain intact. Final canonical and delivered-package
acceptance remain separate.
