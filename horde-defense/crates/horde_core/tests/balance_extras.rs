//! Hard extras (W1-29, design-enemy-balance §7 and ruling E3, schema v7 §3.1
//! and §7): the catalogue `PresetExtras` on each balance preset, the saved
//! `Campaign.balance_extras_basis` that remembers which named preset a Custom
//! edit came from, its writer in `set_balance_settings`, the
//! `balance_extras_preset()` / `balance_extras()` accessors, the load
//! validation and the copies into worker games. `BalanceSettings` itself is
//! unchanged. Nothing reads the extras yet (the fold lands in wave 2).
mod support;
use horde_core::{
    balance::{BalanceCatalog, BalancePreset, BalanceSettings, PresetExtras, catalog},
    campaign::Campaign,
    command::Command,
    content::Mode,
    progression::catalog as progression_catalog,
    sim::{Game, ParallelBattle, SAVE_VERSION, SaveError},
    world::world,
};
use serde_json::{Value, json};
use support::{advance, enemy, strategic_game};

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");
const BASIS: &str = "balance_extras_basis";

fn hard_edit() -> BalanceSettings {
    let mut settings = BalanceSettings::for_preset(BalancePreset::Hard);
    settings.start_lanes = 5;
    settings
}

fn on(preset: BalancePreset) -> Campaign {
    let mut c = Campaign::default();
    c.set_balance_settings(BalanceSettings::for_preset(preset))
        .unwrap();
    c
}

fn extras(elite_rate_pct: u16, front_ramp_pct: u16, boss_phase_bonus: u8) -> PresetExtras {
    PresetExtras {
        elite_rate_pct,
        front_ramp_pct,
        boss_phase_bonus,
    }
}

#[test]
fn preset_extras_catalogue_values_default_and_bounds() {
    for (preset, expected) in [
        (BalancePreset::Easy, extras(50, 75, 0)),
        (BalancePreset::Normal, extras(100, 100, 0)),
        (BalancePreset::Hard, extras(150, 150, 1)),
    ] {
        let definition = catalog().presets.iter().find(|p| p.id == preset).unwrap();
        assert_eq!(definition.extras, expected, "{preset:?}");
        assert_eq!(on(preset).balance_extras(), expected, "{preset:?}");
    }
    assert_eq!(PresetExtras::default(), extras(100, 100, 0));
    // A catalogue without the key (every older LASTLIGHT_BALANCE override)
    // gives every preset Normal's extras.
    let mut bundled: Value = serde_json::from_str(include_str!("../balance.json")).unwrap();
    for preset in bundled["presets"].as_array_mut().unwrap() {
        preset.as_object_mut().unwrap().remove("extras");
    }
    let old = BalanceCatalog::from_json_str(&bundled.to_string()).unwrap();
    assert!(
        old.presets
            .iter()
            .all(|p| p.extras == PresetExtras::default())
    );
    // Each row starts from complete Hard extras, so it parses and only
    // `PresetExtras::validate` can reject it.
    let with_hard_extras = |field: &str, value: Value| {
        let mut catalogue = bundled.clone();
        catalogue["presets"][2]["extras"] =
            json!({"elite_rate_pct": 150, "front_ramp_pct": 150, "boss_phase_bonus": 1});
        catalogue["presets"][2]["extras"][field] = value;
        BalanceCatalog::from_json_str(&catalogue.to_string())
    };
    let ceiling = with_hard_extras("elite_rate_pct", json!(300)).unwrap();
    assert_eq!(ceiling.presets[2].extras, extras(300, 150, 1));
    let ceiling = with_hard_extras("front_ramp_pct", json!(300)).unwrap();
    assert_eq!(ceiling.presets[2].extras, extras(150, 300, 1));
    let ceiling = with_hard_extras("boss_phase_bonus", json!(2)).unwrap();
    assert_eq!(ceiling.presets[2].extras, extras(150, 150, 2));
    for (field, value) in [
        ("elite_rate_pct", 301),
        ("front_ramp_pct", 301),
        ("boss_phase_bonus", 3),
    ] {
        let error = with_hard_extras(field, json!(value))
            .err()
            .unwrap_or_else(|| panic!("{field} = {value} accepted"))
            .to_string();
        assert!(
            error.contains("extras exceed supported ranges"),
            "{field} = {value}: {error}"
        );
    }
    let error = with_hard_extras("elite_rate", json!(1))
        .expect_err("unknown extras key accepted")
        .to_string();
    assert!(error.contains("unknown field `elite_rate`"), "{error}");
}

