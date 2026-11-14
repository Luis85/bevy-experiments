# ux-flow synthesized spec (production pass, goal 2 plus UI for goals 3 and 4, decision 18, battle camera)

**Base:** Angle A, which won 2 of the 3 judges.

**Grafts from B, by per-decision majority:**
- construction palette in the top band;
- context-dependent Results primary;
- boss timer that runs only while unpaused;
- low-HP hysteresis;
- a fixed `BattleLayout`;
- hold-to-retreat;
- an outcome-notice queue (as a proposal to campaign-defenders);
- consuming CD's `defender_power`;
- one Harness size convention;
- `Settings::normalise()`;
- `settings.unreadable.json`;
- `ReloadProfile` gating;
- `keyboard_owner`;
- two-size loop test;
- digest-stability frame test;
- `"Mara"` literal scan;
- wave-3 ordering: Primary, Home, Upgrades, Escape.

## 0. Principles

- ux-flow adds no core saved field and moves no GOLDEN or GAMEPLAY_TRACE.
- Persisted ux state lives in `settings.json`. That file is outside `SAVE_VERSION` and the state digest, and every field in it is `#[serde(default)]` and primitive (bool, f32, String, BTreeSet<String>; never an enum, because `Settings::load` at settings.rs:79-84 resets the whole file on any parse error).
- ux-flow owns these core changes. They are pure and read-only, with no digest effect:
  - `label()` on Slot, Policy, DamageKind and RecruitStatus;
  - the core string fixes at strategy.rs:129 and campaign/loadout.rs:438;
  - `Campaign::feature_unlock`.
- Consumed contracts:
  - campaign-defenders (CD): `available_defenders`, `SetNextDefender`, `SelectCountry`, `RecruitAs`, `recruit_offer`, `defender_power`, `defender_name`, `Attempt.defender`/`Report.defender`, and the outcome notice queue (§6).
  - loot-economy (LE): `Report.drop_ids`, `Report.auto_sold`, `Campaign.auto_sell`, `SetAutoSell`, `ApplyAutoSellToStash` with `CommandError::QuoteChanged`, `auto_sell_preview`, `results_upgrades`.
  - progression-tree (PT): `Report.level_ups`, `Report.level_grant_coins`.
- Presentation timers use the wall clock (`ui.input(|i| i.time)` / `stable_dt`), never core ticks or RNG. Every new effect has a reduced-effects static form, and every warning has a text twin.

## 1. Loop map (copy into docs/plans/2026-11-04-production-pass.md)

| Screen | Amber primary | Escape target |
|---|---|---|
| Menu | Resume journey / New game | Quit modal ("Save and quit" / "Stay") |
| SurvivorSelection | Continue as <name> | Menu |
| Camp/World before first start | Start journey (ChooseStart, opens paused) | Menu |
| Camp/Home (default tab) | `home_primary`: Return to active defense / Choose next country / Go to briefing / Choose a country | Results if a settled report exists, the journey is active and no attempt is live; else the ReturnToMenu modal |
| Camp/Briefing | Deploy personally / Deploy <defender name> | Camp/Home |
| Camp/Equipment, Character, World, Mara (drill-down), Operations (drill-down) | Go to briefing / Choose next country (World: Plan defense / Manage country) | Camp/Home. The observer window and armory confirm close first. |
| Battle | Begin defense / Resume defense (amber only while paused) | Topmost overlay first, then retreat hold cancel, then close construction; else PassToPause only if `bindings[Pause]=="Escape"`, otherwise Ignore |
| Results | `results_primary`: Upgrade (defeat or withdrawal, and the recommended node is `NodeStatus::Available`), else continue_label (Try again / Next defense / Choose next country / Return to active defense) | ReturnToMenu modal ("Save and return to main menu" / "Stay") |
| Upgrades (new Screen) | `upgrades_continue_label`: Back to battle / Choose next country / Return to active defense. With no report: Back to camp | Results if a report exists, else Camp/Home |
| History | — | CloseHistory (return screen) |
| Settings | settings_return_label | Cancel rebind; else DraftWarning when the draft is dirty; else save-back |
| Recruit modal | Recruit as <Archetype> · 100 coins, then Back to camp | Close |

## 2. Decisions (summary table)

