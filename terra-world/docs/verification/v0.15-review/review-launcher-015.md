# Launcher recovery independent cross-review — 0.15

Reviewer: navigation_increment_015. Project: `/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next`.

## Scope and method

Read-only review of `crates/terra-demo/src/launcher/{recovery,mod,model,ui,recovery_tests}.rs`, the managed-profile branch in `crates/terra-demo/src/main.rs`, and the existing backend recovery/output-path guards they call. No Cargo command, project mutation, native run, or filesystem failure injection was performed during this cross-review. The owner is running backend regression tests; root owns rendered native interaction evidence. References below refer to the code at review time and may move during owner fixes.

## Finding: P2 — completed recovery output can disappear from its promised next action

**Files:** `launcher/mod.rs:149–182` (Created and Restored completion), `launcher/mod.rs:546–558` (navigation/refresh rescans), `launcher/model.rs:565–612` (first 256 entries and recent 24), `launcher/recovery.rs:366–388` (first 256 entries and recent 24).

After restore succeeds, `poll_recovery` receives the exact `RestoredWorld.profile_path`, but replaces the available world list with `recent_worlds`. It computes an optional matching index, then unconditionally tells the player that the separate recovered world is ready in Your worlds. If the bounded scan or recency truncation excludes that profile, `selected` is `None`; the player gets neither a matching Open action nor a direct path to the successful result. The backup creation branch likewise discards the exact returned `Backup` when refreshing the bounded listing. Its notice directs the player to Backups without retaining a named review action or showing the omitted folder.

**Deterministic reproduction below the directory scan ceiling:** have 24 valid managed world profiles whose last-played/creation timestamps are tomorrow (possible after the system clock has been corrected), then restore a backup at the current time. Sorting and truncation retain those 24 profiles and exclude the restored world. The completion still claims that the new world is available in Your worlds. For backup creation, 24 valid retained backup records dated tomorrow exclude the new backup in the same way. Above 256 directory entries, filesystem iteration order can also omit a new result even without a clock change; incomplete folders count toward that scan.

**Impact:** successful recovery is not reliably actionable from the UI that initiated it. This is a discoverability/handoff defect, not loss or corruption of the restored data.

**Recommended correction:** retain the exact completed output as a session-level identity, and pin that validated result within the bounded displayed list or expose an explicit named Open/Verify result action. Preserve the 24-item/256-inspection bounds, list revisions, stale-profile checks, and original-world protection. Reapply the result on navigation and refresh, since the Create completion notice sends the user through Backups, which scans again. Do not make scans unbounded to fix this.

Finding sent to root and `build_environment_015`; owner follow-up pending at the time of this note.

## Areas reviewed with no additional blocking issue identified

- **Publication and preservation:** backup record publishes only after the backend finishes; a recovered `world.json` publishes only after backend copy, source/config restoration, and profile validation. Failure cleanup is confined to a newly claimed output directory. No original-world overwrite or deletion was found.
- **Selection and review identity:** world list revisions reject stale indexes; selected profiles are compared before and after backup. Reviews retain canonical directory and exact bounded metadata bytes. Verify and restore require the retained bundle identity and generation, and metadata is rechecked around work. A verification finishing after navigation does not reopen the abandoned review.
- **Filesystem scope:** recovery paths reject symlink components and parent traversal, and ordinary/private file checks reject hard-linked records on Unix. Creating output ancestors protects database and reserved WAL/SHM/journal names. Restore refuses a destination inside its retained backup. Concurrent administrator mutation remains explicitly outside the quiescent-tree contract.
- **Metadata/work limits:** launcher records are limited to 8 KiB, canonical, schema checked, character/name/time bounded, and identity/generation checked. Wrapper names are exact. Listings inspect at most 256 entries and show at most 24 copies. Backend source/database/manifest limits remain in force.
- **Faithful recovery:** profile character, quality, and initial-hour settings survive separately from authoritative world data. The new-location backend rehomes runtime source/political/database/cache/manual-save paths. Managed profiles explicitly preserve a missing political source rather than acquiring a demo default. Previously saved profiles reject missing checkpoint files instead of silently generating an empty world.
- **UI state:** long work runs on one thread; polling does not block the UI waiting for unfinished work. A second operation, launch, or application close is rejected while work runs, with a visible explanation; navigation remains available. Recovery confirmation is tied to an exact review revision, with Back as the initial review focus. Errors and long backup paths are inside the scroll region. Existing Tab/Enter/Escape navigation is retained.
- **Privacy:** world data is retained as local private files, exact private archive content is not emitted in the bounded test report, and the UI states that local copies do not provide device-loss protection. Unix mode enforcement does not qualify Windows ACL protection or offsite/deployment recovery.

## Remaining verification scope

The reported P2 needs an owner regression covering omitted completed results through navigation/refresh. Root should finish actual native keyboard, minimum-window, text-scale, busy/error, cancellation/back, original-tree-loss and reopen journeys. This code review does not replace Windows clean-machine qualification, power-loss testing, network-storage failure handling, offsite transfer, or a deployment recovery-time objective; those remain outside this increment's stated local workflow.

## Follow-up: reported P2 corrected

The owner added `MenuState.completed_world` and `completed_backup`, with shared `refresh_worlds` / `refresh_backups` helpers. The exact completed paths are revalidated and placed first after completion, navigation and explicit refresh; existing entries are deduplicated and lists remain capped at 24. A failed reread clears the pin and produces a specific unavailable/error outcome rather than an unconditional ready notice. Revision checks and exact metadata/profile validation remain in place. Pins deliberately last for the current launcher session; they are not a new persistent world index.

I independently read the helper, completion and navigation code plus the regression `launcher::recovery::tests::completed_backup_and_world_stay_reachable_after_bounded_list_refresh`. It constructs 24 future-dated valid records for each browser, asserts raw scans omit the real completed output, then exercises the actual Bevy poll/action systems and confirms reachability after navigation/refresh, bounds and backup preservation. The retained `/tmp/terra-build/launcher-focused-015.log` records that case passing and **23 launcher tests passed, zero failed/ignored**. I did not rerun Cargo. No additional blocking issue found in this follow-up. Native journey acceptance remains parent-owned.
