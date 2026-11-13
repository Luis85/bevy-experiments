//! A village that grows: buildings as data, recorded placement, construction
//! sites, stores and stalls, hauling and the civic planner (regional profile).
//!
//! Scripted tests move a resident next to a site or a store and call the same
//! validated operations a leaf or a player command uses, so every transfer is
//! checked for coin and goods conservation without waiting for a walk. The
//! live tests let the residents do everything themselves.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::TestbedIds;
use island_content::village::{
    self, CoinPurse, VillageAct, VillageState, Villager, Wallet, civic, colony, economy,
    frontier::{
        economy_spec::EconomySpec,
        labor::{self, ResidentProfile},
        settlements,
        terrain::{self, TerrainState, TerrainSurface},
    },
    market,
};
use npc_framework::interaction::Item;
use npc_framework::kernel::commands::{CommandRegistry, SimCommand, submit};
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};

const SEED: u64 = 7;

fn shipped() -> App {
    island_content::new_sim_app(SEED, "living_world")
}

/// The shipped world with its `living_world` block edited as JSON only.
fn try_world_with(edit: impl FnOnce(&mut Value)) -> Result<App, String> {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    edit(&mut doc.world["living_world"]);
    no_founding(&mut doc.world["living_world"]);
    island_content::scenario::load_scenario(&doc, SEED).map(|loaded| loaded.app)
}

/// These tests are about what residents and the planner build: no founding
/// field or trading post stands, no boat calls and nobody is born or lands.
/// `tests/island_fields.rs`, `island_merchant.rs` and `island_demography.rs`
/// cover those.
fn no_founding(world: &mut Value) {
    world["economy"]["merchant"]["enabled"] = json!(false);
    world["economy"]["demography"]["enabled"] = json!(false);
    // Without fields a grain shortage would be the planner's first reason.
    world["economy"]["tuning"]["planner_crop_shortage_units"] = json!(100_000);
    if let Some(buildings) = world["economy"]["buildings"].as_array_mut() {
        for def in buildings {
            if def.get("field").is_some() {
                def["field"]["founding"] = json!(false);
            }
        }
    }
}
fn world_with(edit: impl FnOnce(&mut Value)) -> App {
    try_world_with(edit).unwrap_or_else(|error| panic!("{error}"))
}

/// Small free buildings that isolate one rule each; all of it is data.
fn test_buildings(world: &mut Value) {
    let anywhere = json!({"surfaces": ["grass", "soil", "stone"], "max_slope_mm": 3000,
        "clearance": 0});
    let list = world["economy"]["buildings"]
        .as_array_mut()
        .expect("buildings");
    for (id, kind, solid) in [
        ("pad", json!({"type": "storage", "capacity": 40}), false),
        ("post", json!({"type": "storage", "capacity": 40}), true),
        ("kiosk", json!({"type": "stall", "capacity": 40}), false),
    ] {
        list.push(json!({"id": id, "display": id, "work_ticks": 20,
            "footprint": {"width": 1, "depth": 1}, "kind": kind, "solid": solid,
            "civic": false, "placement": anywhere}));
    }
    list.push(json!({"id": "woodpile", "display": "Woodpile",
        "cost": [{"good": "log", "count": 4}], "work_ticks": 20,
        "footprint": {"width": 2, "depth": 2}, "kind": {"type": "storage", "capacity": 40},
        "solid": false, "civic": false}));
}

fn settlement(world: &World, key: &str) -> SimId {
    world
        .resource::<TestbedIds>()
        .id(&format!("settlement_{key}"))
}

fn entity(world: &World, id: SimId) -> Entity {
    world.resource::<SimIndex>().get(id).expect("entity")
}

fn residents(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world.get::<ResidentProfile>(**entity).is_some()
                && settlements::member(world, **entity) == Some(settlement)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

/// Every coin in the world: wallets, dropped purses and the civic fund.
fn money(world: &World) -> u64 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(world.get::<Wallet>(*entity).map_or(0, |w| w.coins))
                + u64::from(world.get::<CoinPurse>(*entity).map_or(0, |p| p.coins))
        })
        .sum::<u64>()
        + u64::from(world.resource::<VillageState>().fund)
}

/// Every unit of a good: in packs, on the ground, in stores and in sites.
fn units(world: &mut World, kind: &str) -> u32 {
    let items: u32 = world
        .query::<&Item>()
        .iter(world)
        .filter(|item| item.kind == kind)
        .map(|item| item.stack)
        .sum();
    items + civic::goods_in_custody(world, kind)
}

/// Moves existing coins from residents into the civic fund, as a levy would:
/// the reserve kept for destitution grants plus `coins` to spend on building.
fn fund(world: &mut World, coins: u32) {
    let reserve = village::frontier::economy_spec::tuning(world)
        .expect("tuning")
        .civic_reserve;
    let mut left = coins + reserve;
    let people: Vec<Entity> = world.resource::<SimIndex>().map.values().copied().collect();
    for actor in people {
        if left == 0 {
            break;
        }
        if world.get::<ResidentProfile>(actor).is_none() {
            continue;
        }
        let part = world
            .get::<Wallet>(actor)
            .map_or(0, |wallet| wallet.coins.saturating_sub(100))
            .min(left)
            .min(20);
        world.get_mut::<Wallet>(actor).expect("wallet").coins -= part;
        world.resource_mut::<VillageState>().fund += part;
        left -= part;
    }
    assert_eq!(left, 0, "residents could not fund {coins} coins");
}

fn teleport(world: &mut World, actor: Entity, to: SimPos) {
    // A test that moves somebody wants it awake: a late riser may still be
    // in bed in the first minutes of the day.
    if let Some(mut person) = world.get_mut::<Villager>(actor) {
        person.resting_until = 0;
    }
    world.entity_mut(actor).remove::<MoveIntent>().insert(to);
}

fn place(app: &mut App, building: &str, settlement: SimId) -> SimId {
    let def = civic::definition(app.world(), building)
        .expect("definition")
        .clone();
    let (x, z, rotation) =
        civic::find_site(app.world(), &def, settlement, 36).expect("a buildable site");
    civic::PlaceBuilding {
        building: building.into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed")
}

/// Lets `builder` work a supplied site to completion.
fn build(app: &mut App, site: SimId, builder: Entity) {
    let target = entity(app.world(), site);
    let door = *app.world().get::<SimPos>(target).expect("site position");
    let id = *app.world().get::<SimId>(builder).expect("builder identity");
    let mut rounds = 0;
    while app.world().get::<civic::ConstructionSite>(target).is_some() {
        rounds += 1;
        assert!(rounds < 4000, "the site was never finished");
        if app.world().get::<civic::SiteWork>(builder).is_none() {
            // Whatever the builder was doing ends through the shared stop action.
            let _ = VillageAct::new(id, "stop_work").perform(app.world_mut());
            teleport(app.world_mut(), builder, door);
            civic::begin_work(app.world_mut(), builder, target).expect("work starts");
        }
        run_ticks(app.world_mut(), 5);
    }
}

/// Places a free building and has a resident raise it; returns its identity.
fn raise(app: &mut App, building: &str, settlement: SimId) -> SimId {
    let site = place(app, building, settlement);
    let builder = residents(app.world(), settlement)[0].1;
    build(app, site, builder);
    site
}

/// A resident of the settlement carrying at least `count` free units of `kind`.
fn holder(world: &World, settlement: SimId, kind: &str, count: u32) -> Entity {
    residents(world, settlement)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| economy::count(world, *actor, kind) >= count)
        .unwrap_or_else(|| panic!("nobody carries {count} {kind}"))
}

