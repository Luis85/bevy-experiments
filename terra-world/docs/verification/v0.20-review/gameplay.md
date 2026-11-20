# Gameplay/economy/progression review and plan — 0.20

Reviewed against the 0.19 source and current G01–G18 register, 29 October 2026.
Source review covers settlement founding/upgrade/escrow, supply acceptance and
fulfillment, finite market and company household circulation, depot transfers,
community departure/handover and owned movement admission. No new native play,
balance study, geometry or hardware qualification is claimed.

## Selected concrete closure (G03/G04)

`apply_settlement` rejects expired fulfillment at `elapsed >= expires_at`, but
AcceptContract previously checked only ownership/capacity and then reset expiry.
An unclaimed offer whose deadline elapsed between maintenance batches could be
accepted by stale ID, extending an expired offer without first refreshing the
board. This is observable through both the direct SDK entry and ordinary wrapped
command. The board's 60-second maintenance cadence leaves a real time interval
where the stale offer remains present; UI reviewed-object checks do not make the
SDK command boundary consistent.

Plan: reject expired acceptance before any mutation, using the same inclusive
boundary as fulfillment. Preserve active offers' ordinary deadline reset and
retain existing maintenance reissue semantics/escrow. Cover exact deadline and
post-deadline, both public dispatch paths, complete serialized-state equality,
before-deadline success, sub-minute clock crossing, serialized resume and explicit
acceptance after normal maintenance refresh. No prices, reward, item, save/wire
or ownership/succession policy changes are needed.

## Broader product conclusions

- Finite careers and cost disclosures remain useful correctness evidence, not
  long-session balance or uncoached comprehension qualification (G03/G04).
- Physical navigation and source constraints remain incumbent; dense/long
  interactions and recovery need their declared geometry/workload evidence (G05).
- Community handover/removal preserves communal property and clears shared home
  access. Unavailable-leader succession/dissolution requires explicit policy;
  this increment does not invent confiscation or beneficiaries (G06).
- Motor Town specialty depth, production models/audio/geography, physical devices,
  Windows execution and public/WAN operations remain separately qualified gaps.

## Acceptance ownership

Implementation adds an early expiry guard and two lifecycle regressions to the
settlement suite. Root owns Cargo execution and broader integrated gates. This
review is source evidence; test success must be read from newly executed records.

## Implementation delivered

An inclusive expired-offer guard now runs before active-contract capacity checks
and all mutation. Its error directs the player to refresh the noticeboard.
The two regressions cover direct/wrapped exact/post deadline rejection and full
state equality, immediately-before deadline acceptance/reset, an actual 0.5-second
maintenance-free crossing, serialized resume, normal 60-second board refresh and
explicit reacceptance with conserved funded reward and total money.

The pinned formatter completed. No Cargo run was performed by this owner;
`cargo test -p terra-sandbox --test settlements --locked --offline` is assigned
to the integrator. The incumbent expired-claimed-work reopening/fulfillment
regressions remain relevant alongside the new unclaimed-offer boundary.
