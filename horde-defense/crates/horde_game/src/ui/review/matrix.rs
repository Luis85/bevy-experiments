//! The review scenario matrix: maps a `--scenario` name to its staged screen.
use super::{fixtures::*, gesture_fixture};
use crate::ui::{FrontendParts, Screen, Tab, armory};
use horde_core::{
    content::{Mode, Slot, content},
    sim::{Game, Input},
    survivors::SurvivorId,
};

pub(in crate::ui) fn prepare(f: &mut FrontendParts) {
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
    if launch.practice {
        f.status.message = "Practice scenario - progress is isolated from your campaign".into();
    }
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
        .staging()
        .campaign()
        .choose_survivor(identity)
        .expect("review survivor identity");
    f.campaign.preview_survivor = identity;
    if screen == "menu-female" {
        f.screen = Screen::Menu;
        return;
    }
    // The unsaveable 400-enemy perf fixture (launch refuses it as practice):
    // it never submits a save, so no autosave can fail on it.
    #[cfg(feature = "review")]
    if screen == "stress" {
        f.game = horde_core::sim::stress_render_fixture().expect("stress render fixture");
        f.runtime.forbid_saves();
        f.screen = Screen::Battle;
        f.campaign.mode = Mode::Npc;
        f.campaign.selected_country = f
            .game
            .attempt()
            .and_then(|a| a.country_id.clone())
            .expect("stress country stage");
        f.status.message = "Synthetic stress fixture · 400 drawn + 3 expeditions · perf headroom, not a gameplay claim".into();
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
        .staging()
        .campaign()
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
            if f.game.attempt().is_none() {
                break;
            }
            let input = horde_core::sim::smoke_input(&f.game);
            f.game.tick(input);
        }
        if f.game.attempt().is_some() {
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
        // Before the clears: their levels grant the character points that
        // prepare_progression invests (without the flag the third allocation fails).
        enable_legacy_ledgers(&mut f.game);
        let count = f
            .game
            .campaign()
            .world
            .active_country()
            .expect("review country")
            .level_count();
        // Explicit completed-run entitlements for a staged view, not an earned
        // balance proof. Repeated templates still count as victories.
        for stage in 0..count {
            f.game
                .staging()
                .campaign()
                .first_clear(stage % content().levels.len(), Mode::Personal);
            f.game.staging().campaign().finish_attempt();
            f.game
                .staging()
                .campaign()
                .record_country_stage_clear(stage)
                .expect("explicit review progression entitlement");
        }
        if screen == "progressed-battle" {
            f.game
                .staging()
                .campaign()
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
            f.game.staging().campaign().player.level =
                f.game.staging().campaign().player_level_cap();
            f.game.staging().campaign().player.xp = 0;
            f.game.staging().campaign().player.points = f.game.staging().campaign().player.level
                - 1
                - f.game.staging().campaign().player.body.iter().sum::<u32>();
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
                            .staging()
                            .campaign()
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
                .campaign()
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
        "skills-search" => {
            // A search with a goal path: "reload" matches, and the focused
            // depth-3 node draws its unowned chain and "N steps · M coins".
            f.screen = Screen::Results;
            f.campaign.tree_search_seed = Some("reload".into());
            f.campaign.improvement_focus = Some(
                horde_core::campaign::RecommendationAction::LearnNode("combat_reload".into()),
            );
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
                let half = f.game.attempt().unwrap().magazine / 2;
                for _ in 0..240 {
                    let a = f.game.attempt().expect("live reload review");
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
                    .attempt()
                    .unwrap()
                    .enemies
                    .first()
                    .map(|e| [e.x, e.y()])
                    .unwrap_or([
                        f.game.attempt().unwrap().width_yards * 0.6,
                        f.game.attempt().unwrap().defender_position()[1],
                    ]);
                f.game.tick(Input {
                    aim: target,
                    fire: true,
                    ..Default::default()
                });
            }
            f.battle.aim = [
                f.game.attempt().unwrap().width_yards * 0.6,
                f.game.attempt().unwrap().defender_position()[1],
            ];
        }
        "damage-4x" => {
            prepare_battle(f, "npc");
            f.battle.speed = 4;
            f.battle.aim = [
                f.game.attempt().unwrap().width_yards * 0.6,
                f.game.attempt().unwrap().defender_position()[1],
            ];
        }
        "npc-expanded" => prepare_battle(f, "npc"),
        "critical-npc" => {
            prepare_battle(f, "npc");
            let mut a = f
                .game
                .staging()
                .attempt_slot()
                .take()
                .expect("live NPC review");
            let lane = a.lane_count / 2;
            f.game
                .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, false);
            let enemy = a.enemies.last_mut().unwrap();
            enemy.x = 9.0;
            enemy.hp = -2;
            enemy.injury.hp = -2;
            *f.game.staging().attempt_slot() = Some(a);
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
                .staging()
                .campaign()
                .add_item_with_quality(definition, 3, Some(quality))
                .expect("explicit isolated loot fixture");
            let a = f.game.staging().attempt().unwrap();
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
        // Launch validation admits only registered names, so only the
        // workshop scenes reach this arm; workshop.rs draws them.
        _ => {
            debug_assert!(
                crate::review_scenes::is_workshop(&screen),
                "registry admitted {screen}"
            );
            f.nav.tab = Tab::World;
        }
    }
    f.game.set_paused(true);
}
