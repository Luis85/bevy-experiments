# AI / behavior-tree prior art (researched 2026-10-23)

## Bevy crates (status late 2026)
| Crate | Bevy 0.20? | Model | Take / avoid |
|---|---|---|---|
| bevior_tree 0.12 | yes | shared tree Asset + per-agent boxed `NodeState`, exclusive-World tick | take shared-def/per-agent-state split; avoid boxed state + exclusive World |
| bevy_behave 0.5/0.6 | no (0.19 on main) | ego_tree cloned per agent; leaf spawns an entity with action components, reports via trigger | take "actions are ordinary components + ordinary systems"; avoid spawn churn, f32 Time timers, no reactive aborts |
| beet / beet_flow | main only | entity-per-node, observer cascades | avoid for 1000+ lockstep agents |
| big-brain 0.22 | archived | utility AI (IAUS) | — |
| bonsai-bt 0.14 | engine-agnostic | struct holds def+state | — |
| seldom_state, bevy_dogoap | no | FSM / GOAP | — |

Conclusion: build our own; borrow patterns.

## Shipped-game lessons
- **RimWorld**: ThinkTree (priority selector + PrioritySorter utility) *chooses* a Job; JobDriver runs Toils (init/tick, completion mode, fail conditions). Re-think on job end + constant think tree for interrupts. `ReservationManager` (target, layer, max claimants) released at job end. Needs with thresholds, staggered ticks. → *BT chooses, job runs; re-think on events.*
- **The Sims**: smart objects advertise need satisfaction; agent scores with per-need curves + distance. → *knowledge lives in objects.*
- **Halo 2**: priority list re-evaluated, higher branch preempts; stimulus behaviors. **Halo 3**: objectives layer for squads → template for group control.
- **Unreal BT**: event-driven, decorators with observer aborts (Self / LowerPriority / Both), services refresh blackboard, latent tasks; **shared tree asset + flat per-agent memory with per-node offsets.**
- **Hysteresis** on thresholds (enter urgent < 20 %, leave > 60 %) to stop flip-flopping.

## Determinism (lockstep-ready)
- f32 +,-,*,/,sqrt are deterministic across x86_64/aarch64/wasm; transcendentals, `mul_add`, SIMD-vs-scalar are not. → sim state in integers / fixed point; floats only in presentation.
- Query order depends on archetype history; inserting a render component on a sim entity on a client but not on a server changes order. → no presentation components on sim entities; `SparseSet` for churny components; **sort by `SimId` wherever order matters**.
- `Entity` ids differ across machines → `SimId(u32)` from a sim-owned counter + `SimId → Entity` map.
- Unordered systems → ambiguity detection = Error on sim schedule. `par_iter` only for per-entity writes. HashMap iteration order depends on insertion history → BTreeMap / sorted Vec.
- Sim never reads `Time`; driven by a tick counter. RNG = counter-based `hash(seed, sim_id, tick, purpose)` (SplitMix64) → independent of processing order.
- Desync detection: hash sim components in SimId order every N ticks.

## Tree definitions
Code builder and (later) RON both compile to one flat serializable IR: pre-order nodes with child ranges, leaves reference registered `ActionId`/`CondId` with typed params. Content hash comparable across peers.

## Recommended runtime shape
- Resources: `TreeRegistry`, `ActionRegistry` (start/abort fn ptrs), `CondRegistry`, `Reservations` (BTreeMap), `SimClock { tick }`, `WorldSeed`.
- Agent components: `SimId`, `Needs`, `Brain { tree }`, `BrainMem` (one u32 per node), `Blackboard` (small typed slots), `ActiveLeaf { node, status, started }`, `ThinkDue`, `Dirty` (SparseSet), one SparseSet component per action kind.
- Tick order (chained): ApplyInputs → Needs → Services → Think (sorted by SimId, single-threaded; aborts + starts + reservation claims) → Actions (one system per action component, may be parallel) → ResolveEffects (sorted) → Cleanup (release reservations, advance clock, optional hash).
- Status contract: `start` may return Success/Failure/Running; afterwards only the action system writes status; abort is synchronous and always releases reservations.
- Later: squad entity with its own Brain writes `Order`s/blackboard on members; multiplayer compares tree content hash at connect.
