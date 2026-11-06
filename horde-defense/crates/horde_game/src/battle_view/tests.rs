use super::{
    interpolation::{MAX_TICKS_PER_STEP, blend_fraction},
    readouts::{LOG_TAIL, WeaponView},
    *,
};
use crate::launch::ReviewOptions;
use crate::{animation::SceneAnimation, launch::LaunchConfig, visual_assets::enemy_visuals};
use bevy::prelude::*;
use horde_core::{
    content::{BossKind, Mode},
    enemy_catalog::VisualSprite,
    sim::{Game, Input},
};
use horde_core::{
    content::{EnemyKind, StructureKind, content},
    kind_map::KindMap,
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, EnemyCondition},
    world::world,
};
use std::collections::BTreeMap;

fn fixture() -> Game {
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut().attempts = 1;
    game.deploy(0, Mode::Personal).unwrap();
    let mut a = game.attempt_slot_mut().take().unwrap();
    game.spawn_enemy(&mut a, EnemyKind::Runner, 4, false);
    game.spawn_enemy(&mut a, EnemyKind::Brute, 1, false);
    game.spawn_enemy(&mut a, EnemyKind::Shambler, 2, true);
    for (n, e) in a.enemies.iter_mut().enumerate() {
        e.x = 18. + n as f32;
    }
    a.paused = false;
    *game.attempt_slot_mut() = Some(a);
    game
}
fn view(a: &Attempt, blend: SceneBlend) -> BattleViewModel {
    BattleViewModel::build(a, &SceneAnimation::sample(a, false), blend, false)
}
fn drawn_at(a: &Attempt, blend: SceneBlend, id: u64) -> [f32; 2] {
    view(a, blend)
        .enemies
        .into_iter()
        .find(|e| e.id == id)
        .unwrap()
        .position
}

