//! Headless probes and deterministic integration demonstrations.
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error, path::PathBuf, process::ExitCode};
use terra_core::{
    ActorKind, ActorState, GeoPosition, LocalFrame, TileKey, WeatherSample, WorldClock, season,
    solar_position, synthetic_weather,
};
use terra_sandbox::{Catalog, Command, FactionRole, SandboxState};
use terra_stream::{StreamConfig, fetch_weather, load_chunk};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const HELP: &str = "Terra World — headless geospatial and sandbox tools

USAGE
  terra-cli probe [--lat 48.137 --lon 11.575 --level 16] [--live]
                  [--no-osm] [--no-weather] [--strict] [--cache PATH]
                  [--output PATH]
  terra-cli simulate [--steps 120 --dt 0.05] [--output PATH]
  terra-cli sandbox-demo [--output PATH]

probe         Load one geographic chunk, print source/height/feature/weather JSON.
              Offline defaults are explicitly synthetic. --live enables terrain,
              OSM and current weather HTTP adapters. --strict requires --live
              and returns nonzero instead of accepting failed sources.
              --output writes a complete chunk + weather + provenance snapshot.
simulate      Deterministically advance a GPS actor, UTC clock and environment.
sandbox-demo  Exercise claim, gather, craft, build, home, farm, harvest, damage
              and two-player faction ownership with role-based building.
              --output writes a validated, reloadable sandbox save.

Examples
  cargo run -p terra-cli -- probe --lat 48.137 --lon 11.575
  cargo run -p terra-cli -- probe --live --no-osm --strict --output munich.json
  cargo run -p terra-cli -- sandbox-demo --output sandbox-save.json

Public endpoints are development inputs. See docs/DATA.md for terms,
coverage, attribution and production hosting considerations.
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("terra-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<()> {
    if arguments.is_empty() || arguments.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let command = &arguments[0];
    let mut options = Options::parse(&arguments[1..])?;
    let output = options.take("--output").map(PathBuf::from);
    let result = match command.as_str() {
        "probe" => probe(&mut options, output.as_deref())?,
        "simulate" => {
            let steps: u32 = options.number("--steps", 120)?;
            let dt: f64 = options.number("--dt", 0.05)?;
            options.finish()?;
            let report = simulate(steps, dt)?;
            if let Some(path) = output {
                write_json(&path, &report)?;
            }
            report
        }
        "sandbox-demo" => {
            options.finish()?;
            let (state, mut report) = sandbox_demo()?;
            if let Some(path) = output {
                state.save(&path)?;
                let restored = SandboxState::load_with_catalog(&path, &Catalog::builtin())?;
                if restored != state {
                    return Err("sandbox save roundtrip mismatch".into());
                }
                report["save"] = json!({"path":path, "roundtrip_verified":true});
            }
            report
        }
        _ => return Err(format!("unknown command {command:?}; use --help").into()),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[derive(Default)]
struct Options(BTreeMap<String, String>);

impl Options {
    fn parse(arguments: &[String]) -> Result<Self> {
        let mut result = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            let name = &arguments[index];
            if !name.starts_with("--") {
                return Err(format!("unexpected argument {name:?}").into());
            }
            let value =
                if ["--live", "--strict", "--no-osm", "--no-weather"].contains(&name.as_str()) {
                    "true".into()
                } else {
                    index += 1;
                    arguments
                        .get(index)
                        .filter(|value| !value.starts_with("--"))
                        .ok_or_else(|| format!("missing value for {name}"))?
                        .clone()
                };
            if result.0.insert(name.clone(), value).is_some() {
                return Err(format!("duplicate option {name}").into());
            }
            index += 1;
        }
        Ok(result)
    }
    fn take(&mut self, name: &str) -> Option<String> {
        self.0.remove(name)
    }
    fn flag(&mut self, name: &str) -> bool {
        self.take(name).is_some()
    }
    fn number<T: std::str::FromStr>(&mut self, name: &str, default: T) -> Result<T> {
        self.take(name)
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("invalid number for {name}: {value}").into())
            })
            .unwrap_or(Ok(default))
    }
    fn finish(&self) -> Result<()> {
        if let Some(key) = self.0.keys().next() {
            Err(format!("unknown option {key}; use --help").into())
        } else {
            Ok(())
        }
    }
}

