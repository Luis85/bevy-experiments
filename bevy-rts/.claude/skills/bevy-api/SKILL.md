---
name: bevy-api
description: Resolve a Bevy 0.20 API (type, method, trait, plugin, system param, feature flag, BRP method, WESL import, or a 0.18/0.19 to 0.20 migration) from the pinned local sources before writing, reviewing or explaining Bevy code. Use whenever code touches bevy, bevy_ecs, bevy_app, bevy_remote or bevy_brp_*, or a Bevy name is uncertain. Never answer Bevy API questions from memory.
---

# bevy-api: resolve Bevy 0.20 APIs from the pinned sources

Bevy 0.20 shipped after your training data, and the workspace pins `=0.20.0`. Names, modules, features and
signatures you remember from 0.14–0.19 are often wrong (Messages vs Events, `bevy_shape`, WESL, required
components, `Single`, …). **Never write or approve a Bevy API from memory.** Resolve it in this order and stop
at the first step that answers the question.

## 1. Local sources (primary)

The exact crates the workspace builds are in the cargo registry:

```
~/.cargo/registry/src/*/bevy*-0.20.0/
```

On this machine: `C:\Users\<you>\.cargo\registry\src\index.crates.io-*\bevy_<crate>-0.20.0\`. Look in:
- `src/` for definitions and doc comments (the `bevy` umbrella re-exports `bevy_internal`, which re-exports the
  `bevy_*` crates; `bevy_internal-0.20.0/Cargo.toml` lists the cargo features and what each enables);
- `bevy-0.20.0/examples/` (the shipped examples, organised by topic) for working usage;
- `tests/` and doc-tests for edge cases.

Search with the Grep tool or `rg -n` (for example `rg -n "pub struct Single" ~/.cargo/registry/src/*/bevy_ecs-0.20.0/src`).
Find a definition first, then a use in an example. Never edit anything under `~/.cargo`.

### 1b. Migration guides and release notes (local)

`~/.cargo/registry/src/*/bevy-0.20.0/_release-content/`:
- `migration-guides/*.md` (one file per change, 0.19 → 0.20) and `migration_guides.md`;
- `release-notes/*.md` and `release_notes.md`.

Grep them for the old name you remember to find its 0.20 replacement.

### BRP and `bevy_brp_*`

- `bevy_remote-0.20.0/src/` (built-in `world.*` and `rpc.discover` methods, `RemotePlugin`, `RemoteHttpPlugin`).
- `bevy_brp_extras-0.23.0/` and `bevy_brp_mcp-0.23.0/` (the MCP server in `.mcp.json`): the default port is
  15702 (`DEFAULT_REMOTE_PORT`); `BRP_EXTRAS_PORT` overrides it; `register_agent_tool` publishes `fl.*` methods
  to `brp_list_agent_tools` / `brp_execute`. Each agent that runs the game uses its own port.
- Built-in `world.*` methods see only the client's main World; sim state is reached through `fl.*` methods.

## 2. `docs/research/bevy020-api-reference.md`

Verified snippets for this project (app setup, camera, meshes, ECS, picking, states, assets, UI), the 0.18 →
0.20 breaking changes that matter here (its §3 migration notes), the unverified list (§4), third-party crate
versions (§5) and the probe sections (`dynamic_linking`; `bevy_remote` once Task 27 lands). Each entry says
VERIFIED or UNVERIFIED; treat UNVERIFIED as a lead for step 1, not an answer.

## 3. A compile probe in `probes/`

When the sources leave a question open (trait bounds, feature gating, type inference, runtime behaviour),
write the smallest program that answers it in a `probes/<name>/` crate (probes are workspace members but never
default members; see `probes/dynlink` and `probes/remote`) and `cargo check -p probe_<name>` or run it. A probe
build counts as one of the two allowed cargo builders.

## Record what you learn

Add every new fact to `docs/research/bevy020-api-reference.md`: the API, a minimal snippet, VERIFIED (with the
source path, example or probe that proves it) or UNVERIFIED, and the date. Correct wrong entries in place. If
the reference is outside your task's track, put the fact in your report so the lead records it.
