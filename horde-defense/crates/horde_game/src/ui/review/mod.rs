//! Explicit isolated screenshot fixtures. These startup-only staged campaigns
//! are never claimed as balance proof or used by the ordinary user profile.
#[cfg(feature = "review")]
use super::BattleUi;
use super::{FrontendParts, Screen, Tab, armory, battle};
use crate::settings::Settings;
use horde_core::{
    campaign::{BodyAttribute, NpcTraining, Skill},
    content::{Mode, Slot, StructureKind, content},
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::{Game, Input, Outcome},
    survivors::SurvivorId,
    world::world,
};

#[cfg(feature = "review")]
mod player_model;
#[cfg(feature = "review")]
mod workshop;
#[cfg(feature = "review")]
pub(super) use workshop::{NativeModelReport, model_report, model_workshop};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureFixture {
    Throws,
    Melee,
}

fn gesture_fixture(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    practice: bool,
) -> Option<GestureFixture> {
    if !motion || !sequence || practice {
        return None;
    }
    match scene {
        Some("gesture-throws") => Some(GestureFixture::Throws),
        Some("gesture-melee") => Some(GestureFixture::Melee),
        _ => None,
    }
}

#[cfg(any(test, feature = "review"))]
fn scheduled_gesture_input(
    kind: GestureFixture,
    frame: u32,
    attempt: &horde_core::sim::Attempt,
) -> Option<Input> {
    let lane = attempt.lane_count / 2;
    let aim = [attempt.width_yards * 0.6, f32::from(lane) + 0.5];
    let mut input = Input {
        aim,
        ..Default::default()
    };
    match (kind, frame) {
        (GestureFixture::Throws, 64) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: [(attempt.width_yards * 0.6).floor() as u8, lane],
            })
        }
        (GestureFixture::Throws, 96) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: [(attempt.width_yards * 0.7).floor() as u8, lane],
            })
        }
        (GestureFixture::Melee, 64) => input.melee = true,
        _ => return None,
    }
    Some(input)
}

