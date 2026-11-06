//! Egui harness tests: battle input, HUD and layout.
use super::*;

#[test]
fn pointer_pause_and_resume_freeze_and_restart_authoritative_ticks() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Battle, "Pause");
    assert!(f.game.global_paused());
    assert!(f.game.attempt().unwrap().paused);
    let frozen = serde_json::to_string(&f.game).unwrap();
    for _ in 0..30 {
        f.game.tick(Input::default());
    }
    assert_eq!(serde_json::to_string(&f.game).unwrap(), frozen);
    h.click(f, View::Battle, "Begin defense");
    assert!(!f.game.global_paused());
    assert!(!f.battle.construction);
    for _ in 0..30 {
        f.game.tick(Input::default());
    }
    assert_eq!(f.game.attempt().unwrap().tick, 30);
}

#[test]
fn npc_controls_do_not_take_a_permanent_column_from_the_fullscreen_arena() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        settings::configure(&h.ctx, scale);
        let f = &mut fixture.frontend;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.game.campaign_mut().attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.screen = Screen::Battle;
        h.frame_size(f, View::Battle, vec![], size);
        let arena = f.battle.arena_rect.unwrap();
        let canvas = h.ctx.content_rect();
        assert!(arena.left() >= canvas.left() && arena.right() <= canvas.right());
        assert!(
            arena.width() >= canvas.width() * 0.90,
            "Battlefield must fill the screen instead of reserving a sidebar"
        );
        assert!(
            arena.height() > 11.0 * 12.0,
            "All eleven lanes need a visible tactical rectangle"
        );
    }
}

#[test]
fn owned_defense_blueprints_allow_live_building_without_an_automatic_pause() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert!(!f.battle.construction);
    h.click(f, View::Battle, "Begin defense");
    assert!(!f.game.attempt().unwrap().paused);
    h.click(f, View::Battle, "Build defenses");
    assert!(f.battle.construction);
    assert!(!f.game.global_paused());
    assert!(!f.game.attempt().unwrap().paused);
    assert!(Game::load(&f.paths.campaign()).is_ok());
}

#[test]
fn keyboard_pause_and_construction_stop_toggle_firing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.battle.fire_latched = true;
    h.key(f, View::Battle, Key::Escape);
    assert!(f.game.global_paused());
    assert!(!f.battle.fire_latched);
    h.key(f, View::Battle, Key::Escape);
    assert!(!f.game.global_paused());
    f.battle.fire_latched = true;
    h.key(f, View::Battle, Key::B);
    assert!(f.battle.construction);
    assert!(!f.game.global_paused());
    assert!(!f.battle.fire_latched);
    h.key(f, View::Battle, Key::B);
    assert!(!f.battle.construction);
    assert!(!f.game.global_paused());
}

#[test]
fn mouse_reload_button_queues_a_legal_reload_without_firing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt_mut().unwrap().ammo = 3;
    h.click(f, View::Battle, "Reload");
    assert!(f.battle.pending.reload);
    f.game.tick(f.battle.pending.clone());
    let a = f.game.attempt().unwrap();
    assert!(a.reload_left > 0);
    assert_eq!(a.ammo, 3);
    assert_eq!(a.shots, 0);
}

#[test]
fn remapped_keyboard_fire_toggles_once_per_press_and_ignores_repeat_and_unbound_mouse() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.bindings[Action::Fire as usize] = "Space".into();
    f.settings.toggle_fire = true;
    f.battle.arena_hover = false;
    assert!(h.combat_input(f, vec![key_event(Key::Space, true)]).fire);
    assert!(
        f.battle.fire_latched,
        "The remapped keyboard fire works outside the arena"
    );
    assert!(
        h.combat_input(f, vec![]).fire,
        "Holding the key must retain the latch"
    );
    let repeat = Event::Key {
        key: Key::Space,
        physical_key: Some(Key::Space),
        pressed: true,
        repeat: true,
        modifiers: Modifiers::NONE,
    };
    assert!(
        h.combat_input(f, vec![repeat]).fire,
        "OS key-repeat cannot toggle firing off"
    );
    assert!(h.combat_input(f, vec![key_event(Key::Space, false)]).fire);
    assert!(!h.combat_input(f, vec![key_event(Key::Space, true)]).fire);
    assert!(!f.battle.fire_latched);
    h.combat_input(f, vec![key_event(Key::Space, false)]);
    f.battle.arena_hover = true;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "Unbound Mouse1 cannot toggle keyboard fire"
    );
    assert!(!h.combat_input(f, vec![pointer_event(false)]).fire);
}

