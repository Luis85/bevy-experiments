# Content and transport presentation increment 0.15

Owner: content_increment_015. Workspace: `/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next`.

## Implemented scope

Original Comet compact hatchback through the validated asset pipeline: exterior, transparent cabin glazing, four empty seats, dashboard, mirrors, lights, bumpers and alloy wheel spokes. It uses the existing 1.9 × 1.65 × 4.2 m Compact dimensions and 2.55 m axle separation, with 0.31 m nominal wheel radius. It changes presentation only, not collision/body, authority, ownership, rental eligibility, boarding commands, saved character data or vehicle paint data.

New JSON target `{"type":"vehicle_model","value":"compact"}` restricts the example to the existing catalog model, leaving pickups and other vehicles unchanged. All nine catalog models are accepted, unknown names rejected, model targets resolve before generic `kind: vehicle`, and a model hint cannot apply to another actor kind. Lookup uses the same public/private transport appearance API, so remote vehicles do not require private owner state. ActorSceneAnchor shape stays unchanged. Optional new fields serialize away when absent; new target/rig fields require a 0.15 reader even though schema 1 remains compatible with old packs.

The optional `vehicle_motion` animation policy names 2–8 rolling nodes and up to 4 steering pivots, with a finite radius in 0.05–3 metres. Validation enforces existing unique names, distinct rig nodes, non-root wheels, no nested rolling wheels, at most one steering ancestor per wheel, every steering pivot parenting a wheel, unit scale throughout the wheel ancestry, and rejects conflicting channels in every clip, including unused clips. Wheel and steering node rotation channels are allowed as export previews and overwritten by the runtime rig. Translation/scale channels on rig nodes and other animation on their ancestor chain are rejected.

Runtime phase integrates signed admitted body velocity projected onto actual heading, in f64, without animal-gait floors or clip-rate caps. Reverse reverses phase; zero speed retains phase; full pause returns before clips, blend, phase, heading history or steering changes. Local rotations preserve each node's rest rotation and hierarchy. Manual steering can turn at rest. Autonomous/public steering derives accepted heading change and known catalog wheelbase; absent metadata still rolls but does not invent a wheelbase. Each binding owns its scale/policy and each instance owns its phase. Admission reserves the largest selected clip's channels plus each rolling/steering channel (Comet: 5 + 4 + 2 = 11); all 91 model nodes remain counted.

Procedural actor/fallback remains visible until a ready validated scene takes over. Existing root visibility hides the exterior in first person and keeps the original procedural cockpit. No humanoid substitution or boarding occupant claim is made.

## Files

Runtime/import:
- crates/terra-bevy/src/asset_pack.rs
- crates/terra-bevy/src/asset_authoring.rs
- crates/terra-bevy/src/render_scenes.rs
- crates/terra-bevy/src/render_pack.rs
- crates/terra-bevy/src/render_animation.rs
- crates/terra-bevy/src/render_budget.rs
- crates/terra-bevy/src/render_budget_tests.rs
- crates/terra-bevy/tests/transport_assets.rs

New original content:
- crates/terra-bevy/assets/transport-pack/generate.py
- crates/terra-bevy/assets/transport-pack/comet.glb
- crates/terra-bevy/assets/transport-pack/pack.json
- crates/terra-bevy/assets/transport-pack/README.md
- crates/terra-bevy/assets/transport-demo.pack.json
- crates/terra-bevy/assets/transport-authoring.recipe.json

Presentation polish:
- crates/terra-demo/src/driving.rs
- crates/terra-demo/src/interface.rs
- crates/terra-demo/src/interaction.rs

Handoff docs:
- docs/ASSET_AUTHORING.md
- docs/ASSET_PACKS.md

No original file under wildlife-pack, terrain or traffic fixtures was modified. Parent owns version, global reports, native fixture, release gates, desktop payload and third-party-notices statement.

## Content shapes and identities

New GLB: 210,148 bytes, 91 nodes, 78 meshes, clips `parked` (2 s) and `rolling` (1 s). Original stdlib Python mesh/material/animation generation, no downloaded art/textures/fonts. Generator normal mode writes only the four new generated artifacts; `--check` is read-only and deterministic.

