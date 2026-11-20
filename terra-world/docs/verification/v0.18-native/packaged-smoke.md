# Extracted Linux desktop smoke — 0.18

The final Linux archive SHA-256 is
`4f2fffea8081d4928569d670ea403a05a74b619a06b8fc3e7a96ff54558455bb`.
It was freshly extracted to a private temporary directory with archive permission
modes preserved. Its executable hash matches the canonical accepted binary:
`e16c25fabf30d8e96ff6b68cea99766ea1f4cc4d67afc1fbaeaff4433ffa8881`.

Two actual native processes completed **exit 0**, without timeout or forced kill:

1. The extracted `start-terra.sh --frames 100` ran from an unrelated working
   directory. The launcher selected its own packaged root, and XDG_DATA_HOME
   pointed to private temporary runtime storage. This verifies the shipped
   executable launcher mode and its working-directory handoff on this host.
2. The extracted binary ran from the extracted package root with the packaged
   connected-heightmap configuration and original character/transport/wildlife
   pack. The regional database, user data and final telemetry destination were
   explicitly private temporary paths. `--frames 100` bounded native updates.

The final direct-run telemetry reports 49 rendered chunks, no pending terrain,
the expected connected heightmap identity, and the seven-model pack identity
with **91 ready instances**, zero loading/fallback/failed/rejected instances and
no model error. This does not mean all seven models were instantiated; the
scene's allocation report records six resident models. The final checkpoint is
generation **25**, with no pending/blocked storage, no unsaved changes and no
world/simulation/storage error. Process exit 0 is retained separately from the
`graceful_exit` diagnostic sample.

Every extracted regular-file path, SHA-256 and permission mode was compared
before and after both processes. The entire payload is unchanged, with no file
additions/deletions, and the source ZIP and accepted executable hashes remain
unchanged. Runtime databases and user data are excluded from delivery.

See [exact commands, process exits and payload snapshot](packaged-smoke.json),
[direct-run telemetry](packaged-prepared-telemetry.json), and the two process logs.
These checks use fresh Xvfb over loopback TCP and Linux llvmpipe software Vulkan.
They qualify neither a clean machine nor Windows, hardware/controller behavior,
frame-time targets or unattended long sessions. No new screenshot/visual review
is inferred. The earlier interactive native evidence remains unchanged.