fn effects(world: &World, kind: &str) -> Vec<Value> {
    world
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some(kind) => {
                Some(serde_json::to_value(effect).expect("effect JSON"))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_shipped_world_states_the_built_in_buildings_and_they_express_its_layout() {
    let default = EconomySpec::default();
    let app = shipped();
    assert_eq!(civic::catalog(app.world()), default.buildings.as_slice());
    let kinds: Vec<&str> = default
        .buildings
        .iter()
        .map(|def| def.kind.name())
        .collect();
    assert_eq!(kinds.iter().filter(|kind| **kind == "home").count(), 2);
    assert_eq!(
        kinds.iter().filter(|kind| **kind == "workplace").count(),
        13
    );
    // The field is a storage building with plots; the trading post is a stall.
    assert_eq!(kinds.iter().filter(|kind| **kind == "storage").count(), 3);
    assert_eq!(kinds.iter().filter(|kind| **kind == "stall").count(), 2);
    assert_eq!(
        default
            .buildings
            .iter()
            .filter(|def| def.field.is_some())
            .count(),
        1
    );
    // Every authored workplace has a definition with its profession and station.
    let harbour = settlement(app.world(), "harbour");
    let definition = settlements::definition(app.world(), harbour).expect("harbour");
    for site in &definition.sites {
        let station = default
            .profession(site.role)
            .expect("profession")
            .station
            .clone();
        assert!(
            default.buildings.iter().any(|def| matches!(&def.kind,
                village::frontier::building_spec::BuildingKind::Workplace {
                    profession, station: named, capacity
                } if *profession == site.role && *named == station && *capacity == site.capacity)),
            "no building definition for {}",
            site.label
        );
    }
    // The authored homes are cottages: two beds each.
    assert!(definition.homes.iter().all(|home| home.capacity == 2));
    // Building data adds no entity: the founding layout and identities stay.
    assert!(civic::sites(app.world()).is_empty());
    assert!(civic::buildings(app.world()).is_empty());
    assert_eq!(residents(app.world(), harbour).len(), 14);
}

#[test]
fn building_definitions_are_validated_with_errors_naming_the_entry() {
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"id": "cottage", "display": "Again", "work_ticks": 10,
                "footprint": {"width": 2, "depth": 2}, "kind": {"type": "home", "beds": 2}}),
            "duplicate building `cottage`",
        ),
        (
            json!({"id": "hall", "display": "Hall", "work_ticks": 10,
                "cost": [{"good": "marble", "count": 2}],
                "footprint": {"width": 2, "depth": 2}, "kind": {"type": "home", "beds": 2}}),
            "building `hall` cost references unknown good `marble`",
        ),
        (
            json!({"id": "hall", "display": "Hall", "work_ticks": 10,
                "cost": [{"good": "axe", "count": 1}],
                "footprint": {"width": 2, "depth": 2}, "kind": {"type": "home", "beds": 2}}),
            "building `hall` cost `axe` is a tool or clothing",
        ),
        (
            json!({"id": "hall", "display": "Hall", "work_ticks": 0,
                "footprint": {"width": 2, "depth": 2}, "kind": {"type": "home", "beds": 2}}),
            "building `hall` work_ticks must be 1..=100000",
        ),
        (
            json!({"id": "hall", "display": "Hall", "work_ticks": 10,
                "footprint": {"width": 0, "depth": 2}, "kind": {"type": "home", "beds": 2}}),
            "building `hall` footprint must be 1..=12 cells",
        ),
        (
            json!({"id": "hall", "display": "Hall", "work_ticks": 10,
                "footprint": {"width": 2, "depth": 2}, "kind": {"type": "home", "beds": 0}}),
            "building `hall` is a home and needs 1..=8 beds",
        ),
        (
            json!({"id": "forge", "display": "Forge", "work_ticks": 10,
                "footprint": {"width": 2, "depth": 2},
                "kind": {"type": "workplace", "profession": "smith", "station": "village_saw"}}),
            "building `forge` is a workplace for `smith`, whose station is `village_smelt`",
        ),
        (
            json!({"id": "shed", "display": "Shed", "work_ticks": 10,
                "footprint": {"width": 2, "depth": 2},
                "kind": {"type": "storage", "capacity": 0}}),
            "building `shed` capacity must be 1..=10000 units",
        ),
        (
            json!({"id": "shed", "display": "Shed", "work_ticks": 10,
                "footprint": {"width": 2, "depth": 2},
                "kind": {"type": "storage", "capacity": 10},
                "placement": {"surfaces": []}}),
            "building `shed` placement needs at least one buildable surface",
        ),
    ];
    for (definition, expected) in cases {
        let Err(error) = try_world_with(|world| {
            world["economy"]["buildings"]
                .as_array_mut()
                .expect("buildings")
                .push(definition);
        }) else {
            panic!("`{expected}` was accepted");
        };
        assert!(
            error.contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }
    // Water is never a buildable surface, and unknown fields are refused.
    for broken in [
        json!({"id": "pier", "display": "Pier", "work_ticks": 10,
            "footprint": {"width": 2, "depth": 2}, "kind": {"type": "stall", "capacity": 5},
            "placement": {"surfaces": ["water"]}}),
        json!({"id": "pier", "display": "Pier", "work_ticks": 10, "floors": 2,
            "footprint": {"width": 2, "depth": 2}, "kind": {"type": "stall", "capacity": 5}}),
        json!({"id": "pier", "display": "Pier", "work_ticks": 10,
            "footprint": {"width": 2, "depth": 2}, "kind": {"type": "lighthouse"}}),
    ] {
        assert!(
            try_world_with(|world| {
                world["economy"]["buildings"]
                    .as_array_mut()
                    .expect("buildings")
                    .push(broken);
            })
            .is_err()
        );
    }
    // The planner's thresholds are tuning data with checked ranges.
    let Err(error) = try_world_with(|world| {
        world["economy"]["tuning"]["planner_review_ticks"] = json!(5);
    }) else {
        panic!("a five-tick review was accepted");
    };
    assert!(error.contains("tuning.planner_review_ticks"), "{error}");
}

fn set_cell(world: &mut World, x: i32, z: i32, height_mm: i32, surface: TerrainSurface) {
    let bounds = world.resource::<TerrainState>().bounds();
    let index = bounds.index(CellCoord::new(x, z)).expect("cell") as usize;
    let mut cell = world.resource::<TerrainState>().cells[index];
    cell.height_mm = height_mm;
    cell.surface = surface;
    world.resource_mut::<TerrainState>().cells[index] = cell;
    world
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(x, z), cell.navigation());
}

fn refused(world: &World, building: &str, x: i32, z: i32, rotation: u8, expected: &str) {
    let reason = civic::placement_check(world, building, x, z, rotation)
        .expect_err(&format!("`{expected}` was accepted"));
    assert!(
        reason.contains(expected),
        "expected `{expected}` in `{reason}`"
    );
}

