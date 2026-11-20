//! Replay an existing owned walking intent on its exact prepared source.
//! Usage: cargo run -p terra-net --example walking_replay -- EXPORT.json SOURCE.json UNIT_ID
//! Reads a complete regional export; never writes it or publishes private state.
//! JSON output contains route measurements and checks, not accounts/capabilities.
#[cfg(feature = "reference-server")]
#[allow(dead_code)]
#[path = "support/prepared_session_surface.rs"]
mod prepared_session_surface;

#[cfg(not(feature = "reference-server"))]
fn main() {
    eprintln!("walking_replay requires reference-server (enabled by default)");
    std::process::exit(2);
}

#[cfg(feature = "reference-server")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run::main()
}

#[cfg(feature = "reference-server")]
mod run {
    use crate::prepared_session_surface::PreparedSessionWorld;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::{collections::BTreeMap, error::Error, path::Path};
    use terra_core::{ActorKind, GeoPosition};
    use terra_net::WorldEnvironment;
    use terra_sandbox::{Catalog, Inventory, LivingContacts, SandboxState, UnitOrder};

    #[derive(Deserialize)]
    struct Export {
        format_version: u32,
        source_generation: u64,
        identity: Value,
        catalog: Catalog,
        state: SandboxState,
    }

    fn route_length(from: GeoPosition, path: &[GeoPosition]) -> f64 {
        let mut previous = from;
        path.iter()
            .map(|point| {
                let distance = previous.surface_distance_m(*point);
                previous = *point;
                distance
            })
            .sum()
    }

    fn goods(state: &SandboxState) -> BTreeMap<String, u64> {
        let mut totals = BTreeMap::new();
        let mut add = |stock: &Inventory| {
            for (item, quantity) in stock {
                if *quantity > 0 {
                    *totals.entry(item.clone()).or_default() += u64::from(*quantity);
                }
            }
        };
        for player in state.players.values() {
            add(&player.inventory);
        }
        for unit in state.living.units.values() {
            add(&unit.supplies);
        }
        for site in state.living.producers.values() {
            add(&site.stock);
        }
        for shop in state.living.shops.values() {
            add(&shop.stock);
        }
        for vehicle in state.transport.vehicles.values() {
            add(&vehicle.cargo);
        }
        for assignment in state
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
        {
            add(&assignment.reserved_cargo);
        }
        for company in state.transport.companies.values() {
            add(&company.household_consumed);
            for depot in &company.depots {
                add(&depot.stock);
            }
        }
        for settlement in state.settlements.territories.values() {
            add(&settlement.stock);
        }
        totals
    }

    fn cash(state: &SandboxState) -> u64 {
        state.transport.treasury
            + state.living.accounts.values().map(|a| a.money).sum::<u64>()
            + state
                .living
                .units
                .values()
                .map(|u| u.pocket_money)
                .sum::<u64>()
            + state.living.shops.values().map(|s| s.money).sum::<u64>()
            + state
                .transport
                .jobs
                .values()
                .filter_map(|j| j.assignment.as_ref())
                .map(|a| a.reward_escrow)
                .sum::<u64>()
            + state
                .transport
                .companies
                .values()
                .map(|c| {
                    c.cash
                        + c.departed_wages
                        + c.drivers
                            .iter()
                            .map(|d| {
                                d.wages_earned
                                    + d.trip.as_ref().map_or(0, |t| {
                                        t.reward_escrow
                                            + t.goods_payment.as_ref().map_or(0, |p| p.total)
                                    })
                            })
                            .sum::<u64>()
                })
                .sum::<u64>()
            + state
                .settlements
                .territories
                .values()
                .map(|s| s.treasury)
                .sum::<u64>()
            + state
                .settlements
                .contracts
                .values()
                .map(|c| c.reward)
                .sum::<u64>()
    }