/// Capture-service timing bridge. Only explicit isolated gesture exports
/// can enqueue these ordinary controls; fixed runtime/core owns all effects.
#[cfg(feature = "review")]
pub(crate) fn schedule_gesture_review(
    launch: &crate::launch::LaunchConfig,
    game: &Game,
    battle: &mut BattleUi,
    frame: u32,
) {
    let Some(kind) = gesture_fixture(
        launch.scenario.as_deref(),
        launch.motion_review(),
        launch.sequence_path().is_some(),
        launch.practice,
    ) else {
        return;
    };
    let Some(attempt) = game
        .attempt
        .as_ref()
        .filter(|attempt| attempt.mode == Mode::Personal && !attempt.paused)
    else {
        return;
    };
    let Some(input) = scheduled_gesture_input(kind, frame, attempt) else {
        return;
    };
    battle.aim = input.aim;
    battle.pending.aim = input.aim;
    if input.melee {
        battle.pending.melee = true;
        battle.pending_attack_aims.melee = Some(input.aim);
    }
    if let Some(request) = input.throw {
        battle.attack_selection = match request.kind {
            OrdnanceKind::Grenade => battle::AttackSelection::Grenade,
            OrdnanceKind::Molotov => battle::AttackSelection::Molotov,
        };
        battle.pending.throw = Some(request);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LootCaptureMode {
    Disabled,
    Motion,
    Still,
}
fn select_loot_capture_mode(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    still: bool,
    practice: bool,
) -> LootCaptureMode {
    if scene != Some("quality-loot") || practice {
        LootCaptureMode::Disabled
    } else if motion && sequence {
        LootCaptureMode::Motion
    } else if !motion && !sequence && still {
        LootCaptureMode::Still
    } else {
        LootCaptureMode::Disabled
    }
}
impl crate::launch::LaunchConfig {
    pub(super) fn reticle_capture_preview(&self) -> bool {
        matches!(
            self.scenario.as_deref(),
            Some("reticle-reload" | "damage" | "damage-4x")
        ) && !self.practice
            && (self.capture_path().is_some()
                || (self.motion_review() && self.sequence_path().is_some()))
    }
    pub(super) fn damage_capture_mode(&self) -> LootCaptureMode {
        let allowed = matches!(self.scenario.as_deref(), Some("damage" | "damage-4x"));
        select_loot_capture_mode(
            if allowed { Some("quality-loot") } else { None },
            self.motion_review(),
            self.sequence_path().is_some(),
            self.capture_path().is_some(),
            self.practice,
        )
    }
    pub(super) fn loot_capture_mode(&self) -> LootCaptureMode {
        select_loot_capture_mode(
            self.scenario.as_deref(),
            self.motion_review(),
            self.sequence_path().is_some(),
            self.capture_path().is_some(),
            self.practice,
        )
    }
}

/// Review-only presentation override. Persistent and ordinary profiles keep
/// their chosen preference, and no game clock or random state is inspected.
pub(super) fn apply_reduced_override(
    f: &mut FrontendParts,
    scenario_present: bool,
    practice: bool,
    force_reduced: bool,
) {
    if scenario_present && !practice && force_reduced {
        f.settings.reduced_effects = true;
    }
}

pub(super) fn prepare(f: &mut FrontendParts) {
    let launch = f.launch.clone();
    let Some(screen) = launch.scenario.clone() else {
        return;
    };
    if screen == "npc-stutter" {
        prepare_copied_npc(f);
        return;
    }
    f.game = Game::default();
    if !launch.practice {
        f.settings.master = 0.0;
    }
    if let Some(scale) = launch.review().and_then(|r| r.scale) {
        f.settings.scale = scale;
    }
    f.status.message = "Practice scenario - progress is isolated from your campaign".into();
    if screen == "menu" {
        return;
    }
    if screen == "survivor-select" {
        f.screen = Screen::SurvivorSelection;
        f.campaign.preview_survivor = SurvivorId::Rook;
        return;
    }
    let identity = if matches!(
        screen.as_str(),
        "menu-female" | "battle-female" | "gesture-melee"
    ) {
        SurvivorId::June
    } else {
        SurvivorId::Rook
    };
    f.game
        .campaign
        .choose_survivor(identity)
        .expect("review survivor identity");
    f.campaign.preview_survivor = identity;
    if screen == "menu-female" {
        f.screen = Screen::Menu;
        return;
    }
    if matches!(screen.as_str(), "peak" | "progressed-peak" | "max-lanes") {
        f.game = if matches!(screen.as_str(), "progressed-peak" | "max-lanes") {
            horde_core::sim::maximum_progression_render_fixture()
                .expect("maximum authored progression render fixture")
        } else {
            horde_core::sim::peak_render_fixture().expect("valid paused peak-render fixture")
        };
        f.screen = Screen::Battle;
        f.campaign.mode = Mode::Npc;
        f.campaign.selected_country = "USA".into();
        f.status.message = if matches!(screen.as_str(),"progressed-peak"|"max-lanes") { "Synthetic maximum-rank fixture · authored population cap plus boss · no gameplay or FPS claim" } else { "Paused peak-render fixture · no simulation FPS claim" }.into();
        return;
    }
    f.screen = Screen::Camp;
    if screen == "history-empty" {
        f.screen = Screen::History;
        f.nav.history_return = Screen::Menu;
        f.nav.history_world_paused = true;
        f.game.set_paused(true);
        return;
    }
    if screen == "world" {
        f.game.set_paused(true);
        return;
    }
    let country = match screen.as_str() {
        "arid" => "EGY",
        "tropical" => "BRA",
        _ => "DEU",
    };
    f.game
        .campaign
        .choose_start_country(country)
        .expect("review country");
    f.campaign.selected_country = country.into();
    if matches!(screen.as_str(), "briefing" | "briefing-npc") {
        if screen == "briefing-npc" {
            prepare_progression(&mut f.game);
            f.campaign.mode = Mode::Npc;
        }
        f.act(|c| c.open_briefing());
        f.game.set_paused(true);
        return;
    }
    if matches!(screen.as_str(), "journey-victory" | "journey-liberated") {
        prepare_journey_victory(f, screen == "journey-liberated");
        return;
    }
    if screen == "journey-upgrades" {
        f.game
            .deploy_country_stage(0, Mode::Personal)
            .expect("first journey review round");
        f.game.set_paused(false);
        for _ in 0..60 * 300 {
            if f.game.attempt.is_none() {
                break;
            }
            let input = horde_core::sim::smoke_input(&f.game);
            f.game.tick(input);
        }
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
        f.game.set_paused(true);
        f.campaign.mode = Mode::Personal;
        f.act(|c| c.focus_next_upgrade());
        f.screen = Screen::Results;
        return;
    }
    if screen == "intro" {
        f.game
            .deploy_country_stage(0, Mode::Personal)
            .expect("first-defense fixture");
        f.screen = Screen::Battle;
        f.campaign.mode = Mode::Personal;
        f.game.set_paused(true);
        return;
    }
    if matches!(screen.as_str(), "progressed-battle" | "leveled-armory") {
        let count = f
            .game
            .campaign
            .world
            .active_country()
            .expect("review country")
            .level_count();
        // Explicit completed-run entitlements for a staged view, not an earned
        // balance proof. Repeated templates still count as victories.
        for stage in 0..count {
            f.game
                .campaign
                .first_clear(stage % content().levels.len(), Mode::Personal);
            f.game.campaign.finish_attempt();
            f.game
                .campaign
                .record_country_stage_clear(stage)
                .expect("explicit review progression entitlement");
        }
        if screen == "progressed-battle" {
            f.game
                .campaign
                .enter_country("POL")
                .expect("neighbor after liberation");
            f.campaign.selected_country = "POL".into();
        }
    }
    prepare_progression(&mut f.game);
    match screen.as_str() {
        "gesture-throws" | "gesture-melee" => {
            assert!(
                gesture_fixture(
                    Some(&screen),
                    launch.motion_review(),
                    launch.sequence_path().is_some(),
                    launch.practice
                )
                .is_some(),
                "Gesture fixtures require isolated motion+sequence opt-ins"
            );
            prepare_gesture_battle(f, screen == "gesture-melee");
        }
        "character" => f.nav.tab = Tab::Character,
        "character-max" => {
            f.game.campaign.player.level = f.game.campaign.player_level_cap();
            f.game.campaign.player.xp = 0;
            f.game.campaign.player.points =
                f.game.campaign.player.level - 1 - f.game.campaign.player.body.iter().sum::<u32>();
            f.nav.tab = Tab::Character;
        }
        "campaign" => f.nav.tab = Tab::World,
        "active-camp" => {
            prepare_battle(f, "npc");
            f.screen = Screen::Camp;
            f.nav.tab = Tab::World;
        }
        "survivor" => f.nav.tab = Tab::Survivor,
        "armory" | "leveled-armory" | "quality-armory" => {
            f.nav.tab = Tab::Armory;
            if screen == "quality-armory" {
                let definition = content()
                    .items
                    .iter()
                    .position(|i| i.slot == Slot::Ranged && i.tier == 1)
                    .expect("review rifle");
                for quality in horde_core::quality::Quality::ALL {
                    f.armory.selected_item = Some(
                        f.game
                            .campaign
                            .add_item_with_quality(definition, 4, Some(quality))
                            .expect("explicit quality fixture entitlement"),
                    );
                }
                f.armory.sort = armory::ArmorySort::Quality;
                f.game.set_paused(true);
                return;
            }
            f.armory.selected_item = f
                .game
                .campaign
                .items
                .iter()
                .find(|i| !i.protected)
                .map(|i| i.id);
        }
        "skills" | "skills-survival" | "skills-ordnance" => {
            f.screen = Screen::Results;
            f.nav.tab = Tab::Skills;
            f.campaign.improvement_focus =
                Some(horde_core::campaign::RecommendationAction::LearnNode(
                    match screen.as_str() {
                        "skills-survival" => "field_fitness",
                        "skills-ordnance" => "grenade_training",
                        _ => "pistol_training",
                    }
                    .into(),
                ));
        }
        "engineering" => {
            f.screen = Screen::Results;
            f.campaign.improvement_focus = horde_core::skill_catalog::skills()
                .nodes
                .iter()
                .find(|n| n.branch == "Fortification")
                .map(|n| horde_core::campaign::RecommendationAction::LearnNode(n.id.clone()));
        }
        "history" => prepare_history(f),
        "settings" => f.screen = Screen::Settings,
        "operations" => prepare_operations(f),
        "reticle-reload" | "damage" => {
            prepare_battle(f, "battle");
            f.game.set_paused(false);
            if screen == "reticle-reload" {
                // Consume actual shots; the resource readout never invents ammo.
                let half = f.game.attempt.as_ref().unwrap().magazine / 2;
                for _ in 0..240 {
                    let a = f.game.attempt.as_ref().expect("live reload review");
                    if a.ammo <= half {
                        break;
                    }
                    let aim = [a.width_yards * 0.6, a.defender_position()[1]];
                    f.game.tick(Input {
                        fire: true,
                        aim,
                        auto_reload: false,
                        ..Default::default()
                    });
                }
                f.game.tick(Input {
                    reload: true,
                    ..Default::default()
                });
            } else {
                let target = f
                    .game
                    .attempt
                    .as_ref()
                    .unwrap()
                    .enemies
                    .first()
                    .map(|e| [e.x, e.y()])
                    .unwrap_or([
                        f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                        f.game.attempt.as_ref().unwrap().defender_position()[1],
                    ]);
                f.game.tick(Input {
                    aim: target,
                    fire: true,
                    ..Default::default()
                });
            }
            f.battle.aim = [
                f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                f.game.attempt.as_ref().unwrap().defender_position()[1],
            ];
        }
        "damage-4x" => {
            prepare_battle(f, "npc");
            f.battle.speed = 4;
            f.battle.aim = [
                f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                f.game.attempt.as_ref().unwrap().defender_position()[1],
            ];
        }
        "npc-expanded" => prepare_battle(f, "npc"),
        "critical-npc" => {
            prepare_battle(f, "npc");
            let mut a = f.game.attempt.take().expect("live NPC review");
            let lane = a.lane_count / 2;
            f.game
                .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, false);
            let enemy = a.enemies.last_mut().unwrap();
            enemy.x = 9.0;
            enemy.hp = -2;
            enemy.injury.hp = -2;
            f.game.attempt = Some(a);
            f.battle.speed = 4;
        }
        "quality-loot" => {
            prepare_battle(f, "battle");
            let definition = content()
                .items
                .iter()
                .position(|i| i.slot == Slot::Ranged && i.tier == 1)
                .expect("review rifle");
            let quality = horde_core::quality::Quality::Epic;
            let item_id = f
                .game
                .campaign
                .add_item_with_quality(definition, 3, Some(quality))
                .expect("explicit isolated loot fixture");
            let a = f.game.attempt.as_mut().unwrap();
            a.loot += 1;
            a.loot_events.push(horde_core::quality::LootEvent {
                item_id,
                definition: horde_core::content::ItemDefId::at(definition),
                level: 3,
                quality,
                name: content().items[definition].name.clone(),
                tick: a.tick,
                at: [a.width_yards * 0.62, a.defender_position()[1]],
            });
        }
        "battle" | "battle-female" | "progressed-battle" | "construction" | "ordnance" | "npc"
        | "boss" | "results" | "arid" | "tropical" => prepare_battle(f, &screen),
        _ => f.nav.tab = Tab::World,
    }
    f.game.set_paused(true);
}
/// Strict opt-in copied checkpoint; production and source files are never targets.
fn prepare_copied_npc(f: &mut FrontendParts) {
    let launch = f.launch.clone();
    assert!(
        !launch.practice,
        "npc-stutter requires isolated non-practice review"
    );
    let input = launch
        .review()
        .and_then(|r| r.input_save.clone())
        .expect("npc-stutter requires LASTLIGHT_REVIEW_INPUT_SAVE copied generation");
    let canonical = input
        .canonicalize()
        .expect("copied NPC checkpoint must exist");
    let copied_settings = input
        .parent()
        .expect("checkpoint parent")
        .join("settings.json");
    for source in [&canonical, &copied_settings] {
        if let Ok(source) = source.canonicalize() {
            for target in [
                &f.paths.campaign(),
                &f.paths.campaign().with_extension("bak"),
                &f.paths.campaign().with_extension("tmp"),
                &f.paths.settings(),
            ] {
                if let Ok(target) = target.canonicalize() {
                    assert_ne!(
                        source, target,
                        "Copied inputs must differ from every review output generation"
                    );
                }
            }
        }
    }
    let bytes = std::fs::read(&canonical).expect("read copied NPC checkpoint");
    f.game = Game::decode_save_generation(&bytes)
        .expect("Copied NPC generation invalid; no fallback is permitted");
    assert!(
        f.game.attempt.as_ref().is_some_and(|a| a.mode == Mode::Npc),
        "Copied checkpoint must contain the active NPC battle"
    );
    f.settings = Settings::load(&copied_settings);
    if !launch.perf().is_some_and(|perf| perf.audio) {
        f.settings.master = 0.0;
    }
    if let Some(scale) = launch.review().and_then(|r| r.scale) {
        f.settings.scale = scale;
    }
    f.game.set_paused(false);
    f.campaign.mode = Mode::Npc;
    f.screen = Screen::Battle;
    f.battle.speed = launch.review().and_then(|r| r.perf.npc_speed).unwrap_or(1);
    assert!(
        matches!(f.battle.speed, 1 | 2 | 4),
        "NPC diagnostic speed must be1,2or4"
    );
    let a = f.game.attempt.as_ref().unwrap();
    f.battle.aim = [a.width_yards * 0.6, a.defender_position()[1]];
    f.status.message = "Copied checkpoint/resumed NPC battle · isolated performance review".into();
}

/// Staged near-defeated boss; real core settlement supplies the report/rewards.
fn prepare_journey_victory(f: &mut FrontendParts, liberated: bool) {
    let count = f
        .game
        .campaign
        .world
        .active_country()
        .expect("review country")
        .level_count();
    let stage = if liberated { count - 1 } else { 0 };
    for n in 0..stage {
        f.game
            .campaign
            .record_country_stage_clear(n)
            .expect("explicit earlier-country review milestones");
    }
    f.game
        .deploy_country_stage(stage, Mode::Personal)
        .expect("journey outcome fixture");
    let mut a = f.game.attempt.take().unwrap();
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game
        .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, true);
    let target = a.enemies.last_mut().unwrap();
    target.x = a.width_yards * 0.55;
    target.hp = 1;
    target.armor = 0;
    let aim = [target.x, target.y()];
    f.game.attempt = Some(a);
    f.game.set_paused(false);
    f.game.tick(Input {
        fire: true,
        aim,
        ..Default::default()
    });
    assert!(f.game.attempt.is_none());
    assert_eq!(
        f.game.last_result.as_ref().unwrap().outcome,
        Outcome::Victory
    );
    f.game.set_paused(true);
    f.campaign.mode = Mode::Personal;
    f.act(|c| c.focus_next_upgrade());
    f.screen = Screen::Results;
}