#[test]
fn mouse_fire_toggle_is_restricted_to_the_arena_and_ui_clicks_preserve_its_state() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.toggle_fire = true;
    f.battle.arena_hover = false;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "Clicking a UI panel cannot begin firing"
    );
    h.combat_input(f, vec![pointer_event(false)]);
    assert!(!f.battle.fire_latched);
    f.battle.arena_hover = true;
    assert!(h.combat_input(f, vec![pointer_event(true)]).fire);
    assert!(h.combat_input(f, vec![pointer_event(false)]).fire);
    f.battle.arena_hover = false;
    assert!(
        h.combat_input(f, vec![pointer_event(true)]).fire,
        "A UI click cannot silently stop an active latch"
    );
    h.combat_input(f, vec![pointer_event(false)]);
    assert!(f.battle.fire_latched);
    f.battle.arena_hover = true;
    assert!(
        !h.combat_input(f, vec![pointer_event(true)]).fire,
        "A second arena click stops firing"
    );
    assert!(!f.battle.fire_latched);
}

#[test]
fn quick_fire_tap_survives_release_and_reaim_before_a_fixed_opportunity() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.battle.arena_rect = Some(arena);
    f.battle.arena_hover = true;
    f.battle.aim = [8.0, 5.5];
    let pressed = art::world(arena, 8.0, 5.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
        ],
    );
    f.battle.aim = [27.0, 0.5];
    let released = art::world(arena, 27.0, 0.5);
    h.publish_combat(
        f,
        vec![
            attack_pointer(pressed, PointerButton::Primary, false),
            Event::PointerMoved(released),
        ],
    );
    assert_eq!(f.game.attempt().unwrap().shots, 0);
    f.step();
    let attempt = f.game.attempt().unwrap();
    assert_eq!(attempt.shots, 1);
    assert_eq!(
        attempt.hits, 1,
        "The released tap must retain its original target direction"
    );
    for _ in 0..60 {
        f.step();
    }
    assert_eq!(f.game.attempt().unwrap().shots, 1);
}

#[test]
fn one_render_frame_click_and_motion_uses_the_mouse_press_event_position() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.battle.arena_rect = Some(arena);
    f.battle.arena_hover = true;
    f.battle.aim = [27.0, 0.5];
    let pressed = art::world(arena, 8.0, 5.5);
    let moved = art::world(arena, 27.0, 0.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
            attack_pointer(pressed, PointerButton::Primary, false),
            Event::PointerMoved(moved),
        ],
    );
    f.step();
    let attempt = f.game.attempt().unwrap();
    assert_eq!(attempt.shots, 1);
    assert_eq!(attempt.hits, 1);
}

#[test]
fn quick_outer_lane_melee_tap_keeps_its_selected_lane_after_release_and_motion() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 0, horde_core::sim::DEFENSE_LINE);
    input_enemy(f, 0, 2.2);
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.battle.arena_rect = Some(arena);
    f.battle.arena_hover = true;
    f.battle.aim = [1.5, 0.5];
    let pressed = art::world(arena, 1.5, 0.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Secondary, true),
        ],
    );
    f.battle.aim = [1.5, 10.5];
    h.publish_combat(
        f,
        vec![
            attack_pointer(pressed, PointerButton::Secondary, false),
            Event::PointerMoved(art::world(arena, 1.5, 10.5)),
        ],
    );
    let mut strikes = 0;
    for tick in 0..150 {
        f.step();
        let attempt = f.game.attempt().unwrap();
        for event in attempt.events.iter().filter(|event| {
            event.tick == attempt.tick && event.kind == horde_core::sim::EventKind::Melee
        }) {
            strikes += 1;
            assert_eq!(event.to[1], 0.5);
        }
        if tick == 0 {
            assert_eq!(
                strikes, 1,
                "The tap must strike the original outer contact lane"
            );
        }
    }
    assert_eq!(
        strikes, 1,
        "A released tap cannot repeat after melee cooldown"
    );
}

#[test]
fn sidebar_strike_button_uses_the_actual_selected_outer_contact_lane() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 0, horde_core::sim::DEFENSE_LINE);
    f.battle.aim = [1.5, 0.5];
    h.click(f, View::Battle, "Strike contact lane");
    assert!(f.battle.pending.melee);
    h.publish_combat(f, vec![]);
    f.step();
    let attempt = f.game.attempt().unwrap();
    let event = attempt
        .events
        .iter()
        .find(|event| event.kind == horde_core::sim::EventKind::Melee)
        .expect("A legal targeted strike should resolve");
    assert_eq!(event.to[1], 0.5);
}

