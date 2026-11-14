//! Site selection and the selected site's work are separate presentation states.
use super::*;

pub(super) fn preferred_plot(observed: &Value, tab: Tab, current: Option<u32>) -> Option<u32> {
    let plots: Vec<_> = array(&observed["sections"]["homestead"]["plots"])
        .iter()
        .filter(|plot| tab != Tab::Build || plot["use_kind"] == "housing")
        .collect();
    let current = plots
        .iter()
        .copied()
        .find(|plot| id(&plot["id"]) == current);
    if current.is_some_and(|plot| tab != Tab::Garden || plot["own"] == true) {
        return current.and_then(|plot| id(&plot["id"]));
    }
    plots
        .iter()
        .copied()
        .filter(|plot| plot["own"] == true)
        .min_by_key(|plot| (plot["use_kind"] != "housing", id(&plot["id"])))
        .or(current)
        .and_then(|plot| id(&plot["id"]))
}

pub(super) fn preferred_bed(observed: &Value, plot: Option<u32>) -> Option<u32> {
    let plot = array(&observed["sections"]["homestead"]["plots"])
        .iter()
        .find(|candidate| id(&candidate["id"]) == plot)?;
    array(&plot["beds"])
        .iter()
        .min_by_key(|bed| {
            (
                match bed["stage"].as_str() {
                    Some("needs_water") => 0,
                    Some("ready") => 1,
                    Some("growing") => 2,
                    Some("empty") => 3,
                    _ => 4,
                },
                id(&bed["index"]),
            )
        })
        .and_then(|bed| id(&bed["index"]))
}

fn ownership(plot: &Value) -> &'static str {
    if plot["own"] == true {
        "Your land"
    } else if plot["state_known"] != true {
        "Go there to see whose it is"
    } else if plot["owner"].is_null() || plot["owner_alive"] == false {
        "Available"
    } else {
        "Claimed by a neighbour"
    }
}

pub(super) fn rows(
    observed: &Value,
    state: &PlayerState,
    stock: &BTreeMap<String, u64>,
    ready: bool,
) -> Vec<Row> {
    let farm = &observed["sections"]["homestead"];
    let building = state.tab == Some(Tab::Build);
    let plots: Vec<_> = array(&farm["plots"])
        .iter()
        .filter(|plot| !building || plot["use_kind"] == "housing")
        .collect();
    let mut rows = Vec::new();
    let selected = plots
        .iter()
        .copied()
        .find(|plot| id(&plot["id"]) == state.plot);
    let Some(plot) = selected else {
        rows.push(Row::note(
            "land_intro",
            if building {
                "Choose a house site"
            } else {
                "Choose a growing space"
            },
            "Select a discovered site to see its ownership and available work.",
        ));
        if plots.is_empty() {
            rows.push(Row::note(
                "land_empty",
                "Find the house lane",
                "Explore south of the village to discover house sites and their garden beds.",
            ));
        }
        for plot in plots {
            let Some(target) = id(&plot["id"]) else {
                continue;
            };
            rows.push(Row::action(
                format!("plot_{target}"),
                plot["name"].as_str().unwrap_or("Plot"),
                ownership(plot),
                Action::Plot(target),
                true,
            ));
        }
        rows.push(Row::action(
            "explore_link",
            "Explore for a site",
            "Directions and places you have discovered.",
            Action::Open(Tab::Explore),
            true,
        ));
        return rows;
    };
    let target = id(&plot["id"]).expect("selected plot has an identity");
    let door = position(&plot["front_door"]);
    let mut summary = Row::heading(
        "selected_site",
        plot["name"].as_str().unwrap_or("Your site"),
    );
    summary.detail = ownership(plot).into();
    rows.push(summary);
    if plot["own"] != true {
        if plot["state_known"] != true
            && let Some(pos) = door
        {
            rows.push(Row::action(
                "visit_site",
                "Walk to this site",
                "Go there to see whose it is.",
                Action::Walk(pos),
                ready,
            ));
        }
        let available = plot["state_known"] == true
            && (plot["owner"].is_null() || plot["owner_alive"] == false);
        rows.push(Row::action(
            "claim_site",
            "Claim this site",
            if plot["state_known"] != true {
                "Go there to see whose it is first."
            } else if available {
                "You may own one house site and one garden. Claiming grants no supplies."
            } else {
                "This land belongs to a living neighbour."
            },
            work(
                "Claim your land",
                "claim_plot",
                Some(target),
                json!({}),
                door,
            ),
            ready && available,
        ));
    } else if building {
        if farm["building"]["plot"].as_u64() == Some(u64::from(target)) {
            let remaining = farm["building"]["ends"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(observed["tick"].as_u64().unwrap_or(0));
            rows.push(Row::note(
                "building",
                "Your cottage is taking shape",
                format!(
                    "About {} game seconds remain. Stop releases unspent reserved materials.",
                    remaining.div_ceil(20)
                ),
            ));
        } else if plot["cottage_built"] == true {
            rows.push(Row::note(
                "home_complete",
                "Welcome home",
                "Your cottage is complete. Rest at its door or tend its garden.",
            ));
            rows.push(Row::action(
                "open_garden",
                "Tend your garden",
                "",
                Action::Open(Tab::Garden),
                true,
            ));
            rows.push(Row::action(
                "home_rest",
                "Go home and rest",
                "",
                work("Rest at home", "rest_home", None, json!({}), door),
                ready,
            ));
            home_heating(&observed["sections"]["village"], &mut rows);
        } else {
            // What the cottage takes, as chips on its own row; the button
            // keeps its place and says what is short.
            let inputs = recipe_inputs(stock, &farm["cottage"]["inputs"]);
            let have = inputs.iter().all(|(_, owned, wanted)| owned >= wanted);
            rows.push(Row {
                reason: recipe_reason(&inputs, true),
                inputs,
                ..Row::action(
                    "build_home",
                    "Build this cottage",
                    "Walk to the site and build with the materials in your pouch.",
                    work(
                        "Build your cottage",
                        "build_cottage",
                        Some(target),
                        json!({}),
                        door,
                    ),
                    ready && have,
                )
            });
            rows.push(Row::action(
                "source_materials",
                "Find building supplies",
                "Gather logs and make planks at the carpenter's workshop.",
                Action::Open(Tab::Craft),
                true,
            ));
        }
    } else {
        garden(observed, state, stock, ready, plot, target, &mut rows);
    }
    rows.push(Row::action(
        "change_site",
        "Change site",
        "Choose another discovered plot.",
        Action::ChoosePlot,
        true,
    ));
    if plot["state_known"] == true
        && let Some(pos) = door
    {
        rows.push(Row::action(
            "visit_site",
            "Walk to this site",
            "",
            Action::Walk(pos),
            ready,
        ));
    }
    rows
}

fn bed_stage(bed: &Value) -> &'static str {
    match bed["stage"].as_str() {
        Some("empty") => "Empty soil",
        Some("needs_water") => "Needs water",
        Some("growing") => "Growing",
        Some("ready") => "Ready to harvest",
        _ => "Go there to look",
    }
}

