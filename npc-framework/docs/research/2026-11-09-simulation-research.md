# Island simulation research and improvement brief

Research date: 2026-11-09. Scope: autonomous production and trade, resident AI,
interruption and reservations, households, social behavior, survival, ecology, and
content authoring. This research follows the verified player-flow baseline. It is
an implementation brief, not evidence that the proposed changes have passed tests.
See [baseline validation](../_notes/2026-11-09-island-player-flow-validation.md).

The recommendations below are our application of the sources to this repository.
They are not claims that another game's architecture or balance will work here.
The user confirmed the 3D island and current survival/permanent-death rules.

## Primary evidence

Each entry summarizes only the part relevant to this project. Original papers are
linked directly; no secondary summaries or forum advice are used as technical
evidence. The older papers are useful design evidence, not current engine API
documentation.

| Source | Relevant evidence | Application to this code |
| --- | --- | --- |
| **Bard Tree architecture**, updated August 2026: [overview](https://bardtreelimited.com/architecture/overview), [modules](https://bardtreelimited.com/architecture/module), [repositories](https://bardtreelimited.com/architecture/repository), [object repositories](https://bardtreelimited.com/architecture/object-repositories), [orchestrator](https://bardtreelimited.com/architecture/orchestrator) | Explicit startup registration and ownership separate feature modules, collection access, operations, and update coordination. Its orchestrator can distribute work at different rates. | The facade, island composition root, `SimIndex`, domain queries, and host driver already provide these seams. Keep them. Camera-dependent authoritative update rates would violate this project's deterministic contract, so that particular optimization is inapplicable. |
| **Jeff Orkin, GDC 2006**, [Three States and a Plan: The AI of F.E.A.R.](https://www.gamedevs.org/uploads/three-states-plan-ai-of-fear.pdf), especially pp. 8–11 | Goals and actions are decoupled through shared world knowledge. The door/window example shows that replanning becomes useful when previous failures change what the planner knows. Procedural preconditions complement symbolic planning. | Preserve one need/work arbiter and shared physical executors. Investigate why a first step failed and whether another target is usable before enlarging the GOAP search budget. |
| **Éric Jacopin, Game AI Pro 2**, [Optimizing Practical Planning for Game AI](https://www.gameaipro.com/GameAIPro2/GameAIPro2_Chapter13_Optimizing_Practical_Planning_for_Game_AI.pdf), sections 13.2–13.3 | The chapter separates correctness tests, branching-factor stress tests, and profiling. It also describes text-authored actions as a way to iterate independently of recompilation. | Measure expanded states, repeated first-step failures, and representative supplier/recipe branching. Do not add an index, cache, or planner rewrite solely because the current implementation scans collections. |
| **Jonathon Doran and Ian Parberry, 2012**, [Emergent Economies for Role Playing Games](https://ianparberry.com/pubs/econ.pdf), sections VI–IX | Experiments include temporary timber-production interruption and delayed downstream recovery, not only steady-state trading. The paper also reports sensitivity to floating-point perturbations. Its adaptive population model replaces bankrupt professions. | Add a bounded shortage/recovery experiment and retain integer accounting. The island has fixed named professions and permanent lives, so the paper's recovery results cannot establish this village's resilience, and automatic agent replacement must not be copied as a hidden bailout. |
| **Lars Doucet, 2013**, [BazaarBot: An Open-Source Economics Engine](https://gamedev.net/tutorials/programming/general-and-gameplay-programming/bazaarbot-an-open-source-economics-engine-r3206) | The implementation separates economic exchange from game-specific production. Doucet describes a missing-profession trap: profitability-based replacement cannot select a profession whose recorded profit stays zero because nobody practices it. | Keep `market`, funded production, and physical actions separate. Specialist loss is an explicit feature gap; do not label a healthy twelve-resident soak as proof that the economy recovers from permanent labor loss. |
| **Luke Hodorowicz, 2013**, [More Bugs: Pathfinding Problems](https://shiningrocksoftware.com/2013-11-21-more-bugs-pathfinding-problems/) | Banished's author found that a destination close in straight-line distance could be much farther by walking. Full path searches for every candidate were too expensive; a coarser connectivity graph supplied cheaper estimates. | The island's supplier/source selectors and GOAP estimates deserve a blocked/long-detour candidate fixture. A new hierarchical navigation system is not justified for this small map without measurements. |
| **Luke Hodorowicz, 2012**, [Update](https://shiningrocksoftware.com/2012-06-05-update/) | The development notes join mechanical polish with feedback: avoid duplicate household stocking, show reserves/status, reflect occupancy in chimney smoke, and expose soil quality. | Surface the existing hearth's actual fuel and duration rules. Keep household procurement singular. Ecological additions should expose their state and consequences; cosmetic weather or new crop names alone do not create ecology. |
| **Luke Hodorowicz, 2018**, [Game Code Design](https://shiningrocksoftware.com/2018-11-09-game-code-design/) | The retrospective describes hardcoded professions/materials becoming expensive to extend, AI decisions spread across owners, and state-machine composition failures. It advocates configurable content and clearer ownership, while admitting the proposed redesign still requires validation. | Extend the current typed catalogs and immutable construction seams incrementally. Avoid a wholesale scripting/runtime replacement during polish. The existing one-window host and shared control styling already address related duplication. |
| **Richard Evans and Emily Short, 2014**, [Versu—A Simulationist Storytelling System](https://www.cs.engr.uky.edu/~sgware/reading/papers/evans2014versu.pdf), sections III–IV and XI | Social practices suggest context-dependent actions while individual agents retain control. The system exposes reasons and available actions so players can understand the simulated social rules. | A small shared availability query and legible relationship feedback fit this island better than a second social controller. A complete social-practice language, emotions, and dramatic narrative are separate features. |

The user-provided engineering retrospective adds an independent requirement:
avoid duplicated need/work controllers and scene-search dependencies; make
interruption, ownership, and upgrade behavior explicit. Its example preserves
reservations during suspension. This island currently cancels and replans NPC
work when a critical need interrupts it; that existing contract must not be
described as equivalent suspension. The note's recommendation to test upgrades
in slices should guide a future schema migration, not justify lenient loading.

## Current implementation assessed

`village/behavior.rs` supplies the single resident tree and its saved
`LifeDecision`. `goap.rs` is a bounded work planner under that tree; it does not
own a clock. `ai.rs` performs physical actions. `production.rs` alone mutates
`ProductionPlan`; `colony/housing.rs` exposes construction/material intent;
`colony/social.rs` owns social effects. Generic crates remain content-independent.
These are useful existing boundaries, not missing abstractions.

The tests cover funded joined inputs, finite cash, double-count prevention,
physical trade/craft/hunting, clothing replacement, emergency construction
cleanup, observer purity, and snapshot continuation. The ordinary colony soak
covers three days at seeds 7 and 42; an explicitly ignored extended test covers
336,000 ticks at seed 7. Neither test demonstrates general population growth,
specialist replacement, or arbitrary disaster recovery. No such claim follows
from the new player cottage/garden playtest either.

## Prioritized compatible improvements

### 1. Explain the real household heating rule

**Observed gap:** `content/testbed_content/src/village/colony/housing.rs` consumes
one available log carried by a living household member within 1,500 mm of the
door, during cold weather, when the hearth is unlit. It heats the home for half a
simulation day. The actor observation in `village/agent.rs` exposes cold/warmth
and clothes, but no corresponding own-home heating state. The Pouch's rest copy
in `content/testbed_view/src/player_hud/model.rs` only says heating helps.

**Proposed change:** housing owns one pure, actor-scoped heating query. Expose
whether the actor has a household, whether it is heated, remaining heat duration,
and the public fuel/reach rule; show concise guidance near home/rest controls.
Use existing authoritative fields. Do not disclose co-residents' private bags,
create a second household inventory, or consume anything from the view.

**Validation:** query purity and privacy; cold/unheated versus heated and warm
weather; absent household; same one-log debit at actual ignition; continued heat
after save/load. Extend `tests/island_colony.rs` hearth coverage and an
actor-observation test. Native Pouch/home capture verifies the explanation fits.

### 2. Use one eligibility rule for social meetings

**Observed gap:** `colony/social.rs::social_target` filters living actors and a
cooldown, while `social::tick` grants meetings from proximity and cooldown alone.
It can award affinity to sleeping or committed-working participants. This is a
model simplification rather than evidence of corruption; it conflicts with the
module's stated low-priority leisure behavior.

**Proposed change:** define a pure shared eligibility predicate in the social
context and use it for both target choice and settlement. Initially exclude
resting, committed busy work, and urgent survival states, while preserving the
same deterministic pair ordering and existing fields. Reuse the exported
`village::activity` projection and active policy thresholds where appropriate;
do not duplicate another list of work components or critical-need constants.
Do not reject every `activity.busy` value blindly: it also includes walking, so
that would invalidate an actor's own social approach on its next decision.
The social context supplies
opportunities; it must not cancel work or write movement. Explain the resulting
behavior plainly instead of claiming deep emotions or reciprocal conversation.

**Validation:** nearby resting/busy/urgent actors produce no meeting or affinity;
eligible pairs do; resuming eligibility works; gathering policy and cooldown
still apply; target inspection remains pure; save/restore stays equivalent.
Extend the existing social proximity test and add focused fixtures. Re-run the
colony soak because changed leisure timing can affect routes and work cadence.

### 3. Establish bounded economic recovery evidence

**Observed gap:** `tests/colony_economy_soak.rs` demonstrates baseline circulation,
not resilience to an introduced supply interruption. `living_production.rs`
already covers a priced-out market recovery, so another identical price fixture
would add little.

**Proposed check:** a declared headless fixture temporarily exhausts an upstream
patch and schedules its ordinary replenishment. Let the existing systems respond;
do not inject emergency goods or money. Track currency including purses/fund,
resident survival, relevant source and downstream production, transactions, and
recovery after replenishment. Use bounded sample intervals and an explicit
deadline. Compare continuation through a snapshot inside the disruption.

**Acceptance:** the test should demonstrate a particular recoverable shortage,
or report the actual failure. Do not loosen survival assertions, change balances
merely to pass, or call this proof of recovery after losing the only specialist.
That latter case requires a labor/population design, not a hidden rescue rule.

### 4. Make planner limitations visible before expanding it

**Observed gap:** `goap.rs::Facts` contains goods, remaining sources, coins, and
position. It does not simulate future pack capacity, path feasibility, or station
reservation availability. `domain` chooses one supplier and gather source per
good. Execution safely revalidates, but `explain` combines unavailable supplies
and exhausted search budget into one message. `routing.rs` correctly bounds
corner-recovery attempts; it does not provide alternative-target planning.

**Proposed first step:** separate read-only planning outcomes such as no funded
goal, unavailable prerequisites, and bounded-search exhaustion, without adding
authoritative state. Add a fixture comparing a blocked or expensive detour target
with an available alternative. Measure the result before deciding whether a
bounded alternative list or existing-grid reachability query is warranted.

**Validation:** plans remain pure and deterministic; rejected first steps cannot
spend money or reserve twice; a deliberately tiny search budget is distinguishable
from missing supply. Existing `tests/colony_ai.rs`, GOAP unit tests, and
`tests/colony_route_recovery.rs` provide the nearby regression suite. Persisted
failure memory or new navigation caches need a separate save-state design.

### 5. Resolve mode-switch interruption semantics explicitly

**Observed inconsistency:** `testbed_view/player_actions.rs::release` calls
`stop()` before detaching native control. Inspector/Editor requests use that path.
The title path now uses navigation-only suspension and preserves construction.
Changing camera/control mode can therefore cancel work that Save and title keeps.

**Proposed investigation:** add a regression showing the current mid-build,
mid-craft, and queued-navigation behavior across Character → Inspector/Editor →
Character. Prefer preserving committed work when a mode change merely changes
control, provided the existing detach/action ownership contract permits it.
Keep explicit Stop, critical-need cancellation, and death semantics distinct.
Do not retain foreign controller authority or force a tick to settle receipts.

## Separate feature scope and cautions

- **Demographics and flexible labor:** named residents have fixed professions;
  households provide beds and fuel rather than family formation, aging, births,
  immigration, or automatic worker replacement. Those Banished-inspired features
  need explicit economic and persistence contracts.
- **Richer relationships:** affinity is a simple symmetric accumulated value.
  Personality, differing preferences, gifts, conflicts, schedules and relationship
  consequences require game design beyond the eligibility correction.
- **Ecology:** current resource replenishment and seasonal grain availability,
  plus timed grain/berry beds, do not model soil depletion, crop rotation,
  biodiversity, or weather-dependent harvests. Such additions need visible rules,
  balancing, and migration decisions, not presentation-only promises.
- **General content/BT editor:** current `island_ai` JSON edits safe priorities,
  thresholds, intervals and planner budgets. Tree structure, profession catalogs
  and much of `village/geography.rs` remain Rust-authored. A general graph editor
  or resource-schema import is valuable future scope, not an existing capability.
- **Scaling and scheduling:** profile before indexing `SimIndex` scans or pairwise
  meetings. Keep profiling outside authoritative hashes and preserve stable
  iteration. Camera distance must never choose which authoritative ticks occur.
- **Migrations:** preserve frozen registry names and strict saves in this pass.
  New persisted state needs versioned transformation fixtures and rollback rules.
  No source recommends silently replacing protected replay/snapshot baselines.

## Suggested execution order

Implement household feedback and social eligibility as separate content/view
workstreams; add the shortage/recovery evidence; investigate mode interruption
in the host/action workstream. Keep planner diagnostics small and pure. Then run
affected tests, native captures, independent review, and the final full gate.
Record improvements actually implemented and all remaining limitations in the
validation report; this research does not itself satisfy that verification.

## Implementation handoff

The scoped content changes are now authored for root verification: actor-owned
heating facts with shared hearth constants; a shared social availability query;
active-garden guidance; pure distinct planner diagnostics; and a bounded
shortage/recovery test with snapshot continuation. No new registered or persisted
state, or protected baselines were introduced; the actor observation has additive
heating fields. The planner's action
ordering, costs, budgets and selected plans are unchanged. Navigation alternatives,
demographic/labor changes, richer relationships and ecology remain separate scope.
No Cargo tests were run by this research/implementation agent; the orchestrator
owns integration checks and the final verdict.

The subsequent full gate exposed a housing-procurement liveness failure under
the changed social routes. The unchanged two-day commission test left Jun one
log short while an unaffordable forecast clothing bid reserved the remaining
cash. Later general work depleted some of his planks as well. Failure diagnostics
confirmed a funding problem rather than a blocked construction footprint.
The corrective change uses the production context's existing whole-bundle
financier for explicit commissions after urgent bodily/cold needs, before future
comfort reserves. It separately accounts for the log used to make missing planks
and the logs retained for construction. Initial isolated commission verification
passed. A subsequent exact-tick coat check caught a real cold-recovery omission
but was also too narrow to judge garment production: tick 96,000 precedes the
next day's finite redistribution. An independent code review confirmed that
redistribution would fund the cashless tailor from existing wealthy wallets.
No spare liquidation, price changes or extra currency were introduced.

The corrective cold path funds real household fuel before forecast clothing and
lets unprotected low-warmth residents use existing home rest even at full energy.
If heat/fuel is unavailable, night-time shelter yields to real fuel procurement;
the resident then returns to heat. The added post-construction contract now spans
the complete 96,000–120,000 redistribution/procurement/production interval.
Observed evidence: cottage completed at tick 8,800 under the unchanged 48,000
deadline; all twelve residents alive and 2,400 currency conserved through 120,000;
one new garment produced and traded in the final interval; builder warmth 33,364
through real home heat, with no claim that he was wearing a coat. The latest
targeted colony/finance/recovery suite passed 34 tests; the legacy production
suite passed 17. The orchestrator still owns the complete workspace verdict.
