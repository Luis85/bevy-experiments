# Living Island next iteration

> Historical plan, recovered on 2026-11-20 from the leftover branch
> `audit/npc-next-iteration` (`2bb8a14`). It was superseded the same day by
> [the island colony plan](2026-11-08-island-colony.md), which started from this
> baseline. The compact Island Village and Living Island Homestead worlds it
> names were removed with the older island tiers (ADR 0044); the island is now
> `island_content`, `island_view` and `island_game`.

## Starting point

Local `main` at `7bd5de8` contains the merged NPC Framework and browser viewer
playtest. The tested increment includes the residents' daily-life behavior tree,
needs-driven jobs and adaptive prices, finite gathering and hunting, physical
production and trade, and the native hero's cottage and grain garden. Native and
browser viewers can follow one host. The native panel and registered agent tools
submit actions to the same simulation.

- The merged source passed `cargo xtask verify --pretty` with `ok:true`, 1,749
  Nextest passes, 12 skips, eight verified replays and six goldens. The native
  widget rehearsal completed a cottage and crop loop without material grants.
  [Native validation](../../_notes/2026-11-04-native-homestead-validation.md)
- The expanded island kept 12 of 12 residents alive at every half-day checkpoint
  in a 14-day audit on seed 7. The shorter Living Island stability test covers
  three seeds and samples food availability every 200 ticks on the last day.
  The compact island also has trading-visitor and seasonal soaks. The longer
  audit can miss shortages that begin and end between checkpoints.
  [Expanded island validation](../../_notes/2026-11-02-living-homestead-validation.md)
- The authenticated Codex evidence covers a bounded two-turn compact-island
  rehearsal. The expanded-island trade and homestead evidence uses registered
  engineering clients. Record those separately in future closeouts.
  [Herp stream closeout](../../_notes/2026-10-29-herp-stream-closeout.md)

## Proposed focus

Make it easier for a hero to earn a living from a village with visible shortages,
recovering production chains and real NPC buyers. Begin with repeatable evidence
from the expanded world. Let the measured bottleneck determine the first change
to resident behavior, production, or trade.

### 1. Capture a repeatable economy baseline

- Start from `living_stability`, `living_replacement` and the registered Living
  cases in `island_playtest`. Use fixed seeds and comparable cases for an
  autonomous village, an active trading visitor, and a controlled cold-weather
  or supply interruption. Include seed 7 as the prior extended-run reference.
  Keep the longer matrix bounded and report the seed, profile, tick range and
  any scripted intervention for each case.
- Capture daily survival, time in urgent needs, essential stockout duration,
  successful production and sales by good, and intervals where an actor cannot
  resume its work. Record the reasons available from the resident decision,
  market and journal data. Use public summaries in the observer and reserve
  privileged inspection for the offline test harness.
- Check that a recorded workload repeats to the same hash and conserves coins.
  Include an uninterrupted run and a save/resume comparison at the same tick,
  using an active production order or a growing bed for the checkpoint.

### 2. Improve the first reproduced bottleneck

- Trace a measured failure through the existing job tree, navigation, input
  bundles and cash-backed demand. For example, distinguish a producer waiting for
  inputs from a buyer that has a funded bid but cannot reach a seller.
- Add a focused regression that reproduces the failure. Change the owning rule in
  village content or a shared action so residents, the native player and agent
  clients agree on the physical outcome. Rerun the baseline and report the
  before/after metric alongside survival, trade throughput and conservation.
- Keep compact saves loadable, retain stable names and integer accounting, and
  leave historical replay and golden files under their existing approval process.

### 3. Rehearse a day in the expanded village

- Start a fresh **Living Island Homestead**. Use native controls to gather or
  craft, build or tend a home, satisfy a need, and earn coins in an actual sale.
  Repeat the relevant path via registered tools on a separate disposable world.
  Confirm that inventory, payment and crop deadlines persist across resume.
- Follow the same live host from native 3D and the browser at 1x. Verify that
  speech, action outcomes, visible shortages and recovery cues make the activity
  understandable. Fix concrete usability problems found during the run and
  capture the affected screens again.
- Run `cargo xtask verify --pretty` inside `npc-framework/` and run viewer tests
  and the production build if its source changes. Record any skips and keep any
  authenticated provider rehearsal separately authorized and bounded.

New housing systems, crops and recipes can follow once the baseline identifies
which goods or activities would give residents and the hero more meaningful work.