#[test]
fn placement_is_validated_rule_by_rule_with_a_reason() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    let before = state_hash(app.world_mut());
    let cottage = civic::definition(app.world(), "cottage")
        .expect("cottage")
        .clone();
    let (x, z, rotation) =
        civic::find_site(app.world(), &cottage, harbour, 36).expect("cottage site");
    civic::placement_check(app.world(), "cottage", x, z, rotation).expect("valid site");
    let plot = civic::placement_plot(app.world(), "cottage", x, z, rotation).expect("plot");
    assert_eq!(
        ((plot.hi_x - plot.lo_x + 1) * (plot.hi_z - plot.lo_z + 1)) as u32,
        cottage.footprint.width * cottage.footprint.depth
    );
    assert!(!plot.contains(x, z), "the entrance is outside the body");
    // The query is pure, and so is the search for a site.
    assert_eq!(before, state_hash(app.world_mut()));

    refused(app.world(), "castle", x, z, 0, "unknown building `castle`");
    refused(app.world(), "cottage", x, z, 4, "rotation must be 0..=3");
    let bounds = terrain::bounds(app.world());
    refused(
        app.world(),
        "pad",
        bounds.min.x,
        bounds.min.z,
        2,
        "the footprint leaves the map",
    );

    // Terrain rules, on the cottage's own valid site.
    let body: Vec<CellCoord> = plot.cells().collect();
    let probe = body[body.len() / 2];
    let ground = terrain::cell(app.world(), probe.x, probe.z).expect("ground");
    set_cell(
        app.world_mut(),
        probe.x,
        probe.z,
        -200,
        TerrainSurface::Water,
    );
    refused(
        app.world(),
        "cottage",
        x,
        z,
        rotation,
        "Cottage cannot stand on water",
    );
    set_cell(
        app.world_mut(),
        probe.x,
        probe.z,
        ground.height_mm,
        TerrainSurface::Soil,
    );
    refused(
        app.world(),
        "cottage",
        x,
        z,
        rotation,
        "Cottage cannot stand on a road or bare soil",
    );
    set_cell(
        app.world_mut(),
        probe.x,
        probe.z,
        ground.height_mm + 1200,
        ground.surface,
    );
    refused(
        app.world(),
        "cottage",
        x,
        z,
        rotation,
        "the ground is too steep",
    );
    set_cell(
        app.world_mut(),
        probe.x,
        probe.z,
        ground.height_mm,
        ground.surface,
    );
    let door = terrain::cell(app.world(), x, z).expect("door ground");
    set_cell(app.world_mut(), x, z, -200, TerrainSurface::Water);
    refused(
        app.world(),
        "cottage",
        x,
        z,
        rotation,
        "the entrance is on water",
    );
    set_cell(app.world_mut(), x, z, door.height_mm, door.surface);
    civic::placement_check(app.world(), "cottage", x, z, rotation).expect("restored");

    // Entrances, resource sites and reserved plots.
    let home = settlements::home_def(app.world(), harbour, 0)
        .expect("home")
        .door;
    let (hx, hz) = (home.x.div_euclid(1000), home.z.div_euclid(1000));
    refused(
        app.world(),
        "pad",
        hx,
        hz - 1,
        0,
        "the footprint covers an entrance or a public place",
    );
    let patch = app.world().resource::<TestbedIds>().id("harbour_patch_log");
    let patch = *app
        .world()
        .get::<SimPos>(entity(app.world(), patch))
        .expect("patch");
    let (px, pz) = (patch.x.div_euclid(1000), patch.z.div_euclid(1000));
    refused(
        app.world(),
        "pad",
        px + 2,
        pz - 1,
        0,
        "too close to a resource site",
    );
    let plot_def = &village::frontier::spec(app.world()).expect("spec").plots[0];
    let (gx, gz) = (plot_def.x.div_euclid(1000), plot_def.z.div_euclid(1000));
    let reason = civic::placement_check(app.world(), "pad", gx, gz - 1, 0).expect_err("plot");
    assert!(
        reason.contains("a reserved homestead plot"),
        "expected the plot in `{reason}`"
    );

    // Other buildings: overlap, the clearance lane, and the open-site limit.
    let first = civic::PlaceBuilding {
        building: "cottage".into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed");
    refused(
        app.world(),
        "cottage",
        x,
        z,
        rotation,
        "the footprint overlaps another building",
    );
    let across = if rotation % 2 == 0 { (1, 0) } else { (0, 1) };
    let width = cottage.footprint.width as i32;
    let reason = civic::placement_check(
        app.world(),
        "cottage",
        x + across.0 * width,
        z + across.1 * width,
        rotation,
    )
    .expect_err("a touching neighbour");
    assert!(
        reason.contains("another building") || reason.contains("entrance"),
        "{reason}"
    );
    let body_cell = plot.cells().next().expect("body cell");
    refused(
        app.world(),
        "pad",
        body_cell.x,
        body_cell.z,
        0,
        "the entrance is blocked by a building",
    );

    // Somebody standing where a solid body would rise.
    let pad = civic::definition(app.world(), "pad").expect("pad").clone();
    let (ox, oz, turn) = civic::find_site(app.world(), &pad, harbour, 36).expect("pad site");
    civic::placement_check(app.world(), "post", ox, oz, turn).expect("free for a post");
    let spot = civic::placement_plot(app.world(), "post", ox, oz, turn).expect("post plot");
    let walker = residents(app.world(), harbour)[3].1;
    // Positions are read as they are now, not from the index of the last tick:
    // a resident who just stepped in must not be walled in.
    teleport(
        app.world_mut(),
        walker,
        SimPos::new(spot.lo_x * 1000 + 500, spot.lo_z * 1000 + 500),
    );
    refused(
        app.world(),
        "post",
        ox,
        oz,
        turn,
        "somebody or something is in the way",
    );
    civic::placement_check(app.world(), "pad", ox, oz, turn)
        .expect("an open yard does not mind a passer-by");

    // Too far from every settlement.
    let centres: Vec<SimPos> = settlements::all(app.world())
        .into_iter()
        .filter_map(|(id, _)| settlements::definition(app.world(), id).map(|def| def.center))
        .collect();
    let far = (bounds.min.z..bounds.min.z + bounds.height as i32)
        .flat_map(|cz| (bounds.min.x..bounds.min.x + bounds.width as i32).map(move |cx| (cx, cz)))
        .find(|(cx, cz)| {
            let at = SimPos::new(cx * 1000 + 500, cz * 1000 + 500);
            centres
                .iter()
                .all(|centre| centre.dist2(at) > 60_000_i64.pow(2))
                && terrain::cell(app.world(), *cx, *cz).is_some_and(|cell| cell.walkable())
                && civic::placement_check(app.world(), "pad", *cx, *cz, 0)
                    .is_err_and(|reason| reason.contains("too far from any settlement"))
        });
    assert!(
        far.is_some(),
        "no dry cell far from every settlement was refused for distance"
    );

    // An entrance walled in by water cannot be reached.
    let (ix, iz, _) = (ox, oz, turn);
    let saved: Vec<(i32, i32, terrain::TerrainCell)> = (-2..=2)
        .flat_map(|dz| (-2..=2).map(move |dx| (dx, dz)))
        .filter(|(dx, dz): &(i32, i32)| dx.abs() == 2 || dz.abs() == 2)
        .filter_map(|(dx, dz)| {
            Some((
                ix + dx,
                iz + dz,
                terrain::cell(app.world(), ix + dx, iz + dz)?,
            ))
        })
        .collect();
    for (cx, cz, _) in &saved {
        set_cell(app.world_mut(), *cx, *cz, -200, TerrainSurface::Water);
    }
    let reason = civic::placement_check(app.world(), "pad", ix, iz, turn).expect_err("moat");
    assert!(reason.contains("cannot be reached on foot"), "{reason}");
    for (cx, cz, cell) in &saved {
        set_cell(app.world_mut(), *cx, *cz, cell.height_mm, cell.surface);
    }

    // At most four open sites per settlement.
    let mut open = vec![first];
    while open.len() < civic::MAX_OPEN_SITES - 1 {
        open.push(place(&mut app, "pad", harbour));
    }
    // Two spots that are both valid now; the fourth site takes the first.
    let (ax, az, aturn) = civic::find_site(app.world(), &pad, harbour, 36).expect("fourth");
    let (bx, bz) = (4..16)
        .flat_map(|d| [(ax + d, az), (ax - d, az), (ax, az + d), (ax, az - d)])
        .find(|(bx, bz)| civic::placement_check(app.world(), "pad", *bx, *bz, aturn).is_ok())
        .expect("a second free spot");
    civic::PlaceBuilding {
        building: "pad".into(),
        x: ax,
        z: az,
        rotation: aturn,
    }
    .perform(app.world_mut())
    .expect("fourth site");
    refused(
        app.world(),
        "pad",
        bx,
        bz,
        aturn,
        "already has 4 open building sites",
    );
    assert_eq!(civic::sites(app.world()).len(), civic::MAX_OPEN_SITES);
    assert_eq!(open.len(), civic::MAX_OPEN_SITES - 1);
}

