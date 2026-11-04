//! Presentation assembled exclusively from the played actor's shared observation.
use std::collections::BTreeMap;

use bevy::prelude::*;
use npc_framework::prelude::{SimId, SimIndex, SimPos};
use serde_json::{Value, json};
use testbed_content::village::{VillageState, catalog, homestead};

use crate::player_actions::{self, ControlState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Land,
    Supplies,
    Living,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Toggle,
    Close,
    Take,
    Release,
    Stop,
    Resume,
    Tab(Tab),
    Quantity(i32),
    Plot(u32),
    Bed(u32),
    Walk(SimPos),
    Work {
        verb: String,
        target: Option<u32>,
        args: Value,
        approach: Option<SimPos>,
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub key: String,
    pub text: String,
    pub action: Option<Action>,
    pub enabled: bool,
    pub selected: bool,
    pub heading: bool,
}

impl Entry {
    fn note(key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: text.into(),
            action: None,
            enabled: true,
            selected: false,
            heading: false,
        }
    }
    fn heading(key: &str, text: &str) -> Self {
        Self {
            heading: true,
            ..Self::note(key, text)
        }
    }
    fn button(
        key: impl Into<String>,
        text: impl Into<String>,
        action: Action,
        enabled: bool,
    ) -> Self {
        Self {
            action: Some(action),
            enabled,
            ..Self::note(key, text)
        }
    }
}

pub(super) fn friendly(kind: &str) -> String {
    kind.strip_prefix("village_")
        .unwrap_or(kind)
        .replace('_', " ")
}

pub(super) fn position(value: &Value) -> Option<SimPos> {
    let (x, z) = if let Some(array) = value.as_array() {
        (array.first()?.as_i64()?, array.get(1)?.as_i64()?)
    } else {
        (value.get("x")?.as_i64()?, value.get("z")?.as_i64()?)
    };
    Some(SimPos::new(i32::try_from(x).ok()?, i32::try_from(z).ok()?))
}

pub(super) fn items(observation: &Value) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for item in array(&observation["sections"]["inventory"]["items"]) {
        if let (Some(kind), Some(count)) = (item["kind"].as_str(), item["stack"].as_u64()) {
            *totals.entry(kind.to_owned()).or_default() += count;
        }
    }
    totals
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}
fn id(value: &Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}
fn work(
    verb: &str,
    target: Option<u32>,
    args: Value,
    approach: Option<SimPos>,
    label: &str,
) -> Action {
    Action::Work {
        verb: verb.into(),
        target,
        args,
        approach,
        label: label.into(),
    }
}

pub(super) struct Context<'a> {
    pub world: &'a World,
    pub observation: &'a Value,
    pub tab: Tab,
    pub plot: Option<u32>,
    pub bed: u32,
    pub quantity: u32,
    pub ready: bool,
}

