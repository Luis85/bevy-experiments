# Launcher recovery workstream — 0.15

Implementation owner: build_environment_015.

## Implemented scope

- Your worlds offers Make backup beside eligible saved regional worlds. Backups and recovery remains reachable when a damaged world is omitted from valid profiles.
- One background worker creates or verifies/reopens a backup; navigation remains available, another operation/launch/close waits for completion.
- Native backup wrapper: private `world-backup.json` plus unmodified backend `bundle/`; canonical metadata is bounded to8KiB and pins title/character/quality/start hour/creation time/checkpoint generation/bundle identity. No extra files enter immutable bundles.
- Recent browser inspects at most 256 folders and shows 24. External wrapper folders use the same strict read/verification path. The exact most recent successful backup and recovered profile are pinned across navigation/refresh for the current menu session, retaining the 24-item limit; failed reopens remove the pin and show a failure instead of claiming success.
- Complete backend verification precedes a named review. Metadata bytes, canonical path, list revision and confirmation revision prevent silent retargeting.
- Restore revalidates metadata and bundle, creates a new private managed folder, copies exact dependencies and checkpoint into its `world/` child, validates character/profile, then publishes `world.json` last. No automatic launch, overwrite or deletion of retained backups.
- Managed world-profile loading treats its saved political-source setting as explicit, preserving deliberately politics-free restores.
- Protected output ancestors cannot occupy DB/WAL/SHM/journal namespaces. Symlinked inputs/ancestors, hard-linked or public wrapper records, unknown wrapper files, unsupported record fields/versions, invalid character/name and changed generation reject.

## Files owned

`crates/terra-demo/src/launcher/{mod,ui,model,recovery,recovery_tests}.rs`; small `main.rs` profile-resolution fix; test-only terra-persistence dependency in demo manifest/lock; `docs/RECOVERY_BUNDLES.md`, `docs/GAME_FLOW.md`.

## Focused verification completed

`cargo test -p terra-demo launcher:: --locked --offline -- --nocapture` passed all 23 tests (0 failed/ignored). Log: `/tmp/terra-build/launcher-focused-015.log`.

The 11 recovery tests cover exact checkpoint/character/presentation/dependency restoration after original loss, stale record and generation rejection, malformed/tampered external metadata, exact selected profiles, protected output ancestors, symlink/hardlink/extra-file rejection, explicit politics-free profiles, stale queued selections/cancelled reviews, one bounded background worker with clear failure, navigation during verification, and completed-result discoverability despite 24 future-dated list entries.

Navigation agent independently reviewed the implementation, reported the bounded-list handoff P2, and then passed a read-only review of the pinning correction and regression. Review: `/tmp/terra-build/review-launcher-015.md`.

Retained report gate exact test:
`launcher::recovery::tests::managed_recovery_journey_preserves_complete_world_after_original_loss`

It emits one `LAUNCHER_RECOVERY_REPORT {JSON}` line, with schema_version/source_generation/nonempty_regions/paged_records/global_records/players/living_bodies/resources and boolean continuity checks. No private paths, inventories or session identities in the report.

Optional native review fixture retention: set `TERRA_LAUNCHER_REVIEW_DIR` to a NEW absolute private temporary directory for the exact report test. The original inputs are deleted by the rehearsal; surviving `<dir>/player-data` includes a restored managed world and two intact native backup wrappers. Other tests never use that environment variable. With no variable, fixtures are deleted on drop.

The exact retained journey passed, with one report line in `/tmp/terra-build/launcher-review-015.log`. Retained private fixture: `/tmp/terra-native/launcher-review-fixture-015/player-data`. Generation 2, 19 nonempty regions, 57 paged records, 25 global records, 2 players, 52 living bodies and 16 resources. All 12 continuity/result booleans are true. Native UI acceptance remains with the parent; the fixture is preparation evidence, not a screenshot or native play claim.

After these tests, two UI unavailable-reason strings were polished to describe unsupported recovery honestly; no recovery behavior changed. `RECOVERY_BUNDLES.md` now includes a manual-retention/unsupported-scope table. `GAME_FLOW.md` lists the live Company activity and `backups` screen. Parent has been told the final release gates must include these copy-only changes.

Parent's initial exact Comet native fixture preparation test compiled but failed a preparation-only 3 m approach assertion at `frontier.rs:1621`; `/tmp/terra-build/comet-review-015.log`. Shared Cargo slot was released after reporting that result. Parent then corrected the assertion to the actual 20 m `INTERACTION_REACH_M` rule, and the content owner reran the exact test successfully in `/tmp/terra-build/comet-review-015-final.log` (SHA-256 `bd52b35c348765ff167f2a2c2fc25262b28bb8491fe341179ac4fbf6c400339c`). The ordinary EnterVehicle command passed at 5.517181314610624 m after separate 0.05 s garage/vehicle approach steps; rental debit was 6,180 money units from the normal 25,000-unit starting wallet. The retained paused fixture is `/tmp/terra-native/comet-review-fixture-015/{comet.json,comet-config.json}`. This closes preparation; native visual acceptance is still separate.

## Native UI route for parent

Start `terra-demo --menu --user-data <private-player-data> --menu-screen worlds`. Use Make backup, wait for named Backup ready, then Backups and recovery → Verify → Back (cancel); Verify again → Recover as new world. Success returns to Your worlds and focuses the new recovered copy without launching. Exercise external folder field and changed metadata separately. New --menu-screen backups is available for review setup. Capture 1100x700 /150% text and ordinary native launch of restored profiles, including politics-free case.

## Explicit remaining limits

Only supported local reference regional worlds/builtin catalog; operator policy still required for portable JSON/server/historical/live/custom catalogs. Whole native wrappers may be copied elsewhere but no offsite service/encryption/automatic retention/signing is introduced. Unix private permissions are enforced; Windows execution/ACL qualification remains open. Filesystem administrator races remain outside the maintenance contract. Presentation pack binaries are application content, not retained world dependencies.
