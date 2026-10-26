use battle_core::{Battle, MapKind, SIM_HZ, ScenarioConfig, UnitCategory};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Serialize)]
struct Report {
    backend: &'static str,
    requested_units: u32,
    requested_ticks: u64,
    completed_ticks: u64,
    seed: u64,
    map: MapKind,
    blue_category: UnitCategory,
    red_category: UnitCategory,
    wall_seconds: f64,
    simulated_seconds: f32,
    ticks_per_wall_second: f64,
    tick_ms_p50: f64,
    tick_ms_p95: f64,
    tick_ms_max: f64,
    alive: [u32; 2],
    casualties: [u32; 2],
    last_neighbor_checks: u64,
    peak_cell_occupancy: usize,
    last_full_detail_units: u32,
    last_strategic_units: u32,
    peak_full_detail_units: u32,
    estimated_state_bytes: usize,
    initialization_seconds: f64,
    build_profile: &'static str,
    state_valid: bool,
    outcome: Option<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Fehler: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut units = 1_200_u32;
    let mut ticks = 600_u64;
    let mut seed = 42_u64;
    let mut map = MapKind::Meadow;
    let mut blue_category = UnitCategory::Creature;
    let mut red_category = UnitCategory::Creature;
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "battle-bench [--units GESAMT] [--ticks N] [--seed N] [--map meadow|valley|crossing] [--blue-category KATEGORIE] [--red-category KATEGORIE] [--output DATEI.json]\nCPU-Referenzbackend mit Simulations-LOD, höchstens 1.000.000 reale Einheiten. --units ist die Gesamtzahl beider Armeen.\nKategorien: creature, vehicle, building, air_unit, sea_unit, underground_unit, space_unit."
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Wert für {argument} fehlt"))?;
        match argument.as_str() {
            "--units" => {
                units = value
                    .parse()
                    .map_err(|_| "--units erwartet eine positive Ganzzahl")?
            }
            "--ticks" => {
                ticks = value
                    .parse()
                    .map_err(|_| "--ticks erwartet eine positive Ganzzahl")?
            }
            "--seed" => seed = value.parse().map_err(|_| "--seed erwartet eine Ganzzahl")?,
            "--map" => {
                map = match value.as_str() {
                    "meadow" => MapKind::Meadow,
                    "valley" => MapKind::Valley,
                    "crossing" => MapKind::Crossing,
                    _ => return Err("--map erwartet meadow, valley oder crossing".into()),
                }
            }
            "--output" => output = Some(value.into()),
            "--blue-category" => blue_category = parse_category(&value)?,
            "--red-category" => red_category = parse_category(&value)?,
            _ => return Err(format!("Unbekanntes Argument {argument}")),
        }
    }
    if units < 2 || ticks == 0 || ticks > 72_000 {
        return Err("Mindestens 2 Einheiten und 1 bis 72.000 Ticks wählen.".into());
    }
    let mut config = ScenarioConfig {
        seed,
        map,
        time_limit_secs: 3_600,
        ..Default::default()
    };
    config.blue.count = units / 2;
    config.red.count = units - config.blue.count;
    config.blue.category = blue_category;
    config.red.category = red_category;
    let initialization_started = Instant::now();
    let mut battle = Battle::new(config)?;
    let initialization_seconds = initialization_started.elapsed().as_secs_f64();
    let mut timings = Vec::with_capacity(ticks as usize);
    let mut peak_cell_occupancy = 0;
    let mut peak_full_detail_units = 0;
    let began = Instant::now();
    for _ in 0..ticks {
        if battle.outcome.is_some() {
            break;
        }
        battle.step();
        timings.push(battle.stats.step_ms as f64);
        peak_cell_occupancy = peak_cell_occupancy.max(battle.stats.max_cell_occupancy);
        peak_full_detail_units = peak_full_detail_units.max(battle.stats.full_detail_units);
    }
    let wall_seconds = began.elapsed().as_secs_f64();
    // Validate after stopping the clock so this independent scan cannot affect
    // either the per-tick samples or the measured simulation wall time.
    validate_final_state(&battle)?;
    timings.sort_by(f64::total_cmp);
    let percentile = |amount: f64| timings[((timings.len() - 1) as f64 * amount).round() as usize];
    let report = Report {
        backend: "CPU SoA / logical 20 Hz / camera-independent activity LOD / bounded candidates",
        requested_units: units,
        requested_ticks: ticks,
        completed_ticks: battle.stats.tick,
        seed,
        map,
        blue_category,
        red_category,
        wall_seconds,
        simulated_seconds: battle.stats.elapsed_secs,
        ticks_per_wall_second: battle.stats.tick as f64 / wall_seconds.max(f64::EPSILON),
        tick_ms_p50: percentile(0.5),
        tick_ms_p95: percentile(0.95),
        tick_ms_max: *timings.last().unwrap(),
        alive: battle.stats.alive,
        casualties: battle.stats.casualties,
        last_neighbor_checks: battle.stats.neighbor_checks,
        peak_cell_occupancy,
        last_full_detail_units: battle.stats.full_detail_units,
        last_strategic_units: battle.stats.strategic_units,
        peak_full_detail_units,
        estimated_state_bytes: battle.stats.estimated_state_bytes,
        initialization_seconds,
        build_profile: if cfg!(debug_assertions) {
            "development"
        } else {
            "release"
        },
        state_valid: true,
        outcome: battle.outcome.map(|outcome| outcome.label()),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    println!("{json}");
    if let Some(path) = output {
        std::fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())?;
    }
    eprintln!(
        "Logische Taktrate: {SIM_HZ:.0} Hz. Keine Grafikmessung oder Echtzeitgarantie; state_bytes schließt Render-/Snapshot-/Prozessspeicher aus. Kein 20-Millionen-Nachweis."
    );
    Ok(())
}

