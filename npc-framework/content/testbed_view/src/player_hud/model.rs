//! Actor-scoped player projections. Queries describe choices; they never apply work.
mod sites;
use bevy::prelude::*;
use npc_framework::prelude::{SimId, SimPos};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::depth::{self, LiveRequest};
use super::{Action, PlayerState, Tab, WorkRequest};
use crate::player_actions;
use testbed_content::village::{catalog, view};

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
    /// What a recipe needs: name, how many you have, how many it takes.
    pub inputs: Vec<(String, u64, u64)>,
    /// Why the row cannot be used now, as a sentence; shown under it.
    pub reason: String,
    /// A detail card: the named things that can be done with what it shows.
    pub buttons: Vec<RowButton>,
    /// Drawn as a detail card whatever its key.
    pub card: bool,
    /// A bar under the row: the share and its words ("3.2 of 20 kg").
    pub meter: Option<(f32, String)>,
    /// Pips under the row: how many are filled, of how many.
    pub pips: Option<(u32, u32)>,
}

/// One action of a detail card: its button, and why it is unavailable.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowButton {
    pub title: String,
    pub detail: String,
    pub action: Action,
    pub enabled: bool,
    pub reason: String,
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
            inputs: Vec::new(),
            reason: String::new(),
            buttons: Vec::new(),
            card: false,
            meter: None,
            pips: None,
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
/// What a recipe takes against what the pouch holds: name, have, need.
pub(super) fn recipe_inputs(
    stock: &BTreeMap<String, u64>,
    inputs: &Value,
) -> Vec<(String, u64, u64)> {
    array(inputs)
        .iter()
        .map(|input| {
            let kind = input["kind"].as_str().unwrap_or("material");
            (
                friendly(kind),
                stock.get(kind).copied().unwrap_or(0),
                input["count"].as_u64().unwrap_or(0),
            )
        })
        .collect()
}

/// Why a recipe cannot be made now: what is short first, then a workshop
/// that has not been found. Empty when nothing of the recipe's own stands
/// in the way.
pub(super) fn recipe_reason(inputs: &[(String, u64, u64)], reachable: bool) -> String {
    let short: Vec<String> = inputs
        .iter()
        .filter(|(_, owned, wanted)| owned < wanted)
        .map(|(name, owned, wanted)| format!("{} more {}", wanted - owned, name.to_lowercase()))
        .collect();
    if !short.is_empty() {
        format!("You need {}.", short.join(" and "))
    } else if !reachable {
        "You have not found the way to this workshop yet.".to_owned()
    } else {
        String::new()
    }
}

/// What the character is busy with, for the pill above the hotbar.
pub(super) fn working_words(kind: &str) -> &'static str {
    match kind {
        "walking" => "Walking",
        "gathering" => "Gathering",
        "crafting" => "Making something",
        "building" => "Building",
        "resting" => "Resting",
        "restoring" => "Tending the land",
        "hunting" => "Hunting",
        "blocked" => "Waiting for the way to clear",
        _ => "Busy",
    }
}

/// The status beside a workspace's name, where the observation has one:
/// who is in reach to trade with, how many workshops are known.
/// The workspace's badge: where the character stands when that is a known
/// workshop ("At the Harbour carpenter"), else what the observation knows.
/// `at` is the workshop within reach, if any.
pub(super) fn badge(
    observed: &Value,
    tab: Tab,
    at: Option<&str>,
) -> Option<(crate::game_ui::tokens::Status, String)> {
    use crate::game_ui::tokens::Status;
    let village = &observed["sections"]["village"];
    if let (Tab::Craft, Some(place)) = (tab, at) {
        return Some((Status::Good, format!("At the {place}")));
    }
    match tab {
        Tab::Trade => Some(match array(&village["nearby_market"]).len() {
            0 => (Status::Idle, "No merchant in reach".to_owned()),
            1 => (Status::Good, "1 merchant in reach".to_owned()),
            count => (Status::Good, format!("{count} merchants in reach")),
        }),
        Tab::Craft => Some(match array(&village["environment"]["workstations"]).len() {
            0 => (Status::Idle, "No workshop found yet".to_owned()),
            1 => (Status::Info, "1 workshop known".to_owned()),
            count => (Status::Info, format!("{count} workshops known")),
        }),
        _ => None,
    }
}