| # | Question | Chosen | Rejected (reason) |
|---|---|---|---|
| 1 | How do Results and Upgrades split | `Screen::Upgrades`; `Screen::ALL` grows to 8 | ResultsPage sub-state (Escape and Screen asserts cannot distinguish it); Tab::Upgrades (breaks "Fields paused during upgrades" at navigation.rs:235) |
| 2 | 3-action rule | Exactly 3 action-bar buttons, plus a tested whitelist of inline content controls | Recruit replacing continue (hides the loop spine); prose-only rule (untested) |
| 3 | Results amber primary | Pure `results_primary`; Upgrade only when the recommended node is Available | Always continue (hides the loss-to-upgrade loop); always Upgrade (wrong at 0 coins) |
| 4 | Liberation, then Recruit, then Back to camp | `AppModal::Recruit` with phases Choose and Joined; Joined has the amber "Back to camp" | Detour via Upgrades; a 4th Results action |
| 5 | Camp home and row | `Tab::Home` is the default; the row has 6 entries; Mara and Operations become drill-downs with breadcrumbs | Shortened 8-entry row (about 700 px, fragile at 1.35); a Home overlay |
| 6 | World clock key | Extra binding `world_clock="P"`, Home only, plus `normalise()` | Space (activates the focused egui button); Pause=Escape on Home (breaks Back) |
| 7 | Escape router | One pure `escape_action` with A's overlay order, consuming in one place | Per-screen handlers (same-frame race); Bevy KeyCode system (cannot see popups); B's details-first order (a modal over Details is not topmost) |
| 8 | Escape in battle | PassToPause only when `bindings[Pause]=="Escape"` (literal decision 18) | Hard-wired pause regardless of binding (deviates from decision 18) |
| 9 | Retreat confirmation | 0.8 s wall-clock hold | Modal (covers the field and suppresses fire mid-combat); two-step arm (double-click-through) |
| 10 | Camera | `fit_field` = today's clearing (with its 160/86 clamps) scaled uniformly until it touches | Fixed `LANE_PER_YARD` from 1440 (changes the small-window ratio 1.16 to 1.356 and shrinks 11x30 and 17x30); non-uniform fill (stretches); vfx clamp hybrid (not uniform) |
| 11 | Construction palette | Compact palette in the top-band selector slot; field never covered | Right overlay at alpha 200 (hides spawns while combat runs); bottom-bar row (breaks the 2-row budget) |
| 12 | Observer window | Shared `fit_field(rect, lanes, width, 0.0)` | Keep clearing_rect (two camera contracts) |
| 13 | Onboarding seen-set | `settings.json` per profile, with a seeding pass | Campaign.seen_callouts + MarkCalloutSeen (core, digest and migration surface for presentation memory; B's "shared settings" premise is false, since profile.rs:8-18 gives one settings.json per profile); session-only (repeats) |
| 14 | Expedition notices | CD-owned `StrategyState.outcome_notices` queue that never drops, plus a folded overflow counter | Per-battle `announced` flag (pruning at parallel.rs:144-149 can remove an unannounced settled battle); B's drop-oldest cap (loses notices); cursor (out-of-order settlement); UI HashSet (repeats after restart) |
| 15 | defender_power | Consume the CD-owned `Game::defender_power` | A's own formula (duplicate ownership, ignores threat) |
| 16 | Save failure "Open folder" | Runner `AppCommand::OpenSaveFolder` + injectable `FolderOpener`; Copy path always shown | file:// open_url (OS handling unverifiable headless; on Windows it may open a browser) |
| 17 | Settings types | Primitives only: `fullscreen: bool`, `colour_blind_palette: bool` | `WindowModeSetting` / `QualityPalette` enums (one bad variant resets bindings) |
| 18 | Primary registry | `FramePrimaries` cleared at the start of `draw_screen`, with a Screen/Modal surface split | Clearing on the first record of a new frame (stale entries on frames with no primary); fill detection (fragile) |
| 19 | HUD 2 rows | Reload, Strike and Toggle firing move to Management "Field actions"; fixed `BattleLayout` | Icon buttons (new art); overflow menu (hides Heal and Brace) |
| 20 | Boss banner | Wall time accumulated only while unpaused on Screen::Battle; `BOSS_BANNER_SECS=2.0` | Sim-time age (0.25 s at 4x); HUD-only emphasis |
| 21 | Low HP | Enter at <=30%, exit at >35% (hysteresis); 8 px RED edge plus "LOW" text | Single threshold (flicker); danger-readout trigger (misses attrition); a user setting |
| 22 | Debug labels | Core `label()` methods plus a source-scan test | UI-only label fns (cannot fix the core strings); display_label over Debug (wrong for BossFocus) |

## 3. Types, functions, constants

### horde_game/src/ui.rs

`primary(ui, label) -> Response`:
- fill AMBER (239,181,82), INK text, strong, no stroke (a 1 px INK inner stroke on focus);
- min size 190x40;
- hover fill rgb(250,200,110); disabled fill AMBER*0.4 with MUTED text;
- pushes `PrimaryRecord{label:String, surface:Surface{Screen,Modal}}` into egui temp data `FramePrimaries(Vec<PrimaryRecord>)`.

`primary_in_modal()` records `Surface::Modal`. `draw_screen` clears `FramePrimaries` first, then runs `escape_router`, then `controls()`. results.rs:182-189 is replaced by `primary()`.

### ui/escape.rs (new)

```rust
pub(super) struct EscapeContext {
    screen: Screen, tab: Tab, overlay: Overlay,
    settled_report: bool, journey_active: bool, journey_started: bool, attempt_live: bool,
    draft_dirty: bool, durable_pending: bool, pause_is_escape: bool,
    retreat_hold: bool, construction: bool,
}
pub(super) enum Overlay { Rebind, TextFocus, AppModal(ModalKind), NewJourneyModal, Details, Popup, Observer, ArmoryConfirm, None }
// resolved topmost-first in exactly that order
pub(super) enum EscapeAction { LeaveToEgui, CancelRebind, CloseModal, CloseNewJourney, CloseDetails, CloseObserver, CancelArmoryConfirm, CancelRetreat, CloseConstruction, GoTab(Tab), Go(Screen), OpenModal(ModalKind), CloseHistory, SettingsBack, PassToPause, Ignore }
pub(super) fn escape_action(cx: &EscapeContext) -> EscapeAction; // pure
pub(super) fn escape_router(ctx: &egui::Context, f: &mut UiCtx);
```

- **Overlay rules:**
  - TextFocus and Popup resolve to LeaveToEgui. egui closes popups itself on `key_pressed(Escape)` (popup.rs:625).
  - CloseNewJourney is blocked while a save or reset is pending.
  - `durable_pending` turns every Go and OpenModal into Ignore.
- **Battle, no overlay:** retreat_hold gives CancelRetreat; else construction gives CloseConstruction (no pause); else PassToPause if `pause_is_escape`, otherwise Ignore.
- **Consumption:** `consume_key(Modifiers::NONE, Key::Escape)` for every action except LeaveToEgui, PassToPause and Ignore.
- **Removed Escape sites:** preferences.rs:13-20, history.rs:101-106, feedback.rs:127, camp.rs:289-307 (its Escape and Action::Pause use), battle.rs:67-71. The guard at preferences.rs:154-157 stays.
- **`keyboard_owner(ctx,f) -> KeyboardOwner{Free,Egui,Overlay}`** replaces `overlay_owns_keyboard` (feedback.rs:125-133). It gates battle `controls`, `camp_controls` and `gather_combat_input` (battle.rs:94-97).

### ui/modal.rs (new)

`StatusUi.modal: Option<AppModal>` is a single slot. Variants:
- Quit
- ReturnToMenu
- Retreat: unused; reserved, not built, because the hold replaces it
- DraftWarning{then: LeaveSettings{Back, Menu}}
- Storage(StorageError)
- Recruit{country, phase: RecruitPhase{Choose, Joined{recruit_id}}}
- ApplyAutoSell{quote}

A Storage error replaces any open modal; this is documented. `menu.new_journey_confirm` stays separate (fewer test rewrites).

### ui/toast.rs (or a feedback.rs section)

`StatusUi.toasts: VecDeque<Toast{text, kind:ToastKind{Info,Success,Error}, shown_at:Option<f64>}>`:
- the same text arriving again refreshes the timer;
- TTL 3.0 s, or 6.0 s for Error (kept while hovered);
- 0.25 s fade, no fade in reduced effects;
- camp/results/upgrades: up to 3 visible, top-right Area at (-16, 52), 280 px wide;
- battle: 1 visible, in the `BattleLayout.slot`.

`toast_layer(ctx,f)` runs at the end of `draw_screen`.

### art.rs

```rust
pub fn fit_field(maximum: Rect, lanes: u8, width: f32, reserve_top: f32) -> Rect
```

- `base = today_clearing_size(maximum, lanes, width)`: the exact clearing_rect size formula, including `.max(160)/.max(86)/.min(maximum)`, moved here.
- `F` = `maximum` with its top inset by `reserve_top`.
- `z = min(F.w/base.w, F.h/base.h)`. The result is `Rect::from_center_size(F.center(), base*z)`.
- `clearing_rect` (battle.rs:475-489) is deleted. `battle_arena(canvas)` loses its construction parameter.
- `reserve_top = 88.0` when `view.first_deployment` (campaign.attempts == 0, battle_view.rs:660, stable for the whole attempt), else 0.

### ui/battle/layout.rs (new)

`BattleLayout::new(canvas) -> BattleLayout` is pure. Regions:

| Region | Rect |
|---|---|
| top band | canvas.top..+124 |
| hud_left | (14,12) 322x108 |
| hud_right | (right-206, 12) 192x86 |
| middle column | x left+350..right-222 (205.8 px at small) |
| selector_or_palette | middle column, y +12..+56 |
| slot | middle column, y +60..+118; holds toast > loot receipt |
| arena_safe | (left+24, top+124)..(right-24, bottom-84); equals today's `maximum` |
| bar | (left+12, bottom-80)..(right-12, bottom-17) |
| legend | baseline bottom-3, 10 pt |

### ui/state.rs

- `Tab::Home` is the first variant and `#[default]`. `Navigation::default().tab = Home`.
- `BattleUi` gains:
  - `retreat_hold: Option<f64>`;
  - `boss_banner: Option<BossBanner{key:(u64 attempt,u64 tick), shown:f32}>`;
  - `low_hp: bool`.
- `SettingsUi` gains `scale_draft: Option<f32>` and `rebind_extra: Option<ExtraAction>`.

### Constants

| Constant | Value |
|---|---|
| `RETREAT_HOLD_SECS` | 0.8 |
| `BOSS_BANNER_SECS` | 2.0 |
| `LOW_HP_ENTER_PCT` | 30 |
| `LOW_HP_EXIT_PCT` | 35 |
| `TOAST_SECS` | 3.0 |
| `TOAST_ERROR_SECS` | 6.0 |
| `FIRST_DEFENSE_RESERVE` | 88.0 |

### Pure fns

- `home_primary(&Game) -> HomePrimary{ReturnToDefense, ChooseNextCountry, GoToBriefing, ChooseCountry}`. ChooseNextCountry applies when last_result is a liberation and the journey is active; it dispatches ContinueJourney. The same rule is used by `mission_actions` (briefing.rs:228-254).
- `results_primary(report, rec_status: Option<NodeStatus>, live) -> ResultsAction{Upgrade, Continue}`:
  - live gives Continue;
  - Victory or Liberation gives Continue;
  - Defeat or Withdrawal with `campaign.recommendation_fallback(UpgradeFocus::Next(mode))` in `NodeStatus::Available(_)` (view.rs:56,80) gives Upgrade;
  - otherwise Continue.
- `upgrades_continue_label(f)`:
  - live attempt: "Return to active defense";
  - settled liberation and journey active: "Choose next country";
  - settled report and journey active: "Back to battle" (ContinueJourney);
  - no report: the button is hidden and "Back to camp" is the primary.
- `low_hp(hp, max_hp, was_low) -> bool` applies the 30/35 hysteresis.

### chrome.rs

- `defender_picker(ui,f)` replaces `mode_picker` (chrome.rs:31-61):
  - selectable chips for up to 3 options, a ComboBox beyond that;
  - a disabled option shows the core typed reason;
  - selecting dispatches `SetNextDefender`;
  - it renders only when `available_defenders().len() > 1`.
- `defender_label(game, Option<&Attempt>) -> String` reads `Attempt.defender` / `Report.defender` through `Game::defender_name`.
- `quality_marker(ui, quality, settings)` draws a painted shape plus the name. Shapes: Poor ▽ hollow, Common ●, Uncommon ◆, Rare ■, Epic ▲, Legendary ★. With `colour_blind_palette`, colours are Okabe-Ito:
  - Poor (120,120,120);
  - Common TEXT;
  - Uncommon (0,158,115);
  - Rare (86,180,233);
  - Epic (204,121,167);
  - Legendary (230,159,0).

### Runner

- `AppCommand::OpenSaveFolder` runs through `trait FolderOpener { fn open(&self, dir:&Path) -> Result<(),String>; }`. The default spawns explorer.exe on Windows, `open` on macOS and `xdg-open` elsewhere. The test double records the path.
- `AppCommand::ReloadProfile` answers with `RuntimeNotice::ProfileReloaded(Result<(),String>)`. It is enabled only while `!runtime.commands_applied_since_load() && !runtime.saved_since_load()`. After a save, the unreadable file was replaced after its recovery copy (apply.rs:380-404).

### settings.rs

- `Settings::load_checked(path) -> (Settings, Option<String>)`. `Settings::load` keeps its signature and behaviour.
- On a parse error, profile.rs (runner-side IO) copies the file to `settings.unreadable.json` before any save can overwrite it.
- `Settings::normalise()` runs after load. Any extra binding that equals a core binding becomes "" (shown as "Unbound").
- `BindingId{Core(Action), Extra(ExtraAction)}`, `ExtraAction{WorldClock, CycleAttack}`, `binding(id)`, `set_binding(id,key)`. A duplicate swaps across both sets, generalising preferences.rs:148-157. `pressed_extra(ctx, ExtraAction)`.

### Core (ux-owned, pure)

- `Slot::label` (Ranged, Melee, Outfit, Accessory), `Policy::label` (e.g. "Boss focus"), `DamageKind::label` (e.g. "Small piercing"), `RecruitStatus::label` (Ready, Guarding <Country>, On expedition, Recovering).
- `Campaign::feature_unlock(CampaignFeature) -> Option<FeatureUnlock{node_id, name, cost, liberations_required}>`, read from skill_catalog (skills.json:4010 companion_contact 75/1; :4032 recruitment_network 100/1).

## 4. Screens

### Results (ui/results.rs)

Results hides the camp row and keeps the outcome strip (it appends ` · <name>` for a non-player defender), then these content blocks:
- **Loot:** "Drops: 3 Common · 1 Uncommon · 1 Rare" with quality markers.
  - Up to 3 "Better than equipped" rows from `Game::results_upgrades(&Report)`, which reads `Report.drop_ids`. Each row has "Equip" for Player or Mara; for a recruit it shows "Better than <Name>'s gear · Operations".
  - "Auto-sold N · +C" from `Report.auto_sold`, or the muted "Auto-sell is off · Equipment › Auto-sell".
  - The block is hidden when there are 0 drops.
- **Level grant:** "Level N · +C coins", hidden when zero.
- **Liberation block:** "Recruit from <Country> · 100 coins" opens the Recruit modal.
  - If the node is unowned, it shows the muted "Learn Survivor network in Upgrades to recruit from <Country> (100 coins)".
  - If the core blocker is AlreadyRecruited or RosterFull, it names that reason.
- **Next defender:** `defender_picker`.
- **"N upgrades affordable"** muted line.

Action bar: `Panel::bottom("results_actions")`, exactly [Upgrade] [continue_label] [Leave journey]. Amber goes to the `results_primary` choice, and positions are fixed.
- "More", "Plan or replay a defense" and "Save journey" are removed (results.rs:200-212). Replays stay reachable from the Briefing "Local defense" picker; Save moves to the header.

Inline whitelist: defender picker items, "Recruit from …", "Equip", and status "Details"/"Dismiss". If Results is drawn with no last_result, it redirects to Home.

### Upgrades (ui/upgrades.rs, new)

Upgrades shows the camp row, a top action strip ([continue] and "Back to camp"), and then `progression::skill_tree` at full height (moved from results.rs:169).
- `ambience` is Paused.
- settings_return_label is "Save and return to upgrades". Results' label becomes "Save and return to results".

Routing:
- NavEntry::Upgrades opens Screen::Upgrades (navigation.rs:96).
- `open_upgrades` (context.rs:327) goes to Upgrades.
- The Skills/Engineering redirects (camp.rs:16-21, context.rs:336-345) go to Upgrades.
- Withdraw and SimulationFinished (notices.rs:175-184, :291-302) go to Results with `go_tab(Tab::Home)`.
- Resuming a settled journey (context.rs:396) stays on Results.

### Recruit modal

Choose phase:
- 2 cards from `Game::recruit_offer(country)`, each with name, origin trait and archetype;
- buttons "Recruit as <Archetype> · 100 coins" (modal primary on the focused card), dispatching `Command::RecruitAs{country, archetype}`;
- "Not now".

On `CommandOutcome::Id(recruit_id)` the phase becomes Joined: "<Name> joined your camp.", with amber "Back to camp" (`open_camp_home(Some(Focus::Recruit(id)))`, which highlights the roster row) and "Stay here". Escape closes the modal. Operations' "Recruit survivor" (operations.rs:149-161) and the Home Offers card open the same modal.

*As built (W3-19, `ui/modal.rs`, `ui/results.rs`):* `AppModal::Recruit{country, phase}` and `ModalKind::Recruit`; `StatusUi::open_recruit` opens it from Results, Home's "Recruit ›" and Operations' "Recruit survivor" (which no longer dispatches `Command::Recruit`). The modal reads `Game::recruit_offer` every frame: one recruit (name and origin trait), two archetype rows (starting policy and stats) and "Recruit as <Archetype> · N coins" at `offer.price`, the first archetype being the modal primary. An offer core withdraws while the modal is open shows the blocker's label and "Close". The notice system moves Choose to Joined on `RecruitAs`'s `CommandOutcome::Id` (`StatusUi::recruit_joined`). The Results block shows only for a victory whose report freed its country; it is "Recruit from <Country> · N coins", the muted "Learn <node> in Upgrades to recruit from <Country> (<node price> coins)" for `RecruitBlocker::NeedsNode`, or the muted "Recruit from <Country>: <blocker label>" for any other blocker. W3-03 had not landed, so the block sits under the existing action row (continue, "More", "Leave journey"), which `results::action_bar_labels` lists.

### Camp/Home (ui/home.rs, new)

Two columns when the content width is >= 720.

Left column:
- **Next defense card:** "<Country> · Defense N · <defender>" plus `defender_picker`.
- **Roster grid** over `available_defenders()`, with columns:
  - Name;
  - Status (`RecruitStatus::label`);
  - Readiness ("HP 31/31 · Lv 4");
  - "Power P · threat T · Ready|Stretch|Underpowered" from CD `defender_power(defender, next_threat)`, as text plus colour;
  - "Defend next" (SetNextDefender; disabled with the core reason). Mara's row adds "Training ›".

Right column:
- **Alerts:** up to 3 outbreaks with "Plan response" (SelectCountry), plus "+N more in Operations".
- **Expeditions:** "N live · M reports" with "Open operations ›".
- **Offers:** recruit offers, plus "Upgrade: <node> · N coins" when affordable.
- **Recent reports.**

Footer: the `home_primary` button plus the clock controls (moved from camp.rs:46-48 and time_controls at camp.rs:318-380). The clock appears on Home only, and the `world_clock` key works on Home only.

Camp row (navigation.rs:113-128):
- 6 entries: Camp, Upgrades, Briefing, Equipment, Character, World map (measured about 563 px of 742 at small).
- Tab::Survivor and Tab::Operations render the breadcrumb "‹ Camp home · Mara/Operations", and Camp stays selected.
- The header plague alert and "View live fields" still deep-link to Operations.
- Before the first start, a Home request redirects to World.
- `open_camp_home(focus)` is new; `open_campaign()` stays the World route.
- `resume()` falls back to Home.
- The review matrix fallback (matrix.rs:358) becomes Home, coordinated with qa-captures.

### World and Operations

- For a non-first country, the World footer is "Plan defense" or "Manage country", dispatching `SelectCountry(id)` and then `open_preparation(Tab::Briefing)`. It never creates an attempt.
- Operations "Defend now" (operations.rs:47-57) becomes "Plan response".
- ChooseStart stays for the first country.
- `country_summary` (camp.rs:191-210) adds "<N> defenses to liberate <Country>" and "Liberating it opens Mara (75 coins) and recruiting (100 coins)" from `feature_unlock` and `level_count()`, plus "Smaller countries liberate sooner." while none is liberated.

### Briefing

- Primary: "Deploy personally" for the player, else "Deploy <name>" (replacing briefing.rs:265-272).
- briefing.rs:206 becomes "Review <name>'s training". Loadout lines read the next defender (briefing/view.rs:76-79).
- `begin()` (context.rs:278-286) derives mode from `campaign.next_defender`.

### Save UX

The manual save is labelled "Save":
- a header right-group button on Camp, Results and Upgrades (between History and Menu);
- battle Management "Save" (replacing "Save deployment", battle.rs:452).

Removed: "Save campaign" (camp.rs:43) and "Save journey". Compound actions are "Save and quit" (menu.rs:177, renamed from "Save and exit") and "Save and return to main menu" (battle.rs:455). The pending text is "Saving…".

`RuntimeNotice::Saved` Ok for any intent other than Autosave pushes "Saved" (Success). With recovery copies it pushes "Saved · recovery copies kept" (Info, 6 s). The status_line stays on Camp and Settings, and battle status becomes toasts.

### Storage modal

Save failure (any intent, Autosave included; it replaces whatever modal is open):
- "Retry" (amber; re-enqueues the save, and Autosave becomes Manual);
- "Open folder" (OpenSaveFolder);
- "Copy path" (`ctx.copy_text`);
- "Dismiss".

Load failure (profile.rs:84-98, `LoadedProfile.load_error`):
- "Retry load" (ReloadProfile, gated as in §3);
- "Open folder";
- "Copy path";
- "Continue with a fresh journey".

Settings failure: the text is "Settings could not be read; defaults are in use. The original is kept as settings.unreadable.json.", with "Open folder" and "Use defaults".

### Settings

- **Draft warning.** `draft_dirty = prefs.balance_draft.as_ref() != Some(&campaign.balance_settings)`. Every leave route (Escape, the primary at preferences.rs:27, "Save and return to main menu" at :31, and header buttons) opens DraftWarning:
  - "Apply and return" (amber; `SetBalance(draft)` only when `draft.validate()` is Ok, else disabled with "Fix the highlighted value first.");
  - "Discard and return";
  - "Keep editing" (also the Escape action).
- **Scale slider** edits `scale_draft`, labelled "Interface scale (applies on release)". It commits on `drag_stopped()`, or on `changed() && !dragged()`.
- **Display:** "Fullscreen (borderless)", "VSync", "Low latency (restart to fully apply)". The Bevy system `apply_display_settings` (a Local holding the last-applied tuple) sets `Window.mode` and `present_mode`; perf owns the fallback.
- **Readability:** "Quality markers (text and shape)" and "Colour-blind quality palette", with a 6-chip preview.
- **Audio:** Master, Combat effects, Atmosphere, "Music & stingers" (`music`) and "Interface sounds" (`ui`).
- **Controls:** the 8 core rows ("Pause (battle)") plus "World clock (camp home)" and "Cycle attack". `cycle_attack` cycles the unlocked Gun, Grenade and Molotov selections, setting `f.battle.attack_selection` as attack_selector does (battle.rs:731-736).

### Battle

- **Camera:** `fit_field(layout.arena_safe, lanes, width, reserve_top)`. The construction toggle never changes the rect.
- **Construction palette:** in `layout.selector_or_palette`.
  - [Trap · s] [Wall · s] [Turret · s] as selectable buttons sharing (column width - 8)/3 (66 px at small), 12 pt;
  - one truncated line "<N> supply · <name>: H HP · D dmg · R yd";
  - disabled hover reasons are re-pointed to "Upgrades · Fortification";
  - "Save country plan" and "Clear country plan" move into Management;
  - the 280 px overlay (battle.rs:330-347) and the stale text (battle.rs:995-1000) are deleted;
  - construction still hides the attack selector, personal controls and pause banner.
- **Placement:**
  - the first-defense hint (battle.rs:638-659) draws in the reserve band (F.top-88+4, height 84);
  - the pause banner keeps its place (bottom-centre of the safe area), with a test that it never intersects the defender lane;
  - the loot receipt and battle toasts go in `layout.slot`;
  - the attack selector goes in `layout.selector_or_palette`. Both are decoupled from the field top (battle.rs:308-322, :679-684).
- **Bar:**
  - personal: [Pause|Begin defense|Resume defense] [Emergency aid] [Brace the line] [Build defenses|Close construction] [Retreat & train (hold)] [Management];
  - NPC: [Pause] [Observe 1×2×4×] [Tactics] [Recall & train] [Management]. The "Survivor" popup is renamed "Tactics", and its title names the defender;
  - Reload, Strike contact lane and Toggle firing (battle.rs:842-864) move to Management "Field actions"; their keys and legend stay;
  - the battle status row (battle.rs:467-469) is removed.
- **Retreat hold:**
  - `retreat_hold` is set while `is_pointer_button_down_on()`, or while the button is focused and Enter or Space is down;
  - an AMBER fill grows left to right, and the label reads "Hold to retreat…";
  - releasing early cancels and shows the hint "Hold to retreat" for 1.5 s; Escape cancels;
  - at 0.8 s it dispatches `Command::Withdraw`;
  - reduced effects shows a stepped fill (0/25/50/75/100%);
  - hover: "Hold 0.8 s to end this defense and open Results. Earned coins and items stay banked.";
  - "Recall & train" behaves the same.
- **Boss banner:**
  - a new key (attempt id, BossWarning tick) from the single per-pass BattleScreen resets `shown`;
  - `shown += stable_dt.min(0.1)` only while unpaused and on Screen::Battle;
  - visible while `shown < 2.0`;
  - rect: centred at `arena_safe.center().x`, top = F.top+6, size (min(380, 0.6*w), 34), 20 pt AMBER on PANEL at alpha 220, with the subline "<boss> · arrives in N s";
  - normal mode fades in 0.15 s and out 0.30 s; reduced mode is static;
  - fx.rs:178-194 is removed;
  - the Boss event swaps the text to "<BOSS> HAS ARRIVED" for 1.5 s, replacing the 0.45 s tag at fx.rs:205-227;
  - the HUD "BOSS IN mm:ss" (battle.rs:569-571) stays.
- **Low HP:**
  - an 8 px RED band inside all 4 canvas edges, painted after the field and before the HUD frames;
  - normal mode pulses alpha 0.25 to 0.50 at 1 Hz; reduced mode is a static 0.40;
  - the HP text becomes "<name> · HP h / m · LOW";
  - it applies to Personal and main-run NPC defenders, not to the observer.

### Onboarding (ui/onboarding.rs)

`CALLOUTS: &[Callout{id, text, open: fn(&mut UiCtx), unlocked: fn(&Game)->bool}]` covers these ids:
- feature:Companion ("New: Mara - find her in Camp home")
- feature:Recruitment ("New: Recruiting - liberation Results and Camp home")
- feature:Parallel ("New: Expeditions - Camp home › Operations")
- feature:Operations
- blueprint:Trap|Wall|Turret ("New: Build defenses - press <Build key> in battle")
- ordnance:Grenade|Molotov
- takeover ("New: Recruits can defend - choose on Results or Briefing")
- auto-sell (on the first quality drop)

Behaviour:
- One callout shows at a time, under the camp row on Camp and Upgrades, never in Battle or on Results.
- Buttons "Show me" and "Got it". Visiting the place also marks it seen.
- It is hidden but not marked when `show_guidance` is false.
- **Seeding:** on the first frame with `callouts_seeded == false`, insert every unlocked id and set seeded to true. "Restore defaults" re-seeds.
- Settings persist on every save (persistence.rs:95-96).

### Expedition notices

- `toast_layer` reads the CD `Game::unannounced_outcomes()`. Each notice becomes exactly one toast, such as "<Recruit> liberated <Country> · N new fronts" or "<Recruit> held <Country> · +C coins", and is added to Home "Recent reports".
- The UI dispatches `Command::AcknowledgeOutcomes{seqs, folded}` the same frame.
- Delivery is deferred while Screen::Battle shows an unpaused live attempt.
- A nonzero `folded` count shows one toast, "N more expedition outcomes · Operations".
- Toasts never mention auto-sell.

### Auto-sell panel (ui/armory.rs section, or ui/armory/auto_sell.rs)

A CollapsingHeader "Auto-sell" under "Manage equipment" containing:
- "Auto-sell new drops";
- a "Sell below" ComboBox (Uncommon/Rare/Epic/Legendary) with markers;
- 4 slot checkboxes (`Slot::label`);
- "Keep upgrades for the defender who ran it";
- "Also on expeditions and operations";
- "Would have sold N · +C" from `auto_sell_preview`;
- "Apply rule to stash now…".

Every change dispatches `SetAutoSell` immediately. "Apply…" opens ApplyAutoSell: "Sell N items for C coins?" with amber "Sell N items" (`ApplyAutoSellToStash{expected: quote}`) and "Cancel". On `QuoteChanged` the modal re-quotes. Success toasts "Sold N · +C". Battle never paints sale text.

## 5. Settings fields (settings.json; `bindings: [String;8]` unchanged)

| Field | Default |
|---|---|
| `extra_bindings{world_clock, cycle_attack}` | "P", "Q" (both free in today's defaults: Mouse1, Mouse2, R, 1, 2, B, Escape, F) |
| `music` | 0.5 |
| `ui` | 0.6 |
| `fullscreen` | false |
| `vsync` | true |
| `low_latency` | false |
| `quality_markers` | false |
| `colour_blind_palette` | false |
| `seen_callouts` | empty BTreeSet<String> |
| `callouts_seeded` | false |

Audio may retune `music` and `ui` for the -23 LUFS gate at default sliders.

## 6. Contract proposed to campaign-defenders and save-determinism (consumed by ux)

`StrategyState.outcome_notices: Vec<OutcomeNotice{seq, battle_id, recruit_id:Option<u64>, country_id:String, outcome, coins:u64, liberated:bool, fronts_opened:u16}>`, `next_notice_seq: u64` and `folded_notices: u32`. Each is `#[serde(default, skip_serializing_if = empty/zero)]`.
- One notice is pushed per settlement. The queue survives pruning (it is independent of `ParallelBattle`).
- The cap is `MAX_OUTCOME_NOTICES = 64`. On overflow the oldest is folded into `folded_notices` (counted, never silently dropped), so every outcome is announced exactly once, individually or in the folded toast.
- `Command::AcknowledgeOutcomes{seqs:Vec<u64>, folded:u32}` is idempotent: unknown seqs are ignored and `folded` saturates. It returns Ok(Unchanged) when nothing changed.
- v6→v7: empty queue, so nothing is re-announced.
- This supersedes `ParallelBattle.announced`. If CD keeps the flag instead, ux consumes it and the pruning at parallel.rs:144-149 must refuse to prune unannounced settled battles.

Also consumed: the v6→v7 mapping `Report/Attempt.defender` (Npc→Mara, Personal→Player), with ux consumer tests asserting it.

## 7. Ordered steps

See the `steps` array. Wave-3 order: Harness and primary, Home, Upgrades and Results, Escape, then one finding per commit (UX-07, UX-08, UX-09, UX-10, decision 18, camera, palette, UX-14, UX-13, UX-11, UX-12, picker, SelectCountry, recruit, loot, auto-sell, onboarding, notices, labels), and finally the full loop. The escape matrix table grows a row family in each step that adds an overlay (DraftWarning, Storage, Recruit, ApplyAutoSell, retreat_hold), so every commit stays green.

## 8. Digest moves

None from ux-flow. Labels and feature_unlock are pure. Settings are outside the digest. The CD queue fields use skip_serializing_if, and GOLDEN runs have no parallel battles (tests/determinism.rs). `ui_frames_do_not_change_state_digest` renders 100 paused frames and asserts the digest is unchanged.

## 9. Risks

1. **Camera interpretation for fields with more than 11 lanes.** These are pixel-identical to today (they already touch an axis). Confirm at the wave-3 capture review. 3x12 lanes are 157 pt at 1440, so vfx checks sheet upscale blur.
2. **About 20 pinned tests need rewrites with written reasons:**
   - battle.rs:1089-1104 and :1179-1199;
   - battle_tests.rs:668-685, :733, :748;
   - camp_tests.rs:27, :52;
   - results_tests.rs:110-160, :189-256;
   - navigation_tests.rs:22-68, :205-268;
   - screen.rs:84;
   - app_tests (default tab).
3. **Escape and egui internals.** This depends on popup.rs:625 and modal.rs:156. Consuming Escape at the top of draw_screen hides it from `Settings::pressed`, so battle pause relies on PassToPause (pinned by a test).
4. **Wave-2 dependencies.** These arrive from CD, LE and PT in wave 2. The loop test fails at the first missing API until then.
5. **perf-architecture's single per-pass BattleScreen.** The banner and low-HP code must read it; today it is built 3 times (battle.rs:68, :105, :217). The Interface target is <=1.13 / <=1.03 ms.
6. **Mouse-only players.** They lose visible Reload, Strike and Toggle buttons. The keys stay in the legend; add a playtest note.
7. **Seen callouts are per profile.** A New game in the same profile does not re-teach. This is documented.
8. **OpenSaveFolder** needs a manual check in PLAYTEST_GUIDE.
9. **The single modal slot** drops a preempted modal when a Storage error arrives. This is documented.
10. **The 66 px palette buttons at small size** need a capture check.

## 10. Numbers to measure (before and after, at 1440x900 and 1050x700@1.35)

- Arena rect per geometry: 3x12, 5x14, 7x16, 9x18, 11x20, 11x30, 13x22, 17x30.
  - Expected at 1440: 3x12 1392x471.8 (yard 116.0, lane 157.3); 5x14 1392x674; 7x16 1166.6x692; 9x18 1020.8x692; the rest identical.
  - Expected at small: 3x12 729.8x215.0 (with the 86 clamp; yard 60.8, lane 71.7); 7x16 611.6x310.5.
- Gap from the build zone to the palette or field: the palette must not intersect the field.
- Bar rows at small: <=2 (796 px with "Resume defense").
- Primary contrast, about 10:1.
- Boss banner visible wall time at 1×, 2× and 4×: >=1.5 s, nominally 2.0 s.
- Escape presses from any camp tab to the Quit modal: <=5.
- Toasts: visible count, and TTL against the wall clock.
- Loop test runtime: <30 s in debug.
- Interface frame mean on progressed-peak 225 enemies: must stay within the perf target (<=1.13 ms / <=1.03 ms) once the new overlays are in.
- Debug-format scan hits: 17, then 0.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U4. MusicPlugin scope

- **Owner / wave:** audio, wave 6.
- **Ruling:** MusicPlugin = music director + Music bus + stingers from existing banks; every track slot is Track::Deferred; the slider is labelled "Music & stingers".
- **Supersedes in this spec:** The audio slider label is "Music & stingers" (and "Interface sounds" for ui).

#### U6. Camera

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Uniform zoom-to-fit that keeps the lane:yard proportion; fewer lanes means more zoom; construction never rescales the arena. Detail in R1 and the camera-layout cluster.
- **Supersedes in this spec:** Own fit_field signature and top-band construction palette.

### Orchestrator rulings

#### R1. Camera formula, owner and construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Base = today's per-geometry clearing_rect for (lane_count, width_yards) including its clamps; uniform zoom z = min(arena.w/base.w, arena.h/base.h) (&gt;= 1), centred; no texel or magnification clamp; 17x30 and every other already-filling field stays pixel-identical; the worker observer window uses the same fit function. Owner vfx-lighting, implemented in WAVE 1 because later captures depend on it. Construction is an overlay panel drawn over the field (no arena rescale), with an INCOMING occlusion chevron when the panel covers spawns; Save/Clear plan stay in the panel.
- **Supersedes in this spec:** Battle camera fit_field in wave 3; compact construction palette in the top-band selector slot; moving Save/Clear plan to Management.

#### R2. Defender identity

- **Owner / wave:** campaign-defenders, wave 1-2.
- **Ruling:** Serde key "defender" on Attempt and Report; an absent value means the mode owner (Player for Personal, Companion/Mara for Npc), so existing GOLDEN digests do not move for this reason. Mara stays selectable from the Companion node; recruits from the Recruitment node. One status model (RecruitStatus::Defending appended) and one npc source of truth (the roster entry, justified for determinism in D6). Takeover deploy stamps the recruit's appearance identity, not Mara's Female.
- **Supersedes in this spec:** Any UI that maps Npc to Mara directly: names come from defender_card.

#### R10. UI prices read core

- **Owner / wave:** progression-tree, wave 2-3.
- **Ruling:** Recruit and training prices in the UI read the core price function (Campaign::recruit_price / npc_training_price, Game::recruit_offer().price).
- **Supersedes in this spec:** Hard-coded "· 100 coins" and "· 25 coins" labels.

### Master-planner reconciliations

#### P5. Results loot block and Auto-sell panel owner

- **Owner / wave:** loot-economy, wave 3.
- **Ruling:** loot-economy owns the Results loot block and the Equipment Auto-sell panel (W3-20, W3-21). ux-flow signs off on the primary-action rule and layout.
- **Supersedes in this spec:** UX #22/#23 ownership of these widgets.

### Cluster: camera-layout

#### C-1. Camera owner, wave and file placement

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** vfx-lighting implements camera and battle layout in wave 1 in four commits: C1 crates/horde_game/src/art/camera.rs (pure Rect math, re-exported as art::{clearing_size, fit_field, FieldFit}); C2 ui/battle/layout.rs BattleLayout plus main-battle wiring; C3 observer window; C4 INCOMING chevron. perf's ui/battle split (W1-39) lands before C2. Presentation-only: no saved field, no digest move, no Command change. perf measures the commit before C2 and C2. qa verifies by captures and does not write camera tests. C2 deletes clearing_rect and battle_arena (grep guard).
- **Supersedes in this spec:** "Battle camera fit_field" and BattleLayout::new(canvas) in wave 3.

#### C-2. Camera formula

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** art/camera.rs: REFERENCE_LANES 11.0, REFERENCE_WIDTH_YARDS 30.0, MIN_CLEARING vec2(160, 86); FieldFit{rect, zoom}; clearing_size(reference, lanes, width) is today's clearing_rect size moved verbatim from ui/battle.rs:475-489; fit_field(reference, area, lanes, width): zx = area.w/base.x, zy = area.h/base.y, the smaller wins and the limiting axis snaps exactly to the area; centred. No texel clamp. Every field with &gt;= 11 lanes is bit-identical to today. Pinned zooms: arena 1392x692: 3x12 1392x471.8 z2.5, 5x14 1392x674.0 z2.143, 7x16 1166.6x692 z1.571, 9x18 1020.8x692 z1.222; arena 729.8x310.5: 3x12 729.8x215.0, 5x14 729.8x302.5, 7x16 611.6x310.5, 9x18 535.2x310.5. EXPANSION.md:99 and PIXEL_PRESENTATION.md:77 get a dated supersession note. Tests: fresh_three_lane_battle_fills_the_arena_width_at_small_size (fail-first), fit_field_zoom_table_at_both_target_sizes, arena_fits_full_lanes_and_width_on_the_limiting_axis, fit_field_keeps_every_filling_geometry_bit_identical (pin with two mutation proofs).
- **Supersedes in this spec:** fit_field(maximum, lanes, width, reserve_top) signature.

#### C-3. Single BattleLayout API

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** One pure struct in ui/battle/layout.rs built once per battle frame: BattleLayout{canvas, arena, field, hint_band, hint_callout, effects_clip, hud_left, hud_right, selector, receipt_slot, construction_panel, pause_banner_center, bar, legend_anchor} with BattleLayout::new(canvas, lanes, width_yards, hint_band). Constants TOP_BAND 124, ARENA_SIDE_INSET 24, ARENA_BOTTOM_INSET 84, HINT_BAND 92, HINT_CALLOUT_TOP 4, HINT_CALLOUT_HEIGHT 84, FX_TOP_HEADROOM 60, PANEL_MAX_WIDTH 280, PANEL_RIGHT_MARGIN 16, PANEL_TOP_GAP 8, PANEL_MIN_HEIGHT 180, PANEL_FILL_ALPHA 235, CHEVRON_GAP 10. arena = today's battle_arena(canvas,false); selector and receipt_slot anchor to arena.top (never move with zoom); bar and legend unchanged. f.battle.arena_rect = layout.field.rect, effects_rect = layout.effects_clip. Later owners may append fields, not redefine them.
- **Supersedes in this spec:** BattleLayout::new(canvas) region table (arena_safe, selector_or_palette, slot, bar bottom-80..bottom-17).

#### C-4. Hint band

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** hint_band = view.first_deployment && view.personal(), for the whole attempt, independent of show_guidance. Field area = arena minus a 92-pt top band, honoured only when the fit zoom stays &gt;= 1; otherwise fall back to the full arena (filling fields stay bit-identical). The callout is drawn at today's rect only when show_guidance && first_deployment && personal. Npc first deployments get no band.
- **Supersedes in this spec:** reserve_top / FIRST_DEFENSE_RESERVE = 88 and drawing the hint in the reserve band.

#### C-6. Construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** The construction panel overlays the field; the field rect never depends on construction. width = min(280, canvas.w*0.5-32), left = canvas.right-16-width, top = arena.top+8, height = max(arena.h-16, 180): bit-identical to today at 17x30; never covers a build cell. Fill black alpha 235, scroll area; construction_controls including "Save country plan"/"Clear country plan" stay inside. Field hover is ignored inside the panel. Tests: construction_toggle_keeps_arena_rect (fail-first), construction_panel_never_covers_a_build_cell, construction_panel_keeps_plan_buttons, construction_panel_swallows_field_hover.
- **Supersedes in this spec:** Compact palette in the top-band selector slot, Save/Clear plan moved to Management, deletion of the 280-px overlay.

#### C-8. Observer window fit

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** ui/operations.rs:276-278: effects_clip = r; r = art::fit_field(r, r, lanes, width).rect. No hint band or headroom. BattleUi.observer_arena_rect is written each drawn frame and cleared on close. scene_look stays later (W5-01).
- **Supersedes in this spec:** fit_field(rect, lanes, width, 0.0) signature.

#### C-10. HUD at most 2 rows

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** The HUD row metric is the bottom bar. BattleUi.bar_rows is measured per frame by a BarRows accumulator; ReviewFrameFacts.hud_rows = Some(bar_rows) from then on. Reload, Strike contact lane and Toggle firing move to Management &gt; Field actions (keys kept); the status line is removed and its message goes to the battle toast in layout.receipt_slot. The construction palette is not moved (R1). Tests: battle_bar_rows_measure_todays_bar (characterisation, 3) then battle_bar_stays_within_two_rows_at_small_size.
- **Supersedes in this spec:** selector_or_palette top-band slot and the bar rect bottom-80..bottom-17.

#### C-11. Wave-3 UX consumers of BattleLayout

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** ux-flow does not redefine geometry. The battle toast draws in layout.receipt_slot. The boss banner is centred at arena.center.x, size (min(380, 0.6*arena.w), 34), top = hint_callout.bottom+4 when the callout is visible else arena.top+6. ux may append fields (e.g. boss_banner) computed from existing ones. Escape CloseConstruction closes the overlay.
- **Supersedes in this spec:** Boss banner "top = F.top+6" against the reserve band; the slot region y +60..+118.

### Cluster: defenders

#### D9. next_defender location and commands

- **Owner / wave:** campaign-defenders, wave 2.
- **Ruling:** Campaign.next_defender: Option&lt;DefenderId&gt; (skip None, None in workers); Game gets no field. Game::next_defender() = campaign.next_defender.unwrap_or(last_result.map_or(Player, Report::defender)). Appended SetNextDefender(DefenderId) and BeginDefense{stage} (deploys next_defender, unpauses like Deploy, clears it). Deploy{stage, mode} keeps its recorded meaning and never touches next_defender. ContinueJourney deploys next_defender(); retry/repeat use report.defender(). Dispatching the next defender elsewhere resets it to Player with a note. Mara via Companion, recruits via Recruitment.
- **Supersedes in this spec:** begin() deriving mode from campaign.next_defender (the UI dispatches BeginDefense).

#### D10. Expedition notice persistence and commands

- **Owner / wave:** campaign-defenders, wave 2 (types W1).
- **Ruling:** ParallelBattle.summary: Option&lt;ExpeditionSummary{outcome, stages_cleared, liberated, fronts (&lt;=32), coins, loot_count, acknowledged}&gt; plus StrategyState.folded_outcomes. Written in settle_parallel and finalised at settled = true. Pruning drops summary-None or acknowledged settled battles first; otherwise folds. Game::unacknowledged_outcomes() -&gt; OutcomeNotices. Appended AcknowledgeOutcomes{battles, folded}: idempotent, returns Applied. v6 settled battles are never announced. Types ship in wave 1 (W1-16).
- **Supersedes in this spec:** StrategyState.outcome_notices, next_notice_seq, MAX_OUTCOME_NOTICES 64 and AcknowledgeOutcomes{seqs, folded}.

#### D15. Camp prices in core and existing UI labels

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** PT W2-D adds Campaign::recruit_price() and npc_training_price(); StrategyState::recruit (strategy.rs:296) and training (progression.rs:726) charge them; the same commit replaces ui/operations.rs:158/160/206 and ui/character.rs:193 literals, enable checks and hover text. Test camp_price_labels_follow_core_price_at_rank_10 (200 / 50 coins).
- **Supersedes in this spec:** Flat recruit_cost and "25 coins" literals.

#### D16. Wave-3 defender labels, picker, notices and price text

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** UI reads only core: Recruit modal and liberation block price from Game::recruit_offer(country).price; picker from available_defenders() dispatching SetNextDefender; Briefing primary dispatches BeginDefense{stage}; HUD/Results names from defender_card; HUD "MARA" and the forced female portrait (ui/hud/view.rs:84-91) removed; toasts from unacknowledged_outcomes() with AcknowledgeOutcomes in the same frame. UX does not commit red consumer tests for core contracts.
- **Supersedes in this spec:** Hard-coded "Recruit from &lt;Country&gt; · 100 coins"; Game::unannounced_outcomes() and {seqs, folded}; the w2 "consumer acceptance tests written by ux" step.

### Cluster: settings-audio-tooling

#### S1. Settings v2 struct

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Keep the 13 keys and bindings: [String; 8]. Add #[serde(default)] version: u32 (absent = 0), extra_bindings: ExtraBindings{world_clock "P", cycle_attack "Q"} (struct), window_mode: WindowModeSetting{Windowed, BorderlessFullscreen}, vsync true, low_latency false, music f32 ("Music & stingers"), ui f32 ("Interface sounds"), mono_audio false, quality_markers: QualityMarkers{ColorOnly, Text, Shape, TextAndShape}, palette: Palette{Standard, ColorBlind}, seen_callouts: Option&lt;BTreeSet&lt;String&gt;&gt;. Enums are append-only and safe under per-key salvage. SETTINGS_VERSION 1 from wave 1, 2 from wave 6. Defaults in settings::defaults (audio owns values; wave-1 0.6/0.7/0.15/0.7/0.7). Lands before perf's render_plan.
- **Supersedes in this spec:** fullscreen / colour_blind_palette bools, callouts_seeded, music 0.5 / ui 0.6, primitives-only rule.

#### S2. Settings loader, salvage and migration order

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Pure Settings::decode(bytes) -&gt; SettingsDecode{settings, dropped_keys, not_json} with per-key salvage; IO profile::read_settings(path) -&gt; SettingsLoad{settings, warning, preserved} copies the original to settings.unreadable.json before returning. Order: salvage; version &lt; 1 sets ui = effects; (wave 6) per-field default migration; clamp / non-finite to default; normalise() unbinds extra keys equal to core keys; version = SETTINGS_VERSION. Wave-3 modal text: "Settings could not be read; defaults are in use. The original is kept as settings.unreadable.json."
- **Supersedes in this spec:** load_checked alongside the old load.

#### S3. Startup settings read and display apply

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** The profile is fixed per process. main.rs reads settings once before App::new, builds RenderPlan (present_mode_for: low_latency -&gt; Mailbox, vsync -&gt; Fifo, else AutoNoVsync; pipelined = override or !low_latency && PIPELINED_DEFAULT), and passes the SettingsLoad into FrontendParts::load; load_profile no longer reads settings.json. One system render_plan::apply_display_settings (run_if resource_changed::&lt;Settings&gt;) writes Window.mode and present_mode live; pipelined never changes live; the UI shows "(applies after restart)" exactly when pipelined_for(low_latency, None) != plan.pipelined. UX owns only the widgets.
- **Supersedes in this spec:** Its own apply_display_settings system.

#### S4. seen_callouts seeding, New game and Restore defaults

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** Feature callout ids are "feature:" + CampaignFeature::id() (core, append-only: construction|companion|recruitment|parallel|operations); others "&lt;area&gt;:&lt;name&gt;". ui::onboarding::seed_seen_callouts sets None to Some(every unlocked CALLOUTS id) once in FrontendParts::new. New game sets Some(empty). Settings::restore_defaults resets preferences but keeps seen_callouts.
- **Supersedes in this spec:** callouts_seeded flag, first-frame seeding, "Restore defaults sets None", risk 7 "New game does not re-teach".

#### S5. Audio slider defaults, mix version and mono

- **Owner / wave:** audio, wave 6.
- **Ruling:** No audio_mix_version: Settings.version is the mix version. Wave 6 sets SETTINGS_VERSION 2, switches defaults to calibrated values (targets master 0.8, effects 0.8, ambience 0.5, music 0.7, ui 0.7, retunable in 0.05 steps by the -23 LUFS gate) and enables the per-field migration against defaults::WAVE1_*. Audio wires mono_audio and the "Mono audio" checkbox in wave 6. UX's wave-3 audio section adds only the Music & stingers and Interface sounds sliders.
- **Supersedes in this spec:** No mono checkbox in wave 3.

#### S6. Extra bindings and rebinding

- **Owner / wave:** ux-flow, wave 3.
- **Ruling:** ExtraAction{WorldClock, CycleAttack} with ALL, name() and accessors; one binding_pressed(ctx, binding, held) used by Settings::pressed and pressed_extra; conflict detection and swap span both tables; an unbound ("") extra action never fires.
- **Supersedes in this spec:** Nothing beyond the ruling itself; this remit implements it.

### Cluster: enemies-world-pacing

#### W-2. Operations unlock event for the unlock-order check

- **Owner / wave:** progression-tree, wave 2.
- **Ruling:** Operations unlock = the Operations page becoming reachable: Game::operations_available() moved verbatim from ui/context.rs:230-240 (ui delegates). Milestone::OperationsOpen appended. Fortifications = Feature(Construction), Mara = Feature(Companion). goal5_unlock_order_fortifications_mara_operations: per seed Fort &lt;= Mara &lt;= OperationsOpen and medians strictly increasing.
- **Supersedes in this spec:** Campaign::feature_unlock is a label accessor, not the event.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R21. Custom chip keeps the preset it was clicked from

- **Owner / wave:** enemy-balance (E3 semantics) and ux-flow (the settings screen), wave-1 close.
- **Ruling:** The Custom difficulty chip keeps the basis of the preset it was clicked from (clicking Hard then Custom without edits sends Custom with the Hard basis). Fix in code with TDD.
- **Supersedes in this spec:** Sending an unedited named draft as that named preset after the Custom chip. The core stays one `SetBalance` per step: the screen sends the named preset, then the same values as Custom, so the named preset is the previous preset (as the new-journey reset already does).

### Rulings owned by this remit

C-10, C-11, D16, S4, S6
