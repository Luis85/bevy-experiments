---
paths:
  - "assets/**"
  - "crates/fl_content/**"
---

# Content rules (spec §6; ADR-0004, ADR-0007)

- **Strict schemas.** Every def and kind uses `#[serde(deny_unknown_fields)]`. Unknown component names, rules
  sections and `*.X.ron` kinds are errors that carry file and line. Never add `#[serde(flatten)]` to
  rules-hashed content.
- The base game is a mod (`assets/base/`); third-party mods use the same layout under `mods/`.
- Decimals in RON become `Fx` once, at load. The rules hash is a canonical binary encoding, never RON text.
- Content IDs are stable names (`"base.de.pz3"`); dense IDs come from sorted, frozen registries.
- Validate with `cargo run -p fl_cli -- data-check assets/base` after every content change.
- **Never copy Nival's Blitzkrieg code, data files or assets.** Reading the original source to understand a
  rule is allowed; findings go into `docs/research/blitzkrieg-mechanics.md` in our own words. Stats are our own
  numbers. M0 uses primitive placeholders, and nothing is downloaded.
- Assets: CC0 preferred; CC-BY only with a `credits.ron` entry; nothing NC, ND, SA or unlicensed.
- Root `scenarios/**` is protected; `assets/base/scenarios/` is ordinary content.
- The `fl_content` loader phases and registry are contract paths: change them only with an ADR.