- GLB SHA-256: `e13f7026ad6bee911f52c664e480cfd5f42a3417d2ecd463ea755b7ce72479c4`
- Standalone transport pack: 1 model, 1 binding, identity `sha256:a62831e42111472101fd308ace21b5f7f0d68f7c6a533578f4b97b900acdc96b`
- Direct combined `assets/transport-demo.pack.json`: 6 existing-path models, 6 bindings, identity `sha256:235d1eff7604771e9a5ee78569093df56253439254d38fffda346f03e4a519c0`
- Composed `assets/transport-authoring.recipe.json`: 6 sources/models, 6 bindings; published output identity `sha256:b0732efa65fdfc2a0d4ed91a83b850b8790ea80d697991efd0870c9ec24da0cf`
- Default original wildlife pack remains `sha256:406c12494416b472ca5bee80a809e0ef30a45e382240cab01faa9081cd838d6d`.

The direct combined manifest stays under the common assets root and references contained wildlife/transport child paths, with no duplicated GLB bytes or path escape. Its path/name identity differs intentionally from recipe-built content-named output. The Comet-only manifest leaves wildlife procedural; the combined demo/recipe restores all five original wildlife models.

## Review-driven paint correction

An imported/override scene has a fixed livery; the previous Garage paint button could charge 5 credits for an invisible change. The native Garage now explains the fixed livery and disables paint whenever the selected actual kind/model resolves to a scene. It uses the renderer's model/species/kind precedence, not the showroom selection. Loading/degraded scene bindings stay fixed-livery for this purpose to avoid charging immediately before replacement becomes ready.

Queued actions recheck the current registry. A local displayed-paint context binds principal, restore revision, selected vehicle/model/current paint and garage, and only exists while Garage is visible. Dispatch consumes it, rejects changed contexts/pack, rechecks ordinary service/cash conditions, and only then creates the existing authority command. Duplicated queued clicks cannot reuse the same display. Procedural vehicle paint still uses the unchanged paid command. There is no global domain restriction; saved paint remains usable by other clients/appearances.

Two new driving tests cover fixed-livery charge prevention, showroom-vs-actual selection, generic vehicle and inappropriate human override behavior, ordinary procedural debit/paint, duplicate clicks and changes to pack, selection, restore, paint and panel. Native resource plumbing is optional so minimal test apps remain compatible. All eight focused native driving tests passed, including both new paint regressions; no failures or ignored tests.

## Verification completed

`cargo test -p terra-demo --bin terra-demo driving::tests --locked --offline`: 8 passed; 0 failed/ignored. Receipt `/tmp/terra-build/content-015-driving.log`, SHA-256 `27994f5cceba4688cf44cee1cf24bb5dd9a08c67cdf98aa09ccbf6f6e0123b74`. This also compiled all native source and optional registry plumbing.

`cargo test -p terra-bevy --no-default-features --test transport_assets --test asset_authoring --test asset_cli --locked --offline`:
- 9 authoring + 1 CLI + 5 transport tests passed; 0 failed/ignored.
- Log `/tmp/terra-build/content-015-headless.log`.
- Covers actual GLB/clip admission, bounds and duplicate targets, legacy exact identity, invalid wheel policies, unused-clip/ancestor scale conflict, reproducible six-model recipe publication and original source-byte preservation.

`cargo test -p terra-bevy --lib --locked --offline`:
- 99 passed; 2 existing network tests failed only at UDP bind with `PermissionDenied` in the sandbox.
- Log `/tmp/terra-build/content-015-render.log`.
- Passed all new specificity/public model lookup, binding-scale/hierarchy fallback, wheel phase/reverse/pause/independent binding/slow-frame/huge-finite-delta tests and rig admission cost test.
- Network tests need the normal release-gate escalation; this is not a content failure.

