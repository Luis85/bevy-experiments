# View documentation hierarchy

Each meaningful implemented screen or on-demand task has one Markdown file in [components](components/game.md), with type: View, stable kebab-case id, title, plain-ID parent, status and real source path/symbol. game is the sole root. Parent means containment; entry/return routes may be shared. Reusable drawing helpers, icons and decorative parts are not independent Views.

The primary route is New game confirmation when needed → survivor → Start journey (the first defense opens paused) → live main round → Results → Upgrade (Upgrades, the `journey-upgrades` View) or the continuation or Leave journey; Upgrades leads back to the battle or to Camp home, and Camp home leads to the Briefing, which deploys the next defender. Results is the debrief and keeps the old debrief task IDs as its children; Upgrades holds the tree. [JOURNEYS.md](JOURNEYS.md), "Loop map", gives every screen's amber primary and Escape target. A renderer preview inside Character is not an extra navigation View. Five isolated developer review Views document the actual review shell and parity/parts/matrix/repairs tasks; they are not player-facing progression destinations.

- `game`
  - `battle`
    - `battle-ammo-reticle`
    - `battle-boss-banner`
    - `battle-build`
      - `saved-layouts`
    - `battle-hud`
    - `battle-inspection`
    - `battle-layout`
    - `battle-loot-feedback`
    - `battle-low-hp`
    - `battle-management`
      - `debrief-combat-log`
    - `battle-npc-decision`
    - `battle-ordnance-targeting`
    - `battle-retreat-hold`
    - `battle-weapon-selector`
  - `campaign`
    - `country-browser`
    - `country-dossier`
      - `country-conditions`
  - `journey-upgrades`
    - `skills`
      - `fortifications`
        - `fortification-details`
      - `skill-details`
  - `main-menu`
    - `menu-build-stamp`
    - `new-game-confirmation`
    - `quit-confirmation`
    - `survivor-selection`
  - `player-model-review`
    - `player-model-matrix`
    - `player-model-parity`
    - `player-model-parts`
    - `player-model-repairs`
  - `preparation`
    - `briefing`
      - `briefing-conditions`
      - `briefing-progression`
    - `camp-home`
      - `operations`
        - `observed-battle`
        - `operations-event-log`
        - `operations-guards`
        - `operations-live-fields`
        - `operations-outbreaks`
        - `operations-recruits`
          - `operations-recruit-details`
        - `operations-reports`
      - `survivor`
        - `survivor-ledger`
        - `survivor-policy`
        - `survivor-training`
    - `character`
      - `character-ledger`
    - `equipment`
      - `equipment-auto-sell`
        - `auto-sell-stash-apply`
      - `equipment-bulk-sale`
      - `equipment-details`
        - `equipment-item-sale`
      - `equipment-inventory`
      - `equipment-presets`
      - `equipment-shop`
    - `onboarding-callouts`
  - `results`
    - `debrief-improvement`
    - `debrief-repeat`
    - `defender-picker`
    - `recruit-modal`
    - `results-loot-block`
    - `retreat-debrief`
  - `return-to-menu-confirmation`
  - `run-history`
    - `run-history-detail`
    - `run-history-trends`
  - `settings`
    - `draft-warning`
    - `settings-audio`
    - `settings-combat`
    - `settings-controls`
    - `settings-display`
    - `settings-gameplay`
    - `settings-profile`
  - `storage-error`
  - `toasts`

One connected 177-node DAG has one innate stable pistol_training root and seven camera-focus labels. Personal body development, weapon/ability progression, fortification blueprints/ranks and advanced feature access use the same preflight/payment/detail. Branch focus does not split/filter the tree. Every settled main round ends on Results, one action from this tree; parallel worker orchestration remains separate.

Fresh profiles use coins for personal body nodes and do not mint personal CP from XP. Legacy body allocations remain ownership floors; eligible old CP uses the conserved graph-payment ledger. NPC training retains its separate pinned rules budget. Proficiency never creates an item. Feature nodes and existing liberation milestones progressively disclose companions, construction, recruitment, parallel fields and operations; old capabilities remain compatible.

Normal geometry/balance, every-victory threat, fixed-simulation DPS and saved all-outcome analysis remain as defined in [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md). Active fields stay frozen. Actual equipped definition/level/quality/style/class snapshots supply the shared component renderer/preview without transactions or another hit. Original source ownership and explicit repair/garment sources share the model manifest/cache; source inventory is separate from retained historical assets.

[JOURNEYS.md](JOURNEYS.md) documents durable reset, continuation, the unified graph and legacy boundaries. [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) documents real equipment snapshots, parts/sockets/IK and the shared displayed muzzle. [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md) defines JSON authoring; [ITEM_QUALITY.md](ITEM_QUALITY.md) and [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md) retain their independent contracts.

## Production pass views

The production pass (November 2026, plan `plans/2026-11-05-production-pass.md`) split Results from the tree, gave the camp a home page and moved every confirmation, failure and short notice into one modal slot and one toast layer. Its Views, by parent:

- **Screens.** `journey-upgrades` is now the separate Upgrades screen (`Screen::Upgrades`), under its retained id: the camp row, the amber continuation and Back to camp, then the tree. `camp-home` is the default camp tab (`Tab::Home`) under `preparation`; the Mara (`survivor`) and `operations` pages moved under it as drill-downs with a "‹ Camp home" breadcrumb. `onboarding-callouts` is the one-at-a-time unlock strip under the camp row.
- **Results.** `results-loot-block` (drops by quality, the auto-sold receipt, "better than equipped" rows), `defender-picker` (also drawn on the Briefing and Camp home) and `recruit-modal` (also opened from Camp home and Operations). The retained debrief task ids `debrief-improvement`, `debrief-repeat` and `retreat-debrief` moved from `journey-upgrades` to `results`, the screen they describe.
- **Equipment.** `equipment-auto-sell`, the Auto-sell rule under Manage equipment, and its `auto-sell-stash-apply` confirmation.
- **Modals.** One slot, `StatusUi.modal`: `quit-confirmation` (main menu), `return-to-menu-confirmation` (Camp home and Results), `storage-error` (save, load and settings failures, which take the slot from any other modal), `draft-warning` (Settings), and the Recruit and stash-apply modals above. Escape closes the open modal.
- **Toasts.** `toasts`: the top-right stack on every screen without a field and the one battle toast in the receipt slot, including the expedition-outcome notices.
- **Battle.** `battle-layout` (the `BattleLayout` regions, the zoom-to-fit field and the two-row bar), `battle-retreat-hold` (the 0.8 s hold that replaces the one-click retreat), `battle-boss-banner` and `battle-low-hp`.
- **Menu.** `menu-build-stamp`, the version and commit on the menu footer.

`scripts/check_views.py` checks that the tree above lists every View file exactly once under the parent its front matter names, that this page's inventory count is the number of View files, and that every production-pass View has its file.

## Structural verification

Run the repository's source-aware validator:

```powershell
python scripts/check_views.py
```

The final reconciliation checks actual IDs/root/parents/cycles, source files/symbols, that the tree above matches every View file's parent, and local links. Current inventory: **87 View files**. Completed check results and source/gate/native evidence are recorded at the final documentation handoff and in [VALIDATION.md](VALIDATION.md).

Implemented describes source presence, not GUI, OS interaction, listening, balance or performance proof. Dated earlier validation/archives retain their tested revisions.
