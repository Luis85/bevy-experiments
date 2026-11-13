//! Player guidance projected from an actor-scoped observation, never world truth.
//! This is a current objective, not a reward/checklist simulation. It introduces
//! no counters, saved state, grants or schedule. Actions remain suggestions for
//! the normal native/agent adapter to submit and revalidate through shared verbs.
use super::{catalog, homestead};
use npc_framework::prelude::{SimId, SimPos};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActionHint {
    pub verb: String,
    pub target: Option<SimId>,
    pub args: Value,
    /// Only actor-owned/observed positions are copied here. Target identities
    /// normally resolve through the native adapter's known-position provider.
    pub position: Option<SimPos>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub current: u32,
    pub target: u32,
    pub unit: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Task {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub progress: Option<Progress>,
    pub action: Option<ActionHint>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Journey {
    pub title: &'static str,
    pub home_built: bool,
    pub next: Task,
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}
fn count(value: &Value) -> u32 {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}
fn id(value: &Value) -> Option<SimId> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|id| *id > 0)
        .map(SimId)
}
fn position(value: &Value) -> Option<SimPos> {
    let (x, z) = if let Some(values) = value.as_array() {
        (values.first()?.as_i64()?, values.get(1)?.as_i64()?)
    } else {
        (value.get("x")?.as_i64()?, value.get("z")?.as_i64()?)
    };
    Some(SimPos::new(i32::try_from(x).ok()?, i32::try_from(z).ok()?))
}
fn action(verb: &str, target: Option<SimId>, args: Value) -> Option<ActionHint> {
    Some(ActionHint {
        verb: verb.into(),
        target,
        args,
        position: None,
    })
}
fn task(
    id: &'static str,
    title: &'static str,
    detail: impl Into<String>,
    action: Option<ActionHint>,
) -> Task {
    Task {
        id,
        title,
        detail: detail.into(),
        progress: None,
        action,
    }
}
fn resources<'a>(village: &'a Value, kind: &str) -> Option<&'a Value> {
    array(&village["environment"]["resources"])
        .iter()
        .find(|source| source["kind"] == kind && source["remaining"].as_u64() != Some(0))
}
fn gather(village: &Value, kind: &str) -> Option<ActionHint> {
    let source = resources(village, kind)?;
    action("gather", Some(id(&source["id"])?), json!({"quantity":1}))
}
/// The good that plays `name` in this world (`drink`, `baked`, `timber`,
/// ...), as the observation's catalog roles say; empty when none.
fn role<'a>(village: &'a Value, name: &str) -> &'a str {
    village["economy"]["roles"][name].as_str().unwrap_or("")
}
fn provision(village: &Value, kind: &str, expanded: bool) -> Task {
    let (key, title, detail) = if kind == role(village, "drink") {
        (
            "water",
            "Keep water in your pack",
            "Collect clean water at the village well. Carry a little extra before starting work.",
        )
    } else {
        (
            "food",
            "Find something to eat",
            if expanded {
                "Explore the southeastern berry bushes or buy food from a nearby seller. Food must be in your pack before you can eat it."
            } else {
                "Look for berry bushes or buy food from a nearby seller. Food must be in your pack before you can eat it."
            },
        )
    };
    task(key, title, detail, gather(village, kind))
}

