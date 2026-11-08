# Incremental journey and progression contract

This document describes the current source protocol. Exact build, native interaction, balance and performance evidence belongs in [VALIDATION.md](VALIDATION.md); source presence alone is not a playthrough claim.

## Start, resume and protect an existing profile

The main menu offers New game for a fresh profile and Resume journey for an existing one. An existing journey's New game action opens a confirmation modal. Keep current journey cancels without a reset.

`runner::begin_new_journey` queues the serialized storage barrier in runner/persistence.rs: preserve unreadable source copies, archive the complete current Game under the profile's `journey-archives/journey-<stamp>-<counter>/campaign.json` when a previous profile exists, then save the fresh generation. The live Game changes only after fresh save success. Archive or save failure remains visible and leaves the current journey active; an already-created archive is retained. Display/audio/control preferences stay outside this reset, and selected difficulty is copied to the fresh campaign. No widget performs filesystem replacement. One worker receives immutable snapshots; the main thread owns tickets, epochs and navigation acknowledgments.

The fresh route is survivor confirmation → infected starting country → a Personal defense that waits for Begin defense, with the first-defense hint and the key legend on the field. Rook and June retain equal initial combat statistics; Mara is the independent NPC companion. Resume returns to an active field when one exists, to Journey upgrades for a settled active journey, or to the incomplete survivor/map choice. A restored active field is paused until explicitly resumed.

## Every main round returns to upgrades

The Results screen is the single Journey upgrades home after a main victory, defeat or withdrawal. Its outcome strip reports already-banked coins/items; it does not award them again. It shares the camp header and navigation row (Journey upgrades selected), holds the complete connected progression graph, and its action row is the primary Continue/Try again (or Return to active defense), More (plan or replay a defense, save) and a quiet Leave journey, disabled while a save or reset is pending; History is in the header. Under the outcome line, a loss shows the core `Report.cause` and `Report.tip` wrapped in full ("Overrun at the line in lane 2. Aim at the nearest runner first…"); a victory names what it opened: the fronts a liberation unlocked (`WorldProgress::fronts_opened_by`) and the nodes whose milestone was just reached (`Campaign::milestone_opened_nodes`). Every screen names outcomes with `Outcome::label` (Victory, Defeat, Retreated). It is the only route to the tree: the retired Skills and Fortifications tabs redirect here, and Fortifications frames its branch. Parallel workers retain their own orchestration and do not force the main screen to change.

`Game::continue_journey` rejects workers and unfinished main attempts. A victory continues the country's next completed frontier; defeat/withdrawal retries the reported stage, preserving control mode. A fully liberated country returns `CountryLiberated`; the map opens with "<Country> liberated · N new fronts" and the easiest opened front preselected. Failed deployment restores the prior world selection. Continue performs no purchase or automatic training and starts a legal new round live.

`leave_journey` withdraws an active round if necessary, clears the journey-active marker/repeat plan and pauses. The application returns to menu only after its durable save acknowledgment. Leaving retains earned progression and is not a prestige reset. `journey_revision` migrates older main attempts/results into the explicit journey state.

## One connected personal tree

The current authored `skills.json` contains 177 nodes in one connected DAG rooted at stable `pistol_training` (displayed as Survivor foundations). The seven branch labels—Foundations, Body development, Firearms, Survival, Ordnance, Fortification, Companions and world—are camera-focus/navigation chips with owned/total counts, not separate filtered trees. All 32 prior node IDs remain available.

Selection/focus only inspects. `can_purchase_node` checks preparation/reservations, ownership, every prerequisite, `minimum_victories`, `minimum_liberated` and payment. `node_payment` returns Coins or eligible LegacyBodyPoints; `purchase_node` preflights before consuming either. Prices, requirements and errors come from that same kernel.

BodyDevelopment carries attribute plus target rank, taking the highest owned rank rather than stacking duplicate ranks. The shipped tree has twenty ranks for each of five personal attributes. Blueprint and StructureRank effects integrate traps/walls/turrets and ten authored ranks per type. FeatureUnlock gates construction, companion access, recruitment, parallel deployments and operations together with their authored victory/liberation requirements.

Fresh profiles use `legacy_features=false`: XP still develops character level and future geometry, but does not mint new personal CP. Personal body development is purchased in this graph. Older missing flags use legacy compatibility: allocated body ranks count as ownership floors, existing eligible CP can pay pure BodyDevelopment nodes within the old body cap, and `legacy_graph_points` records consumed legacy points. Legacy level/point accounting remains conserved; coin-trained nodes are not a CP refund source.