fn prepare_progression(game: &mut Game) {
    // Explicit screenshot entitlements; use model methods for every owned item,
    // skill and point investment. This does not represent earned campaign income.
    let campaign = &mut game.campaign;
    // These pre-existing advanced view fixtures explicitly retain legacy ledgers.
    campaign.legacy_features = true;
    campaign.coins = 1800;
    for stage in 0..5 {
        campaign.first_clear(stage, Mode::Personal);
    }
    campaign.finish_attempt();
    campaign.reward(0, 300, Mode::Personal);
    for attribute in [
        BodyAttribute::Vitality,
        BodyAttribute::Strength,
        BodyAttribute::Endurance,
    ] {
        campaign
            .allocate_body(attribute)
            .expect("fixture body investment");
    }
    for skill in [Skill::QuickReload, Skill::SecondWind, Skill::Engineering] {
        campaign.purchase_skill(skill).expect("fixture skill");
    }
    campaign
        .purchase_node("rifle_training")
        .expect("fixture firearm proficiency");
    campaign
        .purchase_node("grenade_training")
        .expect("fixture grenade handling");
    campaign
        .purchase_node("molotov_training")
        .expect("fixture incendiary handling");
    for kind in StructureKind::ALL {
        campaign
            .purchase_blueprint(kind)
            .expect("fixture blueprint");
    }
    for slot in Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .find(|(_, item)| item.slot == slot && item.tier == 1)
            .map(|(n, _)| n)
            .expect("starter-tier shop option");
        let id = campaign
            .buy_item(definition)
            .expect("fixture shop equipment");
        campaign
            .equip(Mode::Personal, id)
            .expect("fixture personal equipment");
        campaign
            .equip(Mode::Npc, id)
            .expect("fixture alternate equipment");
    }
    campaign.record_npc_progress(20, 7200);
    for _ in 0..10 {
        campaign
            .buy_training_point()
            .expect("fixture training service");
    }
    campaign.train_npc(NpcTraining::Guns).expect("fixture guns");
    campaign
        .train_npc(NpcTraining::Melee)
        .expect("fixture melee");
    let definition = content()
        .items
        .iter()
        .position(|item| item.slot == Slot::Ranged && item.tier == 1)
        .unwrap();
    let id = campaign
        .add_item(definition)
        .expect("fixture recovered spare");
    campaign.set_favorite(id, true).expect("fixture favorite");
    campaign
        .save_preset("Prepared patrol".into(), Mode::Personal)
        .expect("fixture preset");
}
fn prepare_history(f: &mut FrontendParts) {
    // Real model deployments/inputs/settlements, never authored plot points.
    for mode in [Mode::Personal, Mode::Npc, Mode::Personal] {
        f.game
            .deploy_country_stage(0, mode)
            .expect("history review deployment");
        f.game.set_paused(false);
        for _ in 0..60 * 18 {
            if f.game.attempt.is_none() {
                break;
            }
            let input = if mode == Mode::Personal {
                horde_core::sim::smoke_input(&f.game)
            } else {
                Input::default()
            };
            f.game.tick(input);
        }
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
    }
    f.nav.history_return = Screen::Camp;
    f.nav.history_world_paused = true;
    f.screen = Screen::History;
}
fn prepare_battle(f: &mut FrontendParts, screen: &str) {
    let mode = if screen == "npc" {
        Mode::Npc
    } else {
        Mode::Personal
    };
    f.game
        .deploy_country_stage(0, mode)
        .expect("review deployment");
    let lanes = f.game.attempt.as_ref().unwrap().lane_count;
    let center = lanes / 2;
    // A legal mixed starting plan in the actual frozen rows.
    for (kind, lane, column) in [
        (StructureKind::Wall, center.saturating_sub(1), 0),
        (StructureKind::Wall, (center + 1).min(lanes - 1), 0),
        (StructureKind::Turret, center, 1),
        (StructureKind::Trap, center, 2),
    ] {
        f.game
            .place(kind, lane, column)
            .expect("fixture affordable placement");
    }
    f.game.set_paused(false);
    let target = match screen {
        "progressed-battle" => 60 * 15,
        "results" => 60 * 480,
        "boss" => ((f.game.attempt.as_ref().unwrap().boss_time() + 12.0) * 60.0).ceil() as u32,
        _ => 60 * 38,
    };
    for _ in 0..target {
        if f.game.attempt.is_none() {
            break;
        }
        let input = if matches!(screen, "boss" | "results") {
            horde_core::sim::smoke_input(&f.game)
        } else {
            Input::default()
        };
        f.game.tick(input);
        if !matches!(screen, "boss" | "results")
            && f.game
                .attempt
                .as_ref()
                .is_some_and(|a| a.elapsed() > 6.0 && a.hp < a.max_hp * 2 / 3)
        {
            break;
        }
        if screen == "boss" && f.game.attempt.as_ref().is_some_and(|a| a.boss_spawned) {
            for _ in 0..60 * 8 {
                if f.game.attempt.is_some() {
                    f.game.tick(Input::default());
                }
            }
            break;
        }
    }
    if screen == "ordnance" && f.game.attempt.is_some() {
        let a = f.game.attempt.as_ref().unwrap();
        let molotov_cell = [(a.width_yards * 0.6).floor() as u8, a.lane_count / 2];
        let grenade_cell = [
            (a.width_yards * 0.75).floor() as u8,
            (a.lane_count / 2 + 1).min(a.lane_count - 1),
        ];
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: molotov_cell,
            }),
            ..Input::default()
        });
        for _ in 0..=horde_core::ordnance::catalog().molotov.flight_ticks {
            f.game.tick(Input::default());
        }
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: grenade_cell,
            }),
            ..Input::default()
        });
        f.battle.attack_selection = battle::AttackSelection::Molotov;
        f.battle.aim = [
            f.game.attempt.as_ref().unwrap().width_yards * 0.6,
            f.game.attempt.as_ref().unwrap().defender_position()[1],
        ];
    }
    if screen == "results" {
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
        f.screen = Screen::Results;
    } else {
        assert!(
            f.game.attempt.is_some(),
            "review battle should still be active"
        );
        f.screen = Screen::Battle;
    }
    f.campaign.mode = mode;
    f.battle.construction = screen == "construction";
    if let Some(enemy) = f.game.attempt.as_ref().and_then(|a| {
        a.enemies
            .iter()
            .find(|e| e.boss)
            .or_else(|| a.enemies.first())
    }) {
        f.battle.aim = [enemy.x, enemy.y()];
    }
}

