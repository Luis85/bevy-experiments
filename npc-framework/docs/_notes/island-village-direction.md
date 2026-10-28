# Island village surface — direction contract

THESIS: Extend the existing native RTS scene. The village itself leads; the HUD explains the played
actor's needs, purchasing power and next available life, rather than introducing a separate dashboard.

OWN-WORLD: Keep the user-approved KayKit 3D characters, houses, trees and furniture, the existing RTS
camera, Fira Mono native controls, dark opaque/translucent panels, ivory text and restrained gold focus.
Reuse bundled assets and their existing attribution. Water/coast, agricultural plots and work props make
the new island legible. Night and season treatment comes from actual simulation state.

STORY: A newcomer starts with no coins, finds villagers and resources, gathers/crafts, sells useful goods,
and buys bread, clean water and lodging. Death leaves a recoverable diary and a continuing world. A new
life is an explicit action. Scene scheduler settings and CLI-link status explain idle-agent continuation.

FIRST VIEWPORT: The island and coast occupy the central world view, buildings arranged around connected
paths, professions identified at workplaces. A compact top-right HUD reports day/season, health, reserves,
coins and scheduler state. Found-diary reading uses a bounded scrollable panel; death exposes New life.

FORM: Inherited RTS world/HUD extension, code-led; no concept seed or new visual identity. The user
confirmed retaining the current 3D art and native UI. Existing menu/editor composition and focus behavior
remain authoritative. Review normal and narrow windows together, with day/night/winter and death/diary states.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict,
DESIGN.md, and every shipping raster carrying its provenance

For this ordinary extension, preserve existing design files. Verify against incumbent code and the prior
native captures; document checked evidence without inventing a new project-wide visual system. No new
shipping raster is required; bundled model/texture attribution remains the provenance authority.
