# View documentation hierarchy

Each meaningful implemented screen or on-demand task has one Markdown file in [components](components/game.md), with type: View, stable kebab-case id, title, plain-ID parent, status and real source path/symbol. game is the sole root. Parent means containment; entry/return routes may be shared. Reusable drawing helpers, icons and decorative parts are not independent Views.

The latest primary route is New game confirmation when needed → survivor → country → live main round → Results → Upgrade (Journey upgrades) or Continue/retry or Leave. Results is the debrief and Upgrades holds the tree; the old debrief task IDs are retained under `journey-upgrades` for actual current portions/actions. A renderer preview inside Character is not an extra navigation View. Five isolated developer review Views document the actual review shell and parity/parts/matrix/repairs tasks; they are not player-facing progression destinations.

- `game`
  - `battle`
    - `battle-ammo-reticle`
    - `battle-build`
      - `saved-layouts`
    - `battle-hud`
    - `battle-inspection`
    - `battle-loot-feedback`
    - `battle-management`
      - `debrief-combat-log`
    - `battle-npc-decision`
    - `battle-ordnance-targeting`
    - `battle-weapon-selector`
  - `campaign`
    - `country-browser`
    - `country-dossier`
      - `country-conditions`
  - `journey-upgrades`
    - `debrief-improvement`
    - `debrief-repeat`
    - `retreat-debrief`
    - `skills`
      - `fortifications`
        - `fortification-details`
      - `skill-details`
  - `main-menu`
    - `new-game-confirmation`
    - `survivor-selection`
  - `operations`
    - `observed-battle`
    - `operations-event-log`
    - `operations-guards`
    - `operations-live-fields`
    - `operations-outbreaks`
    - `operations-recruits`
      - `operations-recruit-details`
    - `operations-reports`
  - `player-model-review`
    - `player-model-matrix`
    - `player-model-parity`
    - `player-model-parts`
    - `player-model-repairs`
  - `preparation`
    - `briefing`
      - `briefing-conditions`
      - `briefing-progression`
    - `character`
      - `character-ledger`
    - `equipment`
      - `equipment-bulk-sale`
      - `equipment-details`
        - `equipment-item-sale`
      - `equipment-inventory`
      - `equipment-presets`
      - `equipment-shop`
    - `survivor`
      - `survivor-ledger`
      - `survivor-policy`
      - `survivor-training`
  - `results`
  - `run-history`
    - `run-history-detail`
    - `run-history-trends`
  - `settings`
    - `settings-audio`
    - `settings-combat`
    - `settings-controls`
    - `settings-display`
    - `settings-gameplay`
    - `settings-profile`

One connected 177-node DAG has one innate stable pistol_training root and seven camera-focus labels. Personal body development, weapon/ability progression, fortification blueprints/ranks and advanced feature access use the same preflight/payment/detail. Branch focus does not split/filter the tree. Every settled main round ends on Results, one action from this tree; parallel worker orchestration remains separate.

Fresh profiles use coins for personal body nodes and do not mint personal CP from XP. Legacy body allocations remain ownership floors; eligible old CP uses the conserved graph-payment ledger. NPC training retains its separate pinned rules budget. Proficiency never creates an item. Feature nodes and existing liberation milestones progressively disclose companions, construction, recruitment, parallel fields and operations; old capabilities remain compatible.

Normal geometry/balance, every-victory threat, fixed-simulation DPS and saved all-outcome analysis remain as defined in [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md). Active fields stay frozen. Actual equipped definition/level/quality/style/class snapshots supply the shared component renderer/preview without transactions or another hit. Original source ownership and explicit repair/garment sources share the model manifest/cache; source inventory is separate from retained historical assets.

[JOURNEYS.md](JOURNEYS.md) documents durable reset, continuation, the unified graph and legacy boundaries. [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) documents real equipment snapshots, parts/sockets/IK and the shared displayed muzzle. [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md) defines JSON authoring; [ITEM_QUALITY.md](ITEM_QUALITY.md) and [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md) retain their independent contracts.

## Structural verification

Run the repository's source-aware validator:

```powershell
python scripts/check_views.py
```

The final reconciliation checks actual IDs/root/parents/cycles, source files/symbols and local links. Current inventory: **70 View files**. Completed check results and source/gate/native evidence are recorded at the final documentation handoff and in [VALIDATION.md](VALIDATION.md).

Implemented describes source presence, not GUI, OS interaction, listening, balance or performance proof. Dated earlier validation/archives retain their tested revisions.
