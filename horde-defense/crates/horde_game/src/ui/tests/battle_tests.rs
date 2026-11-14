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
    // W3-12: the button is under Management > Field actions.
    h.click(f, View::Battle, "Management");
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
    // W3-12: the button is under Management > Field actions.
    h.click(f, View::Battle, "Management");
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
fn fresh_three_lane_battle_fills_the_arena_width_at_small_size() {
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
    let size = vec2(1050.0 / 1.35, 700.0 / 1.35);
    h.frame_size(f, View::Battle, vec![], size);
    let rect = f.battle.arena_rect.unwrap();
    let a = f.game.attempt().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
    // Zoomed to fill: the width-limited 3x12 field spans the whole arena
    // (1050x700@1.35: 729.8 pt) at 2.5x today's 291.9x86 clearing.
    let canvas = h.ctx.content_rect();
    let arena_width = canvas.width() - 48.0;
    assert!(
        (rect.width() - arena_width).abs() < 1e-3 && (rect.height() - 215.0).abs() < 0.05,
        "3x12 field {:?}, expected {arena_width} x 215.0",
        rect.size()
    );
    assert_eq!(
        rect.x_range(),
        egui::Rangef::new(24.0, canvas.right() - 24.0)
    );
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
        // W3-05: the one manual-save label and the compound exit's name.
        "Save",
        "Save and return to main menu",
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

/// The two target battle sizes in points: 1440x900 and 1050x700@1.35.
fn target_sizes() -> [Vec2; 2] {
    [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)]
}

/// An unpaused fresh-balance battle (a zoomed opening field: the first clear
/// that unlocks the trap levels it to 5x14) in `mode` with the trap blueprint.
/// A `veteran` campaign has finished an attempt; otherwise the battle reads as
/// a first deployment (`BattleScreen::first_deployment` is the campaign's
/// attempt count, zeroed after the deploy because Mara joins only after the
/// introductory attempt).
fn fresh_battle(mode: Mode, veteran: bool) -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let campaign = fixture.frontend.game.campaign_mut();
    campaign
        .set_balance_settings(horde_core::balance::BalanceSettings::default())
        .unwrap();
    campaign.choose_start_country("DEU").unwrap();
    fixture.frontend.game.deploy_country_stage(0, mode).unwrap();
    fixture.frontend.game.campaign_mut().attempts = u64::from(veteran);
    fixture.frontend.game.set_paused(false);
    fixture.frontend.screen = Screen::Battle;
    fixture
}

/// The layout the frame drew for the active attempt, without a hint band.
fn field_layout(h: &Harness, f: &FrontendParts) -> battle::BattleLayout {
    let a = f.game.attempt().unwrap();
    assert!(a.lane_count < 11, "the fixture field is zoomed");
    battle::BattleLayout::new(h.ctx.content_rect(), a.lane_count, a.width_yards, false)
}

#[test]
fn construction_toggle_keeps_arena_rect() {
    for size in target_sizes() {
        let mut fixture = IsolatedFrontend::new();
        fixture.unlock_trap();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        assert_eq!(f.game.attempt().unwrap().lane_count, 11);
        h.frame_size(f, View::Battle, vec![], size);
        let (field, effects) = (f.battle.arena_rect.unwrap(), f.battle.effects_rect);
        f.battle.construction = true;
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(text_count(&output, "Save country plan"), 1);
        assert_eq!(
            f.battle.arena_rect.unwrap(),
            field,
            "construction rescaled the {size:?} arena"
        );
        assert_eq!(f.battle.effects_rect, effects);
    }
}

/// The capture sidecar reports the rects the battle frame recorded, as
/// `[left, top, right, bottom]` points, and the bar rows it counted as
/// `hud_rows` (C-10).
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_report_arena_rect_and_hud_rows() {
    let corners = |r: Rect| [r.left(), r.top(), r.right(), r.bottom()];
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.battle.construction = true;
        h.frame_size(f, View::Battle, vec![], size);
        let facts = review::ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            f.screen,
            f.nav.tab,
            &f.battle,
        );
        let a = f.game.attempt().unwrap();
        assert!(f.battle.arena_rect.is_some(), "{size:?}: no field recorded");
        assert_eq!(
            facts.arena_rect,
            f.battle.arena_rect.map(corners),
            "{size:?}"
        );
        assert!(
            f.battle.effects_rect.is_some(),
            "{size:?}: no effects clip recorded"
        );
        assert_eq!(facts.effects_rect, f.battle.effects_rect.map(corners));
        assert_eq!(facts.observer_arena_rect, None, "no observer in a battle");
        assert_eq!(facts.screen, "Battle");
        assert_eq!(facts.tab, None, "a battle has no camp tab");
        assert_eq!(facts.overlays, ["construction"]);
        assert_eq!(
            (facts.lanes, facts.width_yards),
            (Some(a.lane_count), Some(a.width_yards))
        );
        assert!(f.battle.bar_rows > 0, "{size:?}: no bar rows counted");
        assert_eq!(facts.hud_rows, Some(f.battle.bar_rows), "{size:?}");
        assert_eq!(facts.reduced_effects, f.settings.reduced_effects);
        assert_eq!(facts.ui_scale, f.settings.scale);
        // Off the battle screen a stale field is never reported.
        f.screen = Screen::Results;
        let facts = review::ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            f.screen,
            f.nav.tab,
            &f.battle,
        );
        assert_eq!((facts.arena_rect, facts.effects_rect), (None, None));
        assert_eq!((facts.lanes, facts.overlays.len()), (None, 0));
        assert_eq!(facts.hud_rows, None, "{size:?}: no bar off the battle");
    }
}