fn probe(options: &mut Options, output: Option<&std::path::Path>) -> Result<Value> {
    let position = GeoPosition::new(
        options.number("--lat", 48.137)?,
        options.number("--lon", 11.575)?,
        0.0,
    )?;
    let level: u8 = options.number("--level", 16)?;
    if !(10..=19).contains(&level) {
        return Err(
            "probe --level must be 10..19 to keep provider requests local and bounded".into(),
        );
    }
    let live = options.flag("--live");
    let strict = options.flag("--strict");
    if strict && !live {
        return Err("--strict requires --live; offline data is intentionally synthetic".into());
    }
    let mut config = StreamConfig {
        live,
        allow_fallback: !strict,
        osm: !options.flag("--no-osm"),
        weather: !options.flag("--no-weather"),
        ..Default::default()
    };
    if let Some(path) = options.take("--cache") {
        config.cache_dir = path.into();
    }
    options.finish()?;
    config.validate()?;
    let key = TileKey::at(position, level)?;
    let chunk = load_chunk(key, &config)?;
    chunk.validate()?;
    let mut position = position;
    position.altitude_m = chunk
        .height_at(position)
        .ok_or("position not covered by returned chunk")? as f64;
    let mut warnings = chunk.warnings.clone();
    let clock = WorldClock::default();
    let weather = if live && config.weather {
        match fetch_weather(position, &config) {
            Ok(sample) => sample,
            Err(error) if !strict => {
                warnings.push(format!("WEATHER FALLBACK — synthetic sample: {error}"));
                synthetic_weather(&position, &clock, config.seed)
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        synthetic_weather(&position, &clock, config.seed)
    };
    let weather_source = if weather.synthetic {
        json!({"provider":"Terra illustrative weather", "synthetic":true, "time_basis":"fixed demo clock 2026-06-21T12:00:00Z"})
    } else {
        json!({"provider":"Open-Meteo", "synthetic":false, "license":"CC BY 4.0", "attribution":"Weather data by Open-Meteo.com", "url":config.weather_url})
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for feature in &chunk.features {
        *counts.entry(format!("{:?}", feature.kind)).or_default() += 1;
    }
    let mut report = json!({
        "format_version":1, "command":"probe", "live_requested":live, "strict":strict,
        "position":position, "tile":key, "bounds":key.bounds(),
        "height_range_m":chunk.heights.min_max(), "height_samples":[chunk.heights.width,chunk.heights.height],
        "feature_count":chunk.features.len(), "feature_counts":counts,
        "feature_examples":chunk.features.iter().take(8).collect::<Vec<_>>(),
        "sources":chunk.sources, "warnings":warnings, "weather":weather, "weather_source":weather_source,
        "entirely_synthetic_chunk":chunk.is_synthetic(),
        "precision_note":"Source heights may use orthometric datums; ellipsoidal conversion requires a geoid model. See source warnings."
    });
    if let Some(path) = output {
        let snapshot =
            json!({"format_version":1, "report":report, "chunk":chunk, "weather":weather});
        write_json(path, &snapshot)?;
        report["snapshot_path"] = json!(path);
    }
    Ok(report)
}

fn simulate(steps: u32, dt: f64) -> Result<Value> {
    if steps == 0 || steps > 1_000_000 || !dt.is_finite() || !(0.0001..=10.0).contains(&dt) {
        return Err("steps must be 1..1000000 and dt 0.0001..10 seconds".into());
    }
    let mut clock = WorldClock {
        time_scale: 1.0,
        ..Default::default()
    };
    let mut actor = ActorState {
        id: 1,
        kind: ActorKind::Human,
        position: GeoPosition::new(48.137, 11.575, 520.0)?,
        velocity_enu_m_s: [1.5, 0.0, 0.0],
        heading_deg: 90.0,
    };
    let start = actor.position;
    for _ in 0..steps {
        clock.advance(dt);
        actor.advance(dt)?;
    }
    let weather = synthetic_weather(&actor.position, &clock, 42);
    Ok(
        json!({"format_version":1,"command":"simulate","deterministic":true,"steps":steps,"step_seconds":dt,
        "simulated_seconds":steps as f64*dt,"clock":clock,"clock_label":clock.label(),"actor":actor,
        "distance_m":start.surface_distance_m(actor.position),"solar":solar_position(&actor.position,&clock),
        "season":season(&actor.position,&clock),"weather":weather,
        "scope":"Headless geospatial kinematics/environment model. Terrain, collisions and game AI are host systems."}),
    )
}

fn sandbox_demo() -> Result<(SandboxState, Value)> {
    let catalog = Catalog::builtin();
    catalog.validate()?;
    let mut state = SandboxState::new();
    let center = GeoPosition::new(48.137, 11.575, 520.0)?;
    state.add_player(1, center)?;
    let point = |east: f64, north: f64| -> Result<GeoPosition> {
        let mut position = LocalFrame::new(center).to_geo([east, 0.0, -north])?;
        position.altitude_m = center.altitude_m;
        Ok(position)
    };
    state.seed_resource("demo:wood".into(), point(0., 5.)?, "wood", 40, 30.0)?;
    state.seed_resource("demo:stone".into(), point(0., -5.)?, "stone", 30, 0.0)?;
    let mut actions = Vec::new();
    let mut act = |state: &mut SandboxState, command: Command| -> Result<Option<u64>> {
        let outcome = state.apply(1, command.clone(), &catalog)?;
        let id = outcome.created_id;
        actions.push(json!({"player_id":1,"command":command,"outcome":outcome}));
        Ok(id)
    };
    let plot = act(
        &mut state,
        Command::ClaimPlot {
            center,
            radius_m: 30.0,
        },
    )?
    .ok_or("claim returned no id")?;
    act(
        &mut state,
        Command::Gather {
            node_id: "demo:wood".into(),
            amount: 10,
        },
    )?;
    act(
        &mut state,
        Command::Gather {
            node_id: "demo:stone".into(),
            amount: 10,
        },
    )?;
    act(
        &mut state,
        Command::Craft {
            recipe: "planks".into(),
            count: 6,
        },
    )?;
    act(
        &mut state,
        Command::Craft {
            recipe: "stone_blocks".into(),
            count: 4,
        },
    )?;
    let house = act(
        &mut state,
        Command::Build {
            blueprint: "house".into(),
            position: point(8., 0.)?,
        },
    )?
    .ok_or("house returned no id")?;
    act(
        &mut state,
        Command::SetHome {
            structure_id: house,
        },
    )?;
    act(
        &mut state,
        Command::Build {
            blueprint: "workbench".into(),
            position: point(-5., 0.)?,
        },
    )?;
    act(
        &mut state,
        Command::Craft {
            recipe: "tool".into(),
            count: 1,
        },
    )?;
    let fence = act(
        &mut state,
        Command::Build {
            blueprint: "fence".into(),
            position: point(0., -10.)?,
        },
    )?
    .ok_or("fence returned no id")?;
    let crop = act(
        &mut state,
        Command::Plant {
            plot_id: plot,
            position: center,
            crop: "wheat".into(),
        },
    )?
    .ok_or("crop returned no id")?;
    let weather = WeatherSample {
        temperature_c: 20.0,
        ..Default::default()
    };
    state.advance(1_000.0, &weather)?;
    act(&mut state, Command::Harvest { farm_id: crop })?;
    act(
        &mut state,
        Command::Craft {
            recipe: "wheat_seed".into(),
            count: 1,
        },
    )?;
    act(
        &mut state,
        Command::Damage {
            structure_id: fence,
            amount: 25,
        },
    )?;
    act(
        &mut state,
        Command::Damage {
            structure_id: fence,
            amount: 25,
        },
    )?;
    // A second player can build on faction land only after accepting an
    // invitation and receiving builder permissions from the leader.
    drop(act);
    state.add_player(2, center)?;
    let mut shared_act = |player: u64, command: Command| -> Result<Option<u64>> {
        let outcome = state.apply(player, command.clone(), &catalog)?;
        let id = outcome.created_id;
        actions.push(json!({"player_id":player,"command":command,"outcome":outcome}));
        Ok(id)
    };
    let faction = shared_act(
        1,
        Command::CreateFaction {
            name: "Worldbuilders Cooperative".into(),
        },
    )?
    .ok_or("faction returned no id")?;
    shared_act(
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 2,
        },
    )?;
    shared_act(
        2,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
    )?;
    shared_act(
        1,
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 2,
            role: FactionRole::Builder,
        },
    )?;
    shared_act(
        1,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
    )?;
    shared_act(
        2,
        Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
    )?;
    shared_act(
        2,
        Command::Build {
            blueprint: "fence".into(),
            position: point(0., 10.)?,
        },
    )?;
    shared_act(
        2,
        Command::SetHome {
            structure_id: house,
        },
    )?;
    shared_act(
        1,
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 2,
            role: FactionRole::Member,
        },
    )?;
    drop(shared_act);
    state.validate_with_catalog(&catalog)?;
    let report = json!({"format_version":1,"command":"sandbox-demo","deterministic":true,
        "actions":actions,"final_player":state.players.get(&1),"owned_plots":state.plots.len(),
        "remaining_structures":state.structures,"farms_after_harvest":state.farms.len(),"resources":state.resource_nodes,
        "factions":state.factions,"second_player":state.players.get(&2),
        "simulated_growth_seconds":state.tick_seconds,
        "scope":"Authoritative domain rules on an immutable-world overlay; resource deposits and weather in this demonstration are authored/synthetic."});
    Ok((state, report))
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sandbox_demonstration_finishes_with_home_and_tool() {
        let (state, _) = sandbox_demo().unwrap();
        assert!(state.players[&1].home.is_some());
        assert_eq!(state.players[&1].inventory["tool"], 1);
        assert_eq!(state.players[&1].inventory["wheat_seed"], 10);
        assert_eq!(state.structures.len(), 3);
        assert_eq!(state.factions.len(), 1);
        assert!(state.players[&2].home.is_some());
        assert!(state.structures.values().all(|s| s.faction_owner.is_some()));
        assert!(state.farms.is_empty());
    }
    #[test]
    fn complete_demo_save_preserves_every_f64_coordinate_exactly() {
        let (state, _) = sandbox_demo().unwrap();
        let json = serde_json::to_vec_pretty(&state).unwrap();
        let restored: SandboxState = serde_json::from_slice(&json).unwrap();
        restored.validate_with_catalog(&Catalog::builtin()).unwrap();
        assert_eq!(restored, state);
    }
    #[test]
    fn repeated_simulation_is_deterministic_and_bad_steps_fail() {
        assert_eq!(simulate(120, 0.05).unwrap(), simulate(120, 0.05).unwrap());
        assert!(simulate(0, 0.05).is_err());
        assert!(simulate(120, f64::NAN).is_err());
    }
    #[test]
    fn cli_rejects_unknown_and_duplicate_arguments() {
        assert!(run(vec!["unknown".into()]).is_err());
        assert!(Options::parse(&["--lat".into()]).is_err());
        assert!(Options::parse(&["--live".into(), "--live".into()]).is_err());
        assert!(run(vec!["probe".into(), "--strict".into()]).is_err());
    }
}