#[test]
fn custom_edit_from_hard_keeps_hard_extras() {
    // Editing Hard's lanes normalises to Custom but keeps the Hard rules.
    let mut c = on(BalancePreset::Normal);
    c.set_balance_settings(hard_edit()).unwrap();
    assert_eq!(c.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(c.balance_extras_basis, BalancePreset::Hard);
    assert_eq!(c.balance_extras_preset(), BalancePreset::Hard);
    assert_eq!(c.balance_extras(), extras(150, 150, 1));
    // A Custom request made while on Hard keeps Hard as well.
    let mut c = on(BalancePreset::Hard);
    let mut custom = hard_edit();
    custom.preset = BalancePreset::Custom;
    c.set_balance_settings(custom).unwrap();
    assert_eq!(c.balance_extras_preset(), BalancePreset::Hard);
    let saved = serde_json::to_value(&c).unwrap();
    assert_eq!(saved[BASIS], json!("Hard"));
    let loaded: Campaign = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded, c);
    loaded.validate().unwrap();
}

/// `balance_outcome` is the pure preview of `set_balance_settings`: the
/// settings screen uses it to tell whether a draft would change anything.
#[test]
fn balance_outcome_previews_the_normalised_settings_and_basis_without_writing() {
    let c = on(BalancePreset::Normal);
    let before = c.clone();
    let (settings, basis) = c.balance_outcome(hard_edit()).unwrap();
    assert_eq!(c, before);
    let mut expected = hard_edit();
    expected.preset = BalancePreset::Custom;
    assert_eq!((settings, basis), (expected, BalancePreset::Hard));
    let mut applied = c.clone();
    applied.set_balance_settings(hard_edit()).unwrap();
    // The same edit previewed after it was applied is a no-op.
    assert_eq!(
        applied.balance_outcome(hard_edit()).unwrap(),
        (
            applied.balance_settings.clone(),
            applied.balance_extras_basis
        )
    );
    // The same values from another named preset move the basis.
    let mut from_easy = hard_edit();
    from_easy.preset = BalancePreset::Easy;
    assert_eq!(
        applied.balance_outcome(from_easy).unwrap().1,
        BalancePreset::Easy
    );
    assert_eq!(
        c.balance_outcome(BalanceSettings::for_preset(BalancePreset::Hard))
            .unwrap(),
        (
            BalanceSettings::for_preset(BalancePreset::Hard),
            BalancePreset::Normal
        )
    );
    let mut bad = hard_edit();
    bad.enemy_speed = f32::NAN;
    assert!(c.balance_outcome(bad).is_err());
}

