# Island world observer — implementation and evidence

Date: 2026-10-29. Work performed in the user's existing checkout, preserving the pre-existing island
playtest/scheduler/economy changes and Terra work. No dependencies, registry registrations, replays,
goldens or protected policy files in the Rust workspace were changed.

## Delivered

- Sibling independent npm project `npc-world-viewer`, TypeScript / ExcaliburJS 0.32, Vite and local
  loopback Node hosting. The user selected pixel-art style; sprites/terrain are original cached Canvas
  rasters. Silkscreen is self-hosted with its OFL license in the distributed public files.
- Island projection v1: integer millimetres, sorted stable SimIds, actual terrain, native decorative
  paths, places/housing, resource patches, people/player, needs/activity/health/wallet/inventory and
  ground items. Diary pages and private host metadata are excluded. No sim state registrations.
- `npcctl serve --world-state-out FILE --world-state-interval-ms N` in scenario and explorer hosts:
  initial publication before readiness, configurable host-time interval (50–60000 ms, default 500),
  temporary-file sync/rename, warnings/retries after startup. Hooks run between ticks, including while
  paused; no simulation outcome depends on export timing or filesystem state.
- Node polling follows Windows atomic file replacement. Runtime schema, render/file budgets and
  last-valid-state retention protect the view from missing, partial and incompatible updates.
- Browser pan/zoom, fit, follow player, labels, selectable entities, inspection, directory search and
  filters; full replacements handle reset/despawn. Camera/selection remain local to the browser.

## Focused verification

Commands run from their respective project directories:

| Check | Result |
|---|---|
| `cargo test --locked -p testbed_content --test world_projection` | 3 passed: full island contents/order and hash equivalence, unsupported scene, physical diary visible without private pages |
| `cargo test --locked -p npcctl --bin npcctl` | 8 passed, including startup/periodic atomic publication without a hash change |
| `npm test` | 4 passed: schema/unsafe integers/duplicate IDs, stable-ID update/despawn, missing/invalid/atomic file replacement/recovery/reset |
| `npm run check:state -- ../npc-framework/target/world-state/island.json` | `ok:true`, real Rust export tick 11786, 23 entities / 10 people / 9 buildings / 4 resources |
| `npm run build` | TypeScript and production build passed; upstream Zod annotation and large bundle warnings remain informational |
| Production HTTP smoke | HTML 200 with built asset, exported JSON 200 with 23 entities, POST 405, foreign Origin 403 |
| `git diff --check` | Passed |
| Full workspace test phase within `cargo xtask verify --pretty` | Nextest: 1340 passed, 12 skipped (6 slow, 2 leaky); doc tests: 1 passed, 10 ignored, 0 failed |

The file feed also recovers when the original valid bytes are restored after an invalid update; a
paused publisher's repeated file modification time remains fresh even when its tick is unchanged.

## Browser validation

The real headless island host ran with seed 7 and a 500 ms export interval. In the in-app browser,
ticks advanced (838, 1530 and later 11691), villagers changed positions/activities, and Follow player
selected SimId 126 with live needs. Places + search `bakery` returned exactly Mira's bakery. No
browser console errors were reported. The map rendered through Excalibur actors and graphics.

Desktop 1280×720 and mobile configured 390×844 were inspected. Mobile full-page captures have 375px
content width because of the vertical scrollbar, with no horizontal overflow. A first visual check
fixed sidebar footer overlap, crowded person labels and camera fit after viewport changes. The
independent Impeccable review requested a pixel display font; its verdict pass scored that fix resolved
(`disposition: ship`, at the TYPE-fix scope). The documenter extracted DESIGN.md and its sidecar.

Final captures (local ignored verification artifacts):

- `npc-world-viewer/.impeccable/review/desktop.png`
- `npc-world-viewer/.impeccable/review/mobile.png`

These final captures show the accurately marked stale state after the owned simulation host was
stopped during full workspace verification. The last valid world remains visible. Earlier live
captures and AX observations confirmed advancing simulation state. This is browser validation,
not native Bevy GUI validation.

## Full Rust gate

`cargo xtask verify --pretty` completed with exit 1 and **`"ok":false`**. The sole failed step was
Clippy: pre-existing, untracked `apps/npcctl/tests/island_playtest.rs` has collapsible `if` statements
at **28:9** and **359:13**. This task has not changed that file or relaxed the lint. All other steps
passed:

- Format: all files formatted.
- Tests: 1340 passed, 12 skipped (6 slow, 2 leaky); doc tests: 1 passed, 10 ignored, 0 failed.
- Architecture: 7 passed.
- Replays: 8 verified (1 session, 3 testbed, 4 village).
- Goldens: 6 snapshots; none pending, checked with `INSTA_UPDATE=no`.
- Dependency checks: advisories, bans, licenses and sources all passed.

No verifier tools were skipped. The exact JSON verdict is retained locally in the ignored
`npc-framework/target/world-viewer-verify.json`. The gate is not green until those existing Clippy
diagnostics are addressed. New viewer/exporter focused tests, the real-export contract check,
production build and browser validation passed independently as recorded above. Temporary browser
preview servers and the owned demonstration host were stopped after validation.

## Increment boundaries

The producer adapter supports island village only and is opt-in. The browser is an omniscient local
operator observer, not a perception-scoped agent tool. Native-window export flags, other scenes,
remote deployment/authentication and gameplay inputs are outside this increment. A projection is not
a resumable save; final stop can leave the last interval's state. See the sibling README and JSON
contract plus ADR 0024 for launch/configuration, ownership and extension guidance.
