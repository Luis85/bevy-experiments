# Development guide

## Setup

Install Git, Rust through rustup, and PowerShell 7 for `scripts/dev.ps1`.
The script is optional: ordinary Cargo commands work inside each workspace.
Toolchain files pin Rust 1.99.1 with rustfmt and Clippy; do not change the global
Rust default. Commit each application's `Cargo.lock` and use `--locked`.

Native desktop prerequisites:

- Windows: Visual Studio C++ Build Tools, MSVC x64 compiler and Windows SDK.
- Linux: a C/C++ toolchain and `pkg-config`. The combined games can require
  `libasound2-dev libudev-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev`. Consult each
  workspace README for its enabled features and platform validation.
- macOS: Xcode Command Line Tools. Platform support must be checked per game.
- Running a renderer also needs a graphical session and compatible GPU driver.

```powershell
rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
pwsh -NoProfile -File scripts/dev.ps1 doctor
```

The doctor validates all four manifests and prints their build directories.
VS Code users can open `bevy-experiments.code-workspace` to register all four
Cargo projects with rust-analyzer without sharing personal `.vscode/` state.
No global cache tool, custom linker, absolute build directory, API key, server or
Git LFS installation is required for the included offline examples. Optional live
data providers and external agents have additional documented setup.

Cargo discovers configuration from the current directory upwards. Always `cd`
into the chosen workspace before running Cargo; merely passing `--manifest-path`
from the repository root does not load its `.cargo/config.toml` aliases.
See the [Cargo configuration reference](https://doc.rust-lang.org/cargo/reference/config.html).
Compiler caches or short build paths are personal choices: use environment
variables such as `RUSTC_WRAPPER`, `CARGO_BUILD_BUILD_DIR` or `CARGO_TARGET_DIR`,
or your user Cargo configuration. Avoid absolute paths in tracked config.
Each workspace owns its `target/`; never share it between simultaneous worktrees.

## Daily work

1. Pull `main` and create a focused `codex/<description>` or feature branch.
2. Read root and project `AGENTS.md`, the project README and relevant decisions.
3. Change the smallest relevant crate; keep the four workspaces independent.
4. Run focused tests, then the checks below. Record commands and failures.
5. Commit source, assets, lockfiles and docs together as appropriate. Open a pull
   request with the problem, resulting behavior and validation evidence.

```powershell
pwsh -NoProfile -File scripts/dev.ps1 fmt -Project npc-framework
pwsh -NoProfile -File scripts/dev.ps1 check -Project npc-framework
pwsh -NoProfile -File scripts/dev.ps1 verify -Project npc-framework
```

Standard commands inside a workspace:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

For Frontline, append `--exclude probe_dynlink --exclude probe_remote` before
the `--` separator when using `--workspace`: these throwaway historical API
probes are outside its default members and its maintained game/tooling gate.
The helper applies this package selection automatically.

To apply formatting, use `cargo fmt --all` in the selected workspace. Check-only
commands never rewrite source. Avoid more than two Cargo builders on one machine;
prefer one for a cold graphical build. Large optimized builds need substantial
free disk space. Do not rebuild all games for a documentation-only edit.

The helper's Frontline baseline excludes `gpu_*`, `brp_*` and `milestone_*` tests,
as its ordinary project gate does. The direct `cargo test` command above includes
them. On a suitable local machine, use `-IncludeDeviceTests` with the helper to
run the complete suite. These exclusions are explicit test selection, not changes
to the test sources or oracles.

Additional project gates:

| Workspace | Additional verification |
|---|---|
| Frontline | `cargo xtask gate task --track <track> --base origin/main`; `cargo xtask gate pr --base origin/main` before merging. See `OWNERS.toml`, `CLAUDE.md` and ADR-0023. Requires cargo-nextest; API checks use the nightly named in `xtask/api-toolchain.txt`. |
| NPC Framework | `cargo xtask verify` (or `--quick` while iterating). Its JSON verdict must report `"ok":true`. Optional cargo-nextest/cargo-deny are detected by the gate. |
| Legion | Baseline commands plus local native smoke/render checks in its README. |
| Terra | `bash scripts/verify.sh` inside `terra-world/` also exercises the headless adapter examples. |

The root CI baseline does not replace these project gates or interactive visual
verification. GPU, BRP and performance checks belong on a suitable local machine.
Record existing failures honestly; never regenerate an oracle to make CI green.

## Assets, generated files and secrets

Version source assets, attribution, scenario fixtures and approved regression
baselines. Existing screenshot evidence and historical plans remain useful.
Build outputs, editor state, local agent settings, credentials and runtime saves
are ignored. Do not commit `.env`, tokens, private keys or live agent sessions.
Run `git status --short` and review the staged diff before every commit.

Preserve the supplied third-party license files. The root MIT license and the
workspace manifests are retained from the original projects; confirm the desired
license grant before separately publishing dual-license crates.

## AI assisted changes

Give the assistant a target workspace, concrete desired behavior, constraints
and a reproducible validation command. Use root `AGENTS.md` for shared rules and
the project guide for architecture and protected baselines. Claude-specific hooks
are configured inside Frontline and NPC Framework; launch Claude Code from that
workspace to load them. Other assistants must follow the same invariants explicitly.

For a visible change, include a screenshot or playtest result. For simulation
changes, verify determinism and replay compatibility. State which checks were
run, skipped or blocked and why; do not invent validation or performance claims.
