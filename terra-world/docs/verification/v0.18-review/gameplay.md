# Gameplay/economy review — 0.18

Reviewed the attached 0.17 regional-reference implementation, concentrating on authoritative faction, settlement, transport and company transaction boundaries.

## Finding and correction

Assigned-vehicle owner-paid refuel, repair, paint, engine/brake upgrade and rental renewal recorded a company expense before checking the personal wallet and issuer treasury. If the wallet could not afford the service, or the treasury had no remaining capacity, the command returned an error after the company's accounting totals/recent journal had changed. No fuel, repair, paint, upgrade or renewal had been supplied. Retrying could record further phantom expenses and consume finite journal capacity.

The shared `charge_owner_paid_company_vehicle` helper now prepares the optional company receipt without mutation, performs the existing checked wallet/treasury transfer, then commits the receipt. All six service forms use this helper; subsequent service changes are infallible after their existing admission checks. Exhausted accounting fails before payment; failed payment never commits a receipt. Unassigned personal vehicles still incur the ordinary payment without a company allocation. Company cash, acquisition/bond policy, service prices and schemas are unchanged.

## Regression coverage

`rejected_assigned_vehicle_services_leave_no_phantom_owner_expense` exercises eighteen combinations: insufficient wallet, full issuer treasury and exhausted journal, each against refuel, repair, paint, engine upgrade, brake upgrade and real rental renewal. It uses ordinary registration, funding, hiring, routing, assignment and rental purchase setup. Each case validates the state before/after, asserts the specific rejection class, and compares the complete serialized state, including cash, receipt sequence/totals, vehicle/fuel and rental/bond state.

Existing successful owner-paid maintenance and assigned-rental renewal tests provide the successful-transaction counterpart. Full executed results are recorded below when available.

## Limits

This closes an atomicity defect, not G03's complete economic-balancing criteria. It does not establish long career profitability, living/capital costs or human understanding. Community unavailable-leader succession/dissolution (G06), arbitrary traffic geometry (G05), and longer playtests remain open. No new UI behavior, prices, progression ledger, network command or save migration is introduced. Historical screenshots remain historical; this review makes no fresh rendered-playtest claim.

## Executed evidence

From `/tmp/terra-v018-integration/terra-world`, with `PATH=/tmp/terra-build/rust/bin:$PATH`, `CARGO_HOME=/tmp/terra-build/cargo`, `CARGO_TARGET_DIR=/tmp/terra-build/target`, `CARGO_BUILD_JOBS=3`:

```
cargo test --offline -p terra-sandbox --test company_accounting rejected_assigned_vehicle_services_leave_no_phantom_owner_expense
```

Exit 0; **1 passed**, 0 failed, 0 ignored, 11 filtered out; 18 transaction-rejection scenarios inside that test. Build 1m54s; execution 0.16s. The command compiled the integration sources (package metadata was still 0.17.0 at this focused run). An earlier scratch-tree attempt was cancelled because workspace synchronization restored its files; it is explicitly excluded from acceptance. Broader final-version checks belong to the integration validation record.