    pub fn main() -> Result<(), Box<dyn Error>> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() != 3 {
            return Err("Usage: walking_replay EXPORT.json SOURCE.json UNIT_ID".into());
        }
        let id: u64 = args[2]
            .to_str()
            .ok_or("unit ID must be UTF-8 digits")?
            .parse()?;
        let original_bytes = std::fs::read(&args[0])?;
        let export: Export = serde_json::from_slice(&original_bytes)?;
        if export.format_version != 1 {
            return Err("only complete version-one regional exports are supported".into());
        }
        let source = PreparedSessionWorld::open(Path::new(&args[1]))?;
        if export.identity["world"]["base_data_revision"].as_str() != Some(source.revision.as_str())
            || export.identity["world"]["chunk_level"].as_u64() != Some(15)
            || export.identity["world"]["terrain_resolution"].as_u64() != Some(65)
        {
            return Err("prepared source identity differs from the exported world".into());
        }
        let mut state = export.state;
        state.validate_with_catalog(&export.catalog)?;
        if state.is_partial() {
            return Err("walking replay requires an assembled complete world".into());
        }
        let unit = state
            .living
            .units
            .get(&id)
            .ok_or("walking unit not found")?;
        let player = unit.owner.ok_or("walking unit must have an owner")?;
        if !matches!(
            unit.kind,
            ActorKind::Human | ActorKind::Creature | ActorKind::Animal
        ) {
            return Err("selected unit must be an owned ground walker".into());
        }
        let UnitOrder::Move { destination } = unit.order else {
            return Err("selected unit must already have a Move order; no goal is invented".into());
        };
        let start = unit.position;
        let old_length = route_length(start, &unit.path[unit.path_index..]);
        let initial_goods = goods(&state);
        let initial_cash = cash(&state);
        let initial_player = state.players[&player].clone();
        let initial_account = state
            .living
            .accounts
            .get(&player)
            .ok_or("walking owner must have a living account")?
            .clone();
        let resolver = |kind, from, point| {
            if !source.can_enter(point) {
                return None;
            }
            source
                .resolve_movement(kind, from, point)
                .filter(|supported| source.can_enter(*supported))
        };
        let queries = std::cell::Cell::new(0usize);
        state.move_units_with_environment(
            player,
            vec![id],
            destination,
            &export.catalog,
            |kind, from, point| {
                queries.set(queries.get() + 1);
                resolver(kind, from, point)
            },
        )?;
        let planned = &state.living.units[&id];
        let new_length = route_length(start, &planned.path);
        let planning_kept_position = planned.position == start;
        let UnitOrder::Move {
            destination: admitted_goal,
        } = planned.order
        else {
            return Err("planning unexpectedly changed the order kind".into());
        };
        let goal_unchanged = admitted_goal.surface_distance_m(destination) < 1e-6
            && (admitted_goal.altitude_m - destination.altitude_m).abs() < 1e-6;
        let waypoints = planned.path.len();
        let planning_kept_principal = state.players[&player] == initial_player
            && state.living.accounts[&player].money == initial_account.money
            && state.living.accounts[&player].active_unit == initial_account.active_unit;
        let mut restored: Option<SandboxState> = None;
        let mut travelled = 0.;
        let mut largest_step = 0_f64;
        let mut source_admitted = true;
        let mut body_admitted = true;
        let mut conserved = true;
        let mut resume_identical = true;
        let mut steps = 0;
        const DT: f64 = 0.1;
        const MAX_STEPS: usize = 1200;
        while steps < MAX_STEPS && !matches!(state.living.units[&id].order, UnitOrder::Idle) {
            let previous = state.living.units[&id].clone();
            let contacts = LivingContacts::new(&state)?;
            state.advance_living_with_environment(DT, 0., &export.catalog, resolver)?;
            let current = &state.living.units[&id];
            let moved = previous.position.surface_distance_m(current.position);
            largest_step = largest_step.max(moved);
            travelled += moved;
            steps += 1;
            source_admitted &= resolver(current.kind, previous.position, current.position)
                .is_some_and(|p| {
                    p.surface_distance_m(current.position) < 1e-6
                        && (p.altitude_m - current.position.altitude_m).abs() < 1e-6
                });
            body_admitted &= contacts
                .resolve(
                    id,
                    previous.position,
                    previous.heading_deg,
                    current.position,
                    current.heading_deg,
                    current.velocity_enu_m_s,
                )?
                .contact
                .is_none();
            conserved &= goods(&state) == initial_goods && cash(&state) == initial_cash;
            if let Some(restored) = &mut restored {
                restored.advance_living_with_environment(DT, 0., &export.catalog, resolver)?;
                resume_identical &= *restored == state;
            } else if steps == 5 {
                restored = Some(serde_json::from_slice(&serde_json::to_vec(&state)?)?);
                resume_identical &= restored.as_ref() == Some(&state);
            }
        }
        state.validate_with_catalog(&export.catalog)?;
        let unit = &state.living.units[&id];
        let goal_distance = unit.position.surface_distance_m(destination);
        let arrived = matches!(unit.order, UnitOrder::Idle) && goal_distance < 1.5;
        let input_unchanged = std::fs::read(&args[0])? == original_bytes;
        let checks = [
            planning_kept_position,
            planning_kept_principal,
            goal_unchanged,
            new_length + 2. < old_length,
            queries.get() <= 16_384,
            waypoints <= 129,
            largest_step <= 0.451,
            source_admitted,
            body_admitted,
            conserved,
            restored.is_some() && resume_identical,
            arrived,
            input_unchanged,
        ];
        let passed = checks.into_iter().all(|check| check);
        let report = json!({
            "status": if passed { "passed" } else { "failed" },
            "release": env!("CARGO_PKG_VERSION"),
            "snapshot_generation": export.source_generation,
            "snapshot_bytes": original_bytes.len(),
            "source_revision": source.revision,
            "loaded_source_tiles": source.chunks.len(),
            "unit_id": id,
            "original_remaining_route_m": old_length,
            "destination_distance_m": start.surface_distance_m(destination),
            "planned_route_m": new_length,
            "planned_waypoints": waypoints,
            "source_queries": queries.get(),
            "physical_seconds": steps as f64 * DT,
            "physical_distance_m": travelled,
            "maximum_step_m": largest_step,
            "arrival_distance_m": goal_distance,
            "checks": {
                "input_unchanged": input_unchanged,
                "planning_keeps_position": planning_kept_position,
                "planning_keeps_principal_and_money": planning_kept_principal,
                "command_destination_unchanged": goal_unchanged,
                "strict_route_improvement": new_length + 2. < old_length,
                "bounded_queries_and_waypoints": queries.get() <= 16_384 && waypoints <= 129,
                "bounded_physical_steps": largest_step <= 0.451,
                "source_admission_every_step": source_admitted,
                "body_admission_every_step": body_admitted,
                "goods_and_cash_conserved_every_step": conserved,
                "save_resume_exact_every_step_after_half_second": restored.is_some() && resume_identical,
                "physical_arrival": arrived,
            },
            "scope": "Exact exported intent and source; ordinary living motion with calendar delta zero. In-memory save/resume and a 120-second physical bound. No source/actor/ownership edits or private-state output; no global navigation guarantee."
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !passed {
            return Err("walking replay did not meet every declared check".into());
        }
        Ok(())
    }
}
