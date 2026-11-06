# Archive

Not shipped; retained by production-pass decision 10; restore with git mv.

These files were moved out of `assets/` on 6 November 2026 because the game
no longer loads them. Nothing was deleted. Each one sits at its old path with
`assets/` replaced by `archive/`, so `git mv archive/<path> assets/<path>`
restores it. `scripts/package_playtest.ps1` copies `assets/` and never
`archive/`, so nothing here reaches a playtest build.

| Folder | Files | Provenance |
|---|---|---|
| `archive/art/` | the six 1 November 2026 painted sheets: actors, biome panoramas, bosses, checkpoint key art, ground, props (`*-v1.png`) | `archive/art/PROVENANCE.json` |
| `archive/art/animation/` | the five 2 November 2026 walk strips | `archive/art/PROVENANCE.json`; the original strip manifest, moved verbatim, is `archive/art/animation/PROVENANCE.json` |
| `archive/art/pixel/` | `keyart-v1.png`, the edit source of the shipped `assets/art/pixel/keyart-v2.png` | `archive/art/PROVENANCE.json` |
| `archive/audio/` | the twelve legacy WAVs: armor, boss, damage, incoming, melee, npc_shot, ready, reload, shot, structure, trap, turret_shot | `archive/audio/PROVENANCE.json` (written by `scripts/generate_atmosphere_audio.py`) |

Every archive record keeps its original fields and adds `sha256`, `bytes`,
`archived_from`, `archived` and `reason`. The `asset_audit` tests in
`crates/horde_game/src/asset_audit.rs` hold each file to its record and to its
mirrored path. `scripts/generate_audio.py` and
`scripts/generate_atmosphere_audio.py` still render the archived clips here;
both default to `--check`, which writes nothing.
