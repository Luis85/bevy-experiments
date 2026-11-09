# Island colony iteration

The user requested implementation, subagent research/design/execution, then a comprehensive
review and another improvement pass. Scope is the existing NPC Framework checkout only.

## Baseline and coordination

Coordinated with the **Prepare NPC framework playtest** chat. Its clean worktree at
`2bb8a14` contains only documentation above main; no concurrent source edits or builds.
The current NPC baseline matches the previously tested native homestead increment. Prior
evidence: 1,749 tests, eight replays, six goldens; 12/12 residents over 14 days on seed 7.
That evidence is historical and does not certify this iteration.

## Workstreams

1. Economy/AI agent: validated authored routines, bounded GOAP over real economic facts,
   inspectable plans, long-run production/needs/conservation tests.
2. Colony/world agent: finite households, physical social life, funded housing projects,
   dormant inspector visitor, safe integer terrain commands and snapshot tests.
3. Presentation agent: colony ledger, resident inspection, housing/policy controls,
   landscape tools, character control and atmosphere; screenshots and input regressions.
4. Orchestrator: one-scenario launch flow, local persistence, world/AI editor, integration,
   documentation, build coordination and final gate.

## Completion checks

- Scenario tests and multi-seed economic runs; no currency or material grants in ordinary play.
- Record/command determinism, save/resume including edits and in-progress construction.
- Native modes and controls; inspect screenshots, exercise actual widget command paths.
- Independent review of simulation, host persistence and UI; repair concrete findings,
  then confirm the changes in a second pass.
- `cargo xtask verify --pretty` with the JSON verdict and skips reported; never change
  protected oracles or policy files to pass. Preserve unrelated monorepo modifications.

## Closeout

Implementation, independent architecture review, visual correction review and the
polishing pass are complete. `cargo xtask verify --pretty` returned `ok:true`:
1,816 tests, seven architecture checks, eight replays and six snapshots passed.
The bounded fourteen-day economy comparison, native captures, reviewed fixes,
remaining feature limits and Windows renderer/input caveats are recorded in
[the validation note](../../_notes/2026-11-08-island-colony-validation.md).