/// Interpret only the given observation. Unknown plots, resources and residents
/// never become targets just because the operator could inspect the whole world.
pub fn project(observation: &Value) -> Journey {
    let village = &observation["sections"]["village"];
    let farm = &observation["sections"]["homestead"];
    // Only the authored expanded island exposes homesteads. The fixed island
    // has different resource geography and receives direction-neutral help.
    let expanded = !farm.is_null();
    let plots = array(&farm["plots"]);
    let home = plots
        .iter()
        .find(|plot| plot["own"] == true && plot["use_kind"] == "housing");
    let home_built = home.is_some_and(|plot| plot["cottage_built"] == true);
    let wrap = |next| Journey {
        title: if home_built {
            "Life on the island"
        } else {
            "Your first homestead"
        },
        home_built,
        next,
    };
    if village.is_null() {
        return wrap(task(
            "arrival",
            "Arrive on the island",
            "Your life begins when arrival and character control are confirmed.",
            None,
        ));
    }
    if village["alive"] == false {
        return wrap(task(
            "life_ended",
            "This life has ended",
            "The village and your legacy remain. Choose a successor to begin a new life.",
            None,
        ));
    }
    let mut stock = BTreeMap::<String, u32>::new();
    for item in array(&observation["sections"]["inventory"]["items"]) {
        if let Some(kind) = item["kind"].as_str() {
            let held = stock.entry(kind.into()).or_default();
            *held = held.saturating_add(count(&item["stack"]));
        }
    }
    let held = |kind: &str| stock.get(kind).copied().unwrap_or(0);
    let food = [
        role(village, "baked"),
        role(village, "cooked"),
        role(village, "foraged"),
    ]
    .into_iter()
    .find(|kind| !kind.is_empty() && held(kind) > 0);
    let need = |kind: &str| village["needs"][kind].as_i64().unwrap_or(100_000);
    let busy = village["activity"]["busy"] == true;
    if busy && (need("hunger") < 15_000 || need("thirst") < 15_000) {
        return wrap(task(
            "survival_pause",
            "Take care of yourself first",
            "Stop your work, then eat or drink. Unused construction materials stay yours.",
            action("stop_work", None, json!({})),
        ));
    }
    // Moderate needs do not implicitly cancel a committed activity through an
    // approach action. Critical needs above offer an explicit stop first.
    if busy {
        let building = &farm["building"];
        if !building.is_null() {
            let duration = count(&farm["cottage"]["duration_ticks"]).max(1);
            let now = observation["tick"].as_u64().unwrap_or(0);
            let remaining = building["ends"]
                .as_u64()
                .unwrap_or(now)
                .saturating_sub(now)
                .min(u64::from(duration)) as u32;
            let mut next = task(
                "building",
                "Your cottage is taking shape",
                "Stay by the site while construction uses your reserved materials.",
                None,
            );
            next.progress = Some(Progress {
                current: duration - remaining,
                target: duration,
                unit: "work",
            });
            return wrap(next);
        }
        return wrap(task(
            "working",
            "Finish your current activity",
            "Your current work is still in progress. You can stop it from the action controls.",
            None,
        ));
    }
    if need("thirst") < 30_000 {
        return wrap(if held(role(village, "drink")) > 0 {
            task(
                "drink",
                "Take a drink",
                "Use one clean water from your pack before continuing.",
                action("drink", None, json!({})),
            )
        } else {
            provision(village, role(village, "drink"), expanded)
        });
    }
    if need("hunger") < 30_000 {
        return wrap(if let Some(kind) = food {
            task(
                "eat",
                "Have something to eat",
                "Eat from your own pack to recover hunger.",
                action("eat", None, json!({"kind":kind})),
            )
        } else {
            provision(village, role(village, "foraged"), expanded)
        });
    }
    if need("energy") < 25_000 {
        let mut next = task(
            "rest",
            "Head home to rest",
            "Resting at your assigned home is free and restores energy faster than resting on the ground.",
            action("rest_home", None, json!({})),
        );
        if let Some(action) = &mut next.action {
            action.position = position(&village["home"]);
        }
        return wrap(next);
    }
    if held(role(village, "drink")) == 0 {
        return wrap(provision(village, role(village, "drink"), expanded));
    }
    if food.is_none() {
        return wrap(provision(village, role(village, "foraged"), expanded));
    }
    if farm.is_null() {
        return wrap(task(
            "village_life",
            "Explore village life",
            "Visit known workplaces, talk to residents and trade your real supplies.",
            None,
        ));
    }
    if !home_built {
        let inputs = array(&farm["cottage"]["inputs"]);
        let required = |kind: &str, fallback| {
            inputs
                .iter()
                .find(|input| input["kind"] == kind)
                .map_or(fallback, |input| count(&input["count"]))
        };
        let (timber, lumber) = (role(village, "timber"), role(village, "lumber"));
        let plank_target = required(lumber, homestead::PLANK_COST);
        let logs_for_planks = plank_target
            .saturating_sub(held(lumber))
            .div_ceil(catalog::SAW_PLANK_YIELD)
            .saturating_mul(catalog::SAW_LOG_COST);
        let log_target = required(timber, homestead::LOG_COST).saturating_add(logs_for_planks);
        if held(timber) < log_target {
            let mut next = task(
                "timber",
                "Gather timber for a cottage",
                format!(
                    "Keep {log_target} logs: enough to saw missing planks and retain the cottage's timber. Explore the western timber yard if the wood source is not yet known."
                ),
                gather(village, timber),
            );
            next.progress = Some(Progress {
                current: held(timber),
                target: log_target,
                unit: "logs",
            });
            return wrap(next);
        }
        if held(lumber) < plank_target {
            let station = array(&village["environment"]["workstations"])
                .iter()
                .find(|station| {
                    array(&station["recipes"])
                        .iter()
                        .any(|recipe| recipe["name"] == catalog::SAW)
                });
            let action = station
                .and_then(|station| id(&station["id"]))
                .and_then(|target| {
                    action(&format!("craft:{}", catalog::SAW), Some(target), json!({}))
                });
            let mut next = task(
                "planks",
                "Saw your timber into planks",
                "Use the carpenter's workshop. Each batch turns one real log into four planks; keep the remaining logs for your cottage.",
                action,
            );
            next.progress = Some(Progress {
                current: held(lumber),
                target: plank_target,
                unit: "planks",
            });
            return wrap(next);
        }
        if let Some(home) = home {
            let mut hint =
                id(&home["id"]).and_then(|target| action("build_cottage", Some(target), json!({})));
            if let Some(action) = &mut hint {
                action.position = position(&home["front_door"]);
            }
            return wrap(task(
                "cottage",
                "Build your first home",
                "Your materials are ready. Begin construction at your plot's front door; completion gives you a real home and crop beds.",
                hint,
            ));
        }
        let plot = plots.iter().find(|plot| {
            plot["use_kind"] == "housing"
                && plot["state_known"] == true
                && (plot["owner"].is_null() || plot["owner_alive"] == false)
        });
        let mut hint = plot
            .and_then(|plot| id(&plot["id"]))
            .and_then(|target| action("claim_plot", Some(target), json!({})));
        if let (Some(action), Some(plot)) = (&mut hint, plot) {
            action.position = position(&plot["front_door"]);
        }
        return wrap(task(
            "claim",
            "Choose a place to settle",
            "Explore the southern house lane, then claim a housing plot. Living owners' estates are protected.",
            hint,
        ));
    }
    for stage in ["needs_water", "ready"] {
        for plot in plots.iter().filter(|plot| plot["own"] == true) {
            if let Some(bed) = array(&plot["beds"])
                .iter()
                .find(|bed| bed["stage"] == stage)
            {
                let (key, title, detail, verb) = if stage == "needs_water" {
                    (
                        "water_crop",
                        "Water your crop",
                        "Use one carried clean water at the chosen bed to begin its growth.",
                        "water_crop",
                    )
                } else {
                    (
                        "harvest",
                        "Your harvest is ready",
                        "Collect the mature crop. Keep seed for the next planting; eat, craft or trade the surplus.",
                        "harvest",
                    )
                };
                let mut hint = id(&plot["id"]).and_then(|target| {
                    action(verb, Some(target), json!({"bed":count(&bed["index"])}))
                });
                if let Some(action) = &mut hint {
                    action.position = position(&bed["position"]);
                }
                return wrap(task(key, title, detail, hint));
            }
        }
    }
    let mut crop_kinds: Vec<_> = array(&farm["crops"])
        .iter()
        .filter_map(|crop| crop["kind"].as_str())
        .collect();
    if crop_kinds.is_empty() {
        crop_kinds.push(
            farm["crop"]["seed_kind"]
                .as_str()
                .unwrap_or(role(village, "grain")),
        );
    }
    let seed = crop_kinds.iter().copied().find(|kind| held(kind) > 0);
    let active_garden = plots.iter().filter(|plot| plot["own"] == true).any(|plot| {
        array(&plot["beds"]).iter().any(|bed| {
            matches!(
                bed["stage"].as_str(),
                Some("needs_water" | "growing" | "ready")
            )
        })
    });
    for plot in plots.iter().filter(|plot| plot["own"] == true) {
        if let Some(bed) = array(&plot["beds"])
            .iter()
            .find(|bed| bed["stage"] == "empty")
        {
            if let Some(kind) = seed {
                let mut hint = id(&plot["id"]).and_then(|target| {
                    action(
                        "plant",
                        Some(target),
                        json!({"bed":count(&bed["index"]),"kind":kind}),
                    )
                });
                if let Some(action) = &mut hint {
                    action.position = position(&bed["position"]);
                }
                let detail = if crop_kinds.contains(&role(village, "foraged")) {
                    "Plant carried grain for milling and trade, or a berry for an edible crop. Planting consumes a real seed or berry."
                } else {
                    "Plant a carried grain seed in an empty bed, then water it to begin growth."
                };
                return wrap(task(
                    "plant",
                    if active_garden {
                        "Plant another bed"
                    } else {
                        "Start your garden"
                    },
                    detail,
                    hint,
                ));
            }
            return wrap(task(
                "seed",
                if active_garden {
                    "Find seed for another bed"
                } else {
                    "Bring a seed home"
                },
                if active_garden {
                    "Your planted beds are growing. Gather seed when you want to expand; the garden controls show crop choices."
                } else {
                    "Gather a supported seed before planting an empty bed. The garden controls show this world's crop choices."
                },
                crop_kinds.iter().find_map(|kind| gather(village, kind)),
            ));
        }
    }
    let surplus = [role(village, "grain"), role(village, "foraged")]
        .into_iter()
        .find(|kind| {
            held(kind) > 3
                && !array(&village["economy"]["own_offers"])
                    .iter()
                    .any(|offer| offer["kind"] == *kind)
        });
    if let Some(kind) = surplus {
        return wrap(task(
            "trade",
            "Offer a little surplus",
            "Keep some provisions and seed. A listing earns coins only when a resident actually walks over and buys it.",
            action(
                "offer_goods",
                None,
                json!({"kind":kind,"quantity":(held(kind)-1).min(8)}),
            ),
        ));
    }
    let neighbor = array(&village["nearby_market"])
        .first()
        .and_then(|person| id(&person["id"]));
    wrap(task(
        "neighbors",
        "Make a life in the village",
        "While your crops grow, meet a neighbor, use the workshops or explore. The village keeps producing and trading around you.",
        neighbor.and_then(|target| {
            action(
                "talk",
                Some(target),
                json!({"text":"Hello. I am settling into the village."}),
            )
        }),
    ))
}