/// The control bar's row count (UX-14, C-10). W3-11 pinned the bar before
/// W3-12 at 3 rows at 1050x700@1.35 (777.8 pt wide): 2 button rows and a
/// status row. Since W3-12 the fullest personal bar (heal, brace and a
/// blueprint owned, paused before the first tick so the first button reads
/// "Begin defense") is 2 rows there and 1 at 1440x900: Reload, Strike contact
/// lane and Toggle firing are Management field actions, and a status message
/// is the battle toast, not a row. The running bar ("Pause") and the observed
/// bar fit one row at both sizes.
#[test]
fn battle_bar_rows_measure_todays_bar() {
    let [large, small] = target_sizes();
    for (h, size, buttons, observed) in [
        (Harness::small(), small, 2, 1),
        (Harness::large(), large, 1, 1),
    ] {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let f = &mut fixture.frontend;
        assert_eq!(f.battle.bar_rows, 0, "no battle frame has drawn a bar yet");
        let profile = &mut f.game.attempt_mut().unwrap().profile;
        (profile.heal, profile.brace) = (true, true);
        h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(f.battle.bar_rows, 1, "{size:?}: the running bar");
        f.game.set_paused(true);
        let output = h.frame_size(f, View::Battle, vec![], size);
        let canvas = h.ctx.content_rect().size();
        assert!((canvas - size).length() < 0.5, "{canvas:?} is not {size:?}");
        for label in [
            "Begin defense",
            "Emergency aid",
            "Brace the line",
            "Build defenses",
            "Retreat & train",
            "Management",
        ] {
            assert_eq!(text_count(&output, label), 1, "{size:?}: {label}");
        }
        assert_eq!(f.battle.bar_rows, buttons, "{size:?}: buttons");
        f.status.message = "Rewards retained. Rest, train, and retry.".into();
        h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(f.battle.bar_rows, buttons, "{size:?}: with status");
        // The capture sidecar reports the same count.
        #[cfg(feature = "review")]
        assert_eq!(
            review::ReviewFrameFacts::collect(
                &f.game,
                &f.launch,
                &f.settings,
                f.screen,
                f.nav.tab,
                &f.battle
            )
            .hud_rows,
            Some(buttons),
            "{size:?}"
        );
        // The observed bar is counted by the same accumulator.
        let mut fixture = fresh_battle(Mode::Npc, true);
        let f = &mut fixture.frontend;
        let output = h.frame_size(f, View::Battle, vec![], size);
        for label in ["Pause", "Observe", "Tactics", "Recall & train"] {
            assert_eq!(text_count(&output, label), 1, "{size:?}: {label}");
        }
        assert_eq!(f.battle.bar_rows, observed, "{size:?}: observed");
    }
}

#[test]
fn battle_frame_records_layout_rects() {
    use horde_core::balance::BalanceSettings;
    for size in target_sizes() {
        for settings in [BalanceSettings::default(), BalanceSettings::legacy()] {
            for construction in [false, true] {
                let mut fixture = IsolatedFrontend::new();
                fixture
                    .frontend
                    .game
                    .campaign_mut()
                    .set_balance_settings(settings.clone())
                    .unwrap();
                fixture.deploy();
                let h = Harness::new();
                let f = &mut fixture.frontend;
                f.battle.construction = construction;
                h.frame_size(f, View::Battle, vec![], size);
                let a = f.game.attempt().unwrap();
                let first = f.game.campaign().attempts == 0;
                assert!(first, "a fresh fixture is a first deployment");
                let layout = battle::BattleLayout::new(
                    h.ctx.content_rect(),
                    a.lane_count,
                    a.width_yards,
                    first,
                );
                let label = format!(
                    "{}x{} construction {construction} at {size:?}",
                    a.lane_count, a.width_yards
                );
                assert_eq!(f.battle.arena_rect, Some(layout.field.rect), "{label}");
                assert_eq!(f.battle.effects_rect, Some(layout.effects_clip), "{label}");
                assert_eq!(
                    f.battle.effects_keep_clear,
                    layout.effects_keep_clear(),
                    "{label}"
                );
            }
        }
    }
}

/// Pin: "Hide hints" removes the callout but never moves the field; the band
/// belongs to the whole first attempt.
#[test]
fn hide_hints_never_moves_the_field() {
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, false);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        assert!(f.settings.show_guidance);
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(text_count(&output, "Hide hints"), 1);
        let shown = f.battle.arena_rect.unwrap();
        assert!(
            shown.top() >= 124.0 + 92.0,
            "{size:?}: field {shown:?} under the band"
        );
        f.settings.show_guidance = false;
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(text_count(&output, "Hide hints"), 0);
        assert_eq!(
            f.battle.arena_rect.unwrap(),
            shown,
            "{size:?}: hiding hints moved the field"
        );
    }
}

/// Pin: an observed (Npc) first deployment shows no lesson, so it reserves no
/// band.
#[test]
fn npc_first_deployment_has_no_hint_band() {
    let size = vec2(1050.0 / 1.35, 700.0 / 1.35);
    let mut fixture = fresh_battle(Mode::Npc, false);
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert_eq!(f.game.campaign().attempts, 0);
    h.frame_size(f, View::Battle, vec![], size);
    let a = f.game.attempt().unwrap();
    let (lanes, width) = (a.lane_count, a.width_yards);
    assert!(lanes < 11, "Mara's opening field is zoomed");
    let open = battle::BattleLayout::new(h.ctx.content_rect(), lanes, width, false);
    let banded = battle::BattleLayout::new(h.ctx.content_rect(), lanes, width, true);
    assert_ne!(
        open.field.rect, banded.field.rect,
        "the band moves a {lanes}x{width} field"
    );
    assert_eq!(f.battle.arena_rect, Some(open.field.rect));
}