Existing built terra-assets CLI accepted:
- `/tmp/terra-build/content-015-inspect.json`
- `/tmp/terra-build/content-015-standalone.json`
- `/tmp/terra-build/content-015-demo-pack.json`
- `/tmp/terra-build/content-015-build.json`
- `/tmp/terra-build/content-015-composed.json`

Private built recipe output: `/tmp/terra-build/content-015-composed-pack/pack.json`.

All owned Rust source was rustfmt-formatted. Native visuals/gameplay, full gates, packaging and archive verification remain with parent. Do not claim native forward/reverse, first-person or visual acceptance until those runs finish.

## Remaining production gaps

This is a representative original transport handoff, not complete production-art qualification. Open: production human rigs/retargeting, customization attachments, seated humans/boarding transitions and occupancy mapping, dynamic vehicle paint/attachments, audio, DCC export presets, material/LOD/performance qualification, and target hardware/platform coverage. Empty seats are geometric detail, not rendered passenger occupancy. The existing procedural cockpit and character customization remain the tested fallback.

## Native review fixture recommendation to parent

Reuse the prepared heightmap App and physical garage approach from frontier.rs::prepared_company_review_journey_reaches_paid_setup_and_a_physical_loaded_hold in a separate new test. After physical arrival, apply ordinary RentVehicle(Compact), assert exact rental_offer total debit and ownership/stock changes, physically approach and EnterVehicle, then publish a new private LocalWorldSave/config. No forced balances/vehicle insertion or original fixture changes. Preview via --asset-pack using the six-model combined pack, and compare forward/reverse/stop/pause, first-person, exit and Garage fixed-livery caption against the unchanged procedural pickup.

## Independent cross-review

Completed read-only navigation review at `/tmp/terra-build/review-navigation-015.md`; no P1/P2 source defect found, source budget/reconnect/patrol contracts and accepted baseline hashes checked. Two optional coverage suggestions were documented; parent explicitly declined broadening tests absent a concrete defect. Content independent review completed at `/tmp/terra-build/review-content-015-by-recovery.md`: no additional blocker found. Reviewer independently ran generator --check and compared eight original wildlife inputs to accepted 0.14 hashes, all matching.

## Parent-owned Comet fixture execution

After the focused driving pass, ran the parent's exact `frontier::tests::prepared_comet_review_uses_paid_rental_and_physical_approach` test with `TERRA_COMET_REVIEW_OUTPUT=/tmp/terra-native/comet-review-fixture-015`. It passed (1 pass, 0 failed/ignored). Receipt `/tmp/terra-build/comet-review-015-final.log`, SHA-256 `bd52b35c348765ff167f2a2c2fc25262b28bb8491fe341179ac4fbf6c400339c`. Published `comet.json` and `comet-config.json` in that private output folder. Report: 25,000 initial cents, 6,180 rental cost, 4,500 bond, 18,820 remaining; active rental; actual pre-boarding distance 5.5171813146 m within the existing 20 m contract; 0.05 s physical updates for each short approach; world starts paused. No position overwrite or synthetic cash/vehicle insertion. This prepares native review; it is not a walking-distance or performance benchmark. Shared Cargo slot released to parent.

## Native review-driven HUD correction

Parent observed the World drawer covering most of the driving card at 150% text scale while chopped card text protruded. Read screenshot first, then changed only driving.rs so both floating driving cards share the existing route-map rule: hide while World, Inspector or Performance is open, restore when closed. Footer and panel controls are unchanged. Actual ECS regression covers every panel combination, close/reopen, first/third person and RTS. All 9 driving tests and the exact existing scaled-footer layout test passed. Detailed record `/tmp/terra-build/review-native-hud-015.md`. Running candidate executable was left untouched; parent owns next native screenshot confirmation.

- `/tmp/terra-build/content-015-hud-polish.log` — SHA-256 `97c9ace9fa35adc6bbe57d295ad656cc1914e718dd73893923b51da5cc4874cb`
- `/tmp/terra-build/content-015-hud-layout.log` — SHA-256 `369a0a77805c7448159810456ba52301f85f18c7193aaa6f24a2dd5dd8cfd352`