#[test]
fn view_model_mirrors_the_attempt_headlessly_without_mutating_it() {
    let mut game = fixture();
    for _ in 0..30 {
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    let before = serde_json::to_string(a).unwrap();
    let view = view(a, SceneBlend::NONE);
    assert_eq!(before, serde_json::to_string(a).unwrap());
    assert_eq!(view.tick, a.tick);
    assert_eq!(view.lanes, a.lane_count);
    let visible = a
        .enemies
        .iter()
        .filter(|e| e.condition(a.mode) != EnemyCondition::Defeated && e.in_view())
        .count();
    assert_eq!(view.enemies.len(), visible);
    assert!(!view.enemies.is_empty());
    for pair in view.enemies.windows(2) {
        assert!(pair[0].position[1] <= pair[1].position[1]);
    }
    for actor in &view.enemies {
        let e = a.enemies.iter().find(|e| e.id == actor.id).unwrap();
        assert_eq!(actor.position, [e.x, e.y()]);
        assert_eq!(actor.kind, e.kind);
        let sprite = actor.sprite.expect("every catalogue sprite has art");
        let expected = e.resolved.as_ref().map_or_else(
            || {
                let boss = e
                    .boss
                    .then(|| e.boss_kind.unwrap_or(content().levels[a.level].boss));
                enemy_visuals::legacy_sprite(e.kind, boss)
            },
            |profile| profile.visual.sprite,
        );
        assert_eq!(sprite.sprite, expected);
        if e.boss {
            assert_eq!(actor.height.factor, enemy_visuals::active().boss_height);
            assert!(actor.label.is_some());
            assert!(matches!(actor.health, HealthView::Bar { boss: true, .. }));
        } else {
            assert_eq!(actor.height.factor, sprite.height);
            assert!(actor.label.is_none());
        }
    }
    assert_eq!(view.structures.len(), a.structures.len());
    assert_eq!(view.fx.len(), a.events.len());
    assert_eq!(view.defender.position, a.defender_position());
    assert_eq!(view.fire_zones.len(), a.ordnance.fire_zones.len());
    assert_eq!(view.projectiles.len(), a.ordnance.projectiles.len());
}

#[test]
fn legacy_enemies_without_a_profile_draw_their_kind_or_level_boss() {
    let mut game = fixture();
    let a = game.attempt_mut().unwrap();
    for e in &mut a.enemies {
        e.resolved = None;
        e.boss_kind = None;
    }
    let view = view(a, SceneBlend::NONE);
    let level_boss = content().levels[a.level].boss;
    assert_eq!(view.enemies.len(), 3);
    for actor in &view.enemies {
        let boss = actor.boss.then_some(level_boss);
        assert_eq!(
            actor.sprite.unwrap().sprite,
            enemy_visuals::legacy_sprite(actor.kind, boss)
        );
        assert_eq!(actor.tint, None);
        assert_eq!(actor.height.scale, None);
    }
    let brute = view
        .enemies
        .iter()
        .find(|e| e.kind == EnemyKind::Brute)
        .unwrap();
    assert_eq!(brute.height.pixels(10.), 10. * 0.9);
    assert_eq!(
        enemy_visuals::legacy_sprite(EnemyKind::Runner, Some(BossKind::Caller)),
        VisualSprite::Caller
    );
}

#[test]
fn actor_positions_blend_between_fixed_ticks_by_the_overstep() {
    let mut game = fixture();
    let mut cache = ActorInterpolation::default();
    cache.record([(None, game.attempt().unwrap())]);
    let before: BTreeMap<u64, [f32; 2]> = game
        .attempt()
        .unwrap()
        .enemies
        .iter()
        .map(|e| (e.id, [e.x, e.y()]))
        .collect();
    for _ in 0..2 {
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    let moved = a
        .enemies
        .iter()
        .find(|e| before[&e.id] != [e.x, e.y()])
        .expect("an enemy advanced");
    let (id, from, to) = (moved.id, before[&moved.id], [moved.x, moved.y()]);
    cache.fraction = 0.0;
    assert_eq!(drawn_at(a, cache.scene(None, a), id), from);
    cache.fraction = 0.5;
    let half = drawn_at(a, cache.scene(None, a), id);
    for axis in 0..2 {
        assert!((half[axis] - (from[axis] + to[axis]) * 0.5).abs() < 1e-4);
    }
    // Fraction 1 is the authoritative position exactly, as is a scene the
    // snapshot does not describe.
    cache.fraction = 1.0;
    assert_eq!(drawn_at(a, cache.scene(None, a), id), to);
    cache.fraction = 0.5;
    assert_eq!(drawn_at(a, cache.scene(Some(7), a), id), to);
    // A stale snapshot (more than one fixed step of ticks) is not blended.
    for _ in 0..MAX_TICKS_PER_STEP {
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    let current = a.enemies.iter().find(|e| e.id == id).unwrap();
    assert_eq!(
        drawn_at(a, cache.scene(None, a), id),
        [current.x, current.y()]
    );
}

#[test]
fn a_full_four_tick_fixed_step_still_blends_from_its_snapshot() {
    let mut game = fixture();
    let mut cache = ActorInterpolation::default();
    cache.record([(None, game.attempt().unwrap())]);
    let before: BTreeMap<u64, [f32; 2]> = game
        .attempt()
        .unwrap()
        .enemies
        .iter()
        .map(|e| (e.id, [e.x, e.y()]))
        .collect();
    for _ in 0..MAX_TICKS_PER_STEP {
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    let moved = a
        .enemies
        .iter()
        .find(|e| before[&e.id] != [e.x, e.y()])
        .expect("an enemy advanced");
    cache.fraction = 0.0;
    assert_eq!(
        drawn_at(a, cache.scene(None, a), moved.id),
        before[&moved.id]
    );
}

#[test]
fn battle_screen_is_absent_outside_battle_and_geometry_falls_back_to_the_campaign() {
    let game = Game::default();
    assert_eq!(BattleScreen::build(&game), None);
    assert_eq!(
        placement_preview(&game, StructureKind::ALL[0], [3.0, 0.5]),
        None
    );
    assert_eq!(
        arena_geometry(&game),
        (
            game.campaign().lane_count(),
            game.campaign().arena_geometry().1
        )
    );
    let game = fixture();
    let a = game.attempt().unwrap();
    assert_eq!(arena_geometry(&game), (a.lane_count, a.width_yards));
}

#[test]
fn battle_screen_mirrors_the_attempt_headlessly_without_mutating_it() {
    let mut game = fixture();
    for _ in 0..30 {
        game.tick(Input::default());
    }
    let a = game.attempt_mut().unwrap();
    a.log = (0..25).map(|n| format!("line {n}")).collect();
    let a = game.attempt().unwrap();
    let before = serde_json::to_string(a).unwrap();
    let view = BattleScreen::build(&game).unwrap();
    assert_eq!(before, serde_json::to_string(a).unwrap());
    assert_eq!(
        (
            view.mode,
            view.tick,
            view.paused,
            view.lanes,
            view.width_yards
        ),
        (a.mode, a.tick, a.paused, a.lane_count, a.width_yards)
    );
    assert!(view.personal());
    assert_eq!(view.last_loot.as_ref(), a.loot_events.last());
    assert_eq!(view.weapon_class, a.profile.weapon.class.name());
    assert_eq!(
        view.weapon,
        WeaponView {
            ammo: a.ammo,
            magazine: a.magazine,
            reload_left: a.reload_left,
            reload_total: a.reload_duration_ticks(),
        }
    );
    for kind in [OrdnanceKind::Grenade, OrdnanceKind::Molotov] {
        let slot = view.slot(kind);
        assert_eq!(slot.charges, a.ordnance.charges(kind));
        assert_eq!(slot.issued, a.ordnance.issued(kind));
        assert_eq!(slot.cooldown, a.ordnance.cooldown(kind));
        assert_eq!(slot.ready, a.ordnance.ready(kind));
        assert_eq!(
            slot.radius,
            ordnance::catalog().definition(kind).radius
                * a.profile.modifiers.ordnance_radius_multiplier
        );
        assert!(slot.cooldown_total >= 1.0);
    }
    assert_eq!((view.heal, view.brace), (a.profile.heal, a.profile.brace));
    assert_eq!(
        (view.melee_cooldown, view.supply),
        (a.melee_cooldown, a.supply)
    );
    assert_eq!(view.field.threat_rank, a.progression.rank);
    assert_eq!(view.field.hostiles, a.enemies.len());
    assert_eq!(view.field.enemy_cap, a.enemy_cap());
    assert_eq!(view.field.arrival_rate, a.arrival_rate());
    assert_eq!(view.field.difficulty, a.balance.preset.name());
    assert_eq!(view.field.batch_size, a.progression.batch_size);
    assert_eq!(view.field.gear_level, a.progression.item_level);
    assert_eq!(view.field.level_name, content().levels[a.level].name);
    assert_eq!(view.log_tail.len(), LOG_TAIL);
    assert_eq!(view.log_tail[0], "line 24");
    assert_eq!(view.log_tail[LOG_TAIL - 1], "line 7");
    assert_eq!(view.targets.len(), a.enemies.len());
    assert_eq!(view.blueprints, game.campaign().blueprints);
    assert_eq!(
        view.any_blueprint(),
        game.campaign().blueprints.iter().any(|b| *b)
    );
    assert_eq!(view.global_paused, game.global_paused());
    assert!(!view.first_deployment);
}

#[test]
fn country_stages_carry_their_biome_theme_and_local_setting() {
    let mut game = Game::default();
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = game.attempt().unwrap();
    let country = world().country(a.country_id.as_deref().unwrap()).unwrap();
    let view = BattleScreen::build(&game).unwrap();
    assert_eq!(view.field.setting, Some(country.stage_theme(a.stage)));
    assert!(view.theme <= 4);
    // Without a country the backdrop follows the level and no setting shows.
    let mut legacy = fixture();
    legacy.attempt_mut().unwrap().country_id = None;
    let view = BattleScreen::build(&legacy).unwrap();
    assert_eq!(view.theme, legacy.attempt().unwrap().level);
    assert_eq!(view.field.setting, None);
}

#[test]
fn reticle_uses_exact_ammo_and_frozen_reload_ticks_without_a_presentation_clock() {
    let mut game = Game::default();
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = game.attempt_mut().unwrap();
    a.ammo = 3;
    a.reload_left = 63;
    a.profile.personal.reload = 2.1;
    let state = ReticleReadout::build(a, None);
    assert_eq!((state.remaining, state.capacity), (3, a.magazine));
    assert_eq!(state.progress, Some(0.5));
    assert_eq!(state.label, "R 1.0s");
    assert!(!state.observed);
    assert_eq!(ReticleReadout::build(a, None), state);
    assert_eq!(BattleScreen::build(&game).unwrap().reticle(None), state);
    let a = game.attempt_mut().unwrap();
    a.mode = Mode::Npc;
    a.profile.npc.reload_turns = 3;
    a.reload_left = 90;
    let observed = ReticleReadout::build(a, Some(OrdnanceKind::Grenade));
    assert_eq!(observed.progress, Some(0.5));
    // Observed battles never show a throwable readout.
    assert_eq!(observed.label, "R 1.5s");
    assert!(observed.observed);
}

#[test]
fn selected_throwables_report_charges_and_cooldown_on_the_reticle() {
    let mut game = fixture();
    let a = game.attempt_mut().unwrap();
    a.reload_left = 0;
    let kind = OrdnanceKind::Grenade;
    let total = OrdnanceSlotView::of(a, kind).cooldown_total;
    let charges = a.ordnance.charges(kind);
    let issued = a.ordnance.issued(kind);
    let view = BattleScreen::build(&game).unwrap();
    let readout = view.reticle(Some(kind));
    assert_eq!(
        readout,
        ReticleReadout::build(game.attempt().unwrap(), Some(kind))
    );
    assert_eq!(
        (readout.remaining, readout.capacity),
        (charges, issued.max(1))
    );
    assert_eq!(readout.label, format!("{charges} left"));
    assert_eq!(readout.progress, None);
    let mut cooling = view.clone();
    cooling.ordnance = KindMap::from_fn(|k| OrdnanceSlotView {
        cooldown: (total / 2.0) as u32,
        ..*view.slot(k)
    });
    let progress = cooling.reticle(Some(kind)).progress.unwrap();
    assert!((progress - (1.0 - (total / 2.0).floor() / total)).abs() < 1e-6);
    let mut reloading = view;
    reloading.weapon.reload_left = 1;
    assert_eq!(reloading.reticle(Some(kind)).label, "RELOAD");
}

#[test]
fn aim_assist_snaps_to_the_nearest_enemy_inside_the_radius_only() {
    let mut view = BattleScreen::build(&fixture()).unwrap();
    view.targets = vec![[5.0, 1.5], [4.0, 1.5], [4.0, 1.5], [20.0, 0.5]];
    assert_eq!(view.assist([3.5, 1.5], 0.0), [3.5, 1.5]);
    assert_eq!(view.assist([3.5, 1.5], 2.0), [4.0, 1.5]);
    assert_eq!(view.assist([3.5, 1.5], 0.4), [3.5, 1.5]);
    assert_eq!(view.assist([19.0, 0.5], 1.5), [20.0, 0.5]);
    view.lanes = 3;
    assert_eq!(view.selected_lane([0.0, -2.0]), 1);
    assert_eq!(view.selected_lane([0.0, 1.7]), 2);
    assert_eq!(view.selected_lane([0.0, 9.0]), 3);
}

#[test]
fn first_deployment_hints_follow_the_attempt_milestones() {
    let mut game = Game::default();
    game.deploy(0, Mode::Personal).unwrap();
    let hint = |game: &Game| BattleScreen::build(game).unwrap().field.hint;
    assert!(BattleScreen::build(&game).unwrap().first_deployment);
    assert_eq!(hint(&game), FieldHint::Opening);
    let a = game.attempt_mut().unwrap();
    a.tick = 10;
    a.shots = 0;
    assert_eq!(hint(&game), FieldHint::Shoot);
    let a = game.attempt_mut().unwrap();
    a.shots = 2;
    a.reload_ticks = 0;
    a.ammo = a.magazine.saturating_sub(1);
    assert_eq!(hint(&game), FieldHint::Reload);
    let a = game.attempt_mut().unwrap();
    a.ammo = a.magazine;
    a.first_contact = None;
    assert_eq!(hint(&game), FieldHint::Contact);
    game.attempt_mut().unwrap().first_contact = Some(3.0);
    assert_eq!(hint(&game), FieldHint::Banked);
}

#[test]
fn placement_preview_reports_the_aimed_cell_validity_and_occupant() {
    let game = fixture();
    let a = game.attempt().unwrap();
    let kind = StructureKind::ALL[0];
    assert_eq!(placement_preview(&game, kind, [-1.0, 0.5]), None);
    let center = a.build_cell_center(1, 2).unwrap();
    let preview = placement_preview(&game, kind, center).unwrap();
    assert_eq!((preview.lane, preview.column), (1, 2));
    assert_eq!(preview.center, center);
    assert_eq!(preview.half_width, a.construction_cell_width() * 0.5);
    assert_eq!(preview.valid, game.preview_placement(kind, 1, 2));
    assert_eq!(
        preview.occupant,
        a.structures
            .iter()
            .find(|s| s.lane == 1 && s.column == 2)
            .map(|s| s.id)
    );
}

#[test]
fn capture_modes_draw_the_authoritative_tick_exactly() {
    let play = LaunchConfig::default();
    assert_eq!(blend_fraction(&play, 0.25), 0.25);
    assert_eq!(blend_fraction(&play, 1.7), 1.0);
    assert_eq!(blend_fraction(&play, f32::NAN), 1.0);
    for review in [
        ReviewOptions {
            sequence: Some("target/sequence".into()),
            ..Default::default()
        },
        ReviewOptions {
            capture: Some("target/shot.png".into()),
            ..Default::default()
        },
    ] {
        let config = LaunchConfig {
            review: Some(review),
            ..Default::default()
        };
        for overstep in [0.0, 0.3, 0.99] {
            assert_eq!(blend_fraction(&config, overstep), 1.0);
        }
    }
}