/// Pin: the plan buttons stay inside the construction overlay at both sizes.
#[test]
fn construction_panel_keeps_plan_buttons() {
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.battle.construction = true;
        let output = h.frame_size(f, View::Battle, vec![], size);
        let panel = field_layout(&h, f).construction_panel;
        for label in ["Save country plan", "Clear country plan"] {
            let at = output
                .shapes
                .iter()
                .find_map(|shape| text_position(&shape.shape, label))
                .unwrap_or_else(|| panic!("{label} is not painted at {size:?}"));
            assert!(
                panel.contains(at),
                "{label} at {at:?} leaves the panel {panel:?}"
            );
        }
    }
}

/// Pin: the painted construction panel fills `layout.construction_panel`, the
/// rect the INCOMING cues, the callout occluder and the hover refusal all
/// treat as covered. A frame that shrank to its content left lanes visible
/// below it at 1440x900 while they were still cued and refused as hidden.
#[test]
fn construction_panel_paints_its_whole_layout_rect() {
    fn panel_fills(shape: &egui::epaint::Shape, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Rect(rect)
                if rect.fill == Color32::from_black_alpha(battle::PANEL_FILL_ALPHA) =>
            {
                out.push(rect.rect);
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| panel_fills(shape, out));
            }
            _ => {}
        }
    }
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.battle.construction = true;
        let output = h.frame_size(f, View::Battle, vec![], size);
        let panel = field_layout(&h, f).construction_panel;
        let mut fills = Vec::new();
        for clipped in &output.shapes {
            panel_fills(&clipped.shape, &mut fills);
        }
        let painted = fills
            .into_iter()
            .find(|rect| rect.intersects(panel))
            .unwrap_or_else(|| panic!("{size:?}: no panel fill painted"));
        assert!(
            (painted.min - panel.min).length() < 0.5 && (painted.max - panel.max).length() < 0.5,
            "{size:?}: painted panel {painted:?}, layout rect {panel:?}"
        );
        // A bottom-lane enemy under the panel is cued, so the panel must
        // really cover that lane.
        let field = field_layout(&h, f).field.rect;
        let lanes = f32::from(f.game.attempt().unwrap().lane_count);
        let bottom = field.top() + (lanes - 0.5) * field.height() / lanes;
        assert!(
            painted.y_range().contains(bottom),
            "{size:?}: bottom lane centre {bottom} outside the painted panel {painted:?}"
        );
    }
}

/// Pin: a pointer over the construction overlay neither hovers nor aims the
/// field under it.
#[test]
fn construction_panel_swallows_field_hover() {
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.battle.construction = true;
        h.frame_size(f, View::Battle, vec![], size);
        let layout = field_layout(&h, f);
        let (panel, field) = (layout.construction_panel, layout.field.rect);
        let covered = pos2(panel.left() + 4.0, panel.center().y);
        assert!(
            field.contains(covered),
            "{size:?}: the panel overlaps the field"
        );
        let aim = f.battle.aim;
        for _ in 0..2 {
            h.frame_size(f, View::Battle, vec![Event::PointerMoved(covered)], size);
        }
        assert!(
            !f.battle.arena_hover,
            "{size:?}: the panel let the field hover"
        );
        assert_eq!(f.battle.aim, aim, "{size:?}: the panel let the field aim");
        let open = pos2(field.left() + field.width() * 0.25, field.center().y);
        for _ in 0..2 {
            h.frame_size(f, View::Battle, vec![Event::PointerMoved(open)], size);
        }
        assert!(f.battle.arena_hover, "{size:?}: the open field must hover");
        assert_ne!(f.battle.aim, aim);
    }
}

/// C-7: while construction is open, a lane whose live enemy the panel hides
/// gets an INCOMING chevron left of the panel; closing construction clears
/// the lanes and the cue.
#[test]
fn construction_frame_draws_incoming_chevron() {
    use horde_core::content::EnemyKind;
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        let mut a = f.game.attempt_slot_mut().take().unwrap();
        assert!(a.lane_count >= 2);
        a.enemies.clear();
        let width = a.width_yards;
        for (lane, x) in [(0, 1.5), (1, width - 0.5)] {
            f.game.spawn_enemy(&mut a, EnemyKind::Runner, lane, false);
            a.enemies.last_mut().unwrap().x = x;
        }
        // The spawn events' own field flash also reads INCOMING.
        a.events.clear();
        *f.game.attempt_slot_mut() = Some(a);
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert!(
            f.battle.occluded_lanes.is_empty(),
            "{size:?}: no panel, no cue"
        );
        assert_eq!(text_count(&output, "INCOMING"), 0, "{size:?}");
        f.battle.construction = true;
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(f.battle.occluded_lanes, vec![1], "{size:?}");
        assert_eq!(text_count(&output, "INCOMING"), 1, "{size:?}");
        let layout = field_layout(&h, f);
        let at = output
            .shapes
            .iter()
            .find_map(|shape| text_position(&shape.shape, "INCOMING"))
            .unwrap();
        let lane = layout.field.rect.height() / f32::from(f.game.attempt().unwrap().lane_count);
        let centre = layout.field.rect.top() + 1.5 * lane;
        assert!(
            at.x < layout.construction_panel.left() && (at.y - centre).abs() < 1.0,
            "{size:?}: INCOMING at {at:?}, lane 1 centre {centre}"
        );
        h.click_size(f, View::Battle, "Close construction", size);
        assert!(!f.battle.construction);
        // The button closes after this frame's cues drew; the close itself
        // clears the lanes, before any later battle frame.
        assert!(
            f.battle.occluded_lanes.is_empty(),
            "{size:?}: the close button clears"
        );
        let output = h.frame_size(f, View::Battle, vec![], size);
        assert!(f.battle.occluded_lanes.is_empty(), "{size:?}: close clears");
        assert_eq!(text_count(&output, "INCOMING"), 0, "{size:?}");
        // Pausing from the bar closes construction too, and so clears.
        f.battle.construction = true;
        h.frame_size(f, View::Battle, vec![], size);
        assert_eq!(f.battle.occluded_lanes, vec![1], "{size:?}");
        h.click_size(f, View::Battle, "Pause", size);
        assert!(!f.battle.construction);
        assert!(
            f.battle.occluded_lanes.is_empty(),
            "{size:?}: the pause button clears"
        );
    }
}

