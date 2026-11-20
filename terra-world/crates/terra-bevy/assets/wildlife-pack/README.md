# Terra woodland wildlife

Original low-poly animated deer, boar, hare, fox and ibex for Terra's reference
game. MIT OR Apache-2.0; see the repository licenses. Created entirely by the
included deterministic `generate.py` authoring script; no external art assets.

Regenerate with `python3 generate.py`. This writes all GLBs and the content
hashes in `pack.json`. Each GLB has named idle, walk, run, feed and drink clips,
metre units, +Y up, local -Z forward, and feet on Y=0.

See `docs/ASSET_PACKS.md` for the supported import profile, budgets, API,
per-binding animation policy and fallback behavior.
