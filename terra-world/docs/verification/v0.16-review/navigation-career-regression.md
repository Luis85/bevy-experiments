# Navigation career regression and correction

Status: the production correction passed all **75 focused tests**, with zero
failures or ignored tests. The next full release acceptance run remains pending. This record
supersedes no earlier evidence. The failed integrated attempt remains in
[`v0.16-attempt-2/headless.log`](../v0.16-attempt-2/headless.log).

## Failure and causal evidence

The second full release attempt exposed two previously accepted long careers:
`purchased_stock_career_accounts_for_acquisition_setup_and_recovery_without_inventing_profit`
and `six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity`.
Both stopped after six completed deliveries at the unchanged 2,400-step career
limit. Driver Robin retained a running route, no loaded trip and a physical
clearance beside the stationary owner avatar; the original destination was
approximately 154 metres away. The company fixture, timeout and assertions
remain byte-identical to the accepted 0.15 source archive.

A temporary diagnostic copy changed only the failure output to include the
vehicle, route and nearby bodies. Its failure is retained in
[navigation-career-diagnostic.log](navigation-career-diagnostic.log); that Rust
target was removed from the project after diagnosis. A small 155-metre route
with a stationary human reproduced the failed detour admission in
[navigation-reconnect-reproduction.log](navigation-reconnect-reproduction.log).
The same admission probe then passed with the production correction in
[navigation-reconnect-admission-fixed.log](navigation-reconnect-admission-fixed.log).
The permanent regression extends that probe through physical arrival and exact
save/resume rather than stopping after the route is accepted.

The cause was a mismatch between two existing bounds. The new complete
reconnection check allowed up to 160 metres of three-dimensional travel. The
compatibility source resolver allows 512 intervals of 0.25 metres, including
rotational sweep, in a single callback. A supported reconnection above roughly
128 metres therefore failed before its terrain could be examined. The
controller could keep making legal clearance moves without accepting the route
around the stationary avatar.

## Production correction

The original final heading and reverse choice are computed once. The complete
reconnection pose sweep is then divided into source calls bounded by 64 metres
of translation plus rotational travel, with 0.1 metre tolerance. Every call
consumes the existing shared twelve-call allowance. The original 160-metre
three-dimensional limit is checked before any callback, and each actual piece
is bounded again after a provider's preceding support-height adjustment.

This remains one complete admission. Every piece must succeed before either
corner is saved; a failure in the last piece cannot commit an earlier prefix.
The original destination is preserved, no extra waypoint or saved-state field
is introduced, and future physical movement continues to use the existing
handling, live terrain, traffic and body-sweep checks. The narrow and cluster
proposals retain their existing full-leg admission. An original waypoint beyond
the declared local reconnect limit can still hold; the correction does not
introduce a long-range routing claim.

The content owner independently reviewed the implementation without editing or
running it. That review found the full envelope, original heading/reverse,
shared work budget, provider-height bound, final endpoint and all-or-nothing
commit intact. Execution evidence is recorded separately below.

## Verification scope

The permanent `navigation_geometry` additions are:

- `legacy_reconnection_admits_a_complete_bounded_leg_above_one_callback_budget`:
  a 155-metre route whose actual reconnect is greater than 128 and at most 160
  metres, a stationary human, a supported 24-metre strip, complete physical
  arrival, unchanged destination, swept contacts, motion/path/work bounds,
  exact resumed state and conserved carried inventory and pocket money.
- `a_gap_in_the_last_reconnection_segment_prevents_partial_route_commit`:
  a one-metre unavailable strip near the final piece must actually be queried
  and reject the whole maneuver, leaving the original route intact during the
  two-second admission observation.

These are authored flat SDK cases with a frozen calendar, not claims about
arbitrary terrain providers, road throughput or economic profitability. The
unchanged company careers supply the broader finite-stock, cash, payroll,
physical delivery and paid recovery checks. The existing two-kilometre waypoint
and maximum-five-second-step test retains the finite callback bound.

The focused command includes sandbox library tests, `company_depots`,
`navigation_geometry`, `navigation_clusters`, `autonomous_driving`, `traffic`,
`traffic_recovery` and `company_career`, with offline locked dependencies. Final
results and source bindings are in
[navigation-career-regression.json](navigation-career-regression.json).


## Focused result

The [complete focused log](navigation-career-fix-focused.log) records exit zero:
18 library, 16 autonomous-driving, one novice company career, ten company-depot,
five cluster, ten geometry, twelve traffic and three recovery tests passed.
Both six-session warehouse careers completed all 24 deliveries, seven saved
checkpoints and one paid fuel recovery in 2,967.45 physical seconds. Every step
conserved 1,390,000 minor credits, 224 water units and 40 bread units including
actual consumption; the full test retains escrow and accounting checks. This
completion does not establish profitability: both reports record an operating
cash change of minus 469 minor credits excluding owner transfers.

The new 155-metre case physically arrived after 24.4 seconds and 159.501 metres
of accepted travel, with 243 exact resumed steps and a maximum of 6,546 source
queries in one update. The final-piece unavailable-strip regression passed.
All five existing geometry reports remained identical to the earlier focused
record. Historical loaded-courier recovery stayed at 34.0 and 49.5 physical
seconds with exact goods, money and resumed state. No temporary diagnostic Rust
target remains, and no career assertion or time limit was modified.