/// Pin: battlefield sprites clip to the arena, never to the taller effects
/// clip.
#[test]
fn battlefield_sprite_clip_stays_the_arena() {
    for size in target_sizes() {
        let mut fixture = fresh_battle(Mode::Personal, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        let output = h.frame_size(f, View::Battle, vec![], size);
        let layout = field_layout(&h, f);
        assert_ne!(layout.arena, layout.effects_clip);
        let clipped = output
            .shapes
            .iter()
            .filter(|shape| shape.clip_rect == layout.arena)
            .count();
        assert!(
            clipped > 0,
            "{size:?}: no battlefield shape clips to the arena"
        );
    }
}

/// A deployed personal defense with `nodes` bought first.
fn battle_with_nodes(nodes: &[&str]) -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign_mut().coins = 10_000;
    for node in nodes {
        fixture
            .frontend
            .game
            .campaign_mut()
            .purchase_node(node)
            .unwrap();
    }
    fixture.deploy();
    fixture
}

#[test]
fn cycle_attack_key_cycles_unlocked_attacks() {
    use battle::AttackSelection::{Grenade, Gun, Molotov};
    // The pure rule: the next unlocked selection in selector order, wrapping.
    assert_eq!(Gun.next(|_| true), Grenade);
    assert_eq!(Grenade.next(|_| true), Molotov);
    assert_eq!(Molotov.next(|_| true), Gun);
    assert_eq!(Gun.next(|s| s != Grenade), Molotov);
    assert_eq!(Molotov.next(|s| s == Molotov), Molotov);
    assert_eq!(Grenade.next(|_| false), Grenade);
    let h = Harness::new();
    // Nothing but the gun: the key keeps it.
    let mut fixture = battle_with_nodes(&[]);
    let f = &mut fixture.frontend;
    assert_eq!(f.settings.extra_bindings.cycle_attack, "Q");
    h.key(f, View::Battle, Key::Q);
    assert_eq!(f.battle.attack_selection, Gun);

    // Grenades learned: the key skips the locked Molotov.
    let mut fixture = battle_with_nodes(&["grenade_training"]);
    let f = &mut fixture.frontend;
    f.battle.fire_latched = true;
    h.key(f, View::Battle, Key::Q);
    assert_eq!(f.battle.attack_selection, Grenade);
    // As a click on the selector: a firing latch never carries over.
    assert!(!f.battle.fire_latched);
    h.key(f, View::Battle, Key::Q);
    assert_eq!(f.battle.attack_selection, Gun);

    // Both learned: Gun, Grenade, Molotov and round again.
    let mut fixture = battle_with_nodes(&["grenade_training", "molotov_training"]);
    let f = &mut fixture.frontend;
    let mut seen = Vec::new();
    for _ in 0..4 {
        h.key(f, View::Battle, Key::Q);
        seen.push(f.battle.attack_selection);
    }
    assert_eq!(seen, [Grenade, Molotov, Gun, Grenade]);
    // The rebound key cycles; the old one no longer does.
    f.settings.set_binding(
        settings::BindingId::Extra(settings::ExtraAction::CycleAttack),
        "K".into(),
    );
    h.key(f, View::Battle, Key::Q);
    assert_eq!(f.battle.attack_selection, Grenade);
    h.key(f, View::Battle, Key::K);
    assert_eq!(f.battle.attack_selection, Molotov);
    // The construction overlay hides the selector: the key waits.
    f.battle.set_construction(true);
    h.key(f, View::Battle, Key::K);
    assert_eq!(f.battle.attack_selection, Molotov);
    f.battle.set_construction(false);
    // An observed defense has no attack selector to cycle.
    let mut fixture = fresh_battle(Mode::Npc, true);
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 10_000;
    h.key(f, View::Battle, Key::Q);
    assert_eq!(f.battle.attack_selection, Gun);
}

