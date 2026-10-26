---
paths:
  - "crates/fl_client/**"
  - "crates/fl_game/**"
  - "features/*/client/**"
---

# Client rules (spec §5.1, §5.3, §5.5; ADR-0001, ADR-0002)

- **No sim access.** Client crates (`fl_client`, `fl_feat_x_client`) never depend on `fl_sim`, not even as a
  dev-dependency, and never on another feature's sim crate. They read `fl_view` snapshots and events only.
  `fl_game` is the one crate that links the client and `fl_sim` together; keep it to assembly and wiring.
- **Commands only.** Input becomes `PlayerCommand`s sent through `LocalDriver` (input delay 1). The client never
  mutates sim state. Testbed actions that change the sim are `debug.*` command kinds; "reveal all" and the
  controlled-player switch are client-local render overrides.
- Encode command kinds and content IDs by **name** through the string tables from the match header.
- **Windows:** access the window as `Option<Single<&Window>>` (likewise for cameras that may be absent), so
  systems keep running headless, offscreen and in agent mode instead of being skipped or panicking.
- Coordinates: sim `(x, y)` maps to Bevy `Vec3::new(x, height, y)`, yaw = `-heading_radians`. Placeholder meshes
  are built with their length along local +X.
- Floats are fine here; `HashMap`/`Instant` need the scoped `#[expect]` the generated `clippy.toml` asks for.
- Logging goes through Bevy's `LogPlugin`; use module-path `tracing` targets.
- Bevy APIs: resolve every name with the `bevy-api` skill. Shaders are WESL (see `shaders.md`).
- BRP (`debug_remote` feature): register `fl.*` methods in the dev-only debug plugin; use your own port
  (`--remote-port` / `BRP_EXTRAS_PORT`). Screenshots come from an offscreen camera (`Screenshot::image`), so
  they work while the window is covered.