A legacy allocation refund is rejected atomically if clearing those numeric floors would orphan any explicitly learned node prerequisite. Coins, points, ranks and the spent-point ledger remain unchanged on rejection; unrelated supported legacy refunds retain their old behavior. No missing ancestors are granted for free.

Mara/recruit attribute and skill advancement remains its own pinned NPC budget/ledger, not personal-tree body development. Shared engineering may improve structures in either mode; learning a firearm class still requires buying/looting and explicitly equipping an eligible owned gun.

## Authoring and compatibility

Node IDs are stable lowercase ASCII/digit/underscore strings, never saved graph indices. The catalog supports 1–256 nodes, finite globally unique positions within ±10000, up to sixteen prerequisites/effects, and one free prerequisite-free `pistol_training` root with its exact innate Pistol training effect. Every node must be reachable without a cycle. Milestone fields default to zero for old content. Existing price/name/description/effect bounds still apply; see [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md).

New effect encodings are `BodyDevelopment{attribute,rank}`, `Blueprint(kind)`, `StructureRank{kind,rank}` and `FeatureUnlock(CampaignFeature)`. Runtime ownership adapters for old skill/body/blueprint/rank fields preserve earned state. Do not rename or silently remove learned IDs to rearrange the graph. Branch focus changes the camera, not the purchase rules or model.

[CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) covers equipped appearance. [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md) and [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) retain the independent level/arena/threat/balance/DPS/history contracts.

## Durable storage interface

Runtime.durable_transition_pending covers queued/in-flight critical saves and reset work, not just a visible spinner. A queued ordered SetPaused → Save boundary rejects later model commands; raw Settings edits/rebinding also freeze until the matching acknowledgment. Older autosave completion cannot clear a newer user busy state.

runner/persistence.rs owns one filesystem worker, one in-flight job and one latest coalesced pending snapshot, retaining required intents. Game/settings snapshots are owned immutable copies; no live Game mutex or widget IO crosses threads. Encoding/checksum/validation/atomic writes and recovery/archives run there. Main-frame work is snapshot capture/enqueue/poll; startup, explicit report artifacts, asset loading and shutdown are separate boundaries.

NewJourney increments a profile epoch and serializes old-save completion, full archive and fresh commit before live replacement. Old-epoch Quit success cannot navigate/exit the new profile. Quit requested during reset is recaptured against the resulting paused current generation after the reset acknowledgment. Nothing fallible follows the fresh generation's commit before reset success is reported.

PersistenceMetrics and Runtime.persistence_metrics expose pending work and completed/failed background saves/write timing. Measurements and user-stutter conclusions are separate in VALIDATION.md; moving IO is not by itself a universal FPS claim.

The complete graph keeps one-row node names visible with separate payment/state captions and full hover text. Path focus names the selected branch but never filters away other branches.

Tree presentation reads only core view helpers in `horde_core::campaign::tree_view`: `node_state` (Owned, Buyable, NeedsCoins, NeedsPrerequisites, NeedsMilestone, Unavailable) styles each node and the detail chip; `node_requirements` is the ✓/✗ checklist; `path_to_node` gives the unowned chain, total coins and first blocking milestone that the canvas highlights; `node_stat_preview` is the before→after "What changes" table; `recommended_node` is the default selection (never the owned root): the cheapest buyable node that acts now (`node_helps_now`: not a weapon class without its gun, construction without a learnable blueprint, or structure upgrades without a blueprint), body ranks after those; `search_nodes` and `upgrade_goals` back the search box and the "Path to…" menu; `legacy_node_note` explains body-rank floors only to legacy profiles. The canvas zooms on the wheel, pans on drag, moves with arrow keys, learns with Enter and jumps to the recommendation with Home; long cross-graph edges draw as labelled stubs; a translucent minimap moves the view. Learning still goes through `Command::BuySkillNode`, plays the confirm cue and pulses the node unless reduced effects is on. At narrow widths or large scale the detail panel goes below the canvas. Compact starting-country choice reserves the map before a collapsed dossier, rather than presenting management details first. The upgrade home shows one outcome line, the wallet only in the shared header, and deduplicated retreat guidance.

## Equipped presentation boundary

Purchases and equips still use core transactions; the journey screen never grants a visual item. The deployed EquipmentVisual is immutable input to player_model::resolve_equipped_model. Battle and Character preview consume selected source packets and shared sockets; original-bind parity is distinct from current action/garment acceptance. [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) describes this boundary. Developer parity/parts/matrix/repairs routes are isolated review tasks, not additional progression routes.

The current root source gate is recorded as 432 passed / 0 failed / 1 ignored in VALIDATION.md. The separately recorded static model acceptance and checkpoint timing have their own evidence; six-clip gesture visual confirmation is accepted at its recorded scope, and final package verification remains separate.
