//! Read-only colony ledger projections, separate from widgets and control ownership.
use super::{Action, Ledger, Tab};
use crate::IslandMode;
use bevy::prelude::*;
use npc_framework::prelude::{SimClock, SimIndex};
use npc_framework::view_helpers::Selected;
use testbed_content::village::{
    self, Role, VillageState, Villager, Vitality, Wallet, colony, terrain::TerrainSurface,
};

pub(super) struct Entry {
    pub(super) key: String,
    pub(super) text: String,
    pub(super) action: Option<Action>,
    pub(super) heading: bool,
}
impl Entry {
    fn text(key: &str, value: impl Into<String>, heading: bool) -> Self {
        Self {
            key: key.into(),
            text: value.into(),
            action: None,
            heading,
        }
    }
    fn action(key: impl Into<String>, label: impl Into<String>, action: Action) -> Self {
        Self {
            key: key.into(),
            text: label.into(),
            action: Some(action),
            heading: false,
        }
    }
}
pub(super) fn entries(world: &World) -> Vec<Entry> {
    let ledger = world.resource::<Ledger>();
    let summary = colony::summary(world);
    match ledger.tab {
        Tab::Overview => {
            let state = world.resource::<colony::ColonyState>();
            let village = world.resource::<VillageState>();
            let mut rows = vec![
                Entry::text("homes_title", "Homes & wellbeing", true),
                Entry::text(
                    "homes",
                    format!(
                        "{} residents · {} housed\n{} households · {} beds\nMorale {}% · {} social encounters",
                        summary.residents,
                        summary.housed,
                        summary.households,
                        summary.beds,
                        summary.average_morale / 1000,
                        summary.social_events
                    ),
                    false,
                ),
                Entry::text("stores_title", "Food & livelihoods", true),
                Entry::text(
                    "stores",
                    format!(
                        "{} food portions · {} clean water\n{} completed trades · {} community coins",
                        summary.food_units, summary.water_units, village.transactions, village.fund
                    ),
                    false,
                ),
                Entry::text(
                    "lines",
                    "Grain to flour to bread\nTimber to planks, bowls & tools\nOre to ingots & hunting equipment\nHides + fiber to leather & warm clothes",
                    false,
                ),
                Entry::text("policy_title", "Colony policies", true),
                Entry::text(
                    "policy",
                    format!("Household food reserve: {} days", state.food_reserve_days),
                    false,
                ),
            ];
            for days in 1..=4 {
                rows.push(Entry::action(
                    format!("reserve{days}"),
                    format!(
                        "Keep {days} day{} of food",
                        if days == 1 { "" } else { "s" }
                    ),
                    Action::Reserve(days),
                ));
            }
            rows.push(Entry::action(
                "gatherings",
                if state.gatherings {
                    "Evening gatherings: enabled"
                } else {
                    "Evening gatherings: disabled"
                },
                Action::Gatherings,
            ));
            rows.push(Entry::text("build_heading", "Grow the settlement", true));
            for (plot, project) in &state.projects {
                let Some(builder) = world.resource::<SimIndex>().get(project.builder) else {
                    continue;
                };
                let name = world
                    .get::<Villager>(builder)
                    .map_or("Resident", |person| person.name.as_str());
                let status =
                    if let Some(build) = world.get::<village::homestead::CottageBuild>(builder) {
                        format!(
                            "building · {} ticks remaining",
                            build.ends.saturating_sub(world.resource::<SimClock>().tick)
                        )
                    } else {
                        format!(
                            "supplies: {}/8 planks · {}/4 logs",
                            village::economy::count(world, builder, "plank"),
                            village::economy::count(world, builder, "log")
                        )
                    };
                rows.push(Entry::text(
                    &format!("project{}", plot.0),
                    format!("{name} · {status}"),
                    false,
                ));
                rows.push(Entry::action(
                    format!("cancel{}", plot.0),
                    "Cancel this housing commission",
                    Action::CancelHome(*plot),
                ));
            }
            rows.push(Entry::text(
                "projects",
                format!(
                    "{} active construction projects",
                    summary.construction_projects
                ),
                false,
            ));
            let mut builders: Vec<_> = world
                .resource::<SimIndex>()
                .map
                .iter()
                .filter_map(|(id, entity)| {
                    world
                        .get::<Villager>(*entity)
                        .filter(|person| {
                            person.role != Role::Visitor && village::living(world, *entity)
                        })
                        .map(|person| (person.role != Role::Carpenter, *id, person.name.as_str()))
                })
                .collect();
            builders.sort_by_key(|(priority, id, _)| (*priority, *id));
            for (plot_id, plot_entity) in village::homestead::plots(world) {
                let Some(plot) = world.get::<village::homestead::HomesteadPlot>(plot_entity) else {
                    continue;
                };
                if plot.cottage_built {
                    continue;
                }
                let Some(definition) = village::homestead::definition(world, plot_entity) else {
                    continue;
                };
                if definition.use_kind != "housing" {
                    continue;
                }
                if let Some((_, builder, name)) = builders.iter().find(|(_, builder, _)| {
                    colony::ColonyAct {
                        verb: "commission_home".into(),
                        resident: Some(*builder),
                        house: Some(plot_id),
                        value: 0,
                    }
                    .check(world)
                    .is_ok()
                }) {
                    rows.push(Entry::action(
                        format!("build{}", plot_id.0),
                        format!("Commission {} · {name}", definition.label),
                        Action::Commission(*builder, plot_id),
                    ));
                }
            }
            rows.push(Entry::text("building_cost", "Cottages require 8 planks and 4 logs. The named resident buys or makes the supplies and builds on site.", false));
            rows.push(Entry::text("scope", "Residents gather, craft and exchange real goods. Policies change their decisions; stock and coins remain finite.", false));
            rows
        }
        Tab::Residents => resident_entries(world),
        Tab::Landscape => {
            let mut rows = vec![
                Entry::text("landscape_title", "Shape the island", true),
                Entry::text(
                    "landscape_intro",
                    if *world.resource::<IslandMode>() == IslandMode::Editor {
                        "Click the world to paint. The outline previews the brush; a red outline means the edit is blocked."
                    } else {
                        "Choose Terraform above to paint the world. Inspection never changes terrain."
                    },
                    false,
                ),
                Entry::text(
                    "brush",
                    format!(
                        "Surface: {}\nElevation: {:.2} m\nRadius: {} m · Width: {} m · {} cells",
                        surface_name(ledger.surface),
                        ledger.height_mm as f32 / 1000.0,
                        ledger.radius,
                        ledger.radius * 2 + 1,
                        super::brush_cells(
                            ledger.cell.map_or(0, |(x, _)| x),
                            ledger.cell.map_or(0, |(_, z)| z),
                            ledger.radius
                        )
                        .len()
                    ),
                    false,
                ),
            ];
            for surface in [
                TerrainSurface::Grass,
                TerrainSurface::Soil,
                TerrainSurface::Stone,
                TerrainSurface::Water,
            ] {
                rows.push(Entry::action(
                    format!("surface{surface:?}"),
                    surface_name(surface),
                    Action::Surface(surface),
                ));
            }
            for (key, label, action) in [
                ("lower", "Lower ground 0.25 m", Action::Height(-250)),
                ("raise", "Raise ground 0.25 m", Action::Height(250)),
                ("smaller", "Smaller brush", Action::Radius(-1)),
                ("larger", "Larger brush", Action::Radius(1)),
            ] {
                rows.push(Entry::action(key, label, action));
            }
            rows.push(Entry::text(
                "cursor",
                ledger.cell.map_or_else(
                    || "Move the pointer over the island.".into(),
                    |(x, z)| format!("Cell {x}, {z}"),
                ),
                false,
            ));
            if let Some(error) = &ledger.preview_error {
                rows.push(Entry::text(
                    "guard",
                    format!("Cannot paint here: {error}"),
                    false,
                ));
            }
            rows.push(Entry::text("protection", "Homes, workplaces and occupied cells are protected. Edits cannot sever the village's routes. Terrain is stored with the world.", false));
            rows
        }
    }
}
fn surface_name(surface: TerrainSurface) -> &'static str {
    match surface {
        TerrainSurface::Grass => "Grass",
        TerrainSurface::Soil => "Soil",
        TerrainSurface::Stone => "Stone",
        TerrainSurface::Water => "Water",
    }
}
fn resident_entries(world: &World) -> Vec<Entry> {
    let mut rows = vec![Entry::text(
        "people_title",
        "The people of the island",
        true,
    )];
    if let Some(id) = world
        .get_resource::<Selected>()
        .and_then(|selected| selected.0)
        && let Some(entity) = world.resource::<SimIndex>().get(id)
        && let Some(person) = world.get::<Villager>(entity)
    {
        let activity = village::activity(world, entity).0;
        let health = world
            .get::<Vitality>(entity)
            .map_or(0, |life| life.health / 1000);
        let coins = world.get::<Wallet>(entity).map_or(0, |wallet| wallet.coins);
        let mut detail = format!(
            "{} · {}\n{} · Health {}% · {} coins\nFood {}% · Water {}% · Energy {}%",
            person.name,
            person.role.name(),
            activity,
            health,
            coins,
            village::economy::need(world, entity, "Hunger") / 1000,
            village::economy::need(world, entity, "Thirst") / 1000,
            village::economy::need(world, entity, "Energy") / 1000
        );
        if let Some(goal) = village::behavior::decision(world, entity) {
            detail.push_str(&format!("\nThinking: {goal}"));
        }
        if let Some(social) = world.get::<colony::SocialLife>(entity) {
            detail.push_str(&format!(
                "\nMorale {}% · {} relationships",
                social.morale / 1000,
                social.affinity.len()
            ));
        }
        rows.push(Entry::text("selected", detail, false));
        rows.push(Entry::text(
            "plan",
            village::goap::explain(world, entity),
            false,
        ));
        rows.push(Entry::text("housing_heading", "Assign a home", true));
        for (house_id, house_entity) in &world.resource::<SimIndex>().map {
            let Some(house) = world.get::<colony::Household>(*house_entity) else {
                continue;
            };
            let own = house.residents.contains(&id);
            if own {
                rows.push(Entry::text(
                    "current_home",
                    format!(
                        "Lives in {} ({}/{})",
                        house.label,
                        house.residents.len(),
                        house.capacity
                    ),
                    false,
                ));
            } else if house.residents.len() < house.capacity as usize {
                rows.push(Entry::action(
                    format!("house{}", house_id.0),
                    format!(
                        "Move into {} ({}/{})",
                        house.label,
                        house.residents.len(),
                        house.capacity
                    ),
                    Action::Home(id, *house_id),
                ));
            }
        }
        rows.push(Entry::text("roster_heading", "Resident roster", true));
    } else {
        rows.push(Entry::text(
            "select_hint",
            "Choose a resident below to inspect their needs, current work and plan.",
            false,
        ));
    }
    for (id, entity) in &world.resource::<SimIndex>().map {
        let Some(person) = world.get::<Villager>(*entity) else {
            continue;
        };
        if person.role == Role::Visitor {
            continue;
        }
        rows.push(Entry::action(
            format!("resident{}", id.0),
            format!(
                "{} · {} · {}",
                person.name,
                person.role.name(),
                village::activity(world, *entity).0
            ),
            Action::Resident(*id),
        ));
    }
    rows
}