/// R21 (wave-1 close): the Custom chip over an unedited named preset keeps
/// that preset as the basis. It is sent as the preset and then as Custom
/// with the same values, and the preview equals what applying those steps
/// through `Game::apply` stores. Any other draft is one step.
#[test]
fn custom_chip_over_an_unedited_preset_keeps_it_as_the_basis() {
    use horde_core::command::Command;
    let named = [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ];
    for committed in named {
        for chosen in named {
            let mut game = Game::default();
            game.apply(&Command::SetBalance(BalanceSettings::for_preset(committed)))
                .unwrap();
            let draft = BalanceSettings::for_preset(chosen);
            let custom = BalanceSettings {
                preset: BalancePreset::Custom,
                ..draft.clone()
            };
            let steps = Campaign::custom_balance_steps(&draft);
            assert_eq!(steps, vec![draft.clone(), custom.clone()]);
            let preview = game.campaign().custom_balance_outcome(draft).unwrap();
            for settings in steps {
                game.apply(&Command::SetBalance(settings)).unwrap();
            }
            let c = game.campaign();
            assert_eq!(
                preview,
                (c.balance_settings.clone(), c.balance_extras_basis),
                "{committed:?} -> {chosen:?}"
            );
            assert_eq!(c.balance_settings, custom);
            assert_eq!(c.balance_extras_preset(), chosen);
        }
    }
    // An edited named draft already normalises to Custom on its own preset,
    // and a Custom draft keeps the committed basis: one step each.
    let c = on(BalancePreset::Normal);
    let mut custom_edit = hard_edit();
    custom_edit.preset = BalancePreset::Custom;
    for draft in [hard_edit(), custom_edit] {
        assert_eq!(Campaign::custom_balance_steps(&draft), vec![draft.clone()]);
        assert_eq!(
            c.custom_balance_outcome(draft.clone()),
            c.balance_outcome(draft)
        );
    }
    let mut bad = hard_edit();
    bad.enemy_speed = f32::NAN;
    assert!(c.custom_balance_outcome(bad).is_err());
}

#[test]
fn returning_to_a_named_preset_resets_basis_to_normal_and_skips_the_key() {
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        let mut c = on(BalancePreset::Normal);
        c.set_balance_settings(hard_edit()).unwrap();
        assert_eq!(c.balance_extras_basis, BalancePreset::Hard);
        c.set_balance_settings(BalanceSettings::for_preset(preset))
            .unwrap();
        assert_eq!(c.balance_settings.preset, preset);
        assert_eq!(c.balance_extras_basis, BalancePreset::Normal, "{preset:?}");
        assert_eq!(c.balance_extras_preset(), preset);
        let saved = serde_json::to_value(&c).unwrap();
        assert!(saved.get(BASIS).is_none(), "{preset:?} wrote the basis");
    }
    // A fresh campaign never writes the key.
    assert!(
        serde_json::to_value(Campaign::default())
            .unwrap()
            .get(BASIS)
            .is_none()
    );
}

#[test]
fn custom_to_custom_edit_keeps_the_existing_basis() {
    let mut c = on(BalancePreset::Easy);
    let mut edit = BalanceSettings::for_preset(BalancePreset::Easy);
    edit.supply = 7;
    c.set_balance_settings(edit).unwrap();
    assert_eq!(c.balance_extras_basis, BalancePreset::Easy);
    for supply in [8, 9] {
        let mut again = c.balance_settings.clone();
        again.supply = supply;
        assert_eq!(again.preset, BalancePreset::Custom);
        c.set_balance_settings(again).unwrap();
        assert_eq!(c.balance_settings.supply, supply);
        assert_eq!(c.balance_extras_basis, BalancePreset::Easy);
        assert_eq!(c.balance_extras(), extras(50, 75, 0));
    }
    // A rejected edit changes neither the settings nor the basis.
    let before = c.clone();
    let mut bad = c.balance_settings.clone();
    bad.enemy_speed = f32::NAN;
    assert!(c.set_balance_settings(bad).is_err());
    assert_eq!(c, before);
}

fn load(value: Value) -> Result<Game, SaveError> {
    Game::from_payload(value, SAVE_VERSION)
}

