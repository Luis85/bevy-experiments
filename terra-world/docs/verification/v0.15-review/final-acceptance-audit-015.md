# Independent final acceptance and desktop review — Terra World 0.15.0

Read-only review of `scripts/package_desktop.py`, the source assembly helper, native runner, CLI, payload allowlist, Comet config, native recovery fixture and retained 0.14 acceptance structure. No implementation, scripts or acceptance files changed.

## Result

No additional code or packaging blocker found. All **139 desktop allowlist paths** are unique and present. The standalone Comet pack and six-model composed pack are included; all six referenced GLBs are allowlisted and match their declared SHA-256 values. The connected heightmap manifest and current product, validation and gap reports are included.

The first 0.15 gate process was interrupted by an environment reset. Its report still says `running`; it cannot support final acceptance. The parent is preserving that attempt separately and restarting the exact frozen implementation. Final native receipts may be bound to the later completed gates only if their executable SHA-256 and implementation hashes match those gates exactly. Candidate records remain candidate evidence.

## Minimal extracted-package confirmation

1. Extract into a fresh `/tmp/terra-desktop-015` directory, validate ZIP CRC, unique paths, exact manifest file set and every SHA-256. Python `ZipFile.extractall()` does not necessarily restore Unix execute modes; explicitly apply the reviewed ZIP modes for `terra-demo` and `start-terra.sh`. Hash the extracted executable against the completed gate record. Do not edit packaged payload.
2. Create a fresh unrelated working directory and fresh external runtime/data directories. Set `XDG_DATA_HOME` to a fresh absolute directory for the launcher script. The script already adds `--menu --user-data ...` and changes directory to its own folder, so do not append a competing `--user-data` argument.
3. Run the extracted launcher script through `/tmp/terra-native/run_015.py --phase package_confirmation --binary ABS_PACKAGE/start-terra.sh --cwd ABS_UNRELATED --trace-log ABS_TRACE NAME -- --frames 90 --screenshot ABS_PNG`. Native runner options must precede NAME. Menu permits at least 30 frames; 90 gives screenshot write time. The runner hash here is the **shell script hash**, so identify it explicitly and separately retain the extracted `terra-demo` hash.
4. Run the extracted executable from the unrelated directory with the direct prepared-world arguments below. This avoids the golden Comet config's absolute checkout heightmap path and deliberately exercises the package's own source and pack files.

```text
--play
--heightmap ABS_PACKAGE/crates/terra-stream/data/heightmap-connected/manifest.json
--asset-pack ABS_PACKAGE/crates/terra-bevy/assets/transport-demo.pack.json
--no-default-politics
--region-store ABS_FRESH_RUNTIME/world.db
--quality performance
--radius 1
--time-scale 0
--camp-view
--noon
--user-data ABS_FRESH_USER_DATA
--telemetry ABS_TELEMETRY
--frames 140
--screenshot ABS_PNG
```

Direct-world `--frames` must be at least 100. Terrain capture additionally waits for resident/rendered chunks and zero pending/building meshes, so a nonzero exit is a failed smoke, not a screenshot success. `--menu` does not accept `--asset-pack` or telemetry flags.

5. Retain both process/log/trace records, the exact package manifest, screenshot and telemetry. Check zero checkout-path mentions in both traces; count successful opens of the **extracted connected manifest**, optional pack JSON and GLBs. Trace scope starts at the game/script process, not the runner's own source-hash reads. Both process exit codes must be zero; source snapshots must remain unchanged.
6. Require final telemetry to report graceful exit, expected connected heightmap identity, successful regional checkpoint, six-model optional pack identity, no import failures/rejections and no world/simulation/persistence errors. This direct fresh-world smoke may have no registered Comet instance. It proves packaged model content validation and world rendering; use the accepted paid-Comet native journey for a visible moving Comet claim. Budgeted distant wildlife fallback is expected and is not an import failure.
7. Rehash every packaged payload after both runs, and separately verify all currently allowlisted project bytes still match the package. Run `ldd` on the extracted trusted executable and retain the resolved-dependencies result. Do not claim another OS, GPU, physical input device or performance certification.

## Receipt contracts and order

- `docs/native-v15-verification.json`: release/status, final gate record SHA, accepted native binary SHA, source count 356, nonempty all-true checks, and artifact SHA map with paths relative to `docs/`.
- `docs/desktop-package-v0.15.json`: version/status, same final gate SHA and binary SHA, source count 356, nonempty all-true checks, exact packager output and package SHA/size. Any nested `{path, sha256}` pair must name a real project-relative evidence file; do not use absolute paths with that key pair.
- `docs/release-verification-v0.15.json`: actual gate/test totals, exact references to final gates, source manifest, native/desktop receipts and preservation report.
- Freeze packaged current docs **before** running `package_desktop.py`; finalize evidence outside that payload afterward. Do not edit allowlisted docs after desktop creation unless rebuilding and re-verifying the archive.
- Publish the final baseline comparison once, then the final release receipt, then execute the already reviewed source assembly helper. It independently verifies full gate sequence, source hashes, artifacts, desktop archive, 959 retained prior paths, 90 original inputs, 560 historical evidence files and all source ZIP CRC/SHA/path checks.

## Recovery evidence detail

The retained original-loss fixture has one existing recovered profile and two complete native backup wrappers. Original `Recovery valley` metadata binds Ari, outfit 5, Performance, no atmospheric sky, start hour 18 and checkpoint 2. Verification is not recovery. After the accepted UI creates a new profile, compare its entire SQLite logical contents with the selected immutable bundle **before opening it**, compare character/quality/hour, and verify absolute rehomed source/politics/database paths. New profile creation/last-played timestamps are intentionally new; requiring equality with the original world's creation timestamp would be an invalid test.