fn prepare_gesture_battle(f: &mut FrontendParts, melee: bool) {
    prepare_battle(f, "battle");
    f.battle.fire_latched = false;
    f.battle.speed = 1;
    f.battle.pending = Input::default();
    // Remove automated damage using the actual legal salvage operation. The
    // single contact placement below is initial fixture staging, never an
    // invented action event, damage result, cooldown or later-frame teleport.
    let structures: Vec<_> = f
        .game
        .attempt
        .as_ref()
        .unwrap()
        .structures
        .iter()
        .map(|structure| structure.id)
        .collect();
    for id in structures {
        f.game
            .salvage(id)
            .expect("isolated gesture fixture salvage");
    }
    let attempt = f.game.attempt.as_mut().unwrap();
    if melee {
        let lane = attempt.lane_count / 2;
        let target = attempt
            .enemies
            .iter_mut()
            .find(|enemy| {
                !enemy.boss
                    && enemy.hp > 0
                    && enemy.kind != horde_core::content::EnemyKind::Demolisher
            })
            .expect("existing live non-explosive contact target");
        target.lane = lane;
        target.x = horde_core::sim::DEFENSE_LINE;
        target.pass_offset = 0.;
    }
    f.battle.aim = [
        attempt.width_yards * 0.6,
        f32::from(attempt.lane_count / 2) + 0.5,
    ];
    f.campaign.mode = Mode::Personal;
    f.screen = Screen::Battle;
}
fn prepare_operations(f: &mut FrontendParts) {
    f.nav.tab = Tab::Operations;
    for id in ["DEU", "FRA", "CHE", "AUT"] {
        f.game.campaign.world.cleared.push(id.into());
        f.game
            .campaign
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let first = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "DEU")
        .expect("review guard");
    let second = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "FRA")
        .expect("review expedition");
    f.game
        .strategy
        .assign_guard(&mut f.game.campaign, first, "DEU")
        .expect("review garrison");
    f.game
        .deploy_survivor(second, "POL", 0)
        .expect("review parallel battle");
    f.game.set_paused(false);
    for _ in 0..60 * 20 {
        f.game.tick(Input::default());
    }
    f.game
        .strategy
        .trigger_outbreak(&f.game.campaign, "DEU")
        .expect("review plague alert");
}