#[test]
fn rapid_remapped_fire_taps_coalesce_without_bypassing_the_weapon_cadence() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 5, 8.0);
    f.settings.bindings[Action::Fire as usize] = "Space".into();
    f.battle.aim = [8.0, 5.5];
    for _ in 0..8 {
        h.publish_combat(f, vec![key_event(Key::Space, true)]);
        h.publish_combat(f, vec![key_event(Key::Space, false)]);
    }
    f.step();
    assert_eq!(f.game.attempt().unwrap().shots, 1);
    for _ in 0..4 {
        h.publish_combat(f, vec![key_event(Key::Space, true)]);
        h.publish_combat(f, vec![key_event(Key::Space, false)]);
        f.step();
    }
    assert_eq!(f.game.attempt().unwrap().shots, 1);
    for _ in 0..60 {
        f.step();
    }
    assert_eq!(
        f.game.attempt().unwrap().shots,
        1,
        "Unready taps cannot become a deferred firing burst"
    );
}

#[test]
fn locked_construction_stays_out_of_the_first_battle_and_explains_the_gate() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .all(|shape| text_position(&shape.shape, "Build defenses").is_none())
    );
    let supply = f.game.attempt().unwrap().supply;
    h.key(f, View::Battle, Key::B);
    assert!(!f.game.global_paused());
    assert!(!f.battle.construction);
    assert_eq!(f.game.attempt().unwrap().supply, supply);
    // The message names the next missing step from the core, not a guess.
    let blocker = f.game.campaign().construction_blocker().unwrap();
    assert!(
        f.status.message.starts_with(&blocker),
        "{}",
        f.status.message
    );
}

#[test]
fn hud_reports_actual_reload_and_outer_lane_damage_without_advancing_them() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt_mut().unwrap().ammo = 3;
    f.game.tick(Input {
        reload: true,
        ..Input::default()
    });
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Reload ·"))
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    input_enemy(f, 10, horde_core::sim::DEFENSE_LINE);
    for _ in 0..180 {
        f.game.tick(Input::default());
        if f.game.attempt().unwrap().hp < f.game.attempt().unwrap().max_hp {
            break;
        }
    }
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "HIT · LANE 11"))
    );
}

#[test]
fn ambience_selection_is_a_read_only_projection_of_actual_battle_and_pause() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let before = serde_json::to_value(&f.game).unwrap();
    let active = f.ctx().ambience_target();
    assert_eq!(active.scene.mode, Some(Mode::Personal));
    assert_eq!(
        active.context,
        crate::atmosphere_audio::AudioContext::Battle
    );
    f.screen = Screen::Settings;
    assert_eq!(
        f.ctx().ambience_target().context,
        crate::atmosphere_audio::AudioContext::Paused
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn long_npc_action_is_one_elided_hud_line_without_changing_height_or_authority() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        settings::configure(&h.ctx, scale);
        let f = &mut fixture.frontend;
        f.settings.scale = scale;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.game.campaign_mut().attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.screen = Screen::Battle;
        f.game.attempt_mut().unwrap().decision = "Aim at Runner in lane 5".into();
        // Egui's first complete panel/legend pass settles its cached sizing
        // (the initial arena differs by one 15-point legend row even without
        // an action change). Compare two established layouts, as native
        // captures do after warmup; none of the geometry assertions are relaxed.
        h.frame_size(f, View::Battle, vec![], size);
        h.frame_size(f, View::Battle, vec![], size);
        let short_arena = f.battle.arena_rect.unwrap();
        let long = "Attack: shoot Armored survivor in lane 11 — close engagement; preparing a fully aimed follow-up when the shared defense line is safe";
        f.game.attempt_mut().unwrap().decision = long.into();
        let before = serde_json::to_value(&f.game).unwrap();
        let output = h.frame_size(f, View::Battle, vec![], size);
        let arena = f.battle.arena_rect.unwrap();
        let text = output
            .shapes
            .iter()
            .find_map(|shape| hud_text(&shape.shape, long, arena))
            .expect("The long current action must remain a visible HUD readout");
        assert_eq!(
            text.galley.rows.len(),
            1,
            "An action cannot wrap under its progress stroke"
        );
        assert!(
            text.galley.elided,
            "The native label must indicate omitted text with ellipsis"
        );
        assert!(text.galley.rect.height() <= 20.0);
        assert!((arena.top() - short_arena.top()).abs() < 0.01);
        assert!(
            (arena.height() - short_arena.height()).abs() < 0.01,
            "Short action arena {:?}; long action arena {:?}",
            short_arena,
            arena
        );
        assert_eq!(
            f.game.attempt().unwrap().decision,
            long,
            "The complete sidebar/tooltip text must remain available"
        );
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}

