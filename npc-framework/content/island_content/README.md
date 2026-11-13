# island_content: Island Village

The simulation of the shipped game: the regional living island (`living_world`), with its residents,
economy, colony, terrain and saves. It is one of three island crates (ADR 0044): `content/island_view` draws
it and `apps/island_game` is the game. No island crate depends on a testbed or village crate, and the reverse;
`xtask/tests/arch_fitness.rs` enforces both directions. Like all content it depends only on the facade and
registers by stable name.

```rust
let mut app = island_content::new_sim_app(7, "living_world"); // or try_new_sim_app(seed, name, 0)
npc_framework::prelude::run_ticks(app.world_mut(), 600);
```

## One world

`living_world` is the only island. There are no older tiers and no profile gates. One construction builds it
everywhere: `construction::shipped()` (the embedded `living_world` scenario with the built-in world's
descriptor, JSON key `world.construction`) is what `try_new_sim_app`, every test and the replay factory build;
the game's New Game builds `construction::from_preset` of the chosen world. A save of another build is refused
with a plain sentence (`village::island_save`, `ISLAND_SAVE_VERSION`); nothing is migrated.

## Where things are

| Path | What |
|---|---|
| `src/village/**` | the island: residents, economy, colony, frontier, terrain, wildlife, saves, player journey |
| `src/construction.rs` | `Construction`, `from_preset`, `shipped` |
| `src/scenario.rs` | the island scenario (`content: "island"`), the loader, `new_sim_app` / `try_new_sim_app`, `island_plugins` |
| `src/lib.rs` | `IslandBasePlugin` (item kinds, skills, sheet templates, recipes, the played unit's `island_idle` tree) |
| `src/replay.rs` | the `npcctl` replay factory; island replays are named `island:<name>` |
| `scenarios/living_world.json` | the embedded scenario |
| `worlds/living_world.json` | the shipped world rules (the game installs a copy as `<home>/island-village/living_world.json`) |
| `trees/resident.json` | the residents' behaviour tree, as data (`docs/island-ai-authoring.md`) |
| `actors/human.json` | the played actor preset |
| `tests/` | the island's behaviour tests; `island_soak_fast` is the fast multi-seed soak |

## From the command line

```sh
cargo run --locked -p npcctl -- run --scenario island:living_world --ticks 4800   # a bare `living_world` works too
cargo run --locked -p npcctl -- replay record --scenario living_world --out r.json   # header: island:living_world
cargo test --locked -p island_content [--test <file>]
```

The island has no protected replay or golden yet; they are staged through `docs/rebaselining.md`.
Guides: `docs/living-world.md`, `docs/island-life.md`, `docs/island-economy-authoring.md`,
`docs/island-ai-authoring.md`, `docs/architecture-island.md`.