#[test]
fn place_building_is_one_recorded_command_only_in_the_regional_profile() {
    let mut app = shipped();
    let harbour = settlement(app.world(), "harbour");
    let def = civic::definition(app.world(), "stockpile")
        .expect("stockpile")
        .clone();
    let (x, z, rotation) = civic::find_site(app.world(), &def, harbour, 36).expect("site");
    let parsed: civic::PlaceBuilding = serde_json::from_value(
        json!({"building": "stockpile", "x": x, "z": z, "rotation": rotation}),
    )
    .expect("JSON shape");
    assert!(
        serde_json::from_value::<civic::PlaceBuilding>(
            json!({"building": "stockpile", "x": x, "z": z, "free": true})
        )
        .is_err(),
        "unknown fields are refused"
    );
    let ids_before: Vec<SimId> = app
        .world()
        .resource::<SimIndex>()
        .map
        .keys()
        .copied()
        .collect();
    submit(app.world_mut(), None, SimCommand::content(&parsed)).expect("accepted");
    run_ticks(app.world_mut(), 1);
    let sites = civic::sites(app.world());
    assert_eq!(sites.len(), 1);
    let site = &sites[0];
    assert_eq!(site.building, "stockpile");
    assert_eq!(site.label, "Harbour stockpile 1");
    assert_eq!(site.settlement, harbour);
    assert_eq!(site.cost.get("log"), Some(&4));
    assert!(!site.civic);
    // A new site is a new identity after every existing one.
    assert!(ids_before.iter().all(|id| *id < site.id));
    // A refused placement is refused at submission, with the same reason.
    let again = submit(app.world_mut(), None, SimCommand::content(&parsed)).expect_err("refused");
    assert!(
        format!("{again:?}").contains("another building"),
        "{again:?}"
    );
    let cancel: civic::CancelBuilding =
        serde_json::from_value(json!({"site": site.id})).expect("JSON shape");
    submit(app.world_mut(), None, SimCommand::content(&cancel)).expect("accepted");
    run_ticks(app.world_mut(), 1);
    assert!(civic::sites(app.world()).is_empty());
    assert!(app.world().resource::<SimIndex>().get(site.id).is_none());
    village::integrity::validate_colony(app.world()).expect("colony integrity");

    for command in ["PlaceBuilding", "CancelBuilding"] {
        assert!(
            app.world()
                .resource::<CommandRegistry>()
                .get(command)
                .is_some()
        );
    }
    for legacy in ["island_colony", "living_village", "ai_village"] {
        let old = island_content::new_sim_app(SEED, legacy);
        assert!(
            old.world()
                .resource::<CommandRegistry>()
                .get("PlaceBuilding")
                .is_none(),
            "{legacy} must keep its command set"
        );
        assert!(!old.world().contains_resource::<civic::CivicState>());
    }
}

