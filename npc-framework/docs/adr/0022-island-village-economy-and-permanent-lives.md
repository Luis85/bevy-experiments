# ADR 0022: Island village economy and permanent actor lives

- Status: Accepted
- Date: 2026-10-28
- Superseded in part by [ADR 0044](0044-island-crates-split.md): the `ai_village` / `island_village` tier is removed.

The new AI scene is opt-in content, exposed as the `ai_village` scenario and `island_village` world
preset. Existing scenarios keep their registries, IDs, behavior and oracle files. Additions register
only for island construction, before content freeze. The world preset's scene discriminator defaults
to the incumbent explorer and is omitted when serializing that default.

Use shared framework items, inventories, navigation, crafting recipes and registered content commands.
Nine resident professions run physical supply chains with separate finite wallets and stock. Transfers
validate again at application, preserve tool identity/wear, and conserve currency. A finite community
fund redistributes merchant surplus to low-cash residents; visiting actors receive no stipend. A needy
producer must keep working when the supply it needs is empty, rather than waiting on its own output.

Needs, health, fatigue, daily/seasonal resources and economic decisions use integers and SimClock.
Automatic effects run in SimId order. Water occupies blocking navigation cells. The native view uses
separate entities, existing KayKit assets and actual calendar/need state; no renderer state feeds back.

The user selected permanent death. A corpse never becomes living again. Death cancels work, drops
possessions/currency and one diary, and replaces a played actor's behavior with a no-gameplay remains
tree. This retains the generic control capability for postmortem meta-actions and a resume, while verb
gates reject gameplay. A successor is a recorded new-life action with a new SimId, fresh needs/memory/
notebooks and no free coins. Prior corpses, diaries, residents and the economy remain in the same world.
The host re-reads current credentials when assignment changes, revokes old control and requires a fresh
enter-world handshake. It never reinstalls a stale token from a cloned session.

Private host notebooks remain outside live simulation state. After death, the host reads only Diary
pages from the shared NotesStores and publishes a bounded immutable copy through a recorded content
command. Replay consumes that command, not files. Memory, proposals, credential/workspace files and
post-death entries are excluded. Unreadable or oversized books report an export error; originals are
preserved. A successor uses a separate life folder. F7 retains an unfinished developer draft in its
original folder until Save/Cancel, then switches to the new actor's fresh books.
