# Adaptive regional residents: research, design and implementation review

Research date: 2026-11-09. Scope: regional labor, identity, physical production,
environmental responses, social opportunities and persistence. This extends the
verified island baseline; it does not establish that the new regional world has
passed the complete gate or a long survival soak. The orchestrator owns those
results. The user retained 3D presentation and survival/permanent death.

## Primary sources

The application column contains our design inferences. It does not claim these
games share this repository's deterministic architecture or balance model.

| Source | Relevant evidence and application |
| --- | --- |
| Luke Hodorowicz, [Game Code Design](https://shiningrocksoftware.com/2018-11-09-game-code-design/) | Hardcoded professions became difficult to extend; decisions scattered among buildings and needs produced interruption problems. Keep authored occupation policy and a single resident arbiter, with explicit ownership of each mechanic. |
| Luke Hodorowicz, [More Bugs: Pathfinding Problems](https://shiningrocksoftware.com/2013-11-21-more-bugs-pathfinding-problems/) | Linear distance gave bad home/work and supply choices. A coarse connectivity graph improved distance estimates while avoiding excessive full searches. Larger regional play needs reachable candidates and measured search cost, not just a larger camera. |
| Crate Entertainment, [Farthest Frontier Population](https://www.farthestfrontier.com/guide/villagers/population/) | Physical buildings support finite work positions. Flexible labor performs useful deliveries and construction; population growth requires provisions and housing. Use workplace capacity and explicit worker eligibility, without replacing dead named people invisibly. |
| Crate Entertainment, [Farthest Frontier Resources](https://www.farthestfrontier.com/guide/gameplay/resources/) | Transport and storage placement affect productivity; production thresholds and ecological resource differences create management decisions. Distinguish local stocks, future production and actual transported goods. Forest depletion and animal habitat should visibly affect useful work. |
| David Graham, [An Introduction to Utility Theory](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter09_An_Introduction_to_Utility_Theory.pdf), section 9.7 | Commitment, cooldowns and decisions after action completion prevent oscillation. Employment reconsideration uses integer priorities, a saved review deadline, a switching cooldown and safe work boundaries. |
| Jeff Orkin, [Three States and a Plan: The AI of F.E.A.R.](https://www.gamedevs.org/uploads/three-states-plan-ai-of-fear.pdf) | Goals and reusable execution actions can remain separate. Job opportunities should feed the existing Work decision; shared movement, gathering, crafting and trade perform the physical actions. |
| Jonathon Doran and Ian Parberry, [Emergent Economies for Role Playing Games](https://ianparberry.com/pubs/econ.pdf) | Their experiments cover supply shocks and changing labor composition. Their bankrupt-agent replacement is unsuitable for permanent residents: preserve identity, possessions, relationships and skills when employment changes. |
| Lars Doucet, [BazaarBot](https://www.gamedeveloper.com/design/bazaarbot-an-open-source-economics-engine) | A vanished profession can retain zero recorded profit and never be selected again. Evaluate vacancies and genuine unmet material demand. The article's multi-city trading section is a proposed extension, not evidence that transport already works in this game. |
| Richard Evans and Emily Short, [Versu](https://cs.uky.edu/~sgware/reading/papers/evans2014versu.pdf) | Social situations provide opportunities while individuals retain control. A market visit or gathering can fit below survival/work in the existing tree. A second social controller would recreate the ownership problem. |
| Bard Tree, [Architecture Overview](https://bardtreelimited.com/architecture/overview) | Explicit modules and shared contracts clarify ownership. Preserve facade registration, content composition, pure domain queries and host-owned time. Camera-dependent simulation frequency is incompatible with authoritative deterministic ticks. |

The supplied developer retrospective reinforces the single-engine and explicit
execution-seam design. Its suspended-tree reservations differ from this project's
cancel-and-replan emergency semantics. We retain the existing contract rather
than claiming equivalent suspension. The note's migration lesson supports an
opt-in profile and frozen construction records, not lenient loading.

## Baseline limitations found in code

- `Villager.role`, `Role::person` and colony integrity tied named people to their
  original occupations. Initial housing lived on workplace entities.
- Production selected the first matching producer and pooled all resident stock.
  Copying the village would concentrate work on early IDs and count remote goods
  as already supplied.
- Food preferences depended on the role discriminant; changing occupation would
  accidentally change a person's preferences.
- The existing GOAP domain excludes skill/tool-bearing recipes. New regional
  recipes remain compatible with this domain; a general planner eligibility
  extension still needs a shared core validation projection rather than copied
  crafting rules.
- Suppliers and resource patches used straight-line distances; hunters selected
  the first available animal. Those choices become conspicuous across villages.
- Social life is proximity/cooldown affinity and morale, not a comprehensive
  relationship, family, demographic or narrative system.

## Implemented ownership and behavior

`frontier::world_spec` freezes regional construction. Settlements and sites have
stable keys and simulation IDs. Each settlement has thirteen occupation sites,
fourteen independent homes and fourteen initial residents. The shared world
contains forty-two initial NPC identities. Initial wallets and supplies are a
finite construction endowment; changing jobs grants no cash, tools or goods.

`frontier::labor` owns persisted identity/qualification metadata and employment
transitions. `ResidentProfile` keeps a stable key, appearance index, preferred
food, allowed occupations and flexibility flag independent of current work.
Existing `Villager.role` and `workplace` remain the single current assignment.
`Employment` records pending requests, review and cooldown deadlines, sustained
blockage and the public reason for a transition. Initial skilled templates supply
the declared qualifications; changing employment never rewrites a character sheet.

The existing Work executor calls `consider_assignment` after immediate subsistence.
A transition is refused during committed work, rest, external control, queued
orders, urgent needs or an owned construction commission. Inspector requests are
recorded `AssignProfession` commands that wait for this same boundary. Capacity
and eligibility are revalidated at commitment; competing requests cannot overfill
a site. Explicit Stop, death and critical-need cancellation retain their existing
action ownership. A successful change keeps the home and possessions, removes
obsolete navigation, clears public job intent through production, invalidates its
plan and returns until the next decision receives a refreshed plan.

Automatic changes consider a missing occupation even before it has any profits.
Flexible workers and surplus workers can fill vacancies; sole useful specialists
retain their positions. A sustained source/access disruption permits reconsideration.
Essential functioning coverage is protected. Reviews occur at bounded tick
intervals and a successful change starts a 6,000-tick cooldown. These are authored
initial heuristics, not a claim of optimal labor allocation.

Production computes separate workplace-catchment rounds for the new profile,
merges actor-keyed plans and distributes additional work over eligible producers.
Physical stock is counted once in its economic catchment. A new profile selects
alternative recipes using available material paths and missing-input cost, so fish
meals and woven coats are not permanently hidden behind an earlier recipe entry.
Old profiles keep their original first-recipe and first-producer paths.

Suppliers prefer local reachable offers; gatherers use reachable route estimates;
hunters prefer reachable local prey. Exchanges still require actual proximity,
inventory capacity and both wallets. Prices and communal redistribution remain
one shared regional reference market/fund. There are no simulated independent
treasuries, caravan contracts or invented deliveries. Previously produced goods
remain sellable after an occupation change while personal reserves remain protected.

The view reads appearance and work projections. Labor observation exposes the
actor's own summary; inspector projections expose permitted civic information.
No view drives employment or writes authoritative components.

## Compatibility and persistence

All new state and commands register only for the `living_world` profile. Older
saved descriptors reconstruct their original registration sets and scenario
identities. The old founder-role validation remains active for old profiles;
regional integrity validates identities, allowed assignments, capacity and home
membership. Frozen profile values are checked against construction during strict
snapshot restoration. New regional fields are not silently invented in old saves.

Any future conversion of an old world should write a copied, versioned save and
validate it before replacement. Protected replay/snapshot baselines are unchanged.

## Post-implementation review and improvement pass

The source review revisited the Banished path-selection and Farthest Frontier
physical-resource guidance after the modules were authored. Concrete improvements:

- Bounded employment reviews and direct stable site-key lookup avoid repeated
  scene-scale searches inside assignment eligibility.
- Original manufacturing selected the first recipe for an output even after new
  alternatives were authored. A pure regional recipe selector now accounts for
  available upstream material paths and price; old profile behavior is unchanged.
- Initial spears and rods must be distinct usable tools, not invalid multi-unit
  stacks. The regional constructor now grants individual tool entities.
- Shared prey validation preserves the actual breeding refuge at both action
  start and completion, including competing hunters.
- Restoration validation must compare saved material claims to the exact work
  recipe, not merely check whichever materials a save happens to list. The review
  reported this to the action owner; strict colony integrity invokes its shared
  `validate_work` contract and rejects work on dead/reserved-arrival identities.

Focused authored regression coverage in `tests/regional_labor.rs` includes identity
and home independence, observer purity, real mid-craft pending reassignment,
simultaneous capacity requests, autonomous specialist loss, sustained depletion
and cooldown, individually usable hunter tools, snapshot continuation and
allocation to multiple producers. These tests were authored by this subagent;
the orchestrator runs Cargo and records exact outcomes.

## Remaining limits and validation needs

The content is a finite, deterministic regional simulation. Automatic labor
heuristics do not prove arbitrary disaster resilience. Immigration, births,
aging, formal apprenticeship, caravan ownership, independent regional prices,
and deep social narratives remain future mechanics. Residents still use the
same single daily-life tree with GOAP under Work. General externally authored
profession schemas and arbitrary behavior-tree topology are not provided by this
increment. Navigation estimates are bounded searches; inability to find a path
within that budget must not be described as proof that no path exists.

Run the focused labor, ecology and action tests, then baseline island tests,
multi-seed regional survival/shortage evidence, snapshot/replay continuation,
native captures and the full `cargo xtask verify` gate. Show a recognizable
resident changing workplace, actual local production and a player action changing
resource recovery. Compilation alone is not GUI or economic liveness evidence.
