# Bevy 0.20: Remote Protocol, system spans, log layers (verified 2026-10-23)

Read from registry sources (bevy_ecs/bevy_log/bevy_app 0.20.0) and from bevy_remote at tag v0.20.0.

## BRP
- Turn it on with `bevy = { features = ["bevy_remote"] }`. Types: `bevy::remote::{RemotePlugin, http::RemoteHttpPlugin, BrpResult, BrpError}`.
- Default address is `127.0.0.1:15702` (`with_port`, `with_address`). Clients POST JSON-RPC 2.0 to `http://127.0.0.1:15702/`.
- **Custom methods:** use `RemotePlugin::default().with_method_main("npc/state", handler)`. In 0.20, `with_method` is private.
  - The handler is a system: `fn(In<Option<serde_json::Value>>, ...) -> BrpResult`. It may take `&mut World`.
  - Handlers run in the `RemoteLast` schedule, after `Last`, with exclusive World access.
- **Built-in methods:** `world.query`, `world.get_components`, `world.list_components`, `world.get_resources`, `world.mutate_components`, `world.insert_components`, `world.spawn_entity`, `world.despawn_entity`, `registry.schema`, `schedule.list`, `schedule.graph`, `rpc.discover`, plus `+watch` streaming variants.
- **Components and resources must be reflected:**
  - Components: `#[derive(Reflect)] #[reflect(Component)]`, plus `app.register_type::<T>()`.
  - Resources: `#[reflect(Resource)]`.
- **Screenshots:** a handler can `world.spawn(Screenshot::primary_window()).observe(save_to_disk(path))`. The capture completes a frame later.

## Per-system spans (`bevy_ecs/trace`)
- Each system has a cached span `info_span!(parent: None, "system", name = <system name>)`, entered on every run. Exclusive systems use the same path.
- **Time systems with `on_enter`/`on_exit` keyed by span id.** `on_new_span` fires only once per system, so it can't be used for timing.
- Spans are entered on worker threads, so the layer must be `Send + Sync`.
- Other spans: `info_span!("schedule", name = ?label)` per schedule run, and `system_commands` while deferred commands are applied.
- **Headless:** enable `bevy_ecs/trace` (and `bevy_app/trace`), then install `tracing_subscriber::registry().with(layer)` as the **global default before systems are added**. A span created while no subscriber is set stays disabled.

## LogPlugin (bevy_log 0.20)
- Fields: `filter: String`, `level`, `custom_layer: fn(&mut App) -> Option<BoxedLayer>`, `fmt_layer`.
- `BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>`.
- `custom_layer` is a plain `fn`, so it can't capture anything. To share a collector, stash it in a resource or static from inside the fn.
- The EnvFilter (`level`/`filter`) also applies to custom layers. System spans are INFO.
- Use `tracing-subscriber = "0.3"` (resolves to 0.3.23) and `tracing = "0.1"`.
- Profiler features:
  - `trace_chrome` (tracing-chrome 0.7) and `trace_tracy` (tracing-tracy 0.12).
  - Neither turns on `trace` by itself; also enable `bevy/trace`.

## Built-ins
- `bevy_diagnostic` has no per-system timing; `Stepping` steps systems but doesn't time them.
- A tracing layer is the simplest way to get per-system timing.
