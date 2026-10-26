# Bevy Experiments

My Bevy playground: games, deterministic simulations and reusable Rust frameworks.

| Project | What it contains | Launch from its directory |
|---|---|---|
| [Frontline](bevy-rts/README.md) | WWII RTS foundation with deterministic simulation, terrain, selection, formations and pathfinding | `cargo run --locked -p fl_game` |
| [Legion Battle Lab](legion-battle-lab/README.md) | Native battle sandbox with configurable armies and a GPU crowd renderer | `cargo run --locked --release -p legion` |
| [NPC Framework](npc-framework/README.md) | NPC behaviour, needs, crafting, vehicles and agent control; village and Actor Testbed demos | `cargo run --locked -p game_menu` |
| [Terra World](terra-world/README.md) | Geographic environment SDK, world explorer, sandbox and multiplayer reference | `cargo run --locked -p terra-demo` |

Each directory is an **independent Cargo workspace**, with its own lockfile,
assets, tests and architecture. There is deliberately no root `Cargo.toml`:
run Cargo from the experiment you are working on. All currently use Rust
**1.99.1**, edition 2024, and Bevy **0.20**.

## Get started

Install [Rust with rustup](https://www.rust-lang.org/tools/install), Git and the
native build tools described in [CONTRIBUTING.md](CONTRIBUTING.md). Windows needs
Visual Studio C++ Build Tools with the Windows SDK. Desktop games also need a
working graphics driver. The first Bevy build can take 15 minutes or more.

```powershell
git clone https://github.com/Luis85/bevy-experiments.git
cd bevy-experiments
rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
cd npc-framework
cargo run --locked -p game_menu
```

For a headless starting point, run `cargo run --locked -p npcctl -- run --seed 1
--ticks 100` inside `npc-framework/`. Assets and third-party notices are included;
keep them when distributing a game.

## Development

With [PowerShell 7](https://learn.microsoft.com/powershell/scripting/install/installing-powershell)
(`pwsh`), use the same commands locally and in GitHub Actions:

```powershell
pwsh -NoProfile -File scripts/dev.ps1 doctor
pwsh -NoProfile -File scripts/dev.ps1 verify -Project terra-world
pwsh -NoProfile -File scripts/dev.ps1 test -Project legion-battle-lab
```

`-Project all` is the default. Tasks: `doctor`, `fmt` (check only), `check`,
`test`, `clippy`, and `verify` (format check, Clippy, workspace tests). Frontline's
baseline excludes its `gpu_*`, `brp_*` and long `milestone_*` tests, matching its
ordinary gate; use `-IncludeDeviceTests` on a suitable local machine to include them. Projects run
sequentially to limit memory and disk use. GitHub Actions runs each workspace in
its own Windows job. These are baseline checks; Frontline and NPC Framework also
have architecture, replay and policy gates described in their agent guides.
GPU screenshots, interactive playtests and performance measurements need a local
machine and are separate from CI.

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks and the Git workflow.
AI coding assistants should start with [AGENTS.md](AGENTS.md), then the selected
project's guide. Existing plans and research are retained as development history;
they do not imply that every planned feature is implemented.

## Licensing

The repository license is [MIT](LICENSE). Workspace manifests declare their own
license expressions; Terra includes its MIT and Apache license texts. Third-party
assets retain their original licenses and attribution: see
[NPC credits](npc-framework/assets/CREDITS.md),
[Legion notices](legion-battle-lab/THIRD_PARTY.md), and
[Terra notices](terra-world/THIRD_PARTY_NOTICES.md).