pub(super) fn rows(world: &World, observed: &Value, state: &PlayerState, ready: bool) -> Vec<Row> {
    let stock = inventory(observed);
    let village = &observed["sections"]["village"];
    let mut rows = Vec::new();
    match state.tab.unwrap_or(Tab::Pouch) {
        Tab::Pouch => {
            if let Some(share) = depth::pack(world) {
                rows.push(Row {
                    meter: Some(share),
                    ..Row::note("pack_weight", "Pack", "")
                });
            }
            if let Some((tool, uses)) = depth::held_tool(world) {
                rows.push(Row::note(
                    "held_tool",
                    format!("In hand: {}", friendly(&tool).to_lowercase()),
                    format!("{uses} uses left before it wears out."),
                ));
            }
            let filter = state.need_filter;
            if let Some(need) = filter {
                let word = need_word(need);
                rows.push(Row::heading(
                    "need_filter",
                    &format!("What helps your {word}"),
                ));
                // The simulation's reason the need is low, under the heading.
                if let Some(reason) = depth::need_reason(world, need) {
                    rows.push(Row::note("need_reason", reason, ""));
                }
                rows.push(Row {
                    compact: true,
                    ..Row::action(
                        "need_all",
                        "Show everything",
                        "",
                        Action::NeedHelp(None),
                        true,
                    )
                });
                if need != "energy" && !stock.keys().any(|kind| depth::helps(need, kind)) {
                    rows.push(Row::note(
                        "need_none",
                        format!("Nothing in your pouch helps your {word}"),
                        need_hint(need),
                    ));
                }
            }
            if stock.is_empty() && filter.is_none() {
                rows.push(Row::note("empty_pack","Your pouch is empty","Find fallen wood, grain or berries. Gathering puts real supplies into your pouch."));
            }
            for (kind, count) in stock
                .iter()
                .filter(|(kind, _)| filter.is_none_or(|need| depth::helps(need, kind)))
            {
                rows.push(Row {
                    item: true,
                    action: Some(Action::Item(kind.clone())),
                    selected: state.item.as_deref() == Some(kind.as_str()),
                    title: friendly(kind),
                    detail: format!("{count} in your pouch"),
                    ..Row::note(format!("item_{kind}"), "", "")
                });
            }
            // The chosen item: what can be done with it here.
            if let Some(kind) = state
                .item
                .as_deref()
                .filter(|kind| stock.contains_key(*kind))
            {
                // One card for the chosen item: its name, how many, and the
                // named things that can be done with it.
                let mut buttons = Vec::new();
                let worn = kind == catalog::WARM_CLOTHES && village["wearing_warm_clothes"] == true;
                let usable = if catalog::food_restore(kind).is_some() {
                    Some((
                        "Eat",
                        "Food restores hunger.",
                        work("Eat food", "eat", None, json!({"kind":kind}), None),
                    ))
                } else if kind == catalog::WATER {
                    Some((
                        "Drink",
                        "Clean water restores thirst.",
                        work("Drink water", "drink", None, json!({}), None),
                    ))
                } else if kind == catalog::WARM_CLOTHES {
                    array(&observed["sections"]["inventory"]["items"])
                        .iter()
                        .find(|item| item["kind"] == kind && item["slot"].is_null())
                        .and_then(|item| id(&item["id"]))
                        .map(|item| {
                            (
                                "Wear",
                                "Warm clothes keep the cold out.",
                                work("Wear warm clothes", "equip", Some(item), json!({}), None),
                            )
                        })
                } else {
                    None
                };
                if let Some((title, detail, action)) = usable {
                    buttons.push(RowButton {
                        title: title.to_owned(),
                        detail: detail.to_owned(),
                        action,
                        enabled: ready && !worn,
                        reason: if worn {
                            "You already wear warm clothes.".to_owned()
                        } else {
                            String::new()
                        },
                    });
                }
                if catalog::trade_kinds(world).contains(&kind) {
                    buttons.push(RowButton {
                        title: "Offer for sale".to_owned(),
                        detail: "A resident must visit and pay before you earn coins.".to_owned(),
                        action: work(
                            "Offer goods",
                            "offer_goods",
                            None,
                            json!({"kind":kind,"quantity":1}),
                            None,
                        ),
                        enabled: ready,
                        reason: String::new(),
                    });
                }
                // Into the homestead chest, once one stands.
                let chest = depth::played(world).and_then(|(unit, _)| view::chest(world, unit));
                if chest.is_some() {
                    let request = LiveRequest::new("store").kind(kind);
                    let check = depth::check(world, &request);
                    buttons.push(RowButton {
                        title: "Store in chest".to_owned(),
                        detail: "Put one in your homestead chest.".to_owned(),
                        action: Action::Live(request),
                        enabled: check.is_ok(),
                        reason: check.err().unwrap_or_default(),
                    });
                }
                let count = stock.get(kind).copied().unwrap_or(0);
                rows.push(Row {
                    detail: if buttons.is_empty() {
                        format!("{count} in your pouch. Nothing to do with it here.")
                    } else {
                        format!("{count} in your pouch.")
                    },
                    buttons,
                    ..Row::note("item_detail", friendly(kind), "")
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
            // The homestead chest: what it holds, each to take back.
            if let Some(chest) = depth::played(world).and_then(|(unit, _)| view::chest(world, unit))
            {
                rows.push(Row::heading("chest", "Your chest"));
                rows.push(Row {
                    meter: Some((
                        chest.used as f32 / chest.capacity.max(1) as f32,
                        format!("{} of {}", chest.used, chest.capacity),
                    )),
                    ..Row::note("chest_space", "Space", "")
                });
                if chest.goods.is_empty() {
                    rows.push(Row::note(
                        "chest_empty",
                        "The chest is empty",
                        "Choose something in your pouch and Store in chest to keep it here.",
                    ));
                }
                for (kind, count) in &chest.goods {
                    let request = LiveRequest::new("take").kind(kind);
                    let check = depth::check(world, &request);
                    rows.push(Row {
                        reason: check.clone().err().unwrap_or_default(),
                        ..Row::action(
                            format!("chest_{kind}"),
                            format!("Take {}", friendly(kind).to_lowercase()),
                            format!("{count} in the chest"),
                            Action::Live(request),
                            check.is_ok(),
                        )
                    });
                }
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
            let repeating = depth::played(world)
                .and_then(|(unit, _)| view::repeat_order(world, unit))
                .is_some();
            if live_mode(world) {
                stepper(state, "Gather", &mut rows);
            }
            for resource in array(&village["environment"]["resources"]) {
                let Some(target) = id(&resource["id"]) else {
                    continue;
                };
                let kind = resource["kind"].as_str().unwrap_or("material");
                let pos = player_actions::known_position(world, SimId(target));
                let stocked = resource["remaining"].as_u64().is_none_or(|count| count > 0);
                let tool_ready = kind != catalog::FISH
                    || stock.get(catalog::FISHING_ROD).copied().unwrap_or(0) > 0;
                let detail = if !tool_ready {
                    "Bring a fishing rod. Craft one at a carpenter's workshop using 1 plank and 2 thread, or buy one.".into()
                } else if !stocked {
                    "Depleted for now. Return after it recovers.".into()
                } else {
                    resource["remaining"].as_u64().map_or_else(
                        || "Walk to the remembered source; availability is checked there.".into(),
                        |count| format!("{count} available · Walk over and gather one"),
                    )
                };
                let usable = ready && stocked && tool_ready && pos.is_some();
                let title = if kind == catalog::FISH {
                    "Cast your fishing rod".to_owned()
                } else {
                    format!("Gather {}", friendly(kind))
                };
                if state.repeat > 1 && live_mode(world) {
                    // A repeat order starts beside the source: walk there first.
                    let far = pos.filter(|pos| !beside(world, *pos));
                    let times = state.repeat;
                    rows.push(if let Some(far) = far {
                        Row::action(
                            format!("gather_{target}"),
                            format!("Walk to the {}", friendly(kind).to_lowercase()),
                            format!("Then choose it again to gather it {times} times."),
                            Action::Walk(far),
                            ready,
                        )
                    } else {
                        Row {
                            reason: if repeating {
                                "A repeat order is already under way; Stop ends it.".to_owned()
                            } else {
                                String::new()
                            },
                            ..Row::action(
                                format!("gather_{target}"),
                                format!("{title} \u{00d7}{times}"),
                                format!("{detail} · {times} times, one after another"),
                                Action::Live(
                                    LiveRequest::new("repeat").target(target).quantity(times),
                                ),
                                usable && !repeating,
                            )
                        }
                    });
                    continue;
                }
                rows.push(Row::action(
                    format!("gather_{target}"),
                    title,
                    detail,
                    work(
                        "Gather one item",
                        "gather",
                        Some(target),
                        json!({"quantity":1}),
                        pos,
                    ),
                    usable,
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
            if live_mode(world) {
                stepper(state, "Make", &mut rows);
            }
            // Everything the known workshops make, or only what you can make now.
            rows.push(Row {
                compact: true,
                selected: !state.craft_all,
                ..Row::action("craft_can", "Can make", "", Action::CraftAll(false), true)
            });
            rows.push(Row {
                compact: true,
                selected: state.craft_all,
                ..Row::action("craft_all", "All", "", Action::CraftAll(true), true)
            });
            for station in array(&village["environment"]["workstations"]) {
                let Some(target) = id(&station["id"]) else {
                    continue;
                };
                let pos = player_actions::known_position(world, SimId(target));
                for recipe in array(&station["recipes"]) {
                    let Some(name) = recipe["name"].as_str() else {
                        continue;
                    };
                    let inputs = recipe_inputs(&stock, &recipe["inputs"]);
                    let have = inputs.iter().all(|(_, owned, wanted)| owned >= wanted);
                    if !have && !state.craft_all {
                        continue;
                    }
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
                    if state.repeat > 1 && live_mode(world) {
                        rows.push(recipe_repeat(
                            world,
                            state.repeat,
                            target,
                            name,
                            &output,
                            pos,
                            (ready, repeating, inputs),
                        ));
                        continue;
                    }
                    rows.push(Row {
                        reason: recipe_reason(&inputs, pos.is_some()),
                        inputs,
                        ..Row::action(
                            format!("recipe_{target}_{name}"),
                            format!("Make {output}"),
                            format!(
                                "{} · {seconds} s at 1×",
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
                        )
                    });
                }
            }
            if !rows.iter().any(|row| row.key.starts_with("recipe_")) {
                let (title, detail) = recipe_empty(
                    !array(&village["environment"]["workstations"]).is_empty(),
                    state.craft_all,
                );
                rows.push(Row::note("recipes_empty", title, detail));
            }
        }
        Tab::Build | Tab::Garden => {
            let site_rows = sites::rows(observed, state, &stock, ready);
            if state.tab == Some(Tab::Build) {
                // An upgrade under way is what Build is for now: it leads.
                let leads = depth::played(world)
                    .is_some_and(|(unit, _)| view::upgrade_site(world, unit).is_some());
                if leads {
                    homestead(world, &stock, ready, &mut rows);
                    rows.extend(site_rows);
                } else {
                    rows.extend(site_rows);
                    homestead(world, &stock, ready, &mut rows);
                }
            } else {
                rows.extend(site_rows);
                garden_repeats(world, observed, state, ready, &mut rows);
            }
        }
        Tab::Trade => {
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
                    rows.push(Row {
                        // The merchant's own words for an offer that is closed.
                        reason: if available {
                            String::new()
                        } else {
                            let reason = offer["reason"].as_str().unwrap_or("Unavailable");
                            let mut sentence = friendly(reason);
                            if !sentence.ends_with('.') {
                                sentence.push('.');
                            }
                            sentence
                        },
                        ..Row::action(
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
                        )
                    });
                }
            }
            // What the merchant boat wants at its next call (or this one):
            // a sale worth carrying to the trading post.
            let boat = view::boat(world);
            let orders = crate::island_shell::model::boat_order_lines(
                &crate::island_shell::boat_orders(&boat),
                boat.docked,
            );
            if boat.enabled && !orders.is_empty() {
                rows.push(Row::heading("boat_orders", "The merchant boat"));
                for (index, line) in orders.into_iter().enumerate() {
                    rows.push(Row::note(
                        format!("boat_order_{index}"),
                        line,
                        "Sell it at the trading post while the boat is in.",
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
        Tab::People => people(world, observed, state, ready, &mut rows),
        Tab::Explore => explore_places(world, observed, ready, &mut rows),
    }
    rows
}

fn resident(observed: &Value, target: u32) -> Option<&Value> {
    array(&observed["sections"]["village"]["social"]["relationships"])
        .iter()
        .find(|person| id(&person["id"]) == Some(target))
}

/// The neighbours the played character knows: those the observation names,
/// and everyone the game keeps a bond with, though not seen lately (the
/// observation forgets faces after a while; a friend is not forgotten).
fn known_people(world: &World, observed: &Value) -> Vec<Value> {
    let mut known: Vec<Value> =
        array(&observed["sections"]["village"]["social"]["relationships"]).to_vec();
    if let Some((unit, _)) = depth::played(world) {
        for relation in view::relationships(world, unit) {
            if !relation.with_player
                || relation.score <= 0
                || known
                    .iter()
                    .any(|person| id(&person["id"]) == Some(relation.other.0))
            {
                continue;
            }
            known.push(json!({"id": relation.other.0, "name": relation.name,
                "role": "neighbour", "affinity": 0}));
        }
    }
    known
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

fn people(world: &World, observed: &Value, state: &PlayerState, ready: bool, rows: &mut Vec<Row>) {
    let tiers = depth::tiers(world);
    let top = tiers.len().saturating_sub(1) as u32;
    let village = &observed["sections"]["village"];
    let known = known_people(world, observed);
    let selected = state.person.and_then(|target| {
        known
            .iter()
            .find(|person| id(&person["id"]) == Some(target))
            .map(|person| (target, person))
    });
    if let Some((target, person)) = selected {
        let name = person["name"].as_str().unwrap_or("Neighbour");
        let mut summary = Row::heading("person_summary", name);
        summary.detail = person_detail(person);
        rows.push(summary);
        if !tiers.is_empty() {
            let (index, tier) = depth::bond(world, target);
            let gives = tiers
                .get(index as usize)
                .map(depth::tier_gives)
                .unwrap_or_default();
            let next = tiers
                .get(index as usize + 1)
                .map_or_else(String::new, |next| {
                    format!(" Next: {}.", depth::tier_word(&next.name))
                });
            rows.push(Row {
                pips: Some((index, top)),
                ..Row::note("bond", depth::tier_word(&tier), format!("{gives}{next}"))
            });
        }
        let pos = person_position(observed, target);
        rows.push(Row::action(
            "greet",
            "Talk",
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
        if !tiers.is_empty() {
            gifts(world, observed, state, target, name, rows);
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
        let mut residents: Vec<_> = known
            .iter()
            .filter_map(|person| id(&person["id"]).map(|target| (target, person)))
            .filter(|(target, _)| Some(*target) != id(&observed["unit"]))
            .collect();
        residents.sort_by_key(|(target, _)| *target);
        for (target, person) in &residents {
            let (index, tier) = if tiers.is_empty() {
                (0, String::new())
            } else {
                depth::bond(world, *target)
            };
            rows.push(Row {
                pips: (!tiers.is_empty()).then_some((index, top)),
                ..Row::action(
                    format!("talk_{target}"),
                    person["name"].as_str().unwrap_or("Neighbour"),
                    if tier.is_empty() {
                        person_detail(person)
                    } else {
                        format!(
                            "{} · {}",
                            depth::tier_word(&tier),
                            friendly(person["role"].as_str().unwrap_or("neighbour"))
                        )
                    },
                    Action::Person(*target),
                    true,
                )
            });
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

/// The prompt at your own door from evening on.
pub(super) fn sleep_row(request: LiveRequest, _ready: bool) -> Row {
    Row::action(
        "context",
        "Sleep until morning",
        "Sleep in your own bed; the night passes quickly.",
        Action::Live(request),
        true,
    )
}

/// Asleep, the context action (E) wakes the character.
pub(super) fn wake_row() -> Row {
    Row::action(
        "context",
        "Wake up",
        "Get up before morning.",
        Action::Live(LiveRequest::new("wake")),
        true,
    )
}

/// Whether the world runs live mode (bonds, goals, repeat orders).
fn live_mode(world: &World) -> bool {
    !depth::tiers(world).is_empty()
}

/// What Gather & craft says when it lists no recipe: what is missing and
/// what makes recipes appear.
pub(super) fn recipe_empty(workshops: bool, all: bool) -> (&'static str, &'static str) {
    match (workshops, all) {
        (false, _) => (
            "No workshop known yet",
            "Walk past a carpenter, a smithy or a kitchen; its recipes are listed here.",
        ),
        (true, false) => (
            "Nothing you can make yet",
            "Gather the inputs, or choose All to see every recipe of the workshops you know.",
        ),
        (true, true) => (
            "No recipe known yet",
            "Visit a workshop you know to read what it makes; its recipes are listed here.",
        ),
    }
}

/// The upgrade card's line while materials are missing.
pub(super) const CARRY_WORDS: &str =
    "Carry materials to your door: they go in by themselves, and work starts once all are in.";

/// What an upgrade under way waits for, in one or two sentences: the
/// materials, what is ordered and who builds.
/// How long ago an order was placed, in island time: "less than an hour
/// ago", "an hour ago", "5 hours ago", "2 days ago". `hour` is ticks per
/// island hour.
pub(super) fn waited_words(ordered_at: u64, now: u64, hour: u64) -> String {
    let hours = now.saturating_sub(ordered_at) / hour.max(1);
    match hours {
        0 => "less than an hour ago".to_owned(),
        1 => "an hour ago".to_owned(),
        2..=23 => format!("{hours} hours ago"),
        24..=47 => "a day ago".to_owned(),
        _ => format!("{} days ago", hours / 24),
    }
}

/// The upgrade card's sentence. `waited` says when the last order was
/// placed ("3 hours ago"), from `UpgradeSiteView::ordered_at`.
pub(super) fn site_words(site: &view::UpgradeSiteView, waited: Option<&str>) -> String {
    site_lines(site, waited).join("\n")
}

/// The upgrade card's status as short rows, one sentence each: where the
/// materials stand; what is ordered and not yet taken, and since when; who
/// is bringing what; who is hired and when they come; what to do meanwhile.
pub(super) fn site_lines(site: &view::UpgradeSiteView, waited: Option<&str>) -> Vec<String> {
    let held = |list: &[(String, u32)], good: &str| {
        list.iter()
            .filter(|(kind, _)| kind == good)
            .map(|(_, count)| *count)
            .sum::<u32>()
    };
    // Everything not yet in is ordered: nothing is left to carry.
    let all_ordered = site
        .cost
        .iter()
        .all(|(good, count)| held(&site.delivered, good) + held(&site.ordered, good) >= *count);
    let mut lines = vec![
        if site.supplied {
            if site.builder.is_some() {
                "The materials are in; your hired builder works on it, with you or without you."
            } else {
                "The materials are in; stay by the cottage door while the work goes on."
            }
        } else if all_ordered {
            "Everything still missing is ordered; work starts once all of it is in."
        } else {
            CARRY_WORDS
        }
        .to_owned(),
    ];
    let ordered: Vec<(&str, u32)> = site
        .ordered
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(kind, count)| (kind.as_str(), *count))
        .collect();
    // An order no one has taken yet says so, rather than look stalled.
    let taken = |kind: &str| {
        site.workers
            .iter()
            .any(|worker| worker.kind == kind && matches!(worker.state, "fetching" | "bringing"))
    };
    let waiting: Vec<String> = ordered
        .iter()
        .filter(|(kind, _)| !taken(kind))
        .map(|(kind, count)| crate::game_ui::words::count(u64::from(*count), kind))
        .collect();
    if !site.supplied && !waiting.is_empty() {
        let since = waited.map_or_else(String::new, |waited| format!(" (ordered {waited})"));
        lines.push(format!(
            "No one has taken this order yet{since}: {}.",
            waiting.join(", ")
        ));
        lines.push("A resident with stock to spare picks it up in working hours.".to_owned());
    }
    // Who is bringing what, then who builds.
    let mut workers: Vec<_> = site.workers.iter().collect();
    workers.sort_by_key(|worker| worker.job == "builder");
    for worker in workers {
        lines.push(worker_sentence(worker));
    }
    if site.workers.is_empty() && site.builder.is_some() && !site.supplied {
        lines.push("A builder is hired and starts once the materials are in.".to_owned());
    }
    let underway = (!ordered.is_empty() || site.builder.is_some()) && !site.supplied;
    if underway || !site.workers.is_empty() {
        lines.push(MEANWHILE_WORDS.to_owned());
    }
    lines
}

/// The goal card while a homestead upgrade is under way and nothing more
/// urgent calls: what is happening now, and what to do meanwhile. Orders
/// and hired work take island hours; this says so, so nothing reads as a
/// hang. `hour` is ticks per island hour.
pub(super) fn upgrade_goal(site: &view::UpgradeSiteView, hour: u64) -> (String, String) {
    let moving: Vec<String> = site
        .workers
        .iter()
        .filter(|worker| matches!(worker.state, "fetching" | "bringing"))
        .map(worker_sentence)
        .collect();
    let ordered = site.ordered.iter().any(|(_, count)| *count > 0);
    if site.supplied {
        let hours = site.remaining_ticks.div_ceil(hour.max(1));
        let left = match hours {
            0 | 1 => "Less than an hour of work is left.".to_owned(),
            n => format!("About {n} hours of work are left."),
        };
        return if site.builder.is_some() {
            (
                format!("Your builder raises {}", site.name),
                format!(
                    "{left} It goes on without you; {}",
                    lower_first(MEANWHILE_WORDS)
                ),
            )
        } else {
            (
                format!("Work on {}", site.name),
                format!(
                    "{left} Stay by the cottage door while the work goes on, or hire a builder in Build."
                ),
            )
        };
    }
    if !moving.is_empty() {
        return (
            "Materials are on the way".to_owned(),
            format!("{} {MEANWHILE_WORDS}", moving.join(" ")),
        );
    }
    if ordered {
        return (
            "Waiting for your order".to_owned(),
            format!(
                "No one has taken it yet: a resident with stock to spare picks it up in working hours. {MEANWHILE_WORDS}"
            ),
        );
    }
    (
        format!("Bring materials for {}", site.name),
        "Carry them to your door, or order them in Build and hire a builder.".to_owned(),
    )
}

/// "Meanwhile at home: materials are on the way." after another goal.
pub(super) fn meanwhile_at_home(title: &str) -> String {
    format!("Meanwhile at home: {}.", lower_first(title))
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().collect::<String>() + chars.as_str()
    })
}

/// What to do while residents fetch, carry and build: the work takes game
/// hours, and nothing needs watching.
pub(super) const MEANWHILE_WORDS: &str = "This takes a few hours of island time; meanwhile gather, tend the garden, or sleep and let the night pass.";

/// One worker on the upgrade, from the simulation's state and goods (the
/// count goes through the one plural helper).
pub(super) fn worker_sentence(
    worker: &testbed_content::village::live::UpgradeWorkerView,
) -> String {
    let name = &worker.name;
    let goods = crate::game_ui::words::count(u64::from(worker.count), &worker.kind);
    match worker.state {
        "fetching" => format!("{name} is buying {goods} for you at the store."),
        "bringing" => format!("{name} is bringing {goods}."),
        "walking" if worker.job == "friend" => format!("{name} is on the way to lend a hand."),
        "walking" => format!("{name} is on the way to work on your homestead."),
        // Standing still between two stages of the work.
        "waiting" => format!("{name} is between stages of work on your homestead."),
        "working" if worker.job == "builder" => format!("{name} is building at your door."),
        "working" => format!("{name} is lending a hand at your door."),
        "harvesting" => format!("{name} is harvesting your garden."),
        "hired" => format!("{name} is hired and will come in working hours."),
        _ => worker.text.trim().to_owned(),
    }
}

/// The known workshop within reach of the played character (3 m), by its
/// name, for the Craft badge.
pub(super) fn station_in_reach(world: &World, observed: &Value) -> Option<String> {
    let (_, actor) = depth::played(world)?;
    let here = *world.get::<SimPos>(actor)?;
    array(&observed["sections"]["village"]["environment"]["workstations"])
        .iter()
        .filter_map(|station| {
            let id = SimId(id(&station["id"])?);
            let at = crate::player_actions::known_position(world, id)?;
            let name = station["name"].as_str()?;
            (at.dist2(here) <= 3_000_i64 * 3_000).then(|| (at.dist2(here), name.to_owned()))
        })
        .min()
        .map(|(_, name)| name)
}

/// The ×1 / ×5 / ×10 stepper of a repeat order; `verb` says what repeats.
/// The first stepper keeps the keys `repeat_N` that scripts know.
fn stepper(state: &PlayerState, verb: &str, rows: &mut Vec<Row>) {
    let first = !rows.iter().any(|row| row.key.starts_with("repeat_"));
    for count in [1, 5, 10] {
        let key = if first {
            format!("repeat_{count}")
        } else {
            format!("repeat_{}_{count}", verb.to_lowercase().replace(' ', "_"))
        };
        rows.push(Row {
            compact: true,
            selected: state.repeat == count,
            ..Row::action(
                key,
                format!("\u{00d7}{count}"),
                if count == 1 {
                    format!("{verb} once.")
                } else {
                    format!("{verb} {count} times, one after another. Stop ends it early.")
                },
                Action::Repeat(count),
                true,
            )
        });
    }
}

/// A recipe made `times` over at its workshop: walk there first, then a
/// recorded repeat order the simulation checks.
fn recipe_repeat(
    world: &World,
    times: u32,
    station: u32,
    recipe: &str,
    output: &str,
    pos: Option<SimPos>,
    (ready, repeating, inputs): (bool, bool, Vec<(String, u64, u64)>),
) -> Row {
    let key = format!("recipe_{station}_{recipe}");
    if let Some(far) = pos.filter(|pos| !beside(world, *pos)) {
        return Row {
            inputs,
            ..Row::action(
                key,
                format!("Walk to the workshop for {output}"),
                format!("Then choose it again to make it {times} times."),
                Action::Walk(far),
                ready,
            )
        };
    }
    let request = LiveRequest::new("repeat")
        .target(station)
        .recipe(recipe)
        .quantity(times);
    let check = depth::check(world, &request);
    Row {
        inputs,
        reason: if repeating {
            "A repeat order is already under way; Stop ends it.".to_owned()
        } else {
            check.clone().err().unwrap_or_default()
        },
        ..Row::action(
            key,
            format!("Make {output} \u{00d7}{times}"),
            format!("{times} times, one after another; each uses its inputs."),
            Action::Live(request),
            ready && !repeating && check.is_ok(),
        )
    }
}

/// Garden work on your own plot, repeated across its beds as one order.
fn garden_repeats(
    world: &World,
    observed: &Value,
    state: &PlayerState,
    ready: bool,
    rows: &mut Vec<Row>,
) {
    if !live_mode(world) {
        return;
    }
    let own = array(&observed["sections"]["homestead"]["plots"])
        .iter()
        .find(|plot| id(&plot["id"]) == state.plot && plot["own"] == true);
    let Some(plot) = own.and_then(|plot| id(&plot["id"])) else {
        return;
    };
    let repeating = depth::played(world)
        .and_then(|(unit, _)| view::repeat_order(world, unit))
        .is_some();
    rows.push(Row::heading("garden_repeat", "Work the whole plot"));
    stepper(state, "Do it", rows);
    let times = state.repeat;
    for (task, title) in [
        ("plant", "Plant"),
        ("water_crop", "Water"),
        ("harvest", "Harvest"),
    ] {
        let request = LiveRequest::new("repeat")
            .target(plot)
            .task(task)
            .quantity(times);
        let check = depth::check(world, &request);
        rows.push(Row {
            reason: if repeating {
                "A repeat order is already under way; Stop ends it.".to_owned()
            } else {
                check.clone().err().unwrap_or_default()
            },
            ..Row::action(
                format!("garden_repeat_{task}"),
                format!("{title} \u{00d7}{times}"),
                "Bed after bed, until the count is done or nothing is left to do.",
                Action::Live(request),
                ready && !repeating && check.is_ok(),
            )
        });
    }
}

/// Whether the played character stands beside a place (2 m).
fn beside(world: &World, pos: SimPos) -> bool {
    depth::played(world)
        .and_then(|(_, entity)| world.get::<SimPos>(entity))
        .is_some_and(|at| {
            let (dx, dz) = (i64::from(at.x - pos.x), i64::from(at.z - pos.z));
            dx * dx + dz * dz <= 2_000 * 2_000
        })
}

/// A need in the pouch filter's heading.
fn need_word(need: &str) -> &'static str {
    match need {
        "hunger" => "hunger",
        "thirst" => "thirst",
        "energy" => "energy",
        "warmth" => "warmth",
        _ => "health",
    }
}

/// What to do when nothing carried helps a need.
fn need_hint(need: &str) -> &'static str {
    match need {
        "hunger" => "Gather berries or grain, or buy bread in Trade.",
        "thirst" => "Draw clean water at a well; see Gather & craft.",
        "warmth" => "Wear warm clothes, or rest in a heated home below.",
        _ => "Eat, drink and rest: health follows your needs.",
    }
}

/// The gift picker under a resident: their taste once known, then the
/// pouch's giftable goods as slots.
fn gifts(
    world: &World,
    observed: &Value,
    state: &PlayerState,
    target: u32,
    name: &str,
    rows: &mut Vec<Row>,
) {
    if state.gift_for != Some(target) {
        rows.push(Row::action(
            "gift_open",
            "Give a gift",
            "Choose something from your pouch. One gift a day counts.",
            Action::GiftFor(Some(target)),
            true,
        ));
        return;
    }
    rows.push(Row::heading("gift_heading", "Choose a gift"));
    rows.push(Row::note(
        "gift_taste",
        match depth::known_tastes(world, target) {
            Some((liked, disliked)) => format!(
                "{name} likes {} and does not care for {}.",
                crate::game_ui::words::noun(&liked, 2),
                crate::game_ui::words::noun(&disliked, 2)
            ),
            None => format!("You do not know yet what {name} likes."),
        },
        "Every resident likes one good and dislikes another; a gift tells you.",
    ));
    let goods = depth::giftable(world, &inventory(observed));
    if goods.is_empty() {
        rows.push(Row::note(
            "gift_none",
            "Nothing in your pouch can be given",
            "Food, water, wood and goods you make can be gifts: gather or make some first.",
        ));
    }
    // Why a gift cannot be handed over now, said once above the slots.
    let mut refused = None;
    for (kind, count) in goods {
        let request = LiveRequest::new("gift").target(target).kind(&kind);
        let check = depth::check(world, &request);
        if let Err(reason) = &check {
            refused.get_or_insert_with(|| reason.clone());
        }
        rows.push(Row {
            item: true,
            ..Row::action(
                format!("gift_{kind}"),
                friendly(&kind),
                format!("{count} in your pouch"),
                Action::Live(request),
                check.is_ok(),
            )
        });
    }
    if let Some(reason) = refused {
        rows.push(Row::note("gift_reason", reason, ""));
    }
    rows.push(Row {
        compact: true,
        ..Row::action("gift_close", "Put it away", "", Action::GiftFor(None), true)
    });
}

/// What a homestead tier gives, in words.
fn tier_gives(tier: &view::TierView) -> String {
    let mut parts = Vec::new();
    if tier.beds > 0 {
        parts.push(format!("{} more bed", tier.beds));
    }
    if tier.plots > 0 {
        parts.push(format!("{} more garden plot", tier.plots));
    }
    if tier.storage > 0 {
        parts.push(format!("a chest for {} goods", tier.storage));
    }
    if tier.gather_percent > 0 && tier.gather_percent < 100 {
        parts.push(format!(
            "gathering takes {}% of the time",
            tier.gather_percent
        ));
    }
    parts.join(", ")
}

/// Seconds at 1x for a number of ticks.
fn seconds(ticks: u64) -> u64 {
    ticks.div_ceil(20)
}

/// Build: the homestead's tiers as cards, and the upgrade under way.
fn homestead(world: &World, stock: &BTreeMap<String, u64>, ready: bool, rows: &mut Vec<Row>) {
    let Some((unit, _)) = depth::played(world) else {
        return;
    };
    let tiers = view::homestead_tiers(world, unit);
    if tiers.is_empty() {
        return;
    }
    rows.push(Row::heading("homestead", "Improve your homestead"));
    let have = |kind: &str| stock.get(kind).copied().unwrap_or(0);
    if let Some(site) = view::upgrade_site(world, unit) {
        let waited = site.ordered_at.map(|at| {
            let day = world
                .get_resource::<npc_framework::prelude::SimConfig>()
                .map_or(2_400, |config| config.day_length.max(24));
            let now = world
                .get_resource::<npc_framework::prelude::SimClock>()
                .map_or(at, |clock| clock.tick);
            waited_words(at, now, day / 24)
        });
        let delivered = |kind: &str| {
            site.delivered
                .iter()
                .find(|(good, _)| good == kind)
                .map_or(0, |(_, count)| u64::from(*count))
        };
        let worked = u64::from(site.work_ticks).saturating_sub(site.remaining_ticks);
        let share = worked as f32 / site.work_ticks.max(1) as f32;
        // Carried goods go in by themselves at the door: no Deliver button.
        let mut buttons: Vec<RowButton> = Vec::new();
        // Order what is missing from residents, paid now from your coins.
        let ordered = |kind: &str| {
            site.ordered
                .iter()
                .find(|(good, _)| good == kind)
                .map_or(0, |(_, count)| u64::from(*count))
        };
        for (kind, price) in &site.order_prices {
            let wanted = site
                .cost
                .iter()
                .find(|(good, _)| good == kind)
                .map_or(0, |(_, count)| u64::from(*count));
            let open = wanted
                .saturating_sub(delivered(kind))
                .saturating_sub(ordered(kind))
                .min(20);
            if open == 0 {
                continue;
            }
            let quantity = u32::try_from(open).unwrap_or(1);
            let request = LiveRequest::new("order").kind(kind).quantity(quantity);
            let check = depth::check(world, &request);
            buttons.push(RowButton {
                title: format!(
                    "Order {}",
                    crate::game_ui::words::count(u64::from(quantity), kind)
                ),
                detail: format!(
                    "About {} coins with the carrying fee; residents bring it from their stock.",
                    u64::from(*price) * open
                ),
                action: Action::Live(request),
                enabled: check.is_ok(),
                reason: check.err().unwrap_or_default(),
            });
        }
        if site.builder.is_none() {
            let hire = LiveRequest::new("hire");
            let check = depth::check(world, &hire);
            buttons.push(RowButton {
                title: "Hire a builder".to_owned(),
                detail: format!(
                    "{} coins for the rest of the work; it goes on while you are away.",
                    site.hire_cost
                ),
                action: Action::Live(hire),
                enabled: check.is_ok(),
                reason: check.err().unwrap_or_default(),
            });
        }
        let cancel = LiveRequest::new("cancel_upgrade");
        let check = depth::check(world, &cancel);
        buttons.push(RowButton {
            title: "Cancel".to_owned(),
            detail: "Stop the work; delivered materials come back to you.".to_owned(),
            action: Action::Live(cancel),
            enabled: check.is_ok(),
            reason: check.err().unwrap_or_default(),
        });
        rows.push(Row {
            card: true,
            meter: Some((
                share,
                format!(
                    "{} s of work left at 1\u{00d7}",
                    seconds(site.remaining_ticks)
                ),
            )),
            inputs: site
                .cost
                .iter()
                .map(|(kind, count)| (friendly(kind), delivered(kind), u64::from(*count)))
                .collect(),
            buttons,
            ..Row::note(
                "homestead_site",
                format!("Under way: {}", site.name),
                site_words(&site, waited.as_deref()),
            )
        });
    }
    for tier in &tiers {
        let gives = tier_gives(tier);
        if tier.done {
            rows.push(Row::note(
                format!("tier_{}", tier.key),
                format!("Built: {}", tier.name),
                gives,
            ));
            continue;
        }
        if tier.remaining_ticks.is_some() {
            continue;
        }
        let request = LiveRequest::new("upgrade").kind(&tier.key);
        let check = depth::check(world, &request);
        let reason = if tier.available {
            check.clone().err().unwrap_or_default()
        } else {
            depth::sentence(&tier.reason)
        };
        rows.push(Row {
            inputs: tier
                .cost
                .iter()
                .map(|(kind, count)| (friendly(kind), have(kind), u64::from(*count)))
                .collect(),
            reason,
            ..Row::action(
                format!("tier_{}", tier.key),
                format!("Build {}", tier.name),
                format!(
                    "{} · {} s of work at 1\u{00d7}",
                    if gives.is_empty() {
                        "An improvement"
                    } else {
                        &gives
                    },
                    seconds(u64::from(tier.work_ticks))
                ),
                Action::Live(request),
                ready && tier.available && check.is_ok(),
            )
        });
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
    let regional = &observed["sections"]["living_world"];
    if regional.is_object() {
        if !array(&regional["chart"]).is_empty() {
            rows.push(Row::note(
                "regional_chart",
                "Coastal chart",
                format!(
                    "{} settlements share this island. Carry food and water for the journey.",
                    array(&regional["chart"]).len()
                ),
            ));
            let here = position(&observed["sections"]["self"]["pos"]);
            for destination in array(&regional["chart"]) {
                let Some(pos) = position(&destination["position"]) else {
                    continue;
                };
                let name = destination["name"].as_str().unwrap_or("Settlement");
                let description = match destination["biome"].as_str() {
                    Some("coast") => "Fishing, village workshops and a place to begin.",
                    Some("woodland") => "Woodland resources, forest homes and habitat restoration.",
                    Some("ridge") => "Upland homes, exposed stone and mining work.",
                    _ => "Follow the paths to another community.",
                };
                let distance = here
                    .map(|here| {
                        format!(
                            "{} m away · ",
                            (here.dist2(pos) as f64).sqrt() as u32 / 1000
                        )
                    })
                    .unwrap_or_default();
                rows.push(Row::action(
                    format!(
                        "destination_{}",
                        destination["key"].as_str().unwrap_or(name)
                    ),
                    format!("Travel to {name}"),
                    format!("{distance}{description}"),
                    Action::Walk(pos),
                    ready,
                ));
            }
        }
        rows.push(Row::heading("landcare", "Care for the landscape"));
        rows.push(Row::note("landcare_intro", "What you take changes the world", "Fish stocks, trees and forage recover over time. Approach a resource site to inspect it, plant tree cuttings or restore its habitat."));
        for site in array(&regional["nearby_sites"]) {
            let (Some(target), Some(pos)) = (id(&site["id"]), position(&site["position"])) else {
                continue;
            };
            let resource = friendly(site["kind"].as_str().unwrap_or("Resource"));
            rows.push(Row::note(
                format!("ecology_{target}"),
                format!("{resource} site"),
                format!(
                    "{} / {} available · Habitat {}%\nSoil {}% · Disturbance {}%{}",
                    site["remaining"],
                    site["capacity"],
                    site["vitality_percent"],
                    site["soil_percent"],
                    site["disturbance_percent"],
                    site["growth_hours"]
                        .as_u64()
                        .map_or(String::new(), |hours| format!(
                            "\n{} plantings growing · Next growth in {hours} game hours",
                            site["plantings"]
                        ))
                ),
            ));
            for action in array(&site["actions"]) {
                let verb = action["verb"].as_str().unwrap_or("");
                let label = if verb == "plant_tree" {
                    "Plant a tree cutting"
                } else {
                    "Restore habitat"
                };
                let available = action["available"].as_bool() == Some(true);
                let detail = if available {
                    if verb == "plant_tree" {
                        "Uses 1 log cutting and 1 water. Growth takes time."
                    } else {
                        "Uses 2 fiber and 1 water. Improves delayed recovery."
                    }
                } else {
                    action["reason"].as_str().unwrap_or("Unavailable")
                };
                rows.push(Row::action(
                    format!("{verb}_{target}"),
                    label,
                    detail,
                    work(label, verb, Some(target), json!({}), Some(pos)),
                    ready && available,
                ));
            }
        }
    } else {
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
            format!("Gather {}", friendly(kind)),
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
            "Inspect this plot",
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
            "Open workshop recipes",
            "See materials and choose what to make.",
            Action::Open(Tab::Craft),
            true,
        ));
    }
    if let Some(neighbour) = resident(observed, target.0)
        && array(&perception["sensed"])
            .iter()
            .any(|person| id(&person["id"]) == Some(target.0) && person["kind"] == "actor")
    {
        // By name once they are known: "Talk to Ada".
        let name = neighbour["name"]
            .as_str()
            .map(|name| name.rsplit_once(" #").map_or(name, |(name, _)| name))
            .filter(|name| !name.is_empty())
            .unwrap_or("your neighbour");
        return Some(Row::action(
            "context",
            format!("Talk to {name}"),
            "Choose a greeting or a conversation.",
            Action::Person(target.0),
            true,
        ));
    }
    None
}
