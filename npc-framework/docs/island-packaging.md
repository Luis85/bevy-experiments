# Packaging Island Village

How to turn a built Island Village into a folder that runs on its own, and how the
game finds its assets there.

## Where the game looks for assets

The asset folder is chosen once, at startup, by `island_view::assets::resolve_asset_dir`.
The first match wins:

| Order | Source | Used when |
|---|---|---|
| 1 | The `ISLAND_ASSETS` environment variable | it names an existing folder (the asset folder itself, not its parent) |
| 2 | An `assets` folder beside the executable | it exists: this is a packaged build |
| 3 | The developer fallback | otherwise: `<BEVY_ASSET_ROOT>/assets` when that variable is set, else the `assets` folder of the workspace the build was compiled in |

A developer build in `target/debug` or `target/release` has no `assets` folder beside
it, so `cargo run` and the capture scripts (which set `BEVY_ASSET_ROOT`) behave as
before. An `ISLAND_ASSETS` value that names no folder is passed over with a warning.

The choice is logged once, when the view loads its assets:

```text
INFO island_view::assets: assets: C:\Games\IslandVillage\assets (from the assets folder beside the executable)
```

If the chosen folder does not exist (a build copied without its assets and without
the workspace it was compiled in), the same line is logged as an error and names
both remedies. That case was not exercised here.

The interface font is compiled into the executable, so text appears even without
the asset folder. Everything else (models, textures, audio) is read from it.

## Making a package

Build the game, then copy it:

```powershell
cargo build --locked --release -p island_game
.\scripts\package_island.ps1 -ProfileDir target\release -OutDir dist\IslandVillage
```

`package_island.ps1` builds nothing. It takes a Cargo profile directory that already
holds the executable and an output folder, and writes:

```text
IslandVillage\
  island_game.exe   the game: title, play and pause in one process
  assets\           models, textures, audio and fonts, as in the workspace (without the
                    retired, untracked `assets/audio/` folder)
  LICENSE.txt       the repository licence
  CREDITS.md        asset sources, authors and licences
  licenses\         the asset packs' own licence texts
```

| Parameter | Meaning |
|---|---|
| `-ProfileDir` | folder holding the built executable, such as `target\release` |
| `-OutDir` | the package folder to create |
| `-Executable` | executable name in the profile directory; `island_game.exe` by default |
| `-Force` | replace `-OutDir` if it exists. Only a folder that already looks like a package (it has the executable and `assets`) or an empty one is replaced |

It stops with an error if the executable, the assets, `CREDITS.md`, the pack licences
or a `LICENSE` file is missing, and it prints the file count and sizes when done.

Run the game by starting `island_game.exe`. The folder can be moved, renamed or zipped.

## What a package does not contain

- **Saves and settings.** They stay in the player's own folders, as in a developer
  run: the island save, its rules and `island-settings.json` in
  `<home>/island-village/` (the settings file sits next to the save). Deleting the
  package deletes no save.
- **Debug symbols.** The `.pdb` beside a built executable is not copied.
- **The developer tools.** The developer menu (`game_menu`), the framework examples
  and `npcctl` expect a workspace; none is packaged.
- **A C++ runtime.** The executable links the Microsoft Visual C++ runtime
  dynamically, as Rust's MSVC target does by default. Windows machines without it
  need the Visual C++ Redistributable.
- **Licences of the Rust crates** compiled into the executable. Only the repository
  licence and the asset credits are copied. Collect the crate licences (for example
  with `cargo about`) before distributing a build outside the team.

Only Windows is covered: the script is PowerShell and names a `.exe`. The run-time
lookup itself is the same on every platform.

## Checking a package

A package can be checked without a release build by packaging the developer build
and taking a screenshot from inside the package, with neither variable set:

```powershell
.\scripts\package_island.ps1 -ProfileDir target\debug -OutDir target\npc-out\package-test\IslandVillage -Force
Remove-Item Env:ISLAND_ASSETS, Env:BEVY_ASSET_ROOT -ErrorAction SilentlyContinue
$run = "$PWD\target\npc-out\package-test\run"
New-Item -ItemType Directory -Force $run | Out-Null
Push-Location target\npc-out\package-test\IslandVillage
Start-Process .\island_game.exe -Wait -RedirectStandardError "$run\game.log" -ArgumentList `
    "--front-end", "--island-save", "$run\island.world.json", "--island-rules", "$run\rules\living_world.json", `
    "--screenshot", "$run\title.png", "--after-secs", "16"
Pop-Location
Select-String -Path "$run\game.log" -Pattern "assets: "
```

`--island-save` and `--island-rules` keep the check away from the player's own save,
rules and settings. A relative `--screenshot` path is written to the current folder, so give
it a full path to keep the package clean.

The log line must name the package's own `assets` folder and "beside the
executable", and the screenshot must show the island behind the title.

Measured this way on 2026-11-10: 161 files and 199.8 MB, of which the developer
executable is 161.0 MB and the assets 38.8 MB. A release executable is much smaller;
it was not built for this check.
