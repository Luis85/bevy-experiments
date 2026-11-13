//! Integer geography, clear future plots and real navigation boundaries.
use bevy_app::App;
use island_content::{
    TestbedIds,
    village::{self, ResourcePatch, VillagePlace, Villager, geometry},
};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use std::collections::{BTreeSet, VecDeque};

fn scene() -> App {
    island_content::new_sim_app(7, "living_village")
}

#[test]
fn fixed_mask_is_exactly_the_incumbent_ellipse_and_living_growth_is_bounded() {
    let fixed = island_content::new_sim_app(7, "ai_village");
    let living = scene();
    let old = geometry::for_world(fixed.world());
    let new = geometry::for_world(living.world());
    assert_eq!(old.radii_mm, (24_000, 20_000));
    assert_eq!(new.radii_mm, (30_000, 25_000));
    assert!(old.expansion_plots.is_empty());
    let mut counts = [0, 0];
    for z in geometry::MIN_CELL..geometry::MAX_CELL_EXCLUSIVE {
        for x in geometry::MIN_CELL..geometry::MAX_CELL_EXCLUSIVE {
            let dx = i64::from(2 * x + 1);
            let dz = i64::from(2 * z + 1);
            let incumbent = dx * dx * 400 + dz * dz * 576 <= 4 * 576 * 400;
            assert_eq!(old.land_cell(x, z), incumbent, "Fixed cell {x},{z}");
            assert_eq!(village::land_cell(x, z), incumbent);
            assert!(!incumbent || new.land_cell(x, z));
            counts[0] += u32::from(incumbent);
            counts[1] += u32::from(new.land_cell(x, z));
        }
    }
    assert_eq!(counts, [1516, 2360]);
    assert_eq!(old.roads.len(), 4);
    assert_eq!(
        new.cell_bounds(),
        living.world().resource::<SpatialGrid>().bounds()
    );
}

#[test]
fn compact_living_world_keeps_its_original_land_and_resource_construction() {
    let mut doc = island_content::scenario::scenario_doc("living_village").unwrap();
    doc.world["island_geography"] = serde_json::json!("compact");
    let app = island_content::scenario::load_with(&doc, 7, |_| {})
        .unwrap()
        .app;
    let geo = geometry::for_world(app.world());
    assert_eq!(geo.radii_mm, geometry::FIXED.radii_mm);
    assert!(geo.expansion_plots.is_empty());
    assert!(geo.districts.is_empty());
    assert!(app.world().get_resource::<geometry::Geography>().is_none());
    for kind in [
        village::catalog::GRAIN,
        "log",
        "ore",
        village::catalog::WATER,
        village::catalog::FIBER,
        village::catalog::BERRIES,
    ] {
        let id = app
            .world()
            .resource::<TestbedIds>()
            .id(&format!("patch_{kind}"));
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        assert_eq!(
            app.world().get::<SimPos>(entity).copied(),
            geometry::FIXED.resource_position(kind)
        );
    }
    let habitat = app.world().resource::<TestbedIds>().id("wildlife_habitat");
    let habitat = app.world().resource::<SimIndex>().get(habitat).unwrap();
    assert_eq!(
        app.world()
            .get::<village::hunting::WildlifeHabitat>(habitat)
            .unwrap()
            .home,
        geometry::FIXED.wildlife_home
    );
}

fn intersects(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    let (ax, az, aw, ad) = a;
    let (bx, bz, bw, bd) = b;
    i64::from(ax).abs_diff(i64::from(bx)) * 2 < (aw + bw) as u64
        && i64::from(az).abs_diff(i64::from(bz)) * 2 < (ad + bd) as u64
}

