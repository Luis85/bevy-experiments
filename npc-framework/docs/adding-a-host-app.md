# Adding a host app to this workspace

A game outside this repository depends on `npc_framework` (plus `bevy`) and needs none of this. Inside the
workspace, a new windowed host (like `apps/rts_units_demo`) runs into one more check: **`cargo deny`'s `bevy`
ban**.

## The rule

`deny.toml` bans the `bevy` umbrella crate except as a *direct* dependency of the crates listed in its
`wrappers`:

```toml
{ crate = "bevy", wrappers = ["npc_view_helpers", "village_view", "npc_tools", "village",
  "testbed_view", "actor_testbed", "rts_units_demo", "game_menu"], reason = "..." }
```

The umbrella pulls the renderer, and the point of the ban is that no core or headless crate (`npc_ai`,
`npc_kernel`, `npc_needs`, `npc_jobs`, `npc_perf`, `village_content`, `npcctl`, the capability crates) ever
depends on it; they name the `bevy_*` sub-crates they need instead. `xtask/tests/arch_fitness.rs` checks the
same boundary on the transitive closure.

So a new crate that depends on `bevy` directly (an app, or a `*_view` crate) fails `cargo deny check bans`
(`cargo xtask verify`'s `deny` step) until its name is in that list: a ban error on `bevy` that names
the new crate as the direct parent.

## What to do

`deny.toml` is a protected oracle (CLAUDE.md, golden rule 4; the `protect_goldens` hook blocks edits). An agent
adding a host app does **not** edit it: it stops and asks the user, stating

1. the crate name and that it is presentation or an app (it renders, so it may depend on `bevy`);
2. that `arch_fitness` still holds for it (for an app that must not reach `content/*`, an
   `*_uses_only_facade` test like `rts_demo_uses_only_facade`);
3. the one-line change: append the crate name to the `bevy` entry's `wrappers`.

The user makes (or approves) that edit. Nothing else in `deny.toml` needs to change for a host app: a host
that reaches the presentation helpers through the facade's `view-helpers` feature does not depend on
`npc_view_helpers` directly, so its ban (wrappers `npc_framework`, `village_view`, `village`) is unaffected.

## Open proposal (not applied)

The list grows with every host app, and each addition needs a protected-file edit. Two options for the user
to decide on, both keeping the invariant (no core crate on the umbrella):

- **Invert the list:** ban `bevy` with no `wrappers` exception for apps, and instead keep the core crates
  off it in `arch_fitness` alone (it already walks the closure). Simpler `deny.toml`, one source of truth.
- **Keep the list, document the step:** as now; this page is the step. Every new app is one reviewed line.

Until then, the list above is the rule.

## The rest of a new host, for reference

- Depend on `npc_framework` with the features you use (`view-helpers`, `command`, `nav`, `perf`, ..) and on
  `bevy` with explicit features (`default-features = false`, the `zstd_c` backend; see `apps/village`'s
  `Cargo.toml` for why `ruzstd` is avoided).
- Profiler in two calls: `DefaultPlugins.set(npc_framework::perf::log_plugin())` and
  `npc_framework::perf::NpcPerfPlugin::new().tag("my_app::sim", "my.sim")`. Budgets for your own mechanics:
  `.budget("my.sim", 10, 2)` on the plugin, or `app.register_budget("my.sim", 10, 2)` (`PerfAppExt`) from your
  sim plugin, in any order.
- RTS input: `view_helpers::command_ui::CommandUiPlugin`; choose the right-click order kind with
  `RightClickMode` from your hotkeys (`Move` / `AttackMove` / `Patrol` / `Custom(name)`), register resolvers with
  `CommandUiAppExt` (`app.add_right_click_order(name, f)`, `app.add_context_order(name, f)`, or the `_subset`
  variants returning a `ClickOrder` for just some selected units; any plugin order), script selections with
  `select_units` + `DragBox`, pick with `pick_unit`, and mark non-unit targets (walls, buildings) `PickTarget`
  so right clicks pick them and selection does not.
- Game orders: `UnitOrder::Custom { name, args }` with `app.register_custom_order(name, executor)`, or a game verb
  on `Interact` with `app.register_interact_verb_executor(verb, name, executor)`; walk into reach with
  `npc_command::approach_step` / `approach_then` (`ApproachSpec::to_point`, `ApproachSpec::to_footprint`); see
  [using-npc-framework.md §6.5](using-npc-framework.md#65-rts-command-command).