#[test]
fn delivered_materials_are_paid_by_the_fund_and_staged_work_finishes_the_building() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 200);
    let coins = money(app.world());
    let logs = units(app.world_mut(), "log");
    let site = place(&mut app, "woodpile", harbour);
    let target = entity(app.world(), site);
    let door = *app.world().get::<SimPos>(target).expect("door");
    let carrier = holder(app.world(), harbour, "log", 2);
    let builder = residents(app.world(), harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| *actor != carrier)
        .expect("builder");

    // Nothing can be built before the materials are in.
    teleport(app.world_mut(), builder, door);
    let early = civic::begin_work(app.world_mut(), builder, target).expect_err("no materials");
    assert!(early.contains("waits for materials"), "{early}");
    // A delivery needs the carrier at the site.
    let far = civic::check_delivery(app.world(), carrier, target, "log", 2).expect_err("far");
    assert!(far.contains("within 1.5 m"), "{far}");

    teleport(app.world_mut(), carrier, door);
    let price = market::unit_price(app.world(), "log").expect("log price");
    let wallet = app.world().get::<Wallet>(carrier).expect("wallet").coins;
    let pot = app.world().resource::<VillageState>().fund;
    let carried = economy::count(app.world(), carrier, "log");
    // The shared sell action delivers: the same path a player uses.
    let mut act = VillageAct::new(*app.world().get::<SimId>(carrier).expect("id"), "sell");
    act.target = Some(site);
    act.kind = "log".into();
    act.quantity = 2;
    act.perform(app.world_mut()).expect("delivered");
    assert_eq!(economy::count(app.world(), carrier, "log"), carried - 2);
    assert_eq!(
        app.world().get::<Wallet>(carrier).expect("wallet").coins,
        wallet + 2 * price
    );
    assert_eq!(app.world().resource::<VillageState>().fund, pot - 2 * price);
    assert_eq!(
        money(app.world()),
        coins,
        "the fund paid the carrier exactly"
    );
    assert_eq!(
        units(app.world_mut(), "log"),
        logs,
        "the logs lie in the site"
    );
    let view = civic::sites(app.world()).remove(0);
    assert_eq!(view.delivered.get("log"), Some(&2));
    // More than the site needs is refused; goods it does not need too.
    let too_many = civic::check_delivery(app.world(), carrier, target, "log", 3).expect_err("3");
    assert!(too_many.contains("does not need that many"), "{too_many}");
    assert!(civic::check_delivery(app.world(), carrier, target, "bread", 1).is_err());

    // A second resident brings the rest.
    let second = residents(app.world(), harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| *actor != carrier && economy::count(app.world(), *actor, "log") >= 2)
        .expect("second carrier");
    teleport(app.world_mut(), second, door);
    civic::deliver(app.world_mut(), second, target, "log", 2).expect("second delivery");
    assert_eq!(money(app.world()), coins);
    assert_eq!(units(app.world_mut(), "log"), logs);

    // Staged work by a builder, then the same identity is the building.
    build(&mut app, site, builder);
    let finished = app
        .world()
        .get::<civic::Building>(target)
        .expect("building");
    assert_eq!(finished.building, "woodpile");
    assert_eq!(finished.label, "Harbour woodpile 1");
    let depot = app.world().get::<civic::Depot>(target).expect("a store");
    assert_eq!((depot.capacity, depot.public, depot.used()), (40, false, 0));
    assert!(civic::sites(app.world()).is_empty());
    assert_eq!(civic::buildings(app.world())[0].id, site);
    assert_eq!(civic::depot_views(app.world())[0].id, site);
    assert!(app.world().get::<civic::SiteWork>(builder).is_none());
    assert_eq!(
        money(app.world()),
        coins,
        "building creates and destroys no coin"
    );
    // The four logs are in the walls now: the one place goods leave the world.
    assert_eq!(app.world().resource::<civic::CivicState>().completed, 1);
    assert_eq!(
        app.world().resource::<civic::CivicState>().spent,
        u64::from(4 * price)
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn cancelling_a_site_refunds_its_materials_to_a_store_or_the_ground() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 200);
    set_journal(app.world_mut(), true, false);

    // Without a store the delivered logs are laid at the entrance.
    let site = place(&mut app, "woodpile", harbour);
    let target = entity(app.world(), site);
    let door = *app.world().get::<SimPos>(target).expect("door");
    let carrier = holder(app.world(), harbour, "log", 2);
    teleport(app.world_mut(), carrier, door);
    civic::deliver(app.world_mut(), carrier, target, "log", 2).expect("delivered");
    let coins = money(app.world());
    let logs = units(app.world_mut(), "log");
    civic::CancelBuilding { site }
        .perform(app.world_mut())
        .expect("cancelled");
    assert!(app.world().resource::<SimIndex>().get(site).is_none());
    assert!(civic::sites(app.world()).is_empty());
    let loose: u32 = app
        .world_mut()
        .query::<(&Item, &SimPos)>()
        .iter(app.world())
        .filter(|(item, pos)| item.kind == "log" && **pos == door)
        .map(|(item, _)| item.stack)
        .sum();
    assert_eq!(loose, 2, "the logs lie at the entrance");
    assert_eq!(units(app.world_mut(), "log"), logs);
    assert_eq!(money(app.world()), coins);
    let cancelled = effects(app.world(), "VillageSiteCancelled");
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0]["dropped"]["log"], 2);
    assert!(
        civic::CancelBuilding { site }.check(app.world()).is_err(),
        "a site is cancelled once"
    );

    // With a store nearby they become the settlement's stock there.
    let store = raise(&mut app, "pad", harbour);
    let site = place(&mut app, "woodpile", harbour);
    let target = entity(app.world(), site);
    let door = *app.world().get::<SimPos>(target).expect("door");
    let carrier = holder(app.world(), harbour, "log", 2);
    teleport(app.world_mut(), carrier, door);
    civic::deliver(app.world_mut(), carrier, target, "log", 2).expect("delivered");
    let coins = money(app.world());
    let logs = units(app.world_mut(), "log");
    civic::cancel_site(app.world_mut(), site, "test");
    let shelf = app
        .world()
        .get::<civic::Depot>(entity(app.world(), store))
        .expect("store");
    assert_eq!(shelf.stock("log"), 2);
    assert_eq!(shelf.lots["log"].get(&civic::CIVIC_OWNER), Some(&2));
    assert_eq!(units(app.world_mut(), "log"), logs);
    assert_eq!(money(app.world()), coins);
    // Selling the settlement's stock returns the coins to the civic fund.
    let buyer = residents(app.world(), harbour)[5].1;
    let store_entity = entity(app.world(), store);
    let at = *app.world().get::<SimPos>(store_entity).expect("store door");
    teleport(app.world_mut(), buyer, at);
    let pot = app.world().resource::<VillageState>().fund;
    let price = market::unit_price(app.world(), "log").expect("price");
    civic::depot_buy(app.world_mut(), buyer, store_entity, "log", 1).expect("bought");
    assert_eq!(app.world().resource::<VillageState>().fund, pot + price);
    assert_eq!(money(app.world()), coins);
    assert_eq!(units(app.world_mut(), "log"), logs);
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn a_stall_sells_for_an_absent_producer_and_credits_them() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    set_journal(app.world_mut(), true, false);
    let stall = raise(&mut app, "kiosk", harbour);
    let stall_entity = entity(app.world(), stall);
    let counter = *app.world().get::<SimPos>(stall_entity).expect("stall");
    assert!(
        app.world()
            .get::<civic::Depot>(stall_entity)
            .expect("stall")
            .public
    );

    // A baker consigns bread, then walks away.
    let producer = residents(app.world(), harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|v| v.role == village::Role::Baker)
        })
        .expect("baker");
    let kind = village::catalog::COOKED_MEAL;
    let stocked = economy::count(app.world(), producer, kind);
    assert!(stocked >= 2, "the baker's founding stock");
    let coins = money(app.world());
    let total = units(app.world_mut(), kind);
    let producer_id = *app.world().get::<SimId>(producer).expect("id");
    let away = *app.world().get::<SimPos>(producer).expect("producer");
    teleport(app.world_mut(), producer, counter);
    // The shared sell action consigns: the same path a player uses.
    let mut consign = VillageAct::new(producer_id, "sell");
    consign.target = Some(stall);
    consign.kind = kind.into();
    consign.quantity = 2;
    consign.perform(app.world_mut()).expect("consigned");
    assert_eq!(economy::count(app.world(), producer, kind), stocked - 2);
    let shelf = app
        .world()
        .get::<civic::Depot>(stall_entity)
        .expect("stall");
    assert_eq!(shelf.lots[kind].get(&producer_id), Some(&2));
    assert_eq!(units(app.world_mut(), kind), total, "consigned, not lost");
    assert_eq!(money(app.world()), coins, "consigning moves no coin");
    teleport(
        app.world_mut(),
        producer,
        SimPos::new(away.x, away.z + 60_000),
    );
    assert!(market::sells(app.world(), stall_entity, kind));
    assert_eq!(
        market::available_to_sell(app.world(), stall_entity, kind),
        2
    );

    // A buyer from another settlement finds the stall, not the producer.
    let buyer = residents(app.world(), settlement(app.world(), "alderwood"))[2].1;
    let buyer_id = *app.world().get::<SimId>(buyer).expect("id");
    teleport(app.world_mut(), buyer, counter);
    assert!(
        market::quote(app.world(), buyer, producer, kind, 1).is_err(),
        "the producer is far away"
    );
    let price = market::unit_price(app.world(), kind).expect("quote");
    let (had, purse) = (
        economy::count(app.world(), buyer, kind),
        app.world().get::<Wallet>(buyer).expect("wallet").coins,
    );
    let earned = app.world().get::<Wallet>(producer).expect("wallet").coins;
    let trades = app.world().resource::<VillageState>().transactions;
    let mut buy = VillageAct::new(buyer_id, "buy");
    buy.target = Some(stall);
    buy.kind = kind.into();
    buy.quantity = 2;
    buy.limit_price = Some(price);
    buy.check(app.world())
        .expect("the stall quotes the market price");
    buy.perform(app.world_mut()).expect("bought at the stall");
    assert_eq!(economy::count(app.world(), buyer, kind), had + 2);
    assert_eq!(
        app.world().get::<Wallet>(buyer).expect("wallet").coins,
        purse - 2 * price
    );
    assert_eq!(
        app.world().get::<Wallet>(producer).expect("wallet").coins,
        earned + 2 * price,
        "the absent producer is credited at once"
    );
    assert_eq!(
        app.world().resource::<VillageState>().transactions,
        trades + 1
    );
    assert_eq!(money(app.world()), coins, "coins are conserved exactly");
    assert_eq!(
        units(app.world_mut(), kind),
        total,
        "goods are conserved exactly"
    );
    let shelf = app
        .world()
        .get::<civic::Depot>(stall_entity)
        .expect("stall");
    assert_eq!(
        (shelf.used(), shelf.sold_units, shelf.turnover),
        (0, 2, u64::from(2 * price))
    );
    let sales = effects(app.world(), "VillageStallSale");
    assert_eq!(sales.len(), 1);
    assert_eq!(sales[0]["coins"], 2 * price);
    // An empty shelf sells nothing, and a limit below the quote is refused.
    assert!(buy.check(app.world()).is_err());
    assert!(!market::sells(app.world(), stall_entity, kind));

    // Storage serves residents of any settlement; only a stall serves visitors.
    let store = raise(&mut app, "pad", harbour);
    let store_entity = entity(app.world(), store);
    let shelf_pos = *app.world().get::<SimPos>(store_entity).expect("store");
    let local = holder(app.world(), harbour, "log", 1);
    teleport(app.world_mut(), local, shelf_pos);
    civic::depot_deposit(app.world_mut(), local, store_entity, "log", 1).expect("stored");
    teleport(app.world_mut(), buyer, shelf_pos);
    assert!(civic::depot_check_buy(app.world(), buyer, store_entity, "log", 1).is_ok());
    assert!(civic::depot_check_buy(app.world(), local, store_entity, "log", 1).is_ok());
    let visitor = entity(app.world(), app.world().resource::<VillageState>().played);
    assert!(!civic::serves(app.world(), store_entity, visitor));
    assert!(civic::serves(app.world(), stall_entity, visitor));
    // Tools and clothing are not stored.
    let woodcutter = holder(app.world(), harbour, "axe", 1);
    teleport(app.world_mut(), woodcutter, shelf_pos);
    assert!(civic::depot_check_deposit(app.world(), woodcutter, store_entity, "axe", 1).is_err());
    assert_eq!(money(app.world()), coins);
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// One scripted round through every transfer, checking both ledgers after
/// each step: nothing but a finished building takes goods out of the world.
#[test]
fn coins_and_goods_are_conserved_across_store_stall_site_and_cancellation() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 300);
    let store = raise(&mut app, "pad", harbour);
    let stall = raise(&mut app, "kiosk", harbour);
    let (store_entity, stall_entity) = (entity(app.world(), store), entity(app.world(), stall));
    let coins = money(app.world());
    let logs = units(app.world_mut(), "log");
    let check = |app: &mut App, step: &str| {
        assert_eq!(money(app.world()), coins, "coins after {step}");
        assert_eq!(units(app.world_mut(), "log"), logs, "logs after {step}");
    };
    let at = |app: &App, target: Entity| *app.world().get::<SimPos>(target).expect("position");

    let owner = holder(app.world(), harbour, "log", 3);
    let owner_id = *app.world().get::<SimId>(owner).expect("id");
    let hauler = residents(app.world(), harbour)
        .into_iter()
        .map(|(_, actor)| actor)
        .find(|actor| *actor != owner)
        .expect("hauler");
    let to = at(&app, store_entity);
    teleport(app.world_mut(), owner, to);
    civic::depot_deposit(app.world_mut(), owner, store_entity, "log", 3).expect("stored");
    check(&mut app, "a deposit");

    // Store to stall: the hauler buys from the owner's lot and consigns.
    teleport(app.world_mut(), hauler, to);
    let before = app.world().get::<Wallet>(owner).expect("wallet").coins;
    civic::depot_buy(app.world_mut(), hauler, store_entity, "log", 1).expect("fetched");
    check(&mut app, "a purchase from storage");
    assert!(app.world().get::<Wallet>(owner).expect("wallet").coins > before);
    let to = at(&app, stall_entity);
    teleport(app.world_mut(), hauler, to);
    civic::depot_deposit(app.world_mut(), hauler, stall_entity, "log", 1).expect("restocked");
    check(&mut app, "restocking the stall");

    // Store to site: bought at the store, sold to the site, paid by the fund.
    let site = place(&mut app, "woodpile", harbour);
    let site_entity = entity(app.world(), site);
    let to = at(&app, store_entity);
    teleport(app.world_mut(), hauler, to);
    civic::depot_buy(app.world_mut(), hauler, store_entity, "log", 2).expect("fetched");
    check(&mut app, "fetching materials");
    let to = at(&app, site_entity);
    teleport(app.world_mut(), hauler, to);
    civic::deliver(app.world_mut(), hauler, site_entity, "log", 2).expect("delivered");
    check(&mut app, "a delivery");
    assert_eq!(civic::goods_in_custody(app.world(), "log"), 3);

    // Cancel: the two logs return to a store as the settlement's own.
    civic::cancel_site(app.world_mut(), site, "test");
    check(&mut app, "a cancellation");
    assert_eq!(civic::goods_in_custody(app.world(), "log"), 3);
    let civic_lots: u32 = [store_entity, stall_entity]
        .into_iter()
        .filter_map(|shelf| app.world().get::<civic::Depot>(shelf))
        .filter_map(|shelf| shelf.lots.get("log")?.get(&civic::CIVIC_OWNER).copied())
        .sum();
    assert_eq!(civic_lots, 2);
    assert!(
        app.world()
            .get::<civic::Depot>(store_entity)
            .is_some_and(|shelf| !shelf.lots.values().any(|lot| lot.contains_key(&owner_id))),
        "the owner's three logs were all sold"
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// Journal effects gathered while a long run evicts the ring buffer.
#[derive(Default)]
struct Log {
    seen: std::collections::BTreeSet<(u64, String)>,
    entries: Vec<Value>,
}
impl Log {
    fn collect(&mut self, world: &World) {
        for entry in &world.resource::<Journal>().entries {
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
                && effect
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| {
                        kind.starts_with("VillageSite")
                            || kind.starts_with("VillageBuilding")
                            || kind.starts_with("VillageCivicProject")
                            || kind.starts_with("VillageGoodsStored")
                            || kind.starts_with("VillageStallSale")
                    })
            {
                let value = serde_json::to_value(effect).expect("effect JSON");
                if self.seen.insert((entry.tick, value.to_string())) {
                    self.entries.push(value);
                }
            }
        }
    }
    fn of(&self, kind: &str) -> Vec<&Value> {
        self.entries
            .iter()
            .filter(|entry| entry["type"] == kind)
            .collect()
    }
}