#[test]
fn six_clear_six_metre_plots_have_land_corners_and_connected_frontages() {
    let app = scene();
    let geo = geometry::for_world(app.world());
    assert_eq!(geo.districts.len(), 3);
    assert_eq!(
        geo.districts
            .iter()
            .map(|district| district.id)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    assert!(
        geo.districts
            .iter()
            .all(|district| geo.contains_mm(SimPos::new(district.x, district.z)))
    );
    assert_eq!(geo.expansion_plots.len(), 6);
    let unique: BTreeSet<_> = geo.expansion_plots.iter().map(|plot| plot.id).collect();
    assert_eq!(unique.len(), 6);
    assert_eq!(
        geo.expansion_plots
            .iter()
            .filter(|plot| plot.use_kind == "housing")
            .count(),
        4
    );
    assert_eq!(
        geo.expansion_plots
            .iter()
            .filter(|plot| plot.use_kind == "garden")
            .count(),
        2
    );
    for plot in geo.expansion_plots {
        assert_eq!((plot.width_mm, plot.depth_mm), (6000, 6000));
        for dx in [-3000, 3000] {
            for dz in [-3000, 3000] {
                let corner = SimPos::new(plot.x + dx, plot.z + dz);
                assert!(
                    geo.contains_mm(corner),
                    "{} corner {corner:?} is on water",
                    plot.id
                );
                assert!(geo.land_cell(corner.x.div_euclid(1000), corner.z.div_euclid(1000)));
            }
        }
        for entity in app.world().resource::<SimIndex>().map.values() {
            let Some(pos) = app.world().get::<SimPos>(*entity) else {
                continue;
            };
            if app.world().get::<VillagePlace>(*entity).is_some() {
                assert!(
                    !intersects((plot.x, plot.z, 6000, 6000), (pos.x, pos.z, 2000, 2000)),
                    "{} overlaps a workplace",
                    plot.id
                );
            }
            if app.world().get::<GridOccluder>(*entity).is_some()
                && let Some(blocker) = app.world().get::<GridBlocker>(*entity)
            {
                assert!(
                    !intersects(
                        (plot.x, plot.z, 6000, 6000),
                        (
                            pos.x,
                            pos.z,
                            blocker.0.half_x_mm * 2,
                            blocker.0.half_z_mm * 2
                        )
                    ),
                    "{} overlaps a building",
                    plot.id
                );
            }
            if app.world().get::<ResourcePatch>(*entity).is_some()
                || app
                    .world()
                    .get::<village::hunting::WildlifeHabitat>(*entity)
                    .is_some()
            {
                assert!(
                    !intersects((plot.x, plot.z, 6000, 6000), (pos.x, pos.z, 4000, 4000)),
                    "{} overlaps a resource reserve",
                    plot.id
                );
            }
        }
        assert!(
            geo.roads.iter().any(|road| {
                let x = (plot.x - road.x).abs() * 2;
                let z = (plot.z - road.z).abs() * 2;
                x <= plot.width_mm + road.width_mm && z <= plot.depth_mm + road.depth_mm
            }),
            "{} has no road frontage",
            plot.id
        );
        assert!(
            !geo.roads.iter().any(|road| intersects(
                (plot.x, plot.z, 6000, 6000),
                (road.x, road.z, road.width_mm, road.depth_mm)
            )),
            "{} contains a through-road",
            plot.id
        );
    }
    for (index, plot) in geo.expansion_plots.iter().enumerate() {
        for other in &geo.expansion_plots[index + 1..] {
            assert!(!intersects(
                (plot.x, plot.z, 6000, 6000),
                (other.x, other.z, 6000, 6000)
            ));
        }
    }
    let mut reached = BTreeSet::from([0_usize]);
    for _ in geo.roads {
        for (a, road) in geo.roads.iter().enumerate() {
            if !reached.contains(&a) {
                continue;
            }
            for (b, other) in geo.roads.iter().enumerate() {
                if (road.x - other.x).abs() * 2 <= road.width_mm + other.width_mm
                    && (road.z - other.z).abs() * 2 <= road.depth_mm + other.depth_mm
                {
                    reached.insert(b);
                }
            }
        }
    }
    assert_eq!(
        reached.len(),
        geo.roads.len(),
        "road rectangles form one connected network"
    );
    for road in geo.roads {
        for sx in [-1, 1] {
            for sz in [-1, 1] {
                assert!(geo.contains_mm(SimPos::new(
                    road.x + sx * road.width_mm / 2,
                    road.z + sz * road.depth_mm / 2
                )));
            }
        }
    }
}

#[test]
fn expanded_shoreline_appends_ten_blockers_and_six_plots_without_changing_incumbent_identities() {
    let baseline = island_content::try_new_sim_app_with(7, "living_village", |app| {
        app.insert_resource(geometry::FIXED);
    })
    .unwrap();
    let expanded = scene();
    assert_eq!(
        baseline.world().resource::<TestbedIds>().names,
        expanded.world().resource::<TestbedIds>().names
    );
    let previous: BTreeSet<_> = baseline
        .world()
        .resource::<SimIndex>()
        .map
        .keys()
        .copied()
        .collect();
    let current: BTreeSet<_> = expanded
        .world()
        .resource::<SimIndex>()
        .map
        .keys()
        .copied()
        .collect();
    assert!(previous.is_subset(&current));
    let added: Vec<_> = current.difference(&previous).copied().collect();
    assert_eq!(added.len(), 16);
    let appended_boundaries = added
        .iter()
        .filter(|id| {
            let entity = expanded.world().resource::<SimIndex>().get(**id).unwrap();
            expanded.world().get::<GridBlocker>(entity).is_some()
        })
        .count();
    let appended_plots = added
        .iter()
        .filter(|id| {
            let entity = expanded.world().resource::<SimIndex>().get(**id).unwrap();
            expanded
                .world()
                .get::<village::homestead::HomesteadPlot>(entity)
                .is_some()
        })
        .count();
    assert_eq!(appended_boundaries, 10);
    assert_eq!(appended_plots, 6);
    assert!(added.iter().all(|id| *id > *previous.last().unwrap()));
    let old_prey: Vec<_> = village::hunting::available_prey(baseline.world())
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    let new_prey: Vec<_> = village::hunting::available_prey(expanded.world())
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    assert_eq!(old_prey, new_prey);
    for (id, entity) in &baseline.world().resource::<SimIndex>().map {
        if baseline.world().get::<Villager>(*entity).is_some() {
            let current = expanded.world().resource::<SimIndex>().get(*id).unwrap();
            assert_eq!(
                baseline.world().get::<Villager>(*entity),
                expanded.world().get::<Villager>(current)
            );
            assert_eq!(
                baseline.world().get::<Inventory>(*entity),
                expanded.world().get::<Inventory>(current)
            );
        }
    }
}

#[test]
fn actual_navigation_keeps_water_blocked_and_reaches_every_source_home_and_plot() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let geo = geometry::for_world(app.world());
    let grid = app.world().resource::<SpatialGrid>();
    let start = grid.cell_of(SimPos::new(500, -1500));
    let mut reached = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        for (dx, dz) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
            let next = CellCoord::new(cell.x + dx, cell.z + dz);
            if geo.land_cell(next.x, next.z) && !grid.is_blocking(next) && reached.insert(next) {
                queue.push_back(next);
            }
        }
    }
    for z in geometry::MIN_CELL..geometry::MAX_CELL_EXCLUSIVE {
        for x in geometry::MIN_CELL..geometry::MAX_CELL_EXCLUSIVE {
            let cell = CellCoord::new(x, z);
            if !geo.land_cell(x, z) {
                assert!(grid.is_blocking(cell), "water cell {cell:?} is open");
            }
            if geo.land_cell(x, z) && !geometry::FIXED.land_cell(x, z) {
                assert!(
                    !grid.is_blocking(cell),
                    "old shoreline still blocks expanded land {cell:?}"
                );
            }
        }
    }
    for (id, entity) in &app.world().resource::<SimIndex>().map {
        if app.world().get::<VillagePlace>(*entity).is_some()
            || app.world().get::<ResourcePatch>(*entity).is_some()
            || app
                .world()
                .get::<village::hunting::Wildlife>(*entity)
                .is_some()
        {
            let pos = *app.world().get::<SimPos>(*entity).unwrap();
            assert!(geo.contains_mm(pos));
            assert!(
                reached.contains(&grid.cell_of(pos)),
                "target {id:?} at{pos:?} is unreachable"
            );
        }
        if let Some(person) = app.world().get::<Villager>(*entity) {
            assert!(
                reached.contains(&grid.cell_of(person.home)),
                "home for{} is unreachable",
                person.name
            );
        }
    }
    for plot in geo.expansion_plots {
        assert!(reached.contains(&grid.cell_of(SimPos::new(plot.x, plot.z))));
    }
}

#[test]
fn geography_is_read_only_and_snapshot_resume_keeps_the_expanded_boundary() {
    let mut app = scene();
    let before = state_hash(app.world_mut());
    assert_eq!(geometry::for_world(app.world()).radii_mm, (30_000, 25_000));
    assert_eq!(state_hash(app.world_mut()), before);
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 77, 400, 40);
}
