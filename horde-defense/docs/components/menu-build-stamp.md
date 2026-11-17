---
type: View
id: menu-build-stamp
title: "Build stamp"
parent: main-menu
status: Implemented
source: "crates/horde_game/src/ui/menu.rs::menu; crates/horde_game/src/build_info.rs::stamp, stamp_from, version_line"
---

# Build stamp

Name the exact build a playtester is reporting on.

- **Entry, exit and actions:** The main menu's footer shows the stamp right-aligned, in 10-point muted type: "v<version> (dev build)" for every build except a playtest package, which reads "v<version> (<commit>)" or "v<version> (<commit>+dirty)". The same stamp is the first game line of every log (before the settings and save reads) and heads crash reports, and `--version` prints "Lastlight: Horde Defense <stamp>".
- **State boundary:** Only `scripts/package_playtest.ps1` sets `LASTLIGHT_BUILD_COMMIT`, at compile time; a commit counts only as 1 to 20 lowercase hex digits with an optional "+dirty". The stamp is never written into a save.
- **Responsiveness and accessibility:** The stamp sits inside the viewport and clear of the footer line at both target sizes.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