#[test]
fn load_rejects_custom_basis_and_named_preset_with_non_normal_basis() {
    let mut g = Game::default();
    let mut custom = BalanceSettings::for_preset(BalancePreset::Hard);
    custom.supply = 1;
    g.campaign_mut().set_balance_settings(custom).unwrap();
    let base = serde_json::to_value(&g).unwrap();
    assert_eq!(base["campaign"][BASIS], json!("Hard"));
    load(base.clone()).unwrap_or_else(|e| panic!("Custom with a Hard basis: {e}"));
    let named = {
        let mut value = base.clone();
        value["campaign"]["balance_settings"] =
            serde_json::to_value(BalanceSettings::for_preset(BalancePreset::Hard)).unwrap();
        value
    };
    let mut normal = named.clone();
    normal["campaign"].as_object_mut().unwrap().remove(BASIS);
    load(normal).unwrap_or_else(|e| panic!("Hard with no basis: {e}"));
    let mut rows = Vec::new();
    let mut custom_basis = base.clone();
    custom_basis["campaign"][BASIS] = json!("Custom");
    rows.push(("Custom basis", custom_basis));
    for basis in ["Easy", "Hard"] {
        let mut value = named.clone();
        value["campaign"][BASIS] = json!(basis);
        rows.push(("named preset with a basis", value));
    }
    for (name, value) in rows {
        match load(value) {
            Err(SaveError::Invalid(message)) if message.contains("extras basis") => {}
            other => panic!("{name}: {:?}", other.err()),
        }
    }
}

/// A strategic game whose Custom settings came from a Hard edit, keeping
/// the legacy geometry so the support helpers' lanes stay valid.
fn strategic_on_hard_custom() -> (Game, String, u64) {
    let mut g = strategic_game();
    let mut settings = g.campaign().balance_settings.clone();
    settings.preset = BalancePreset::Hard;
    g.campaign_mut().set_balance_settings(settings).unwrap();
    assert_eq!(g.campaign().balance_extras_basis, BalancePreset::Hard);
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    (g, country, id)
}

fn battle(g: &Game, id: u64) -> &ParallelBattle {
    g.battles().iter().find(|b| b.id == id).unwrap()
}