#[cfg(test)]
mod loot_capture_tests {
    use super::*;
    #[test]
    fn loot_replay_is_exclusive_to_explicit_isolated_capture_modes() {
        assert_eq!(
            select_loot_capture_mode(Some("quality-loot"), true, true, false, false),
            LootCaptureMode::Motion
        );
        assert_eq!(
            select_loot_capture_mode(Some("quality-loot"), false, false, true, false),
            LootCaptureMode::Still
        );
        for (scene, motion, sequence, still, practice) in [
            (None, true, true, false, false),
            (Some("battle"), true, true, false, false),
            (Some("quality-loot"), true, true, false, true),
            (Some("quality-loot"), true, false, false, false),
            (Some("quality-loot"), false, true, false, false),
            (Some("quality-loot"), false, false, false, false),
        ] {
            assert_eq!(
                select_loot_capture_mode(scene, motion, sequence, still, practice),
                LootCaptureMode::Disabled
            );
        }
    }
    fn prepared_frontend() -> FrontendParts {
        let mut f = crate::ui::test_support::test_frontend();
        f.game = Game::default();
        f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
        f.game.campaign.choose_start_country("DEU").unwrap();
        prepare_progression(&mut f.game);
        f
    }
    #[test]
    fn gesture_controls_require_the_complete_isolated_export_guard() {
        for scene in [Some("gesture-throws"), Some("gesture-melee")] {
            assert!(gesture_fixture(scene, true, true, false).is_some());
            for (motion, sequence, practice) in [
                (false, true, false),
                (true, false, false),
                (true, true, true),
                (false, false, false),
            ] {
                assert_eq!(gesture_fixture(scene, motion, sequence, practice), None);
            }
        }
        for scene in [None, Some("battle"), Some("ordnance"), Some("npc")] {
            assert_eq!(gesture_fixture(scene, true, true, false), None);
        }
    }
    #[test]
    fn scheduled_gesture_inputs_create_real_post_warmup_events_and_recover() {
        use horde_core::sim::EventKind;
        for kind in [GestureFixture::Throws, GestureFixture::Melee] {
            let mut f = prepared_frontend();
            prepare_gesture_battle(&mut f, kind == GestureFixture::Melee);
            f.game.set_paused(false);
            let start = f.game.attempt.as_ref().unwrap().tick;
            let mut observed_throws = std::collections::BTreeMap::new();
            let mut observed_melee = std::collections::BTreeMap::new();
            for frame in 1..=152 {
                let input = scheduled_gesture_input(
                    kind,
                    frame,
                    f.game
                        .attempt
                        .as_ref()
                        .expect("live isolated gesture battle"),
                )
                .unwrap_or_default();
                if frame <= 60 {
                    assert!(!input.melee && input.throw.is_none());
                }
                f.game.tick(input);
                // Combat VFX events legitimately expire. Observe real events
                // while live; never change production TTL to make a fixture pass.
                for event in &f.game.attempt.as_ref().expect("live gesture battle").events {
                    let key = (event.tick, event.actor_id, event.target_id);
                    match event.kind {
                        EventKind::Throw => {
                            observed_throws.entry(key).or_insert_with(|| event.clone());
                        }
                        EventKind::Melee => {
                            observed_melee.entry(key).or_insert_with(|| event.clone());
                        }
                        _ => {}
                    }
                }
            }
            let a = f
                .game
                .attempt
                .as_ref()
                .expect("gesture evidence keeps the real battle active");
            let throws: Vec<_> = observed_throws.values().collect();
            let melee: Vec<_> = observed_melee.values().collect();
            if kind == GestureFixture::Throws {
                assert_eq!(throws.len(), 2);
                assert!(
                    throws
                        .iter()
                        .all(|event| event.tick > start + 60 && event.actor_id.is_some())
                );
                assert_eq!(
                    a.ordnance.charges[0],
                    a.profile.modifiers.grenade_charges - 1
                );
                assert_eq!(
                    a.ordnance.charges[1],
                    a.profile.modifiers.molotov_charges - 1
                );
                assert!(a.tick - throws.last().unwrap().tick >= 24);
                assert!(melee.is_empty());
            } else {
                assert_eq!(melee.len(), 1);
                assert!(melee[0].tick > start + 60 && melee[0].target_id.is_some());
                assert!(melee[0].to[0] <= horde_core::sim::DEFENSE_LINE + 0.01);
                assert_eq!(a.melee_commit, 0);
                assert!(a.tick - melee[0].tick >= 30);
                assert!(throws.is_empty());
            }
        }
    }
    #[test]
    fn short_normal_review_fixtures_keep_live_battles_and_real_throw_cells_valid() {
        for scene in ["battle", "npc", "ordnance", "boss", "results"] {
            let mut f = prepared_frontend();
            let expected = f.game.campaign.arena_geometry();
            prepare_battle(&mut f, scene);
            if scene == "results" {
                assert!(f.game.last_result.is_some());
            } else {
                let a = f.game.attempt.as_ref().unwrap();
                assert_eq!((a.lane_count, a.width_yards), expected);
                for s in &a.structures {
                    assert!(s.lane < a.lane_count && s.x() < a.width_yards * 0.5);
                }
                if scene == "ordnance" {
                    assert!(!a.ordnance.projectiles.is_empty());
                    assert!(!a.ordnance.fire_zones.is_empty());
                    for p in &a.ordnance.projectiles {
                        assert!(horde_core::ordnance::valid_cell_for_arena(
                            p.cell,
                            a.lane_count,
                            a.width_yards
                        ));
                    }
                }
                if scene == "boss" {
                    assert!(a.enemies.iter().any(|e| e.boss));
                }
            }
        }
    }
    #[test]
    fn history_review_uses_real_settlements_and_damage_instead_of_authored_plot_points() {
        let mut f = prepared_frontend();
        prepare_history(&mut f);
        assert_eq!(f.game.campaign.run_history.len(), 3);
        assert!(
            f.game
                .campaign
                .run_history
                .iter()
                .any(|r| r.damage_by_source.total() > 0 && r.average_dps > 0.0)
        );
        assert!(f.screen == Screen::History);
        assert!(f.game.attempt.is_none());
    }
    #[test]
    fn new_journey_victory_and_liberation_fixtures_use_one_real_settlement_each() {
        for liberated in [false, true] {
            let mut f = crate::ui::test_support::test_frontend();
            f.game = Game::default();
            f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
            f.game.campaign.choose_start_country("DEU").unwrap();
            prepare_journey_victory(&mut f, liberated);
            assert_eq!(f.game.campaign.run_history.len(), 1);
            let coins = f.game.campaign.coins;
            let next = f.game.continue_journey().unwrap();
            assert_eq!(
                next,
                if liberated {
                    horde_core::sim::JourneyContinuation::CountryLiberated
                } else {
                    horde_core::sim::JourneyContinuation::Deployed
                }
            );
            assert_eq!(f.game.campaign.coins, coins);
            assert_eq!(f.game.campaign.run_history.len(), 1);
        }
    }
}