pub(super) fn entries(context: Context<'_>) -> Vec<Entry> {
    let Context {
        world,
        observation: obs,
        tab,
        plot,
        bed,
        quantity,
        ready,
    } = context;
    let stock = items(obs);
    let amount = |kind: &str| stock.get(kind).copied().unwrap_or(0);
    let village = &obs["sections"]["village"];
    let mut rows = Vec::new();
    match tab {
        Tab::Land => {
            rows.push(Entry::note("land-help", "Choose a discovered plot. Work buttons walk you to its door or selected bed before acting."));
            let plots = array(&obs["sections"]["homestead"]["plots"]);
            for plot in plots {
                let Some(plot_id) = id(&plot["id"]) else {
                    continue;
                };
                let owner = if plot["own"] == true {
                    "Yours"
                } else if plot["state_known"] != true {
                    "Visit to inspect"
                } else if plot["owner"].is_null() {
                    "Unclaimed"
                } else {
                    plot["owner_name"].as_str().unwrap_or("Owned")
                };
                let mut row = Entry::button(
                    format!("plot-{plot_id}"),
                    format!("{} | {owner}", plot["name"].as_str().unwrap_or("Plot")),
                    Action::Plot(plot_id),
                    true,
                );
                row.selected = Some(plot_id) == context.plot;
                rows.push(row);
            }
            if plots.is_empty() {
                rows.push(Entry::note("no-plots", "Explore the southern house lane or northern gardens. Right-click ground to walk; plots appear after your hero discovers them."));
                explore(world, ready, &mut rows);
                return rows;
            }
            let Some(estate) = plots.iter().find(|estate| id(&estate["id"]) == plot) else {
                return rows;
            };
            let target = id(&estate["id"]).expect("selected plot id");
            let door = position(&estate["front_door"]);
            rows.push(Entry::heading(
                "selected-title",
                estate["name"].as_str().unwrap_or("Selected plot"),
            ));
            if let Some(door) = door {
                rows.push(Entry::button(
                    "visit-plot",
                    "Walk to plot door",
                    Action::Walk(door),
                    ready,
                ));
            }
            if estate["own"] != true {
                rows.push(Entry::note("claim-help", "You may own one house plot and one garden. A living owner's land is protected. Claiming gives no materials."));
                rows.push(Entry::button(
                    "claim-plot",
                    "Claim plot",
                    work("claim_plot", Some(target), json!({}), door, "Claim plot"),
                    ready,
                ));
                return rows;
            }
            let building = &obs["sections"]["homestead"]["building"];
            if !building.is_null() && id(&building["plot"]) == Some(target) {
                rows.push(Entry::note(
                    "cottage-building",
                    "Cottage construction in progress. Stop releases the reserved materials.",
                ));
            } else if estate["cottage_built"] == true {
                rows.push(Entry::note(
                    "cottage-built",
                    "Cottage built | this door is your home. Rest here for free.",
                ));
                rows.push(Entry::button(
                    "rest-cottage",
                    "Walk home and rest",
                    work("rest_home", None, json!({}), door, "Rest at cottage"),
                    ready,
                ));
            } else if estate["use_kind"] == "housing" {
                rows.push(Entry::note(
                    "cottage-cost",
                    format!(
                        "Cottage: {} planks + {} logs. Carrying {} planks, {} logs.",
                        homestead::PLANK_COST,
                        homestead::LOG_COST,
                        amount("plank"),
                        amount("log")
                    ),
                ));
                rows.push(Entry::button(
                    "build-cottage",
                    "Build cottage",
                    work(
                        "build_cottage",
                        Some(target),
                        json!({}),
                        door,
                        "Build cottage",
                    ),
                    ready
                        && amount("plank") >= u64::from(homestead::PLANK_COST)
                        && amount("log") >= u64::from(homestead::LOG_COST),
                ));
                rows.push(Entry::note("cottage-source", "Supplies: gather logs, craft planks at the carpenter, and keep four logs for the cottage."));
            }
            rows.push(Entry::heading("beds-title", "Crop beds"));
            let beds = array(&estate["beds"]);
            for crop in beds {
                let Some(index) = id(&crop["index"]) else {
                    continue;
                };
                let stage = match crop["stage"].as_str().unwrap_or("unknown") {
                    "empty" => "Empty",
                    "needs_water" => "Needs water",
                    "growing" => "Growing",
                    "ready" => "Ready to harvest",
                    _ => "Visit to inspect",
                };
                let mut row = Entry::button(
                    format!("bed-{index}"),
                    format!("Bed {} | {stage}", index + 1),
                    Action::Bed(index),
                    true,
                );
                row.selected = index == bed;
                rows.push(row);
            }
            let Some(crop) = beds.iter().find(|crop| id(&crop["index"]) == Some(bed)) else {
                return rows;
            };
            let approach = position(&crop["position"]);
            let stage = crop["stage"].as_str().unwrap_or("unknown");
            rows.push(Entry::note("crop-cost", format!("Seeds {} | Clean water {}. Plant one grain, water once, harvest three grain. Growth follows world time.", amount(catalog::GRAIN), amount(catalog::WATER))));
            let (verb, label, enabled) = match stage {
                "empty" => ("plant", "Plant grain | 1 seed", amount(catalog::GRAIN) > 0),
                "needs_water" => (
                    "water_crop",
                    "Water crop | 1 clean water",
                    amount(catalog::WATER) > 0,
                ),
                "ready" => ("harvest", "Harvest | 3 grain", true),
                "growing" => {
                    let start = crop["watered_at"].as_u64().unwrap_or(0);
                    let end = crop["ready_at"].as_u64().unwrap_or(start);
                    let now = obs["tick"].as_u64().unwrap_or(start);
                    let progress = now.saturating_sub(start).saturating_mul(100)
                        / end.saturating_sub(start).max(1);
                    rows.push(Entry::note(
                        "crop-growing",
                        format!(
                            "Grain growing | {}%. Keep exploring; the crop will wait when ready.",
                            progress.min(100)
                        ),
                    ));
                    return rows;
                }
                _ => return rows,
            };
            rows.push(Entry::button(
                "tend-bed",
                label,
                work(verb, Some(target), json!({"bed":bed}), approach, label),
                ready && enabled,
            ));
        }
        Tab::Supplies => {
            rows.push(Entry::note("supplies-help", "Gather real materials, then use a discovered workshop. Buttons walk to remembered places; availability is checked again on arrival."));
            rows.push(Entry::heading("resources-title", "Gathering"));
            let resources = array(&village["environment"]["resources"]);
            if resources.is_empty() {
                rows.push(Entry::note(
                    "resources-empty",
                    "No gathering sources discovered yet. Explore the paths and outskirts.",
                ));
            }
            for resource in resources {
                let Some(target) = id(&resource["id"]) else {
                    continue;
                };
                let kind = resource["kind"].as_str().unwrap_or("resource");
                let position = player_actions::known_position(world, SimId(target));
                let available = resource["remaining"].as_u64().is_none_or(|n| n > 0);
                let label = format!("Gather {quantity} {}", friendly(kind));
                rows.push(Entry::button(
                    format!("gather-{target}"),
                    label.clone(),
                    work(
                        "gather",
                        Some(target),
                        json!({"quantity":quantity}),
                        position,
                        &label,
                    ),
                    ready && position.is_some() && available,
                ));
                if !available {
                    rows.push(Entry::note(
                        format!("empty-{target}"),
                        "Depleted | wait for the source to recover.",
                    ));
                }
            }
            rows.push(Entry::heading("workshops-title", "Crafting"));
            for station in array(&village["environment"]["workstations"]) {
                let Some(target) = id(&station["id"]) else {
                    continue;
                };
                let position = player_actions::known_position(world, SimId(target));
                for recipe in array(&station["recipes"]) {
                    let Some(name) = recipe["name"].as_str() else {
                        continue;
                    };
                    let requirements = array(&recipe["inputs"]);
                    let have_inputs = requirements.iter().all(|input| {
                        amount(input["kind"].as_str().unwrap_or(""))
                            >= input["count"].as_u64().unwrap_or(u64::MAX)
                    });
                    let inputs = requirements
                        .iter()
                        .map(|input| {
                            format!(
                                "{} {}",
                                input["count"],
                                friendly(input["kind"].as_str().unwrap_or("item"))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ");
                    let outputs = array(&recipe["outputs"])
                        .iter()
                        .map(|output| {
                            format!(
                                "{} {}",
                                output["count"],
                                friendly(output["kind"].as_str().unwrap_or("item"))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ");
                    rows.push(Entry::note(
                        format!("recipe-{target}-{name}-cost"),
                        format!(
                            "{} | {inputs} -> {outputs}",
                            station["name"].as_str().unwrap_or("Workshop")
                        ),
                    ));
                    let verb = format!("craft:{name}");
                    let label = format!("Make {outputs}");
                    rows.push(Entry::button(
                        format!("recipe-{target}-{name}"),
                        label.clone(),
                        work(&verb, Some(target), json!({}), position, &label),
                        ready && position.is_some() && have_inputs,
                    ));
                }
            }
            explore(world, ready, &mut rows);
        }
        Tab::Living => {
            rows.push(Entry::heading("pack-title", "Your pack"));
            if stock.is_empty() {
                rows.push(Entry::note(
                    "pack-empty",
                    "Your pack is empty. Gather supplies or buy actual goods at the market.",
                ));
            }
            for (kind, count) in &stock {
                rows.push(Entry::note(
                    format!("pack-{kind}"),
                    format!("{} x {count}", friendly(kind)),
                ));
            }
            let food = ["bread", catalog::COOKED_MEAL, catalog::BERRIES]
                .into_iter()
                .find(|kind| amount(kind) > 0)
                .unwrap_or("bread");
            rows.push(Entry::button(
                "eat",
                "Eat from pack",
                work("eat", None, json!({"kind":food}), None, "Eat food"),
                ready && amount(food) > 0,
            ));
            rows.push(Entry::button(
                "drink",
                "Drink clean water",
                work("drink", None, json!({}), None, "Drink water"),
                ready && amount(catalog::WATER) > 0,
            ));
            rows.push(Entry::button(
                "rest-home",
                "Walk home and rest",
                work(
                    "rest_home",
                    None,
                    json!({}),
                    position(&village["home"]),
                    "Rest at home",
                ),
                ready,
            ));
            rows.push(Entry::button(
                "rest-ground",
                "Rest here",
                work("rest_ground", None, json!({}), None, "Rest on ground"),
                ready,
            ));
            rows.push(Entry::heading("market-title", "Nearby market"));
            rows.push(Entry::note("market-help", "Buy and sell with merchants within reach. An offered item earns coins only when an NPC actually buys it."));
            for merchant in array(&village["nearby_market"]) {
                let Some(target) = id(&merchant["id"]) else {
                    continue;
                };
                for offer in array(&merchant["offers"]) {
                    let Some(verb) = offer["direction"].as_str() else {
                        continue;
                    };
                    let Some(kind) = offer["kind"].as_str() else {
                        continue;
                    };
                    let label = format!(
                        "{} {quantity} {} | {} coins each | {}",
                        if verb == "buy" { "Buy" } else { "Sell" },
                        friendly(kind),
                        offer["unit_price"],
                        merchant["name"].as_str().unwrap_or("Merchant")
                    );
                    rows.push(Entry::button(
                        format!("trade-{target}-{verb}-{kind}"),
                        label.clone(),
                        work(
                            verb,
                            Some(target),
                            json!({"kind":kind,"quantity":quantity}),
                            None,
                            &label,
                        ),
                        ready && offer["available"] == true,
                    ));
                }
            }
            rows.push(Entry::heading("offers-title", "Offer your surplus"));
            for kind in catalog::trade_kinds(world) {
                if amount(kind) < u64::from(quantity) {
                    continue;
                }
                let label = format!("Offer {quantity} {} for sale", friendly(kind));
                rows.push(Entry::button(
                    format!("offer-{kind}"),
                    label.clone(),
                    work(
                        "offer_goods",
                        None,
                        json!({"kind":kind,"quantity":quantity}),
                        None,
                        &label,
                    ),
                    ready,
                ));
            }
            for listing in array(&village["economy"]["own_offers"]) {
                if let Some(kind) = listing["kind"].as_str() {
                    rows.push(Entry::note(
                        format!("listing-{kind}"),
                        format!(
                            "Listed: {} {} | {}",
                            listing["quantity"],
                            friendly(kind),
                            "waiting for a physical buyer"
                        ),
                    ));
                    rows.push(Entry::button(
                        format!("withdraw-{kind}"),
                        format!("Withdraw {} offer", friendly(kind)),
                        work(
                            "withdraw_offer",
                            None,
                            json!({"kind":kind}),
                            None,
                            "Withdraw offer",
                        ),
                        ready,
                    ));
                }
            }
        }
    }
    rows
}

fn explore(world: &World, ready: bool, rows: &mut Vec<Entry>) {
    let Some(actor) = world
        .get_resource::<VillageState>()
        .and_then(|v| world.get_resource::<SimIndex>()?.get(v.played))
    else {
        return;
    };
    let Some(center) = world.get::<SimPos>(actor) else {
        return;
    };
    rows.push(Entry::heading("explore-title", "Explore the island"));
    for (name, dx, dz) in [
        ("north", 0, 6000),
        ("south", 0, -6000),
        ("east", 6000, 0),
        ("west", -6000, 0),
    ] {
        rows.push(Entry::button(
            format!("explore-{name}"),
            format!("Walk 6 m {name}"),
            Action::Walk(SimPos::new(
                center.x.saturating_add(dx),
                center.z.saturating_add(dz),
            )),
            ready,
        ));
    }
}

pub(super) fn control_caption(control: &ControlState) -> &'static str {
    match control {
        ControlState::Native => {
            "You control the hero. Right-click ground to walk. AI nudges are paused."
        }
        ControlState::Free => {
            "Choose Play as hero to start. All actions use your real inventory and the village economy."
        }
        ControlState::Foreign { .. } => {
            "Another client controls the hero. Stop/release it in the AI controls, then choose Play as hero."
        }
        ControlState::Unavailable => {
            "This actor cannot play right now. Use New life if your hero has died."
        }
    }
}