/// Runs in 200-tick steps until `done`, checking the coin total at every step.
fn until(
    app: &mut App,
    limit: u64,
    coins: u64,
    log: &mut Log,
    mut done: impl FnMut(&App) -> bool,
) -> bool {
    let end = app.world().resource::<SimClock>().tick + limit;
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), 200);
        log.collect(app.world());
        assert_eq!(
            money(app.world()),
            coins,
            "coins drifted at tick {}",
            app.world().resource::<SimClock>().tick
        );
        if done(app) {
            return true;
        }
    }
    false
}

/// The add-a-building proof: a storage building that exists only in JSON is
/// placed by a recorded command, supplied and built by ordinary residents.
#[test]
fn a_storage_building_defined_only_in_json_is_built_by_the_residents() {
    let granary = json!({"id": "granary", "display": "Granary",
        "cost": [{"good": "log", "count": 2}], "work_ticks": 200,
        "footprint": {"width": 3, "depth": 3}, "kind": {"type": "storage", "capacity": 80},
        "solid": false});
    let mut app = world_with(|world| {
        world["economy"]["buildings"]
            .as_array_mut()
            .expect("buildings")
            .push(granary.clone());
    });
    let plain = shipped();
    let unknown = civic::placement_check(plain.world(), "granary", 0, 0, 0).expect_err("unknown");
    assert!(unknown.contains("unknown building `granary`"), "{unknown}");
    assert!(
        unknown.contains("stockpile"),
        "the known buildings are listed: {unknown}"
    );

    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 150);
    let coins = money(app.world());
    let def = civic::definition(app.world(), "granary")
        .expect("granary")
        .clone();
    let (x, z, rotation) = civic::find_site(app.world(), &def, harbour, 36).expect("site");
    let command: civic::PlaceBuilding = serde_json::from_value(
        json!({"building": "granary", "x": x, "z": z, "rotation": rotation}),
    )
    .expect("JSON shape");
    submit(app.world_mut(), None, SimCommand::content(&command)).expect("accepted");
    run_ticks(app.world_mut(), 1);
    let site = civic::sites(app.world())[0].id;
    let mut log = Log::default();
    let built = until(&mut app, 9_600, coins, &mut log, |app| {
        civic::buildings(app.world())
            .iter()
            .any(|building| building.id == site)
    });
    assert!(
        built,
        "the residents did not finish the granary: {:?}",
        civic::sites(app.world())
    );
    let view = &civic::depot_views(app.world())[0];
    assert_eq!((view.id, view.capacity, view.stall), (site, 80, false));
    assert_eq!(view.label, "Harbour granary 1");
    // Deliveries to this site: the planner may supply its own projects too.
    let deliveries: Vec<_> = log
        .of("VillageSiteDelivery")
        .into_iter()
        .filter(|d| d["site"] == json!(site))
        .collect();
    assert!(!deliveries.is_empty(), "residents carried the logs");
    assert_eq!(
        deliveries
            .iter()
            .map(|d| d["quantity"].as_u64().unwrap())
            .sum::<u64>(),
        2
    );
    assert_eq!(log.of("VillageBuildingCompleted").len(), 1);
    assert!(!log.of("VillageSiteProgress").is_empty(), "work was staged");
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// Ordinary residents use a store by themselves: producers lay surplus in it
/// and neighbours buy there while the producer is elsewhere.
#[test]
fn residents_store_surplus_and_buy_from_the_store_unprompted() {
    let mut app = world_with(|world| {
        test_buildings(world);
        // Any spare unit is worth the walk, so the test does not wait for a glut.
        world["economy"]["tuning"]["depot_working_stock"] = json!(0);
        world["economy"]["tuning"]["depot_deposit_min"] = json!(1);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let store = raise(&mut app, "pad", harbour);
    let coins = money(app.world());
    let mut log = Log::default();
    let sold = until(&mut app, 14_400, coins, &mut log, |app| {
        civic::depot_views(app.world())
            .iter()
            .any(|view| view.id == store && view.sold_units > 0)
    });
    let stored = log.of("VillageGoodsStored");
    assert!(!stored.is_empty(), "no resident stored anything");
    assert!(
        sold,
        "nobody bought from the store; stored: {stored:?}; shelf: {:?}",
        civic::depot_views(app.world())
    );
    let sales = log.of("VillageStallSale");
    assert!(!sales.is_empty(), "no owner was credited");
    // The seller of record credited an owner who was not at the store.
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// The planner: forced bed shortage, a commissioned home, housed newcomers.
#[test]
fn the_planner_builds_a_home_for_homeless_newcomers_and_houses_them() {
    let mut app = world_with(|world| {
        // One bed per cottage: no founder has a spare bed to share.
        for settlement in world["settlements"].as_array_mut().expect("settlements") {
            for home in settlement["homes"].as_array_mut().expect("homes") {
                home["capacity"] = json!(1);
            }
        }
        world["economy"]["tuning"]["planner_review_ticks"] = json!(200);
        world["economy"]["tuning"]["planner_sustain_reviews"] = json!(2);
    });
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 300);
    let coins = money(app.world());
    let newcomers: Vec<SimId> = ["Wanderer", "Drifter"]
        .into_iter()
        .map(|name| labor::settle_newcomer(app.world_mut(), harbour, name).expect("newcomer"))
        .collect();
    assert_eq!(money(app.world()), coins, "newcomers bring no coins");
    assert_eq!(residents(app.world(), harbour).len(), 16);
    assert!(
        newcomers
            .iter()
            .all(|id| colony::home_of(app.world(), *id).is_none())
    );
    village::integrity::validate_colony(app.world()).expect("newcomers are valid residents");

    let mut most_sites = 0;
    let mut log = Log::default();
    let housed = until(&mut app, 21_600, coins, &mut log, |app| {
        most_sites = most_sites.max(
            civic::sites(app.world())
                .iter()
                .filter(|site| site.settlement == harbour && site.civic)
                .count(),
        );
        newcomers
            .iter()
            .all(|id| colony::home_of(app.world(), *id).is_some())
    });
    let projects: Vec<Value> = log.of("VillageCivicProject").into_iter().cloned().collect();
    assert!(
        housed,
        "newcomers are still homeless; planner: {:?}; sites: {:?}; decisions: {projects:?}",
        civic::planner(app.world(), harbour),
        civic::sites(app.world())
    );
    assert_eq!(most_sites, 1, "one active civic project per settlement");
    let commissioned: Vec<&Value> = projects
        .iter()
        .filter(|project| {
            project["decision"] == "commissioned" && project["settlement"] == json!(harbour)
        })
        .collect();
    assert_eq!(commissioned.len(), 1, "{projects:?}");
    assert_eq!(commissioned[0]["building"], "cottage");
    assert!(
        commissioned[0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("no bed")),
        "{commissioned:?}"
    );
    assert!(projects.iter().any(|project| {
        project["decision"] == "completed" && project["settlement"] == json!(harbour)
    }));
    // Both live in the new cottage, which is a real household.
    let home = colony::home_of(app.world(), newcomers[0]).expect("home");
    assert_eq!(colony::home_of(app.world(), newcomers[1]), Some(home));
    let house = entity(app.world(), home);
    assert_eq!(
        app.world()
            .get::<civic::Building>(house)
            .expect("built")
            .building,
        "cottage"
    );
    let household = app
        .world()
        .get::<colony::Household>(house)
        .expect("household");
    assert_eq!(household.capacity, 2);
    for id in &newcomers {
        let person = app
            .world()
            .get::<Villager>(entity(app.world(), *id))
            .expect("person");
        assert_eq!(person.home, household.door);
        assert!(village::living(app.world(), entity(app.world(), *id)));
    }
    // The next review sees nobody homeless; the cottage is no longer a project.
    run_ticks(app.world_mut(), 201);
    let planner = civic::planner(app.world(), harbour).expect("planner");
    assert_eq!((planner.homeless, planner.bed_shortage_reviews), (0, 0));
    assert_ne!(planner.project, Some(home));
    assert_eq!(money(app.world()), coins);
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// The planner's other triggers: storage on lasting overflow, a workplace on
/// lasting unmet demand. One project per settlement, near its centre, and the
/// same seed gives the same decisions.
#[test]
fn the_planner_commissions_storage_on_lasting_overflow_and_is_deterministic() {
    let scene = || {
        let mut app = world_with(|world| {
            let tuning = &mut world["economy"]["tuning"];
            tuning["planner_review_ticks"] = json!(300);
            tuning["planner_sustain_reviews"] = json!(2);
            tuning["planner_storage_overflow_units"] = json!(1);
            // In spring: in the founding's late autumn residents without a
            // coat stay in from the cold and bring in less surplus.
            tuning["founding_day"] = json!(0);
            // Storage alone: a workplace wanted in the same hour would race it.
            tuning["planner_demand_units"] = json!(100_000);
        });
        set_journal(app.world_mut(), true, false);
        fund(app.world_mut(), 400);
        app
    };
    let mut app = scene();
    let harbour = settlement(app.world(), "harbour");
    let coins = money(app.world());
    let mut log = Log::default();
    let mut most_sites = 0;
    let stored = until(&mut app, 12_000, coins, &mut log, |app| {
        for (region, _) in settlements::all(app.world()) {
            most_sites = most_sites.max(
                civic::sites(app.world())
                    .iter()
                    .filter(|site| site.settlement == region && site.civic)
                    .count(),
            );
        }
        civic::depot_views(app.world())
            .iter()
            .any(|store| store.settlement == harbour)
    });
    let projects = log.of("VillageCivicProject");
    assert!(
        stored,
        "Harbour got no store; sites: {:?}",
        civic::sites(app.world())
            .iter()
            .map(|site| (
                site.settlement,
                site.building.clone(),
                site.delivered.clone(),
                site.cost.clone(),
                site.work_done,
                site.builder,
                site.funding.clone()
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(most_sites, 1, "one active civic project per settlement");
    let first = projects
        .iter()
        .find(|project| {
            project["decision"] == "commissioned" && project["settlement"] == json!(harbour)
        })
        .expect("Harbour's first commission");
    assert_eq!(first["building"], "stockpile");
    assert!(
        first["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("have had no store for 2 reviews")),
        "{first}"
    );
    // Every commissioned site is near its own centre, outside the square.
    for project in projects.iter().filter(|p| p["decision"] == "commissioned") {
        let region = SimId(project["settlement"].as_u64().expect("settlement") as u32);
        let centre = settlements::definition(app.world(), region)
            .expect("settlement")
            .center;
        let cell = (
            project["cell"][0].as_i64().expect("x") as i32,
            project["cell"][1].as_i64().expect("z") as i32,
        );
        let distance = centre.dist2(SimPos::new(cell.0 * 1000 + 500, cell.1 * 1000 + 500));
        assert!(
            (7_000_i64.pow(2)..=37_000_i64.pow(2)).contains(&distance),
            "{cell:?} is not near {centre:?}"
        );
        assert!(
            project["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty()),
            "every decision carries its reason: {project}"
        );
    }
    // The store was paid for by the fund.
    let spent = app.world().resource::<civic::CivicState>().spent;
    assert!(spent >= 4 * 2, "a stockpile of four logs: {spent}");
    // Same seed, same decisions: the planner is deterministic.
    let mut twin = scene();
    let ticks = app.world().resource::<SimClock>().tick;
    run_ticks(twin.world_mut(), ticks);
    assert_eq!(state_hash(app.world_mut()), state_hash(twin.world_mut()));
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn a_half_built_site_and_a_stocked_store_survive_save_and_load() {
    let scene = || world_with(test_buildings);
    let mut app = scene();
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 200);
    let store = raise(&mut app, "pad", harbour);
    let store_entity = entity(app.world(), store);
    let owner = holder(app.world(), harbour, "log", 2);
    let shelf = *app.world().get::<SimPos>(store_entity).expect("store");
    teleport(app.world_mut(), owner, shelf);
    civic::depot_deposit(app.world_mut(), owner, store_entity, "log", 2).expect("stored");
    let site = place(&mut app, "woodpile", harbour);
    run_ticks(app.world_mut(), 300);
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut resumed = scene();
    snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).expect("loaded");
    assert_eq!(state_hash(app.world_mut()), state_hash(resumed.world_mut()));
    village::integrity::validate_colony(resumed.world()).expect("loaded colony");
    assert_eq!(civic::sites(resumed.world()), civic::sites(app.world()));
    assert_eq!(
        civic::depot_views(resumed.world()),
        civic::depot_views(app.world())
    );
    run_ticks(app.world_mut(), 600);
    run_ticks(resumed.world_mut(), 600);
    assert_eq!(
        state_hash(app.world_mut()),
        state_hash(resumed.world_mut()),
        "a loaded world continues bit for bit"
    );
    let _ = site;
}

#[test]
fn the_dividend_returns_only_what_open_sites_cannot_use() {
    let mut app = world_with(test_buildings);
    let harbour = settlement(app.world(), "harbour");
    assert_eq!(civic::fund_hold(app.world()), 0);
    assert_eq!(civic::budget(app.world()), 0, "an empty fund buys nothing");
    let reserve = village::frontier::economy_spec::tuning(app.world())
        .expect("tuning")
        .civic_reserve;
    // The reserve for destitution grants is never spent on materials.
    app.world_mut().resource_mut::<VillageState>().fund = reserve;
    assert_eq!(civic::budget(app.world()), 0);
    app.world_mut().resource_mut::<VillageState>().fund = 0;
    fund(app.world_mut(), 100);
    assert_eq!(app.world().resource::<VillageState>().fund, reserve + 100);
    assert_eq!(civic::budget(app.world()), 100);
    let site = place(&mut app, "woodpile", harbour);
    let price = market::unit_price(app.world(), "log").expect("price");
    assert_eq!(civic::outstanding_cost(app.world()), 4 * price);
    assert_eq!(civic::fund_hold(app.world()), 4 * price * 125 / 100);
    assert_eq!(
        civic::material_orders(app.world(), harbour).get("log"),
        Some(&4)
    );
    assert!(civic::material_orders(app.world(), settlement(app.world(), "alderwood")).is_empty());
    civic::cancel_site(app.world_mut(), site, "test");
    assert_eq!(civic::fund_hold(app.world()), 0);
}

/// Every coin of the closed system of the shipped world: the island's and
/// the merchant boat's.
fn all_money(world: &World) -> u64 {
    money(world) + u64::from(village::merchant::purse(world))
}

/// The funded path: in the shipped world, with no help from a test, a cottage
/// the overseer places on the first day is paid for by the building levy,
/// supplied by residents and built, while destitution grants go on.
#[test]
fn a_cottage_placed_on_the_first_day_of_the_shipped_world_is_funded_supplied_and_built() {
    funded_cottage(9_000, true);
}

/// The same site, then two whole days: after each daily levy everybody still
/// has the destitution floor. Run explicitly:
/// cargo test --locked --release -p island_content --test island_buildings -- --ignored --nocapture
#[test]
#[ignore = "explicit: 48300 ticks of the shipped world; run in release"]
fn grants_still_reach_the_floor_while_the_village_pays_for_building() {
    funded_cottage(48_300, false);
}

fn funded_cottage(limit: u64, stop_when_built: bool) {
    let mut app = shipped();
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let coins = all_money(app.world());
    assert_eq!(civic::budget(app.world()), 0, "a new world's fund is empty");
    let site = place(&mut app, "cottage", harbour);
    // The village pays at once: the levy raised what the fund lacked.
    let levy = effects(app.world(), "VillageBuildingLevy");
    assert_eq!(levy.len(), 1, "placing the site raised the building levy");
    let raised = levy[0]["raised"].as_u64().expect("raised") as u32;
    let cost = civic::outstanding_cost(app.world());
    assert!(raised >= cost, "raised {raised} for materials worth {cost}");
    assert!(civic::budget(app.world()) >= cost);
    assert_eq!(
        app.world().resource::<civic::CivicState>().earmarked,
        raised
    );
    assert_eq!(
        all_money(app.world()),
        coins,
        "a levy moves coins, it makes none"
    );
    let view = civic::sites(app.world())
        .into_iter()
        .find(|view| view.id == site)
        .expect("site view");
    assert_eq!(view.placed_at, 2);
    assert_eq!(view.last_delivery_at, None);
    assert!(view.funding.starts_with("Funded"), "{}", view.funding);
    let floor = village::frontier::economy_spec::tuning(app.world())
        .expect("tuning")
        .destitution_floor;
    // Nobody paid more than a twentieth of what they hold above the floor.
    for (_, actor) in residents(app.world(), harbour) {
        assert!(app.world().get::<Wallet>(actor).expect("wallet").coins >= 150);
    }
    let mut first_delivery = None;
    let mut done = None;
    let mut grant_days = 0;
    let mut day = app.world().resource::<SimClock>().tick / 24_000;
    while app.world().resource::<SimClock>().tick < limit {
        run_ticks(app.world_mut(), 300);
        let now = app.world().resource::<SimClock>().tick;
        let today = now / 24_000;
        let new_day = today != day;
        day = today;
        assert_eq!(all_money(app.world()), coins, "coins drifted at tick {now}");
        if first_delivery.is_none() {
            first_delivery = civic::sites(app.world())
                .into_iter()
                .find(|view| view.id == site)
                .and_then(|view| view.last_delivery_at);
        }
        if new_day {
            // The daily levy ran in the ticks just played: grants were paid,
            // and nobody has had 300 ticks to spend them all.
            grant_days += 1;
            for (id, actor) in all_residents_of(app.world()) {
                let held = app.world().get::<Wallet>(actor).expect("wallet").coins;
                assert!(
                    held + 5 >= floor,
                    "resident {} holds {held} coins after the grants of tick {now}",
                    id.0
                );
            }
        }
        if done.is_none()
            && civic::buildings(app.world())
                .iter()
                .any(|building| building.id == site)
        {
            done = Some(now);
            if stop_when_built {
                break;
            }
        }
    }
    println!(
        "FUNDED_COTTAGE placed=2 first_delivery={first_delivery:?} completed={done:?} \
         raised={raised} grant_days_checked={grant_days} spent={} fund={}",
        app.world().resource::<civic::CivicState>().spent,
        app.world().resource::<VillageState>().fund
    );
    assert!(first_delivery.is_some(), "no material was ever delivered");
    assert!(
        done.is_some(),
        "the cottage was not finished in {limit} ticks"
    );
    assert!(stop_when_built || grant_days >= 2);
    assert!(app.world().resource::<civic::CivicState>().spent > 0);
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

fn all_residents_of(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world.get::<ResidentProfile>(**entity).is_some() && village::living(world, **entity)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

/// A site says in words whether it is funded, and why not when it is not.
#[test]
fn a_site_says_whether_the_village_can_pay_for_it_and_the_planner_speaks_plainly() {
    // With the building levy the village pays for what the overseer places.
    let mut app = world_with(test_buildings);
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let coins = money(app.world());
    let site = place(&mut app, "woodpile", harbour);
    let view = civic::sites(app.world()).remove(0);
    assert_eq!(view.id, site);
    assert!(view.funding.starts_with("Funded"), "{}", view.funding);
    assert_eq!(
        (view.placed_at, view.last_delivery_at, view.paid),
        (0, None, 0)
    );
    assert_eq!(money(app.world()), coins);
    assert!(civic::levy_capacity(app.world()) > 0);
    // Cancelling names the site and where it stood.
    civic::CancelBuilding { site }
        .perform(app.world_mut())
        .expect("cancelled");
    let cancelled = effects(app.world(), "VillageSiteCancelled").remove(0);
    assert_eq!(cancelled["label"], "Harbour woodpile 1");
    assert_eq!(cancelled["cell"], json!([view.plot.x, view.plot.z]));
    assert_eq!(cancelled["pos"], json!([view.door.x, view.door.z]));

    // Without a levy and without coins the site does not stall silently.
    let mut poor = world_with(|world| {
        test_buildings(world);
        world["economy"]["tuning"]["building_levy_percent"] = json!(0);
    });
    let harbour = settlement(poor.world(), "harbour");
    place(&mut poor, "woodpile", harbour);
    let view = civic::sites(poor.world()).remove(0);
    assert!(
        view.funding.starts_with("Short of coins") && view.funding.contains("may spend 0"),
        "{}",
        view.funding
    );
    assert_eq!(civic::budget(poor.world()), 0);

    // The planner's decision is a sentence with the building's display name.
    let mut planned = world_with(|world| {
        let tuning = &mut world["economy"]["tuning"];
        tuning["planner_review_ticks"] = json!(300);
        tuning["planner_sustain_reviews"] = json!(2);
        tuning["planner_storage_overflow_units"] = json!(1);
        // Storage alone: a workplace wanted in the same hour would race it.
        tuning["planner_demand_units"] = json!(100_000);
    });
    let harbour = settlement(planned.world(), "harbour");
    let mut text = String::new();
    for _ in 0..40 {
        run_ticks(planned.world_mut(), 300);
        text = civic::planner(planned.world(), harbour)
            .expect("planner")
            .last_decision_text
            .clone();
        if text.starts_with("Commissioned") {
            break;
        }
    }
    assert!(
        text.starts_with("Commissioned a stockpile:") && text.contains("have had no store"),
        "{text}"
    );
    assert_eq!(
        civic::planner(planned.world(), harbour)
            .expect("planner")
            .last_decision,
        "commissioned stockpile",
        "the key stays what it was"
    );
}
