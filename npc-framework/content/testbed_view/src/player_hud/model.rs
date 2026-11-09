//! Actor-scoped player projections. Queries describe choices; they never apply work.
mod sites;
use bevy::prelude::*;
use npc_framework::prelude::{SimId, SimPos};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{Action, PlayerState, Tab, WorkRequest};
use crate::player_actions;
use testbed_content::village::catalog;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub key: String,
    pub title: String,
    pub detail: String,
    pub action: Option<Action>,
    pub enabled: bool,
    pub selected: bool,
    pub heading: bool,
    pub item: bool,
    pub compact: bool,
}
impl Row {
    fn note(key: impl Into<String>, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            detail: detail.into(),
            action: None,
            enabled: true,
            selected: false,
            heading: false,
            item: false,
            compact: false,
        }
    }
    fn heading(key: &str, title: &str) -> Self {
        Self {
            heading: true,
            ..Self::note(key, title, "")
        }
    }
    fn action(
        key: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
        action: Action,
        enabled: bool,
    ) -> Self {
        Self {
            action: Some(action),
            enabled,
            ..Self::note(key, title, detail)
        }
    }
}
pub(super) fn preferred_plot(observed: &Value, tab: Tab, current: Option<u32>) -> Option<u32> {
    sites::preferred_plot(observed, tab, current)
}
pub(super) fn preferred_bed(observed: &Value, plot: Option<u32>) -> Option<u32> {
    sites::preferred_bed(observed, plot)
}
pub(super) fn preferred_crop(observed: &Value, current: &str) -> String {
    let stock = inventory(observed);
    let crops = array(&observed["sections"]["homestead"]["crops"]);
    let kinds: Vec<_> = crops
        .iter()
        .filter_map(|crop| crop["kind"].as_str().or_else(|| crop["seed_kind"].as_str()))
        .collect();
    if kinds.contains(&current) && stock.get(current).copied().unwrap_or(0) > 0 {
        return current.into();
    }
    kinds
        .iter()
        .copied()
        .find(|kind| stock.get(*kind).copied().unwrap_or(0) > 0)
        .unwrap_or(current)
        .into()
}
pub(super) fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}
pub(super) fn id(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok())
}
pub(super) fn position(value: &Value) -> Option<SimPos> {
    let (x, z) = if let Some(a) = value.as_array() {
        (a.first()?.as_i64()?, a.get(1)?.as_i64()?)
    } else {
        (value["x"].as_i64()?, value["z"].as_i64()?)
    };
    Some(SimPos::new(i32::try_from(x).ok()?, i32::try_from(z).ok()?))
}
pub(super) fn friendly(kind: &str) -> String {
    let name = kind
        .strip_prefix("village_")
        .unwrap_or(kind)
        .replace('_', " ");
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
pub(super) fn inventory(observed: &Value) -> BTreeMap<String, u64> {
    let mut stock = BTreeMap::new();
    for item in array(&observed["sections"]["inventory"]["items"]) {
        if item["slot"].as_str().is_some() {
            continue;
        }
        if let (Some(kind), Some(count)) = (item["kind"].as_str(), item["stack"].as_u64()) {
            *stock.entry(kind.into()).or_default() += count;
        }
    }
    stock
}
fn work(
    label: &str,
    verb: &str,
    target: Option<u32>,
    args: Value,
    approach: Option<SimPos>,
) -> Action {
    Action::Work(WorkRequest {
        label: label.into(),
        verb: verb.into(),
        target,
        args,
        approach,
    })
}
fn stock_text(stock: &BTreeMap<String, u64>, inputs: &Value) -> (String, bool) {
    let mut have = true;
    let text = array(inputs)
        .iter()
        .map(|input| {
            let kind = input["kind"].as_str().unwrap_or("material");
            let wanted = input["count"].as_u64().unwrap_or(0);
            let owned = stock.get(kind).copied().unwrap_or(0);
            have &= owned >= wanted;
            format!("{} {owned}/{wanted}", friendly(kind))
        })
        .collect::<Vec<_>>()
        .join("  ·  ");
    (text, have)
}
pub(super) fn rows(world: &World, observed: &Value, state: &PlayerState, ready: bool) -> Vec<Row> {
    let stock = inventory(observed);
    let village = &observed["sections"]["village"];
    let mut rows = Vec::new();
    match state.tab.unwrap_or(Tab::Pouch) {
        Tab::Pouch => {
            rows.push(Row::note(
                "pack_intro",
                "Your belongings",
                "Everything here is yours. Use food and water before your needs become critical.",
            ));
            if stock.is_empty() {
                rows.push(Row::note("empty_pack","A fresh start","Find fallen wood, grain or berries. Gathering puts real supplies into your pouch."));
            }
            for (kind, count) in &stock {
                let action = if catalog::food_restore(kind).is_some() {
                    Some(work("Eat food", "eat", None, json!({"kind":kind}), None))
                } else if kind == catalog::WATER {
                    Some(work("Drink water", "drink", None, json!({}), None))
                } else if kind == catalog::WARM_CLOTHES {
                    array(&observed["sections"]["inventory"]["items"])
                        .iter()
                        .find(|item| item["kind"] == kind.as_str() && item["slot"].is_null())
                        .and_then(|item| id(&item["id"]))
                        .map(|item| work("Wear warm clothes", "equip", Some(item), json!({}), None))
                } else {
                    None
                };
                rows.push(Row {
                    item: true,
                    action,
                    enabled: ready
                        && !(kind == catalog::WARM_CLOTHES
                            && village["wearing_warm_clothes"] == true),
                    title: friendly(kind),
                    detail: format!("{count} in your pouch"),
                    ..Row::note(format!("item_{kind}"), "", "")
                });
            }
            for item in array(&observed["sections"]["inventory"]["items"])
                .iter()
                .filter(|item| item["slot"].as_str().is_some())
            {
                rows.push(Row::note(
                    format!("equipped_{}", item["id"]),
                    friendly(item["kind"].as_str().unwrap_or("Equipment")),
                    format!("Equipped · {}", item["slot"].as_str().unwrap_or("")),
                ));
            }
            rows.push(Row::heading("recovery", "Take care of yourself"));
            home_heating(village, &mut rows);
            rows.push(Row::action(
                "rest_home",
                "Go home and rest",
                "Shelter restores energy. A heated home also helps in cold weather.",
                work(
                    "Rest at home",
                    "rest_home",
                    None,
                    json!({}),
                    position(&village["home"]),
                ),
                ready,
            ));
            rows.push(Row::action(
                "rest_ground",
                "Rest here",
                "Slower than a bed, but useful when you need a break.",
                work("Rest here", "rest_ground", None, json!({}), None),
                ready,
            ));
        }
        Tab::Craft => {
            rows.push(Row::action(
                "explore_link",
                "Explore for supplies",
                "Known places and nearby paths.",
                Action::Open(Tab::Explore),
                true,
            ));
            rows.push(Row::heading("gather", "Gather materials"));
            rows.push(Row::note("gather_amount","One gathering action at a time","Sources and workshops appear after you discover them. Each action collects one item."));
            for resource in array(&village["environment"]["resources"]) {
                let Some(target) = id(&resource["id"]) else {
                    continue;
                };
                let kind = resource["kind"].as_str().unwrap_or("material");
                let pos = player_actions::known_position(world, SimId(target));
                let stocked = resource["remaining"].as_u64().is_none_or(|count| count > 0);
                let detail = if !stocked {
                    "Depleted for now. Return after it recovers.".into()
                } else {
                    resource["remaining"].as_u64().map_or_else(
                        || "Walk to the remembered source; availability is checked there.".into(),
                        |count| format!("{count} available · Walk over and gather one"),
                    )
                };
                rows.push(Row::action(
                    format!("gather_{target}"),
                    format!("Gather {}", friendly(kind)),
                    detail,
                    work(
                        "Gather one item",
                        "gather",
                        Some(target),
                        json!({"quantity":1}),
                        pos,
                    ),
                    ready && stocked && pos.is_some(),
                ));
            }
            if array(&village["environment"]["resources"]).is_empty() {
                rows.push(Row::note(
                    "gather_empty",
                    "Explore the island",
                    "Walk near the trees and fields to discover sources. E uses a nearby resource.",
                ));
            }
            rows.push(Row::heading("recipes", "Make something useful"));
            for station in array(&village["environment"]["workstations"]) {
                let Some(target) = id(&station["id"]) else {
                    continue;
                };
                let pos = player_actions::known_position(world, SimId(target));
                for recipe in array(&station["recipes"]) {
                    let Some(name) = recipe["name"].as_str() else {
                        continue;
                    };
                    let (materials, have) = stock_text(&stock, &recipe["inputs"]);
                    let output = array(&recipe["outputs"])
                        .iter()
                        .map(|out| {
                            format!(
                                "{} {}",
                                out["count"],
                                friendly(out["kind"].as_str().unwrap_or("goods"))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + ");
                    let seconds = recipe["duration_ticks"].as_u64().unwrap_or(0).div_ceil(20);
                    rows.push(Row::action(
                        format!("recipe_{target}_{name}"),
                        format!("Make {output}"),
                        format!(
                            "{} · {seconds} game seconds\n{materials}",
                            station["name"].as_str().unwrap_or("Workshop")
                        ),
                        work(
                            &format!("Make {output}"),
                            &format!("craft:{name}"),
                            Some(target),
                            json!({}),
                            pos,
                        ),
                        ready && have && pos.is_some(),
                    ));
                }
            }
        }
        Tab::Build | Tab::Garden => {
            rows.extend(sites::rows(observed, state, &stock, ready));
        }
        Tab::Trade => {
            rows.push(Row::note("trade_intro","Trade with the village","Neighbours carry finite stock and coins. Walk close to a merchant to see their actual offers."));
            for merchant in array(&village["nearby_market"]) {
                let Some(target) = id(&merchant["id"]) else {
                    continue;
                };
                rows.push(Row::heading(
                    &format!("merchant_{target}"),
                    merchant["name"].as_str().unwrap_or("Neighbour"),
                ));
                for offer in array(&merchant["offers"]) {
                    let (Some(kind), Some(verb)) =
                        (offer["kind"].as_str(), offer["direction"].as_str())
                    else {
                        continue;
                    };
                    let available = offer["available"] == true;
                    rows.push(Row::action(
                        format!("trade_{target}_{verb}_{kind}"),
                        format!(
                            "{} {}",
                            if verb == "buy" { "Buy" } else { "Sell" },
                            friendly(kind)
                        ),
                        format!(
                            "{} coins each · {}",
                            offer["unit_price"],
                            if available {
                                "Available"
                            } else {
                                offer["reason"].as_str().unwrap_or("Unavailable")
                            }
                        ),
                        work(
                            "Trade goods",
                            verb,
                            Some(target),
                            json!({"kind":kind,"quantity":1,"limit_price":offer["unit_price"]}),
                            None,
                        ),
                        ready && available,
                    ));
                }
            }
            if array(&village["nearby_market"]).is_empty() {
                rows.push(Row::note(
                    "trade_empty",
                    "No merchant within reach",
                    "Select a neighbour in People to walk over and meet them.",
                ));
                rows.push(Row::action(
                    "trade_people",
                    "Open People",
                    "",
                    Action::Open(Tab::People),
                    true,
                ));
            }
            rows.push(Row::heading("offers", "Offer a little surplus"));
            let mut has_surplus = false;
            for (kind, count) in &stock {
                if *count == 0 || !catalog::trade_kinds(world).contains(&kind.as_str()) {
                    continue;
                }
                has_surplus = true;
                rows.push(Row::action(
                    format!("offer_{kind}"),
                    format!("Offer {} for sale", friendly(kind)),
                    "A resident must visit and pay before you earn coins.",
                    work(
                        "Offer goods",
                        "offer_goods",
                        None,
                        json!({"kind":kind,"quantity":1}),
                        None,
                    ),
                    ready,
                ));
            }
            if !has_surplus {
                rows.push(Row::note(
                    "offers_empty",
                    if stock.is_empty() { "Your pouch is empty" } else { "Nothing to offer yet" },
                    "Gather or make goods, then offer what you can spare. Keep provisions and seed for yourself.",
                ));
            }
        }
        Tab::People => people(observed, state, ready, &mut rows),
        Tab::Explore => explore_places(world, observed, ready, &mut rows),
    }
    rows
}

fn resident(observed: &Value, target: u32) -> Option<&Value> {
    array(&observed["sections"]["village"]["social"]["relationships"])
        .iter()
        .find(|person| id(&person["id"]) == Some(target))
}

fn person_position(observed: &Value, target: u32) -> Option<SimPos> {
    ["sensed", "remembered"].into_iter().find_map(|section| {
        array(&observed["sections"]["perception"][section])
            .iter()
            .find(|person| id(&person["id"]) == Some(target))
            .and_then(|person| position(&person["pos"]))
    })
}

fn person_detail(person: &Value) -> String {
    format!(
        "{} · Familiarity {}%",
        friendly(person["role"].as_str().unwrap_or("neighbour")),
        person["affinity"].as_u64().unwrap_or(0) / 1000
    )
}

fn people(observed: &Value, state: &PlayerState, ready: bool, rows: &mut Vec<Row>) {
    let village = &observed["sections"]["village"];
    let selected = state
        .person
        .and_then(|target| resident(observed, target).map(|person| (target, person)));
    if let Some((target, person)) = selected {
        let name = person["name"].as_str().unwrap_or("Neighbour");
        let mut summary = Row::heading("person_summary", name);
        summary.detail = person_detail(person);
        rows.push(summary);
        let pos = person_position(observed, target);
        rows.push(Row::action(
            "greet",
            "Say hello",
            if pos.is_some() {
                "Walk to their last known position and greet them."
            } else {
                "Explore to find this neighbour again."
            },
            work("Greet your neighbour", "talk", Some(target), json!({}), pos),
            ready && pos.is_some(),
        ));
        for topic in array(&village["social"]["topics"]) {
            let (Some(key), Some(label), Some(text)) = (
                topic["id"].as_str(),
                topic["label"].as_str(),
                topic["text"].as_str(),
            ) else {
                continue;
            };
            let mut row = Row::action(
                format!("topic_{key}"),
                label,
                "",
                work(
                    "Ask your neighbour",
                    "talk",
                    Some(target),
                    json!({"text":text}),
                    pos,
                ),
                ready && pos.is_some(),
            );
            row.compact = true;
            rows.push(row);
        }
        rows.push(Row::action(
            "change_person",
            "Change neighbour",
            "Choose another known resident.",
            Action::ChoosePerson,
            true,
        ));
        rows.push(Row::note(
            "conversation_reach",
            "Meet face to face",
            "If they move away before you arrive, try again from their latest known position.",
        ));
    } else {
        rows.push(Row::note(
            "people_intro",
            "Get to know your neighbours",
            "Choose someone you have met to see their conversations.",
        ));
        let mut residents: Vec<_> = array(&village["social"]["relationships"])
            .iter()
            .filter_map(|person| id(&person["id"]).map(|target| (target, person)))
            .filter(|(target, _)| Some(*target) != id(&observed["unit"]))
            .collect();
        residents.sort_by_key(|(target, _)| *target);
        for (target, person) in &residents {
            rows.push(Row::action(
                format!("talk_{target}"),
                person["name"].as_str().unwrap_or("Neighbour"),
                person_detail(person),
                Action::Person(*target),
                true,
            ));
        }
        if residents.is_empty() {
            rows.push(Row::note("people_empty", "Meet the village", "Explore the paths to discover residents. Animals are part of the island, but are not trading neighbours."));
            rows.push(Row::action(
                "explore_link",
                "Explore the island",
                "",
                Action::Open(Tab::Explore),
                true,
            ));
        }
    }
    for (index, speech) in array(&village["heard_speech"])
        .iter()
        .rev()
        .filter(|speech| {
            selected.is_none_or(|(target, _)| {
                id(&speech["speaker"]) == Some(target) || speech["speaker"] == observed["unit"]
            })
        })
        .take(6)
        .enumerate()
    {
        let speaker = id(&speech["speaker"])
            .and_then(|target| resident(observed, target))
            .and_then(|person| person["name"].as_str())
            .unwrap_or("Nearby conversation");
        rows.push(Row::note(
            format!("speech_{}_{}_{}", speech["speaker"], speech["tick"], index),
            speaker,
            speech["text"].as_str().unwrap_or(""),
        ));
    }
}

fn home_heating(village: &Value, rows: &mut Vec<Row>) {
    let heating = &village["home_heating"];
    if heating.is_null() {
        return;
    }
    let fuel = heating["carried_fuel"].as_u64().unwrap_or(0);
    let required = heating["fuel_required"].as_u64().unwrap_or(1);
    let (title, detail) = if heating["heated"] == true {
        (
            "Your home is heated",
            format!(
                "Heat lasts about {} more game seconds. Rest at home to warm up.",
                heating["remaining_ticks"]
                    .as_u64()
                    .unwrap_or(0)
                    .div_ceil(20)
            ),
        )
    } else if heating["cold"] != true {
        (
            "A hearth for cold weather",
            format!(
                "Bring {required} log home when it is cold; the hearth lights automatically. {fuel} available in your pouch."
            ),
        )
    } else if heating["ready_to_light"] == true {
        (
            "The hearth is ready",
            format!(
                "Your next household check will use {required} log to light it. Stay near your home door."
            ),
        )
    } else {
        (
            "Your home needs warmth",
            format!(
                "Bring {required} available log to your home door. The hearth lights automatically in cold weather. Fuel {fuel}/{required}."
            ),
        )
    };
    rows.push(Row::note("home_heating", title, detail));
    if heating["cold"] == true && heating["heated"] != true && fuel < required {
        rows.push(Row::action(
            "home_fuel",
            "Find firewood",
            "Gather a log in Craft, then bring it home.",
            Action::Open(Tab::Craft),
            true,
        ));
    }
}

fn explore_places(world: &World, observed: &Value, ready: bool, rows: &mut Vec<Row>) {
    rows.push(Row::note(
        "explore_intro",
        "Explore on foot",
        "Walk 6 m in a world direction. WASD and right-click also move your character.",
    ));
    if let Some(here) = position(&observed["sections"]["self"]["pos"]) {
        for (direction, dx, dz) in [
            ("north", 0, 6000),
            ("east", 6000, 0),
            ("south", 0, -6000),
            ("west", -6000, 0),
        ] {
            let mut row = Row::action(
                format!("explore_{direction}"),
                format!("Walk 6 m {direction}"),
                "",
                Action::Walk(SimPos::new(
                    here.x.saturating_add(dx),
                    here.z.saturating_add(dz),
                )),
                ready,
            );
            row.compact = true;
            rows.push(row);
        }
    }
    rows.push(Row::heading("known_places", "Places you know"));
    let village = &observed["sections"]["village"];
    if let Some(home) = position(&village["home"]) {
        rows.push(Row::action(
            "walk_home",
            "Walk home",
            "Your assigned home door.",
            Action::Walk(home),
            ready,
        ));
    }
    let mut places = BTreeMap::new();
    for key in ["resources", "workstations"] {
        for place in array(&village["environment"][key]) {
            let Some(target) = id(&place["id"]) else {
                continue;
            };
            let Some(pos) = player_actions::known_position(world, SimId(target)) else {
                continue;
            };
            let name = place["name"]
                .as_str()
                .map(friendly)
                .unwrap_or_else(|| friendly(place["kind"].as_str().unwrap_or("Known place")));
            places.insert(target, (name, pos));
        }
    }
    for plot in array(&observed["sections"]["homestead"]["plots"]) {
        if let (Some(target), Some(pos)) = (id(&plot["id"]), position(&plot["front_door"])) {
            places.insert(
                target,
                (plot["name"].as_str().unwrap_or("Known plot").into(), pos),
            );
        }
    }
    for (target, (name, pos)) in places {
        rows.push(Row::action(
            format!("walk_known_{target}"),
            format!("Walk to {name}"),
            "Availability is checked when you arrive.",
            Action::Walk(pos),
            ready,
        ));
    }
}

pub(super) fn context(
    world: &World,
    observed: &Value,
    selected: Option<SimId>,
    ready: bool,
) -> Option<Row> {
    let perception = &observed["sections"]["perception"];
    let mut choices: Vec<_> = array(&perception["sensed"])
        .iter()
        .filter(|p| p["dist_mm"].as_u64().is_some_and(|d| d <= 2000))
        .filter_map(|p| Some((p["dist_mm"].as_u64()?, SimId(id(&p["id"])?))))
        .collect();
    choices.sort();
    if let Some(target) = selected.filter(|target| {
        Some(target.0) != id(&observed["unit"])
            && choices.iter().any(|(_, nearby)| nearby == target)
    }) && let Some(row) = context_for(world, observed, target, ready)
    {
        return Some(row);
    }
    choices
        .into_iter()
        .find_map(|(_, target)| context_for(world, observed, target, ready))
}
fn context_for(world: &World, observed: &Value, target: SimId, ready: bool) -> Option<Row> {
    let pos = person_position(observed, target.0)
        .or_else(|| player_actions::known_position(world, target))?;
    let perception = &observed["sections"]["perception"];
    let village = &observed["sections"]["village"];
    if let Some(resource) = array(&village["environment"]["resources"])
        .iter()
        .find(|r| id(&r["id"]) == Some(target.0))
    {
        let kind = resource["kind"].as_str().unwrap_or("supplies");
        return Some(Row::action(
            "context",
            format!("E · Gather {}", friendly(kind)),
            "Walk into reach and gather one.",
            work(
                "Gather supplies",
                "gather",
                Some(target.0),
                json!({"quantity":1}),
                Some(pos),
            ),
            ready && resource["remaining"].as_u64() != Some(0),
        ));
    }
    if array(&observed["sections"]["homestead"]["plots"])
        .iter()
        .any(|plot| id(&plot["id"]) == Some(target.0))
    {
        return Some(Row::action(
            "context",
            "E · Inspect this plot",
            "Choose a home or tend its beds.",
            Action::InspectPlot(target.0),
            true,
        ));
    }
    if array(&village["environment"]["workstations"])
        .iter()
        .any(|station| id(&station["id"]) == Some(target.0))
    {
        return Some(Row::action(
            "context",
            "E · Open workshop recipes",
            "See materials and choose what to make.",
            Action::Open(Tab::Craft),
            true,
        ));
    }
    if resident(observed, target.0).is_some()
        && array(&perception["sensed"])
            .iter()
            .any(|person| id(&person["id"]) == Some(target.0) && person["kind"] == "actor")
    {
        return Some(Row::action(
            "context",
            "E · Talk to your neighbour",
            "Choose a greeting or a conversation.",
            Action::Person(target.0),
            true,
        ));
    }
    None
}
