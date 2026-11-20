# Terra World 0.20 gap-closing implementation plan

Baseline: saved0.19 source ZIP; 29 October 2026. Six fresh dedicated owners review
before implementing, then cross-review peers. Preserve all original assets/data
and historical evidence, valid save1/regional1/wire10/asset1 contracts.

| Owner / perspective | Selected scope / acceptance |
|---|---|
| UX / player goals, accessibility | Coordinated F9/Load pre-action review across entrypoints; exact target/current world, safe cancel, stale rejection and UI/authority tests. |
| Render / world, content, performance | Bounded dirty-mesh retry on disconnected/poisoned workers; preserve current source identity, two-worker budgets; deterministic failure/success tests. |
| Network / privacy, security, architecture | Consolidate public SDK handshake/snapshot admission or close highest concrete defect with compatibility tests. |
| Gameplay / economy, progression, community | Find and fix reproducible lifecycle/value/permission defect; do not invent leadership/funding policies. |
| Storage / reliability, recovery, operations | Close reproducible storage/recovery integrity or operational defect; positive valid-store compatibility and failure atomicity. |
| Delivery / QA, handoff, supply chain | Review receipt publication and evidence admission; implement bounded robust correction and executable Python checks. |

Root integrates, cross-dispatches review, formats/versions and freezes inputs,
executes pinned offline release gates, investigates failures, verifies final
archive bytes/source/binary identities and preserves baseline content. Use a
stable temporary working directory for all writes and long runs. Package and
save updated source, Linux review build, full G01–G18 report and implementation
plan. Fresh native visuals remain unavailable under observed socket policy;
Windows, physical devices, measured GPUs, uncoached play, production geography/art,
public operators/WAN/soaks and RPO/RTO require actual qualification. No historical
receipt counts as fresh0.20 evidence. Final report distinguishes selected closures
from whole product areas and records deviations/corrections.

## Selected review outcomes

Network: one shared Welcome validator, requested-manifest pinning and reject raw
wire snapshots that bypass replication admission. Storage: hydrate must validate
committed schema/payload even on fully materialized fast paths, while retaining
verified cache hits. Delivery: exclusively publish/read back completed-gates.json
and package from that terminal receipt; preserve progress separately. Mesh failures
retain dirty state with capped backoff and observable failure/retry telemetry.
