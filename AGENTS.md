# Repository instructions for coding assistants

This is a monorepo containing **four independent Cargo workspaces**, not one
combined Rust workspace. Read the relevant nested `AGENTS.md` before editing:

| Directory | Entry point | Run |
|---|---|---|
| `bevy-rts/` | `AGENTS.md`, `CLAUDE.md`, `OWNERS.toml`, `docs/adr/` | `cargo run --locked -p fl_game` |
| `legion-battle-lab/` | `AGENTS.md`, `README.md`, `ARCHITECTURE.md` | `cargo run --locked -p legion` |
| `npc-framework/` | `AGENTS.md`, `CLAUDE.md`, `docs/adr/` | `cargo run --locked -p game_menu` |
| `terra-world/` | `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md` | `cargo run --locked -p terra-demo` |

## Working agreement

- Use the user's current checkout unless isolation is needed or requested. Prefer
  `codex/<description>` for new branches. Preserve unrelated user changes.
- Run Cargo **inside the selected workspace** so local aliases/config load.
  Do not introduce a root Cargo workspace or merge the projects' lockfiles.
- Rust is pinned to 1.99.1 and Bevy to the versions declared in each manifest.
  Resolve APIs from the installed crate source or project research, not memory.
  Do not upgrade dependencies or change global Rust/Cargo/agent configuration
  unless the task calls for it. Use `--locked` for standard builds and tests.
- Preserve deterministic simulation boundaries: stable IDs, sorted iteration,
  integer/fixed-point authoritative state, seeded counter-based randomness,
  headless simulation and separate rendering. Follow each project's specific rules.
- Never weaken tests, remove assertions, bypass policy hooks, silently rebaseline
  replays/snapshots or edit generated Clippy configs. Follow the workspace's
  documented approval/blessing procedure for protected files.
- Make focused changes. Add a regression test for changed behavior when useful.
  Keep existing assets, attribution and license notices; do not commit secrets,
  local agent state, saves or build products.
- Keep builders bounded: at most two Cargo builders, preferably one for a cold
  Bevy build. Each worktree/workspace needs its own build output directory.
- Update relevant documentation when commands, architecture or behavior changes.
  Existing plans and ledgers are historical context, not automatic new tasks.
- Report what changed, exact checks/results and remaining limitations. Never
  claim GUI validation from compilation alone. Do not push or open a PR unless
  the user requested that repository operation.

## Verification

Root helper: `pwsh -NoProfile -File scripts/dev.ps1 verify -Project <directory>`.
It runs format check, Clippy with warnings denied, and workspace tests. Frontline
targets its maintained game/tooling packages, excluding historical `probe_*` packages, and
excludes its device/BRP/milestone test families by default, matching its ordinary
gate; use `-IncludeDeviceTests` for the complete suite on suitable hardware.
Use focused package tests while iterating and the workspace's additional gate
before completion. See `CONTRIBUTING.md` and the nested guide.

Claude Code hooks in nested `.claude/` directories are not automatically loaded
by other tools or from the monorepo root. Read the referenced rules and enforce
their invariants explicitly; do not assume an installed hook protected the edit.