/// W3-12 (C-10, UX-14, CF-1-08): the control bar takes at most two rows at
/// 1050x700@1.35 and one at 1440x900 in every bar state: personal and
/// observed, running, "Begin defense" and "Resume defense", construction open
/// or closed, firing latched or not, with or without a status message. The
/// status message is the battle toast in the receipt slot and never a bar
/// row, Reload, Strike contact lane and Toggle firing are not bar buttons,
/// and the key legend under the bar is clear of every bar widget.
#[test]
fn battle_bar_stays_within_two_rows_at_small_size() {
    #[derive(Clone, Copy, Debug)]
    enum Clock {
        Running,
        Begin,
        Resume,
    }
    const MESSAGE: &str = "Rest and retry.";
    let [large, small] = target_sizes();
    let mut cases = 0;
    for (h, size, most) in [(Harness::small(), small, 2), (Harness::large(), large, 1)] {
        for mode in [Mode::Personal, Mode::Npc] {
            for clock in [Clock::Running, Clock::Begin, Clock::Resume] {
                for (construction, latched) in [(false, false), (false, true), (true, false)] {
                    for status in [None, Some(false), Some(true)] {
                        let label = format!(
                            "{size:?} {mode:?} {clock:?} construction {construction} \
                             latched {latched} status {status:?}"
                        );
                        let mut fixture = fresh_battle(mode, true);
                        let f = &mut fixture.frontend;
                        let profile = &mut f.game.attempt_mut().unwrap().profile;
                        (profile.heal, profile.brace) = (true, true);
                        match clock {
                            Clock::Running => {}
                            Clock::Begin => f.game.set_paused(true),
                            Clock::Resume => {
                                f.game.tick(Input::default());
                                f.game.set_paused(true);
                            }
                        }
                        f.battle.construction = construction;
                        f.battle.fire_latched = latched;
                        if let Some(error) = status {
                            f.status.message = MESSAGE.into();
                            f.status.message_error = error;
                            f.status.feedback_revision += 1;
                        }
                        let output = h.frame_size(f, View::Battle, vec![], size);
                        let canvas = h.ctx.content_rect();
                        assert!((canvas.size() - size).length() < 0.5, "{label}");
                        let a = f.game.attempt().unwrap();
                        let layout =
                            battle::BattleLayout::new(canvas, a.lane_count, a.width_yards, false);
                        // The bar names its state and keeps every other button.
                        let pause = match clock {
                            Clock::Running => "Pause",
                            Clock::Begin => "Begin defense",
                            Clock::Resume => "Resume defense",
                        };
                        let kept: &[&str] = match (mode, construction) {
                            (Mode::Personal, false) => &[
                                "Emergency aid",
                                "Brace the line",
                                "Build defenses",
                                "Retreat & train",
                                "Management",
                            ],
                            (Mode::Personal, true) => {
                                &["Close construction", "Retreat & train", "Management"]
                            }
                            (_, false) => &[
                                "Build defenses",
                                "Observe",
                                "Tactics",
                                "Recall & train",
                                "Management",
                            ],
                            (_, true) => &[
                                "Close construction",
                                "Observe",
                                "Tactics",
                                "Recall & train",
                                "Management",
                            ],
                        };
                        for kept in kept.iter().chain([&pause]) {
                            assert_eq!(text_count(&output, kept), 1, "{label}: {kept}");
                        }
                        for moved in [
                            "Reload",
                            "Strike contact lane",
                            "Toggle firing",
                            "Stop firing",
                        ] {
                            assert_eq!(text_count(&output, moved), 0, "{label}: {moved}");
                        }
                        let rows = f.battle.bar_rows;
                        assert!((1..=most).contains(&rows), "{label}: {rows} bar rows");
                        let bounds = f.battle.bar_bounds.expect("the bar drew");
                        assert!(
                            layout.bar.expand(0.5).contains_rect(bounds),
                            "{label}: the bar {bounds:?} leaves {:?}",
                            layout.bar
                        );
                        // CF-1-08: the legend is drawn whole and under the bar.
                        let legend = f.battle.legend_rect.expect("the legend drew");
                        assert!(
                            legend.top() >= bounds.bottom() && !legend.intersects(bounds),
                            "{label}: the legend {legend:?} lies under the bar {bounds:?}"
                        );
                        assert!(canvas.contains_rect(legend), "{label}: legend {legend:?}");
                        // The status message is the toast, never a bar row.
                        let toast = clipped_bounds(&output, MESSAGE);
                        assert_eq!(toast.is_some(), status.is_some(), "{label}: toast");
                        if let Some((text, clip)) = toast {
                            assert_eq!(text_count(&output, MESSAGE), 1, "{label}");
                            assert!(
                                layout.receipt_slot.contains_rect(text) && clip.contains_rect(text),
                                "{label}: the toast {text:?} leaves the receipt slot"
                            );
                            let color = output
                                .shapes
                                .iter()
                                .find_map(|c| text_color(&c.shape, MESSAGE));
                            let expected = if status == Some(true) {
                                art::RED
                            } else {
                                art::TEXT
                            };
                            assert_eq!(color, Some(expected), "{label}: toast colour");
                        }
                        // The status line's idle text is gone with its row.
                        assert_eq!(
                            text_count(&output, "Coins and equipment bank as they are earned."),
                            0,
                            "{label}"
                        );
                        // A second frame of the same message adds no toast.
                        h.frame_size(f, View::Battle, vec![], size);
                        assert_eq!(
                            f.status.toasts.len(),
                            usize::from(status.is_some()),
                            "{label}"
                        );
                        assert_eq!(f.battle.bar_rows, rows, "{label}: second frame");
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2 * 2 * 3 * 3 * 3);
}

/// W3-12: Reload, Strike contact lane and Toggle firing left the bar for
/// Management > Field actions. Their keys work as before and the legend still
/// names them; the menu shows each action with its bound key, a click does
/// what the bar button did and closes the menu so the keys return to the
/// field. Construction and an observed battle offer no field actions.
#[test]
fn field_actions_keep_their_keys() {
    const ACTIONS: [&str; 3] = ["Reload", "Strike contact lane", "Toggle firing"];
    let painted = |output: &FullOutput, text: &str| {
        output.shapes.iter().any(|s| shape_has_text(&s.shape, text))
    };
    // The bound key is painted on the action's own menu row.
    let key_beside = |output: &FullOutput, action: &str, key: &str| {
        let (action, _) = clipped_bounds(output, action).expect("the action is listed");
        output.shapes.iter().any(|clipped| {
            labeled_bounds(&clipped.shape, key).is_some_and(|key| {
                key.left() > action.right() && action.y_range().contains(key.center().y)
            })
        })
    };
    let open_menu = |h: &Harness, f: &mut FrontendParts| {
        let closed = h.frame(f, View::Battle, vec![]);
        assert_eq!(text_count(&closed, "Field actions"), 0, "the menu is shut");
        assert_eq!(text_count(&closed, "Settings"), 0, "the menu is shut");
        h.click(f, View::Battle, "Management");
        h.frame(f, View::Battle, vec![])
    };
    let mut fixture = fresh_battle(Mode::Personal, true);
    let h = Harness::new();
    let f = &mut fixture.frontend;
    input_enemy(f, 0, horde_core::sim::DEFENSE_LINE);

    // The bar has none of the three; the legend names the keys.
    let output = h.frame(f, View::Battle, vec![]);
    for action in ACTIONS {
        assert_eq!(text_count(&output, action), 0, "{action} left the bar");
    }
    assert!(painted(
        &output,
        "Mouse1 fire · Mouse2 melee · R reload · B build · Escape pause"
    ));

    // The keys work with no button on screen.
    assert!(h.combat_input(f, vec![key_event(Key::R, true)]).reload);
    h.combat_input(f, vec![key_event(Key::R, false)]);
    let arena = f.battle.arena_rect.unwrap();
    f.battle.arena_hover = true;
    h.combat_input(
        f,
        vec![attack_pointer(
            arena.center(),
            PointerButton::Secondary,
            true,
        )],
    );
    assert!(f.battle.pending.melee, "Mouse2 strikes");
    h.combat_input(
        f,
        vec![attack_pointer(
            arena.center(),
            PointerButton::Secondary,
            false,
        )],
    );
    f.battle.arena_hover = false;
    f.battle.pending = Input::default();
    f.battle.pending_attack_aims = AttackAims::default();
    h.key(f, View::Battle, Key::F);
    assert!(f.battle.fire_latched, "F toggles firing");
    h.key(f, View::Battle, Key::F);
    assert!(!f.battle.fire_latched);

    // Management > Field actions lists each with its key.
    let output = open_menu(&h, f);
    assert_eq!(text_count(&output, "Field actions"), 1);
    for (action, key) in ACTIONS.into_iter().zip(["R", "Mouse2", "F"]) {
        assert_eq!(text_count(&output, action), 1, "{action}");
        assert!(key_beside(&output, action, key), "{action} shows {key}");
    }

    // Each click does what the bar button did, and closes the menu.
    f.game.attempt_mut().unwrap().ammo = 3;
    h.click(f, View::Battle, "Reload");
    assert!(f.battle.pending.reload);
    assert!(!f.battle.pending.melee && !f.battle.pending.fire);
    f.battle.pending = Input::default();

    open_menu(&h, f);
    f.battle.aim = [1.5, 0.5];
    h.click(f, View::Battle, "Strike contact lane");
    assert!(f.battle.pending.melee);
    assert_eq!(f.battle.pending.aim[1], 0.5, "the selected contact lane");
    assert!(f.battle.pending_attack_aims.melee.is_some());
    f.battle.pending = Input::default();
    f.battle.pending_attack_aims = AttackAims::default();

    open_menu(&h, f);
    h.click(f, View::Battle, "Toggle firing");
    assert!(f.battle.fire_latched);
    let output = open_menu(&h, f);
    assert_eq!(text_count(&output, "Toggle firing"), 0);
    assert!(key_beside(&output, "Stop firing", "F"));
    h.click(f, View::Battle, "Stop firing");
    assert!(!f.battle.fire_latched);

    // A rebound key follows into the menu and the legend, and it works.
    f.settings.bindings[Action::Reload as usize] = "Q".into();
    let output = open_menu(&h, f);
    assert!(key_beside(&output, "Reload", "Q"));
    assert!(painted(&output, "Q reload"));
    h.click(f, View::Battle, "Reload");
    assert!(f.battle.pending.reload);
    f.battle.pending = Input::default();
    let closed = h.frame(f, View::Battle, vec![]);
    assert_eq!(text_count(&closed, "Field actions"), 0);
    assert!(!h.combat_input(f, vec![key_event(Key::R, true)]).reload);
    h.combat_input(f, vec![key_event(Key::R, false)]);
    assert!(h.combat_input(f, vec![key_event(Key::Q, true)]).reload);
    h.combat_input(f, vec![key_event(Key::Q, false)]);

    // A throwable has no firing latch to toggle.
    f.battle.attack_selection = battle::AttackSelection::Grenade;
    let output = open_menu(&h, f);
    assert_eq!(text_count(&output, "Field actions"), 1);
    assert_eq!(text_count(&output, "Reload"), 1);
    assert_eq!(text_count(&output, "Strike contact lane"), 1);
    assert_eq!(text_count(&output, "Toggle firing"), 0);
    h.click(f, View::Battle, "Reload");
    f.battle.pending = Input::default();
    f.battle.attack_selection = battle::AttackSelection::Gun;

    // Construction hides the personal controls, as the bar did.
    f.battle.construction = true;
    let output = open_menu(&h, f);
    assert_eq!(text_count(&output, "Settings"), 1, "the menu is open");
    assert_eq!(text_count(&output, "Field actions"), 0);
    for action in ACTIONS {
        assert_eq!(text_count(&output, action), 0, "{action} in construction");
    }

    // An observed battle has no field actions.
    let mut fixture = fresh_battle(Mode::Npc, true);
    let f = &mut fixture.frontend;
    let h = Harness::new();
    let output = open_menu(&h, f);
    assert_eq!(text_count(&output, "Settings"), 1, "the menu is open");
    assert_eq!(text_count(&output, "Field actions"), 0);
    for action in ACTIONS {
        assert_eq!(text_count(&output, action), 0, "{action} when observing");
    }
}

/// A paused, undeployed-to-running personal battle on a `lanes` x `width`
/// field (fresh balance otherwise). A `veteran` campaign has finished an
/// attempt; otherwise the battle is a first deployment with its hint band.
fn sized_battle(lanes: u8, width: u8, veteran: bool) -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    let campaign = fixture.frontend.game.campaign_mut();
    campaign
        .set_balance_settings(horde_core::balance::BalanceSettings {
            start_lanes: lanes,
            max_lanes: lanes,
            start_width: width,
            max_width: width,
            ..Default::default()
        })
        .unwrap();
    campaign.choose_start_country("DEU").unwrap();
    fixture
        .frontend
        .game
        .deploy_country_stage(0, Mode::Personal)
        .unwrap();
    fixture.frontend.game.campaign_mut().attempts = u64::from(veteran);
    fixture.frontend.game.set_paused(true);
    fixture.frontend.screen = Screen::Battle;
    let a = fixture.frontend.game.attempt().unwrap();
    assert!(a.paused && a.tick == 0);
    assert_eq!((a.lane_count, a.width_yards), (lanes, f32::from(width)));
    fixture
}

/// CF-1-01 (W3-29): on the zoomed 7x16 field and the 3x12 first defense, at
/// both target sizes, the painted READY and PAUSED banners lie inside the
/// layout's banner slot, whole and unclipped, and share no area with the
/// field. The open construction overlay still hides the banner.
#[test]
fn pause_banner_is_painted_beside_the_field_at_both_sizes() {
    for size in target_sizes() {
        for (lanes, width, veteran) in [(7, 16, true), (3, 12, false)] {
            let mut fixture = sized_battle(lanes, width, veteran);
            let h = Harness::new();
            let f = &mut fixture.frontend;
            let name = format!("{lanes}x{width} at {size:?}");
            for text in [
                "READY · Begin defense or Escape",
                "PAUSED · Escape to resume",
            ] {
                let output = h.frame_size(f, View::Battle, vec![], size);
                let a = f.game.attempt().unwrap();
                let layout = battle::BattleLayout::new(
                    h.ctx.content_rect(),
                    a.lane_count,
                    a.width_yards,
                    !veteran,
                );
                assert_eq!(layout.hint_band.is_some(), !veteran, "{name}");
                assert_eq!(f.battle.arena_rect, Some(layout.field.rect), "{name}");
                let slot = layout.pause_banner_slot;
                let banner = f.battle.pause_banner_rect.expect("the banner drew");
                assert!(
                    slot.expand(0.01).contains_rect(banner),
                    "{name}: {text}: the banner {banner:?} leaves its slot {slot:?}"
                );
                // The compact tag stays legible: 10 pt type or larger.
                assert!(banner.height() >= 16.0, "{name}: {text}: {banner:?}");
                let shared = banner.intersect(layout.field.rect);
                assert!(
                    shared.width() <= 0.0 || shared.height() <= 0.0,
                    "{name}: {text}: the banner {banner:?} lies on the field {:?}",
                    layout.field.rect
                );
                let (drawn, clip) = clipped_bounds(&output, text)
                    .unwrap_or_else(|| panic!("{name}: {text} is not painted"));
                assert!(
                    banner.expand(0.5).contains_rect(drawn) && clip.contains_rect(drawn),
                    "{name}: {text} at {drawn:?} leaves its backing {banner:?}"
                );
                // The second pass: a battle paused after its first tick.
                f.game.set_paused(false);
                f.game.tick(Input::default());
                f.game.set_paused(true);
            }
            f.battle.construction = true;
            h.frame_size(f, View::Battle, vec![], size);
            assert_eq!(f.battle.pause_banner_rect, None, "{name}: construction");
        }
    }
}

/// CF-1-02 (W3-29): with construction open on the 7x16 field, the painted
/// "NO BUILDING BEYOND MIDPOINT" label and `construction_panel` do not
/// intersect at 1050x700@1.35 (nor at 1440x900, where it keeps its one line
/// at its field point).
#[test]
fn midpoint_label_clears_the_construction_panel() {
    for size in target_sizes() {
        let mut fixture = sized_battle(7, 16, true);
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.battle.construction = true;
        let output = h.frame_size(f, View::Battle, vec![], size);
        let layout = field_layout(&h, f);
        let panel = layout.construction_panel;
        let (label, clip) =
            clipped_bounds(&output, art::MIDPOINT_LABEL).expect("the midpoint label drew");
        assert!(
            !label.intersects(panel) && label.right() <= panel.left() - art::OCCLUDER_GAP,
            "{size:?}: the midpoint label {label:?} lies under the panel {panel:?}"
        );
        assert!(clip.contains_rect(label), "{size:?}: {label:?} is clipped");
        let a = f.game.attempt().unwrap();
        let field = art::ArenaProjection::for_attempt(layout.field.rect, a);
        let point = field.world(a.construction_bounds()[1] + 0.2, 0.2);
        assert!(
            (label.left() - point.x).abs() < 1.0 && (label.top() - point.y).abs() < 1.0,
            "{size:?}: the label {label:?} left its field point {point:?}"
        );
        if size.x > 1000.0 {
            assert!(label.height() < 16.0, "{size:?}: one line, {label:?}");
        }
    }
}

/// CF-1-03: a review capture (a still or a frame sequence) never reads the
/// live OS pointer. With the pointer over the field the aim stays where the
/// scene pinned it, the field does not hover, and the frame paints exactly
/// what it paints with no pointer: no hover reticle, no inspector, no
/// crosshair cursor. An interactive launch still aims with the pointer.
#[test]
fn review_capture_aim_ignores_the_os_pointer() {
    use crate::launch::{LaunchConfig, ReviewOptions};
    let still = ReviewOptions {
        capture: Some("headless-review-capture.png".into()),
        exit: true,
        ..ReviewOptions::default()
    };
    let sequence = ReviewOptions {
        motion: true,
        sequence: Some("headless-review-sequence".into()),
        ..ReviewOptions::default()
    };
    for (name, review, pinned) in [
        ("still capture", Some(still), true),
        ("sequence capture", Some(sequence), true),
        ("interactive review", Some(ReviewOptions::default()), false),
        ("play", None, false),
    ] {
        for size in target_sizes() {
            let mut fixture = fresh_battle(Mode::Personal, true);
            let h = Harness::new();
            let f = &mut fixture.frontend;
            f.launch = LaunchConfig {
                scenario: review.is_some().then(|| "max-lanes".to_owned()),
                review: review.clone(),
                ..LaunchConfig::default()
            };
            h.frame_size(f, View::Battle, vec![], size);
            let still_frame = h.frame_size(f, View::Battle, vec![], size);
            let aim = f.battle.aim;
            let field = field_layout(&h, f).field.rect;
            let over = pos2(field.left() + field.width() * 0.25, field.center().y);
            let mut hovered_frame = None;
            for _ in 0..2 {
                hovered_frame =
                    Some(h.frame_size(f, View::Battle, vec![Event::PointerMoved(over)], size));
            }
            let hovered_frame = hovered_frame.unwrap();
            let crosshair =
                hovered_frame.platform_output.cursor_icon == egui::CursorIcon::Crosshair;
            let same_paint = hovered_frame.shapes == still_frame.shapes;
            if pinned {
                assert_eq!(
                    f.battle.aim, aim,
                    "{name} {size:?}: the pointer moved the pinned aim"
                );
                assert!(
                    !f.battle.arena_hover,
                    "{name} {size:?}: the capture let the field hover"
                );
                assert!(!crosshair, "{name} {size:?}: the capture owns the cursor");
                assert!(
                    same_paint,
                    "{name} {size:?}: the pointer changed the painted frame"
                );
            } else {
                // The perturbation half: the same pointer does aim, hover and
                // paint a reticle readout outside a capture.
                assert_ne!(f.battle.aim, aim, "{name} {size:?}: the pointer aims");
                assert!(f.battle.arena_hover, "{name} {size:?}: the field hovers");
                assert!(crosshair, "{name} {size:?}: the field owns the cursor");
                assert!(!same_paint, "{name} {size:?}: the reticle readout paints");
            }
        }
    }
}

/// The music director's input (A6-15) is read off the screen and the shown
/// battle, and reading it changes nothing.
#[test]
fn music_input_follows_the_screen_and_the_battle() {
    use horde_audio::music::{MusicInput, MusicScreen};
    let mut fixture = IsolatedFrontend::new();
    {
        let f = &mut fixture.frontend;
        for (screen, tab, music) in [
            (Screen::Menu, Tab::Home, MusicScreen::Menu),
            (Screen::SurvivorSelection, Tab::Home, MusicScreen::Menu),
            (Screen::Camp, Tab::Home, MusicScreen::Camp),
            (Screen::Camp, Tab::Armory, MusicScreen::Camp),
            (Screen::Camp, Tab::Briefing, MusicScreen::Briefing),
            (Screen::Results, Tab::Briefing, MusicScreen::Results),
            (Screen::Settings, Tab::Home, MusicScreen::Overlay),
            (Screen::History, Tab::Home, MusicScreen::Overlay),
        ] {
            f.screen = screen;
            f.nav.tab = tab;
            assert_eq!(
                f.ctx().music_input(),
                MusicInput {
                    screen: music,
                    battle: None
                },
                "{screen:?} {tab:?}"
            );
        }
    }
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.screen = Screen::Battle;
    for _ in 0..3000 {
        if !f.game.attempt().unwrap().enemies.is_empty() {
            break;
        }
        f.game.tick(Input::default());
    }
    {
        let a = f.game.attempt_mut().unwrap();
        assert!(!a.enemies.is_empty(), "the first wave arrived");
        a.hp = a.max_hp / 2;
        let width = a.width_yards;
        for enemy in &mut a.enemies {
            enemy.hp = enemy.max_hp.max(1);
            enemy.boss = false;
            enemy.x = width;
        }
        a.enemies[0].x = width * 0.5;
        a.paused = false;
    }
    let before = serde_json::to_value(&f.game).unwrap();
    let a = f.game.attempt().unwrap();
    let (count, width, hp) = (
        a.enemies.len() as u32,
        a.width_yards,
        a.hp as f32 / a.max_hp as f32,
    );
    let input = f.ctx().music_input();
    assert_eq!(input.screen, MusicScreen::Battle);
    let battle = input.battle.expect("the battle is read");
    assert_eq!(battle.enemies_alive, count);
    assert_eq!(
        battle.nearest_x_frac,
        horde_audio::pan::lane_position(width * 0.5, width)
    );
    assert_eq!(battle.hp_frac, hp);
    assert!(!battle.boss_alive);
    assert_eq!(battle.paused, f.game.global_paused());
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);

    // A living boss, then a field of the dead.
    f.game.attempt_mut().unwrap().enemies[0].boss = true;
    assert!(f.ctx().music_input().battle.unwrap().boss_alive);
    {
        let a = f.game.attempt_mut().unwrap();
        for enemy in &mut a.enemies {
            enemy.hp = 0;
        }
        a.paused = true;
    }
    let battle = f.ctx().music_input().battle.unwrap();
    assert_eq!(battle.enemies_alive, 0);
    assert_eq!(battle.nearest_x_frac, 1.0);
    assert!(!battle.boss_alive);
    assert!(battle.paused);
    // The camp does not score a battle that runs elsewhere.
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
    assert_eq!(f.ctx().music_input().battle, None);
}