#[test]
fn battle_management_popup_cannot_leak_reload_or_fire_input() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt_mut().unwrap().ammo = 4;
    h.click(f, View::Battle, "Management");
    h.key(f, View::Battle, Key::R);
    assert!(!f.battle.pending.reload && !f.battle.pending.fire);
    assert_eq!(f.game.attempt().unwrap().ammo, 4);
    assert_eq!(f.game.attempt().unwrap().shots, 0);
}

#[test]
fn locked_throwables_are_disabled_buttons_that_explain_where_to_learn_them() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    h.ctx
        .all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
    let f = &mut fixture.frontend;
    let slot = crate::battle_view::OrdnanceSlotView::of(
        f.game.attempt().unwrap(),
        horde_core::ordnance::OrdnanceKind::Grenade,
    );
    assert!(!slot.unlocked);
    let position = h
        .frame(f, View::Battle, vec![])
        .shapes
        .iter()
        .find_map(|clipped| text_position(&clipped.shape, "Grenade"))
        .unwrap();
    let mut shown = false;
    for frame in 0..4 {
        let events = if frame == 0 {
            vec![Event::PointerMoved(position)]
        } else {
            vec![]
        };
        let output = h.frame(f, View::Battle, events);
        shown |= output.shapes.iter().any(|clipped| {
            shape_has_text(
                &clipped.shape,
                "Grenade locked. Learn it in the Ordnance branch of Journey upgrades",
            )
        });
    }
    assert!(shown, "hovering a locked throwable must show its reason");
    // Clicking a locked selector keeps the gun.
    h.click(f, View::Battle, "Grenade");
    assert_eq!(f.battle.attack_selection, battle::AttackSelection::Gun);
}
#[test]
fn selected_grenade_uses_one_press_cell_and_never_repeats_from_hold_or_fire_toggle() {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign_mut().coins = 100;
    fixture
        .frontend
        .game
        .campaign_mut()
        .purchase_node("grenade_training")
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Battle, "Grenade");
    assert_eq!(f.battle.attack_selection, battle::AttackSelection::Grenade);
    f.battle.fire_latched = true;
    let arena = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 220.0));
    f.battle.arena_rect = Some(arena);
    f.battle.arena_hover = true;
    let pressed = art::world(arena, 19.25, 3.75);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(pressed),
            attack_pointer(pressed, PointerButton::Primary, true),
        ],
    );
    f.battle.aim = [28.0, 9.0];
    h.publish_combat(f, vec![Event::PointerMoved(art::world(arena, 28.0, 9.0))]);
    f.step();
    let a = f.game.attempt().unwrap();
    assert_eq!(
        a.ordnance
            .spent(horde_core::ordnance::OrdnanceKind::Grenade),
        1
    );
    assert_eq!(a.ordnance.projectiles[0].cell, [19, 3]);
    assert_eq!(a.shots, 0);
    for _ in 0..180 {
        f.step();
    }
    assert_eq!(
        f.game
            .attempt()
            .unwrap()
            .ordnance
            .spent(horde_core::ordnance::OrdnanceKind::Grenade),
        1
    );
    assert!(!f.battle.fire_latched);
}

