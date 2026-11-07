use super::{
    builds,
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
    BattleViewModel::build(a, SceneAnimation::sample(a, false), blend, false)
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
    assert_eq!(
        view.fx.len(),
        a.events
            .iter()
            .filter(|e| (a.tick - e.tick) as f32 / 60.0 <= crate::art::fx_lifetime(e.kind))
            .count()
    );
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
    for kind in StructureKind::ALL {
        assert_eq!(view.blueprint(kind), game.campaign().blueprints[kind]);
    }
    assert_eq!(
        view.any_blueprint,
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

/// Pin: the allocation-free biome match gives the theme the former
/// `to_lowercase` match gave, for every world biome and other cases.
#[test]
fn biome_themes_ignore_case_like_the_lowercase_match() {
    let lowercase = |biome: &str| match biome.to_lowercase().as_str() {
        "desert" | "arid" => 2,
        "tundra" | "polar" | "boreal" => 3,
        "tropical" | "rainforest" => 4,
        "mediterranean" => 1,
        _ => 0,
    };
    let world_biomes = world().countries.iter().map(|c| c.biome.as_str());
    let others = [
        "DESERT",
        "Tundra",
        "polar",
        "RainForest",
        "Mediterranean",
        "Alpine",
        "Temperate",
        "",
        "arid ",
    ];
    for biome in world_biomes.chain(others) {
        assert_eq!(
            super::screen::biome_theme(biome),
            lowercase(biome),
            "{biome}"
        );
    }
    assert_eq!(super::screen::biome_theme("Arid"), 2);
    assert_eq!(super::screen::biome_theme("Tropical"), 4);
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
#[test]
fn fx_view_keeps_only_events_within_painter_lifetime() {
    use horde_core::sim::{Event, EventKind};
    let mut game = fixture();
    let a = game.attempt_mut().unwrap();
    a.tick = 600;
    a.events.clear();
    let kinds = [
        EventKind::Shot,
        EventKind::Hit,
        EventKind::Kill,
        EventKind::Explosion,
        EventKind::BossWarning,
        EventKind::Spawn,
    ];
    for kind in kinds {
        // Fresh, exactly at the lifetime (kept) and one tick past it (dropped).
        let lifetime_ticks = (crate::art::fx_lifetime(kind) * 60.0).round() as u64;
        for age in [0, lifetime_ticks, lifetime_ticks + 1] {
            a.events.push_back(Event {
                kind,
                from: [2.0, 1.5],
                to: [10.0, 1.5],
                value: age as i32,
                tick: a.tick - age,
                actor_id: None,
                target_id: None,
                lane: None,
                duration_ticks: 0,
                seq: 0,
            });
        }
    }
    let view = view(a, SceneBlend::NONE);
    assert_eq!(view.fx.len(), 2 * kinds.len());
    for kind in kinds {
        let ages: Vec<_> = view
            .fx
            .iter()
            .filter(|fx| fx.kind == kind)
            .map(|fx| fx.age)
            .collect();
        assert_eq!(ages.len(), 2, "{kind:?}");
        assert!(ages.iter().all(|age| *age <= crate::art::fx_lifetime(kind)));
    }
    // The boss warning outlives the 1.5 s ux-flow banner.
    assert!(crate::art::fx_lifetime(EventKind::BossWarning) >= 1.5);
    assert_eq!(crate::art::fx_lifetime(EventKind::Explosion), 0.5);
}

/// The progressed-peak fixture after `ticks` unpaused ticks, with the
/// interpolation snapshot of the tick before and the given blend fraction.
fn live_peak(ticks: usize, fraction: f32) -> (Game, ActorInterpolation) {
    let mut game = horde_core::sim::maximum_progression_render_fixture().expect("fixture");
    game.set_paused(false);
    game.attempt_mut().unwrap().paused = false;
    let mut blend = ActorInterpolation::default();
    for _ in 0..ticks {
        blend.record([(None, game.attempt().unwrap())]);
        game.tick(Input::default());
    }
    blend.fraction = fraction;
    (game, blend)
}

/// The field view reads each enemy's and structure's pose, and each enemy's
/// previous position, from stores in attempt order, without one search by id
/// per actor (W1-67): poses and positions line up with `a.enemies`.
#[test]
fn field_view_reads_poses_and_blended_positions_without_id_lookups() {
    let (game, blend) = live_peak(120, 0.5);
    let a = game.attempt().unwrap();
    assert!(a.enemies.len() > 100, "a crowded progressed peak");
    assert!(!a.structures.is_empty());
    // Both in-place paths run: bodies are collapsing (each reads its saved
    // record through `death_record`) and the blend moves enemies between
    // the snapshot and the current tick.
    let motion = SceneAnimation::sample(a, false);
    let collapsing = motion.deaths.len();
    assert!(collapsing > 0, "the scene has collapsing bodies");
    let scene = blend.scene(None, a);
    let blended = scene.positions(&a.enemies);
    assert!(
        a.enemies
            .iter()
            .zip(&blended)
            .any(|(e, at)| *at != [e.x, e.y()]),
        "the blend is not the identity"
    );
    builds::take_id_lookups();
    let view = BattleViewModel::build(a, motion, scene, false);
    assert!(view.enemies.len() > 100);
    assert_eq!(view.corpses.len(), collapsing);
    assert!(
        view.corpses.iter().all(|body| body.tint.is_some()),
        "every collapsing body found its saved record"
    );
    assert_eq!(builds::take_id_lookups(), 0);
}

/// A pose store or corpse list that no longer lines up with the attempt (a
/// scene sampled before an enemy and a body moved) falls back to ids: the
/// view equals the one built from a scene sampled from that attempt.
#[test]
fn misaligned_stores_fall_back_to_the_ids() {
    let (mut game, _) = live_peak(60, 1.0);
    let a = game.attempt_mut().unwrap();
    let template = a.enemies[0].clone();
    a.corpses.push(horde_core::sim::CorpseRecord {
        id: u64::MAX,
        kind: template.kind,
        boss_kind: None,
        at: [template.x, template.y()],
        visual: template.resolved.as_ref().map(|r| r.visual.clone()),
        tick: a.tick,
    });
    let a = game.attempt().unwrap();
    let motion = SceneAnimation::sample(a, false);
    let mut moved = a.clone();
    moved.enemies.remove(0);
    let mut settled_body = moved.corpses[0].clone();
    settled_body.id = u64::MAX - 1;
    settled_body.tick = 0;
    moved.corpses.insert(0, settled_body);
    builds::take_id_lookups();
    let view = BattleViewModel::build(&moved, motion, SceneBlend::NONE, false);
    assert!(builds::take_id_lookups() > 0, "the stores were searched");
    let expected = BattleViewModel::build(
        &moved,
        SceneAnimation::sample(&moved, false),
        SceneBlend::NONE,
        false,
    );
    assert!(!view.corpses.is_empty());
    assert!(view.corpses.iter().all(|body| body.tint.is_some()));
    assert_eq!(view.corpses, expected.corpses);
    assert_eq!(view.enemies, expected.enemies);
}

/// Each enemy blends from its own previous position, by id, across
/// removals, spawns and a reordered list; an enemy the snapshot lacks draws
/// where it is.
#[test]
fn blended_positions_follow_ids_across_deaths_spawns_and_reorders() {
    let (game, _) = live_peak(30, 1.0);
    let before = game.attempt().unwrap().clone();
    let mut blend = ActorInterpolation::default();
    blend.record([(None, &before)]);
    let previous: BTreeMap<u64, [f32; 2]> = before
        .enemies
        .iter()
        .map(|e| (e.id, [e.x, e.y()]))
        .collect();
    let mut now = before.clone();
    now.tick += 1;
    for e in &mut now.enemies {
        e.x -= 0.5;
    }
    now.enemies.remove(10);
    now.enemies.remove(3);
    now.enemies.swap(0, 5);
    let mut spawned = now.enemies[1].clone();
    spawned.id = previous.keys().max().unwrap() + 1;
    now.enemies.push(spawned);
    let mut unknown = now.enemies[2].clone();
    unknown.id = 0;
    assert!(!previous.contains_key(&0));
    now.enemies.insert(7, unknown);
    for fraction in [0.0, 0.25, 0.5, 1.0] {
        blend.fraction = fraction;
        let drawn = blend.scene(None, &now).positions(&now.enemies);
        assert_eq!(drawn.len(), now.enemies.len());
        for (e, at) in now.enemies.iter().zip(drawn) {
            let current = [e.x, e.y()];
            let expected = match previous.get(&e.id) {
                Some(p) if fraction < 1.0 => [
                    p[0] + (current[0] - p[0]) * fraction,
                    p[1] + (current[1] - p[1]) * fraction,
                ],
                _ => current,
            };
            assert_eq!(at, expected, "enemy {} at fraction {fraction}", e.id);
        }
    }
}

/// Characterisation pin (W1-67): on the progressed-peak and stress fixtures,
/// live, the view read from the stores in attempt order equals the by-id
/// reference: poses from a reversed (so searched) store and positions
/// blended from a by-id map of the previous tick, normal and reduced.
#[test]
fn field_views_equal_the_by_id_reference_on_peak_and_stress() {
    let fixtures: [fn() -> Result<Game, String>; 2] = [
        horde_core::sim::maximum_progression_render_fixture,
        horde_core::sim::stress_render_fixture,
    ];
    for (scene, fixture) in ["progressed-peak", "stress"].into_iter().zip(fixtures) {
        let mut game = fixture().expect("fixture");
        game.set_paused(false);
        game.attempt_mut().unwrap().paused = false;
        let mut blend = ActorInterpolation::default();
        let mut views = 0;
        for tick in 0..240 {
            let a = game.attempt().unwrap();
            let previous: BTreeMap<u64, [f32; 2]> =
                a.enemies.iter().map(|e| (e.id, [e.x, e.y()])).collect();
            blend.record([(None, a)]);
            game.tick(Input::default());
            let Some(a) = game.attempt() else { break };
            if tick % 8 != 0 {
                continue;
            }
            for (fraction, reduced) in [(0.5, false), (0.25, true)] {
                blend.fraction = fraction;
                let view = BattleViewModel::build(
                    a,
                    SceneAnimation::sample(a, reduced),
                    blend.scene(None, a),
                    reduced,
                );
                let mut searched = SceneAnimation::sample(a, reduced);
                searched.enemies.reverse();
                searched.structures.reverse();
                let reference = BattleViewModel::build(a, searched, blend.scene(None, a), reduced);
                assert_eq!(view, reference, "{scene} tick {tick}");
                for actor in &view.enemies {
                    let e = a.enemies.iter().find(|e| e.id == actor.id).unwrap();
                    let expected = previous.get(&e.id).map_or([e.x, e.y()], |p| {
                        [
                            p[0] + (e.x - p[0]) * fraction,
                            p[1] + (e.y() - p[1]) * fraction,
                        ]
                    });
                    assert_eq!(
                        actor.position, expected,
                        "{scene} tick {tick} enemy {}",
                        e.id
                    );
                }
                views += 1;
            }
        }
        assert_eq!(views, 60, "{scene} sampled every eighth tick");
    }
}

/// Six hostiles stacked on the defense line of lane 3 and one behind them,
/// as enemies overlap since W1-69 (user decision, 2026-11-06).
fn stacked_line() -> Game {
    let mut game = fixture();
    let mut a = game.attempt_slot_mut().take().unwrap();
    a.enemies.clear();
    for (n, kind) in [
        EnemyKind::Brute,
        EnemyKind::Runner,
        EnemyKind::Shambler,
        EnemyKind::Armored,
        EnemyKind::Demolisher,
        EnemyKind::Runner,
        EnemyKind::Shambler,
    ]
    .into_iter()
    .enumerate()
    {
        game.spawn_enemy(&mut a, kind, 3, false);
        a.enemies.last_mut().unwrap().x = if n < 6 {
            horde_core::sim::DEFENSE_LINE
        } else {
            4.0
        };
    }
    *game.attempt_slot_mut() = Some(a);
    game
}

#[test]
fn a_lane_paints_far_to_near_then_by_drawn_feet() {
    use super::field::crowd_stagger;
    let game = stacked_line();
    let a = game.attempt().unwrap();
    let view = view(a, SceneBlend::NONE);
    let ids: Vec<u64> = view.enemies.iter().map(|e| e.id).collect();
    let mut expected: Vec<&horde_core::sim::Enemy> = a.enemies.iter().collect();
    expected.sort_by(|p, q| {
        q.x.total_cmp(&p.x)
            .then((p.y() + crowd_stagger(p.id)[1]).total_cmp(&(q.y() + crowd_stagger(q.id)[1])))
            .then(p.id.cmp(&q.id))
    });
    assert_eq!(
        ids,
        expected.iter().map(|e| e.id).collect::<Vec<_>>(),
        "the body behind the stack paints first, the stack by its drawn feet"
    );
    assert_eq!(ids[0], a.enemies[6].id);
    // Inside a stack on one x, a body whose drawn feet are nearer the viewer
    // (lower on screen) never paints under one whose feet are farther.
    for pair in view.enemies.windows(2) {
        if pair[0].position[0] == pair[1].position[0] {
            assert!(
                pair[0].drawn()[1] <= pair[1].drawn()[1],
                "{} (feet {}) paints over {} (feet {})",
                pair[1].id,
                pair[1].drawn()[1],
                pair[0].id,
                pair[0].drawn()[1]
            );
        }
    }
}

/// A killed hostile starts to collapse, and later settles, where it was
/// drawn: the crowd stagger carries over to its body, so a body in a stack
/// does not jump on the frame it dies.
#[test]
fn a_killed_hostile_collapses_and_settles_where_it_was_drawn() {
    use crate::animation::COLLAPSE_TICKS;
    let mut game = stacked_line();
    let a = game.attempt().unwrap();
    assert!(a.corpses.is_empty());
    let victim = a.enemies[2].id;
    let last = view(a, SceneBlend::NONE)
        .enemies
        .into_iter()
        .find(|e| e.id == victim)
        .unwrap()
        .drawn();
    assert_ne!(last, [a.enemies[2].x, a.enemies[2].y()], "a staggered body");
    game.attempt_mut()
        .unwrap()
        .enemies
        .iter_mut()
        .find(|e| e.id == victim)
        .unwrap()
        .hp = 0;
    game.tick(Input::default());
    let a = game.attempt().unwrap();
    let record = a.corpses.iter().find(|c| c.id == victim).expect("a body");
    assert_eq!(record.at, [horde_core::sim::DEFENSE_LINE, 3.5]);
    let collapsing = view(a, SceneBlend::NONE);
    assert_eq!(collapsing.corpses.len(), 1);
    assert_eq!(collapsing.corpses[0].at, last, "the first collapsing frame");
    for _ in 0..COLLAPSE_TICKS {
        game.tick(Input::default());
    }
    let a = game.attempt().unwrap();
    let settled = view(a, SceneBlend::NONE);
    assert!(settled.corpses.is_empty());
    assert_eq!(settled.settled.view(a, 0).expect("a settled body").at, last);
}

#[test]
fn a_stack_on_one_spot_is_drawn_apart_by_a_static_bounded_stagger() {
    use super::field::{CROWD_STAGGER_LANES, CROWD_STAGGER_YARDS, crowd_stagger};
    for id in 0..20_000u64 {
        let [dx, dy] = crowd_stagger(id);
        assert!((0.0..=CROWD_STAGGER_YARDS).contains(&dx), "id {id} dx {dx}");
        assert!(dy.abs() <= CROWD_STAGGER_LANES, "id {id} dy {dy}");
        assert_eq!(crowd_stagger(id), [dx, dy], "a pure function of the id");
    }
    for first in [0u64, 1_000, u64::MAX - 8] {
        let window: Vec<[f32; 2]> = (first..first + 8).map(crowd_stagger).collect();
        for (n, p) in window.iter().enumerate() {
            for q in &window[n + 1..] {
                let apart = (p[0] - q[0]).abs().max((p[1] - q[1]).abs());
                assert!(apart >= 0.03, "ids from {first}: {p:?} and {q:?}");
            }
        }
    }
    let mut game = stacked_line();
    let a = game.attempt().unwrap();
    let rng = serde_json::to_string(&a.rng).unwrap();
    let first = view(a, SceneBlend::NONE);
    assert_eq!(serde_json::to_string(&a.rng).unwrap(), rng);
    let stack: Vec<&EnemyView> = first
        .enemies
        .iter()
        .filter(|e| e.position[0] == horde_core::sim::DEFENSE_LINE)
        .collect();
    assert_eq!(stack.len(), 6);
    for e in &stack {
        assert_eq!(e.stagger, crowd_stagger(e.id));
        assert_eq!(
            e.drawn(),
            [e.position[0] + e.stagger[0], e.position[1] + e.stagger[1]]
        );
        assert!(
            e.drawn()[0] >= horde_core::sim::DEFENSE_LINE,
            "never past the line"
        );
    }
    // Every pair of stacked bodies is drawn at least 0.03 lane or yard
    // apart, so the stack reads as six bodies. The R2 offsets of any eight
    // consecutive ids are 0.036 apart or more.
    for (n, p) in stack.iter().enumerate() {
        for q in &stack[n + 1..] {
            let (dx, dy) = (p.drawn()[0] - q.drawn()[0], p.drawn()[1] - q.drawn()[1]);
            assert!(
                dx.abs().max(dy.abs()) >= 0.03,
                "{} and {} drawn on one spot",
                p.id,
                q.id
            );
        }
    }
    // Static: a later frame of the same bodies keeps the same offsets.
    game.tick(Input::default());
    let a = game.attempt().unwrap();
    for e in view(a, SceneBlend::NONE).enemies {
        assert_eq!(e.stagger, crowd_stagger(e.id));
    }
}