fn parse_category(value: &str) -> Result<UnitCategory, String> {
    match value {
        "creature" => Ok(UnitCategory::Creature),
        "vehicle" => Ok(UnitCategory::Vehicle),
        "building" => Ok(UnitCategory::Building),
        "air_unit" => Ok(UnitCategory::AirUnit),
        "sea_unit" => Ok(UnitCategory::SeaUnit),
        "underground_unit" => Ok(UnitCategory::UndergroundUnit),
        "space_unit" => Ok(UnitCategory::SpaceUnit),
        _ => Err(format!("Unbekannte Kategorie {value}; siehe --help.")),
    }
}

fn validate_final_state(battle: &Battle) -> Result<(), String> {
    let units = &battle.units;
    let expected_total = battle.config.total_units() as usize;
    for (name, length) in [
        ("positions", units.positions.len()),
        ("previous_positions", units.previous_positions.len()),
        ("velocities", units.velocities.len()),
        ("headings", units.headings.len()),
        ("teams", units.teams.len()),
        ("kinds", units.kinds.len()),
        ("categories", units.categories.len()),
        ("health", units.health.len()),
        ("alive", units.alive.len()),
        ("attack_flash", units.attack_flash.len()),
    ] {
        if length != expected_total {
            return Err(format!(
                "Zustandsprüfung nach Tick {}: {name} enthält {length} statt {expected_total} Einträge.",
                battle.stats.tick
            ));
        }
    }
    let mut counted_alive = [0_u32; 2];
    let mut counted_dead = [0_u32; 2];
    for i in 0..expected_total {
        for (name, value) in [
            ("position.x", units.positions[i].x),
            ("position.z", units.positions[i].y),
            ("previous_position.x", units.previous_positions[i].x),
            ("previous_position.z", units.previous_positions[i].y),
            ("velocity.x", units.velocities[i].x),
            ("velocity.z", units.velocities[i].y),
            ("heading", units.headings[i]),
            ("health", units.health[i]),
            ("attack_flash", units.attack_flash[i]),
        ] {
            if !value.is_finite() {
                return Err(format!(
                    "Zustandsprüfung nach Tick {}: Einheit {i}, {name} ist nicht endlich ({value}).",
                    battle.stats.tick
                ));
            }
        }
        if units.health[i] < 0.0 || units.alive[i] != (units.health[i] > 0.0) {
            return Err(format!(
                "Zustandsprüfung nach Tick {}: Einheit {i}, alive={} widerspricht health={}.",
                battle.stats.tick, units.alive[i], units.health[i]
            ));
        }
        let team = units.teams[i].index();
        if units.alive[i] {
            counted_alive[team] += 1;
        } else {
            counted_dead[team] += 1;
        }
    }
    let initial = [battle.config.blue.count, battle.config.red.count];
    for team in 0..2 {
        if counted_alive[team] != battle.stats.alive[team]
            || counted_dead[team] != battle.stats.casualties[team]
            || battle.stats.alive[team].checked_add(battle.stats.casualties[team])
                != Some(initial[team])
        {
            return Err(format!(
                "Zustandsprüfung nach Tick {}: Armee {team}: gezählt {} lebend / {} gefallen, Statistik {} / {}, Anfangsbestand {}.",
                battle.stats.tick,
                counted_alive[team],
                counted_dead[team],
                battle.stats.alive[team],
                battle.stats.casualties[team],
                initial[team]
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_detects_nonfinite_values_and_inconsistent_health() {
        let mut battle = Battle::new(ScenarioConfig::default()).unwrap();
        assert!(validate_final_state(&battle).is_ok());
        battle.units.velocities[0].x = f32::NAN;
        let error = validate_final_state(&battle).unwrap_err();
        assert!(error.contains("Einheit 0, velocity.x"));
        battle.units.velocities[0].x = 0.0;
        battle.units.health[0] = 0.0;
        assert!(
            validate_final_state(&battle)
                .unwrap_err()
                .contains("health")
        );
    }

    #[test]
    fn validation_detects_counter_and_storage_corruption() {
        let mut battle = Battle::new(ScenarioConfig::default()).unwrap();
        battle.stats.casualties[0] = 1;
        assert!(
            validate_final_state(&battle)
                .unwrap_err()
                .contains("Armee 0")
        );
        battle.stats.casualties[0] = 0;
        battle.units.headings.pop();
        assert!(
            validate_final_state(&battle)
                .unwrap_err()
                .contains("headings")
        );
    }
}