#[test]
fn grown_lane_count_drives_outer_row_construction_and_grenade_cells() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().player.level = f.game.campaign_mut().player_level_cap();
    f.game.campaign_mut().player.points = f.game.campaign_mut().player.level - 1;
    f.game.campaign_mut().player.xp = 0;
    f.game.campaign_mut().coins = 100;
    f.game
        .campaign_mut()
        .purchase_node("grenade_training")
        .unwrap();
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.screen = Screen::Battle;
    assert_eq!(f.game.attempt().unwrap().lane_count, 17);
    h.key(f, View::Battle, Key::B);
    assert!(f.battle.construction);
    h.frame(f, View::Battle, vec![]);
    let rect = f.battle.arena_rect.unwrap();
    let projection = art::ArenaProjection::new(rect, 17);
    let point = projection.world(4.2, 16.5);
    h.frame(
        f,
        View::Battle,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, true),
        ],
    );
    h.frame(
        f,
        View::Battle,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, false),
        ],
    );
    assert!(
        f.game
            .attempt()
            .unwrap()
            .structures
            .iter()
            .any(|structure| structure.lane == 16)
    );
    f.battle.construction = false;
    f.battle.attack_selection = battle::AttackSelection::Grenade;
    h.frame(f, View::Battle, vec![]);
    let projection = art::ArenaProjection::new(f.battle.arena_rect.unwrap(), 17);
    let point = projection.world(20.5, 16.5);
    h.publish_combat(
        f,
        vec![
            Event::PointerMoved(point),
            attack_pointer(point, PointerButton::Primary, true),
        ],
    );
    f.step();
    assert_eq!(
        f.game.attempt().unwrap().ordnance.projectiles[0].cell,
        [20, 16]
    );
    assert_eq!(f.game.attempt().unwrap().defender_position(), [0.5, 8.5]);
}

#[test]
fn fresh_three_lane_battle_uses_actual_small_clearing_and_invalid_clicks_do_not_throw() {
    use horde_core::balance::BalanceSettings;
    let mut fixture = IsolatedFrontend::new();
    fixture
        .frontend
        .game
        .campaign_mut()
        .set_balance_settings(BalanceSettings::default())
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.frame_size(f, View::Battle, vec![], vec2(1050.0 / 1.35, 700.0 / 1.35));
    let rect = f.battle.arena_rect.unwrap();
    let a = f.game.attempt().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
    assert!(rect.width() < 400.0 && rect.height() < 150.0);
    assert!(rect.height() >= 86.0);
    let projection = art::ArenaProjection::for_attempt(rect, a);
    let center = projection.inverse(rect.center());
    assert!((center[0] - 6.0).abs() < 0.0001 && (center[1] - 1.5).abs() < 0.0001);
    assert_eq!(projection.world(0.5, 1.5).y, rect.center().y);
    f.battle.attack_selection = battle::AttackSelection::Grenade;
    let outside = rect.right_center() + vec2(8.0, 0.0);
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    h.combat_input_after_pass(f);
    assert!(f.battle.pending.throw.is_none());
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: outside,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    let inside = projection.world(11.5, 2.5);
    h.frame_size(
        f,
        View::Battle,
        vec![Event::PointerButton {
            pos: inside,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        vec2(1050.0 / 1.35, 700.0 / 1.35),
    );
    h.combat_input_after_pass(f);
    assert_eq!(f.battle.pending.throw.unwrap().cell, [11, 2]);
}

#[test]
fn retreat_is_the_one_visible_withdraw_and_management_keeps_its_tools() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Battle, "Management");
    let output = h.frame(f, View::Battle, vec![]);
    for kept in [
        "Settings",
        "Save deployment",
        "Save and return to menu",
        "Combat log",
    ] {
        assert_eq!(text_count(&output, kept), 1, "Management keeps {kept}");
    }
    assert_eq!(text_count(&output, "Retreat & train"), 1);
    for gone in ["Withdraw", "Recall survivor"] {
        assert_eq!(
            text_count(&output, gone),
            0,
            "{gone} duplicates the retreat"
        );
    }
}

#[test]
fn the_first_defense_teaches_on_the_field_and_always_names_the_keys() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert_eq!(f.game.campaign().attempts, 0);
    let painted = |output: &FullOutput, text: &str| {
        output.shapes.iter().any(|s| shape_has_text(&s.shape, text))
    };
    let output = h.frame(f, View::Battle, vec![]);
    // Default bindings are named even though none was changed.
    assert!(painted(&output, "Mouse1 fire · Mouse2 melee · R reload"));
    // The opening lesson is on the field, not only inside Management.
    assert!(painted(&output, "Press Begin defense or Escape to start."));
    assert!(painted(&output, "READY · Begin defense or Escape"));
    h.click(f, View::Battle, "Begin defense");
    let output = h.frame(f, View::Battle, vec![]);
    assert!(!painted(&output, "READY"));
    f.game.tick(Input::default());
    h.click(f, View::Battle, "Pause");
    let output = h.frame(f, View::Battle, vec![]);
    assert!(painted(&output, "PAUSED · Escape to resume"));
    h.click(f, View::Battle, "Hide hints");
    assert!(!f.settings.show_guidance);
}
