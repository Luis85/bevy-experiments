# Candidate checkpoint audit — 0.24

**Result: passed_selected_checkpoint_checks.** Independent read-only inspection of the two requested saved files. All 20 selected checks passed; this is not final release acceptance or an executed reload.

Observed 2026-10-30T12:35:31.526322+00:00 through 2026-10-30T12:35:31.593760+00:00. Both file identities and hashes remained unchanged through the audit. No complete save was copied into the repository.

## Actual checkpoint identities

| Checkpoint | Bytes | File modification time (UTC) | SHA-256 |
|---|---:|---|---|
| Earlier building checkpoint | 1,769,411 | 2026-10-30T12:05:35.617066+00:00 | `412f4e8ff1369137e3f459bdd4e94532598a5c742cfdcffa785992f1cbdeacfe` |
| Final Save and quit checkpoint | 1,776,513 | 2026-10-30T12:32:08.836796+00:00 | `932413c61bd2ee1cff5b5847eb9597afe5e5af946979e2d4b41d15b16484e516` |

The exact paths and selected saved fields are in [the JSON audit](candidate-checkpoint-audit.json). Both use wrapper/domain version 1, player 1 and the same source identity.

## Selected saved state

| Observation | Earlier building checkpoint | Final Save and quit checkpoint |
|---|---|---|
| Personal money | 256.13 credits | 186.13 credits |
| Inventory | 24 wood, 2 planks, 20 stone, 8 wheat seeds | Identical |
| Supply progress | 40 XP, 1 completed contract, no accepted supply claim | Identical |
| Fence #2 | Owned by player 1; 15/40 health | Entire saved structure unchanged |
| Company | Absent | Field Service, owned by player 1 |
| Company cash | No company | 39.34 credits |
| Employee | No company | Mira #1, paused, running=false, no loaded trip, 0 completed company deliveries |
| Assignment | No company | Pickup #1000053, route #2 Town service |
| Route | No company | Stops #2 Town interchange and #3 Hillside interchange; 1 passenger |
| Pickup odometer | 0 m | 36.080697484 m |

The saved vehicle model is `pickup`, whose model display name is **Trail utility pickup**; the owned body retains its authored name **Utility rover**. The final vehicle is stopped, idle and parked, with empty cargo.

## Company money reconciliation

The saved company accounting totals record personal registration of **20.00 credits**, an owner deposit of **50.00**, recruitment of **10.00**, and paid payroll of **0.66**. The actual earlier-to-final personal change is exactly **−70.00**, matching registration plus the owner deposit. Recruitment and payroll came from company cash:

`50.00 − 10.00 − 0.66 = 39.34 credits`

Mira's paid wages are exactly 0.66 credits, with zero spending. The saved fractional payroll accrual is retained separately and is not another paid amount. Company revenue remains zero. This verifies the selected company ledger reconciliation; it does not infer a whole-world cash history from two checkpoints.

## What the files establish about the earlier supply/build steps

Both checkpoints already contain the completed supply progress and damaged fence. Neither is a before-delivery, before-crafting or before-damage snapshot. Therefore this audit does **not** manufacture an earlier wallet, inventory or health value and does not independently prove the 4-wood delivery, +6.13-credit reward, or exact craft sequence.

The actual retained state—24 wood, 2 planks, 40 XP, one completed supply contract and fence #2 at 15/40—is consistent with those separately observed native actions. Current catalog rules confirm that one planks craft uses 2 wood for 4 planks and that a fence uses 2 planks with maximum health 40. Those rules provide interpretation, not a missing historical checkpoint. Event evidence remains the integrator's earlier screenshots and process record.

## Scope

This audit parsed the exact files and compared selected stored values with stable readback. It did not change either save, invoke the native executable, run Cargo, perform complete authoritative save validation, or establish reload, Windows/device or final-package acceptance. The integrator retains the real checkpoint copies for the subsequent restore journey.