#[test]
fn worker_clone_and_operation_chain_copy_the_extras_basis() {
    // clone_for_worker: the expedition's campaign keeps the Hard basis.
    let (mut g, country, id) = strategic_on_hard_custom();
    let b = g.deploy_survivor(id, &country, 0).unwrap();
    let worker = battle(&g, b).game().campaign();
    assert_eq!(worker.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(worker.balance_extras_basis, BalancePreset::Hard);
    assert_eq!(worker.balance_extras(), extras(150, 150, 1));
    load(serde_json::to_value(&g).unwrap()).unwrap();

    // The operation-chain sync copies the basis with the settings it copies.
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let target = world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
        .clone();
    let operation = g.deploy_country_operation(id, &target.id).unwrap();
    assert_eq!(
        battle(&g, operation).game().campaign().balance_extras_basis,
        BalancePreset::Normal
    );
    let mut settings = g.campaign().balance_settings.clone();
    settings.preset = BalancePreset::Hard;
    g.campaign_mut().set_balance_settings(settings).unwrap();
    let b = g
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == operation)
        .unwrap();
    enemy(b.game_mut(), 5, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    let b = battle(&g, operation);
    assert!(!b.settled && b.stage == 1);
    let worker = b.game().campaign();
    assert_eq!(worker.balance_settings, g.campaign().balance_settings);
    assert_eq!(worker.balance_extras_basis, BalancePreset::Hard);
    assert_eq!(worker.balance_extras_preset(), BalancePreset::Hard);
    load(serde_json::to_value(&g).unwrap()).unwrap();
}

fn v6() -> Game {
    Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture loads")
}

#[test]
fn v6_hard_campaign_loads_as_hard_with_normal_basis() {
    let g = v6();
    let c = g.campaign();
    assert_eq!(c.balance_settings.preset, BalancePreset::Hard);
    assert_eq!(c.balance_extras_basis, BalancePreset::Normal);
    assert_eq!(c.balance_extras_preset(), BalancePreset::Hard);
    assert_eq!(c.balance_extras(), extras(150, 150, 1));
    for b in g.battles() {
        let worker = b.game().campaign();
        assert_eq!(worker.balance_extras_basis, BalancePreset::Normal);
        assert_eq!(
            worker.balance_extras_preset(),
            worker.balance_settings.preset
        );
    }
    let saved = serde_json::to_value(&g).unwrap();
    assert!(saved["campaign"].get(BASIS).is_none());
}

/// Pin (W4-08, design-save-determinism §4 "Hard preset after upgrade"):
/// `PresetExtras` live in the catalogue, so the v6 Hard campaign's next
/// deployment freezes the current Hard threat (the Hard-scaled elite rate
/// and the Hard boss-phase bonus at its rank), while the attempt and the
/// expedition that were in flight when the v6 save was written keep their
/// legacy snapshot (`elite_rate_bp` 0, no boss phase) through play and a
/// save round trip. Wave 4 changed no named preset's `BalanceSettings`, so
/// the loaded settings are the catalogue's Hard and no
/// `follow_named_preset` reconcile exists.
#[test]
fn v6_hard_campaign_deploys_with_current_hard_threat_but_in_flight_snapshot_stays_legacy() {
    let mut g = v6();
    let hard = PresetExtras::for_preset(BalancePreset::Hard);
    assert_eq!(
        g.campaign().balance_settings,
        BalanceSettings::for_preset(BalancePreset::Hard),
        "wave 4 changed no Hard BalanceSettings value"
    );
    let live = g.attempt().expect("the v6 fixture has a live attempt");
    let (stage, frozen) = (live.stage, live.progression.clone());
    assert!(
        frozen.rank >= 3,
        "the legacy snapshot is past the elite gate"
    );
    assert_eq!(frozen.elite_rate_bp, 0, "in-flight root snapshot");
    assert_eq!(frozen.boss_phases, 0, "in-flight root snapshot");
    let expeditions = |g: &Game| {
        g.battles()
            .iter()
            .filter_map(|b| b.game().attempt())
            .map(|a| a.progression.clone())
            .collect::<Vec<_>>()
    };
    let in_flight = expeditions(&g);
    assert!(
        !in_flight.is_empty(),
        "the v6 fixture has a live expedition"
    );
    for p in &in_flight {
        assert_eq!((p.elite_rate_bp, p.boss_phases), (0, 0), "{p:?}");
    }

    advance(&mut g, 120);
    let mut g = load(serde_json::to_value(&g).unwrap()).unwrap();
    let a = g.attempt().expect("the in-flight attempt is still live");
    assert_eq!(a.progression, frozen, "a live snapshot is never refreshed");
    for (now, then) in expeditions(&g).iter().zip(&in_flight) {
        assert_eq!(now, then, "an expedition snapshot is never refreshed");
    }

    g.apply(&Command::Withdraw).unwrap();
    g.apply(&Command::Deploy {
        stage,
        mode: Mode::Personal,
    })
    .unwrap();
    let next = &g.attempt().expect("the next deployment").progression;
    let base = progression_catalog().elite.bp(next.rank);
    assert!(base > 0, "rank {} is past the elite gate", next.rank);
    let scaled = (u64::from(base) * u64::from(hard.elite_rate_pct) / 100).min(10_000) as u32;
    assert_eq!(next.elite_rate_bp, scaled, "the current Hard elite rate");
    assert!(next.elite_rate_bp > base, "Hard scales the elite rate up");
    assert!(
        next.boss_phases > hard.boss_phase_bonus,
        "past rank 4 the rank term and the Hard bonus both add a phase: {}",
        next.boss_phases
    );
}

/// Pin: `BalanceSettings` gains no field. A v6 save's Hard settings object
/// has exactly the keys `for_preset(Hard)` writes and parses equal to it.
#[test]
fn old_balance_settings_json_equals_for_preset() {
    let envelope: Value = serde_json::from_str(FIXTURE_V6).unwrap();
    let payload: Value = serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    let old = &payload["campaign"]["balance_settings"];
    assert_eq!(old["preset"], json!("Hard"));
    let keys = |value: &Value| {
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    let hard = BalanceSettings::for_preset(BalancePreset::Hard);
    assert_eq!(keys(old), keys(&serde_json::to_value(&hard).unwrap()));
    assert_eq!(
        serde_json::from_value::<BalanceSettings>(old.clone()).unwrap(),
        hard
    );
    let mut extra = old.clone();
    extra["extras"] = json!({});
    assert!(serde_json::from_value::<BalanceSettings>(extra).is_err());
}