fn garden(
    observed: &Value,
    state: &PlayerState,
    stock: &BTreeMap<String, u64>,
    ready: bool,
    plot: &Value,
    target: u32,
    rows: &mut Vec<Row>,
) {
    let farm = &observed["sections"]["homestead"];
    let held = |kind: &str| stock.get(kind).copied().unwrap_or(0);
    let beds = array(&plot["beds"]);
    let selected = beds.iter().find(|bed| id(&bed["index"]) == Some(state.bed));
    if let Some(bed) = selected {
        let mut current = Row::heading(
            "selected_bed",
            &format!("Bed {} · {}", state.bed + 1, bed_stage(bed)),
        );
        current.detail = bed["kind"].as_str().map(friendly).unwrap_or_default();
        rows.push(current);
        let pos = position(&bed["position"]);
        match bed["stage"].as_str().unwrap_or("") {
            "empty" => {
                rows.push(Row::action(
                    "plant",
                    format!("Plant {} here", friendly(&state.crop)),
                    format!("Uses one seed · {} in your pouch", held(&state.crop)),
                    work(
                        "Plant your crop",
                        "plant",
                        Some(target),
                        json!({"bed":state.bed,"kind":state.crop}),
                        pos,
                    ),
                    ready && held(&state.crop) > 0,
                ));
                for crop in array(&farm["crops"]) {
                    let kind = crop["kind"]
                        .as_str()
                        .or_else(|| crop["seed_kind"].as_str())
                        .unwrap_or(catalog::GRAIN);
                    let mut row = Row::action(
                        format!("crop_{kind}"),
                        crop["label"].as_str().unwrap_or(kind),
                        format!("{} seeds", held(kind)),
                        Action::Crop(kind.into()),
                        true,
                    );
                    row.compact = true;
                    row.selected = state.crop == kind;
                    rows.push(row);
                }
            }
            "needs_water" => rows.push(Row::action(
                "water",
                "Water this bed",
                format!(
                    "Uses one clean water · {} in your pouch",
                    held(catalog::WATER)
                ),
                work(
                    "Water your crop",
                    "water_crop",
                    Some(target),
                    json!({"bed":state.bed}),
                    pos,
                ),
                ready && held(catalog::WATER) > 0,
            )),
            "ready" => rows.push(Row::action(
                "harvest",
                "Harvest this bed",
                "The harvest goes into your pouch.",
                work(
                    "Harvest your crop",
                    "harvest",
                    Some(target),
                    json!({"bed":state.bed}),
                    pos,
                ),
                ready,
            )),
            "growing" => {
                rows.push(Row::note(
                    "growth",
                    "Growing in its own time",
                    "It needs a little longer. Explore, buy and sell, or rest while it grows.",
                ));
            }
            _ => rows.push(Row::note(
                "bed_unknown",
                "Visit this bed",
                "Look at it before you choose work.",
            )),
        }
    }
    if beds.len() > 1 {
        rows.push(Row::heading("other_beds", "Choose a bed"));
    }
    for bed in beds {
        let Some(index) = id(&bed["index"]) else {
            continue;
        };
        let mut row = Row::action(
            format!("bed_{index}"),
            format!("Bed {} · {}", index + 1, bed_stage(bed)),
            bed["kind"].as_str().map(friendly).unwrap_or_default(),
            Action::Bed(index),
            true,
        );
        row.compact = true;
        row.selected = state.bed == index;
        rows.push(row);
    }
}
