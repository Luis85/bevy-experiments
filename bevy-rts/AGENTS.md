# Frontline assistant guide

Read `CLAUDE.md`, `OWNERS.toml` and the relevant `.claude/rules/` files before
changing code. Simulation is deterministic fixed-point/integer state at 20 Hz;
keep `fl_sim` headless and rendering behind the existing client boundary.
Read `docs/adr/` and the applicable spec before changing contracts.

This workspace now lives inside the Bevy Experiments monorepo. ADR-0023 updates
the old machine-specific worktree/build instructions in ADR-0013. Use the current
checkout or an isolated `codex/` branch/worktree as appropriate. Cargo uses the
normal platform compiler/linker and local `target/` by default; sccache is optional.

Run commands from this directory:

```sh
cargo test --locked -p <crate>
cargo xtask gate task --track <track> --base origin/main
cargo xtask gate pr --base origin/main
```

Select tracks from `OWNERS.toml`. Contract changes need an ADR and a `Refs:`
trailer. Follow `Change-Class:`/MCR rules for gameplay changes. Do not invent
co-authorship attribution. Never edit generated Clippy files directly or weaken
protected tests, replays, snapshots or performance baselines; follow the blessing
procedure. New test expectations require independent reasoning and review.
The legacy worktree cleanup hook only removes optional build dirs under
`C:/t/fl/`; ordinary local `target/` directories are intentionally outside it.

Additional tools for full gates: cargo-nextest, cargo-deny, cargo-public-api and
the nightly named in `xtask/api-toolchain.txt`. Game control uses BRP methods
`fl.*`; give concurrent game instances separate ports. A successful build does
not establish rendering correctness or performance. See `docs/debugging.md`.
