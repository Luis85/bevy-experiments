# Fresh final Fleet checkpoint audit — 0.24

**Result: passed_selected_final_fleet_readback.** Every selected saved gameplay invariant matches the actual candidate checkpoint. This is scoped readback evidence; the canonical gates are still running and no release-wide result is asserted.

Audit: 2026-10-30T13:16:48.665481+00:00 through 2026-10-30T13:16:48.761701+00:00. All checkpoints and bound receipt/map/log files remained unchanged through independent readback.

| File | Bytes | SHA-256 |
|---|---:|---|
| Original candidate-world.json | 1,776,513 | `932413c61bd2ee1cff5b5847eb9597afe5e5af946979e2d4b41d15b16484e516` |
| Fresh confirmation-final-fleet.json | 1,776,921 | `c4405f2c220a7211e68f1d827e8c6876cdb40da7c143215609fb92515e5df1f2` |

## Unchanged selected results

- Personal money: **186.13 credits**; company **Field Service** cash: **39.34 credits**.
- Inventory: **24 wood, 2 planks, 20 stone, 8 wheat seeds**.
- Progress: **40 XP, one completed supply contract, no accepted supply claims**.
- Fence #2: entire saved record unchanged, **15/40 health**.
- Mira #1: **paused**, running=false, no loaded trip, assigned pickup #1000053 and route #2.
- Town service: stops #2 and #3, **one passenger**. Company totals remain 50.00 deposit, 20.00 personal registration, 10.00 recruitment and **0.66 paid payroll**; revenue remains zero.

World time advanced **91.099913 seconds**. Whole files are different; this is not a whole-world immutability claim.

## Actual source and executable association

[Fresh Fleet process](confirmation-final-fleet-process.json), PID **79904**, exited 0 with **390 unchanged inputs**, executable SHA-256 `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6` and input-map digest `37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`. The final-build-03 receipt reports the same executable. The parsed build input map independently produces the process map digest; its raw file hash also matches the build receipt. The complete process log matches its recorded digest.

Exact input file timestamps, receipt/map/log hashes, selected before/after values and comparisons are retained in [final-checkpoint-audit.json](final-checkpoint-audit.json). The raw map-file hash and canonical map-content digest use their respective recorded encodings and are intentionally distinct.

No checkpoint/source edits, full-save copy, Cargo/native execution or broad testing occurred in this audit. It does not assert canonical totals, final package acceptance, Windows or physical-device qualification.
