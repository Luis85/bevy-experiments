use super::*;
use island_content::village::terrain::TerrainState;
use model::Limits;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::run_ticks;
use std::collections::BTreeSet;

fn fixture() -> App {
    let mut app = island_content::new_sim_app(7, "island_colony");
    app.init_resource::<ShapeTools>()
        .insert_resource(IslandMode::Editor);
    app
}

fn begin(world: &mut World, cell: (i32, i32)) {
    let anchor_mm = picking::terrain_cell(world, cell.0, cell.1)
        .expect("cell")
        .height_mm;
    world.resource_mut::<ShapeTools>().stroke = Some(ActiveStroke {
        cells: Stroke::default(),
        anchor_mm,
        last_cell: None,
        since_dab: DAB_SECS,
        commands: 0,
    });
}

fn heights(world: &World, cells: &[(i32, i32)]) -> Vec<Cell> {
    cells
        .iter()
        .map(|(x, z)| picking::terrain_cell(world, *x, *z).expect("cell"))
        .collect()
}

/// Record queued commands, let the simulation apply them, then settle.
fn apply(app: &mut App) {
    for _ in 0..40 {
        process(app.world_mut());
        if app.world().resource::<ShapeTools>().queued() == 0 {
            return;
        }
        run_ticks(app.world_mut(), 1);
    }
    panic!("queued terrain commands never settled");
}

#[test]
fn the_outline_matches_the_authoritative_circular_terraform_brush() {
    for radius in 0..=3 {
        let mut app = fixture();
        let before = app.world().resource::<TerrainState>().cells.clone();
        let command = Terraform {
            x: 14,
            z: 16,
            radius,
            height_mm: 250,
            surface: TerrainSurface::Stone,
        };
        command.perform(app.world_mut()).unwrap();
        let changed: BTreeSet<_> = app
            .world()
            .resource::<TerrainState>()
            .cells
            .iter()
            .zip(&before)
            .enumerate()
            .filter(|(_, (after, before))| after != before)
            .map(|(index, _)| ((index % 64) as i32 - 32, (index / 64) as i32 - 32))
            .collect();
        let preview: BTreeSet<_> = model::disc((command.x, command.z), radius)
            .filter(|(x, z)| picking::terrain_cell(app.world(), *x, *z).is_some())
            .collect();
        assert_eq!(
            preview, changed,
            "radius {radius} outline must cover exactly the cells the command modifies"
        );
    }
}

#[test]
fn a_drag_records_relative_edits_and_undo_redo_restore_them_through_commands() {
    let mut app = fixture();
    {
        let mut tools = app.world_mut().resource_mut::<ShapeTools>();
        tools.brush.tool = Tool::Raise;
        tools.brush.radius = 1;
        tools.brush.falloff = Falloff::Soft;
    }
    let cells: Vec<_> = model::disc((14, 16), 1)
        .chain(model::disc((15, 16), 1))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let original = heights(app.world(), &cells);
    let hash = state_hash(app.world_mut());

    begin(app.world_mut(), (14, 16));
    dab(app.world_mut(), (14, 16));
    assert_eq!(
        hash,
        state_hash(app.world_mut()),
        "planning a dab only queues view-side work"
    );
    // The second dab of the drag builds on the first one's planned heights.
    app.world_mut()
        .resource_mut::<ShapeTools>()
        .stroke
        .as_mut()
        .unwrap()
        .since_dab = DAB_SECS;
    dab(app.world_mut(), (15, 16));
    assert_eq!(app.world().resource::<ShapeTools>().queued(), 10);
    finish_stroke(app.world_mut());
    apply(&mut app);

    let sculpted = heights(app.world(), &cells);
    let centre = cells.iter().position(|cell| *cell == (14, 16)).unwrap();
    assert_eq!(
        sculpted[centre].height_mm,
        // Full strength from the first dab plus the rim of the second.
        original[centre].height_mm + 250 + 63,
        "raise is relative and accumulates across overlapping dabs"
    );
    assert!(
        sculpted
            .iter()
            .zip(&original)
            .all(|(after, before)| after.height_mm > before.height_mm
                && after.surface == before.surface)
    );
    assert!(app.world().resource::<ShapeTools>().can_undo());
    assert!(
        app.world()
            .resource::<ShapeTools>()
            .cursor_reason()
            .is_none()
    );

    app.world_mut()
        .resource_mut::<ShapeTools>()
        .requests
        .push(Action::Undo);
    apply(&mut app);
    assert_eq!(heights(app.world(), &cells), original, "undo is exact");
    assert!(app.world().resource::<ShapeTools>().can_redo());

    app.world_mut()
        .resource_mut::<ShapeTools>()
        .requests
        .push(Action::Redo);
    apply(&mut app);
    assert_eq!(heights(app.world(), &cells), sculpted, "redo is exact");
}

#[test]
fn a_stroke_issues_a_bounded_number_of_commands() {
    let mut app = fixture();
    {
        let mut tools = app.world_mut().resource_mut::<ShapeTools>();
        tools.brush.tool = Tool::Raise;
        tools.brush.radius = 3;
        tools.brush.falloff = Falloff::Soft;
    }
    begin(app.world_mut(), (14, 16));
    let mut recorded = 0;
    for step in 0..200 {
        app.world_mut()
            .resource_mut::<ShapeTools>()
            .stroke
            .as_mut()
            .unwrap()
            .since_dab = DAB_SECS;
        // Sweep back and forth; each dab is at most one command per disc cell.
        dab(app.world_mut(), (10 + step % 9, 16));
        let state = app.world().resource::<ShapeTools>();
        let stroke = state.stroke.as_ref().unwrap();
        assert!(stroke.commands <= MAX_STROKE_COMMANDS);
        assert!(state.outbox.len() <= OUTBOX_LIMIT);
        recorded = stroke.commands;
        // Keep the outbox moving like a running world would.
        let drained: Vec<_> = app
            .world_mut()
            .resource_mut::<ShapeTools>()
            .outbox
            .drain(..)
            .collect();
        assert!(drained.len() <= 29, "a radius-3 dab is at most 29 commands");
    }
    assert!(
        recorded > MAX_STROKE_COMMANDS - 29,
        "the stroke reached its cap"
    );
    assert!(
        app.world()
            .resource::<ShapeTools>()
            .status
            .contains("size limit")
    );
}

#[test]
fn a_refused_edit_reports_the_simulations_reason_and_changes_nothing() {
    let mut app = fixture();
    let limits = Limits::LEGACY;
    // Find a protected cell: the authoritative check refuses raising it.
    let blocked = (-31..31)
        .flat_map(|z| (-31..31).map(move |x| (x, z)))
        .find(|(x, z)| {
            let cell = picking::terrain_cell(app.world(), *x, *z).unwrap();
            cell.surface != TerrainSurface::Water
                && cell.height_mm + 250 <= limits.dry.1
                && Terraform {
                    x: *x,
                    z: *z,
                    radius: 0,
                    height_mm: cell.height_mm + 250,
                    surface: cell.surface,
                }
                .check(app.world())
                .is_err()
        })
        .expect("the village protects some ground");
    {
        let mut tools = app.world_mut().resource_mut::<ShapeTools>();
        tools.brush.tool = Tool::Raise;
        tools.brush.radius = 0;
    }
    let before = heights(app.world(), &[blocked]);
    refresh_preview(app.world_mut(), Some(blocked));
    assert!(
        app.world().resource::<ShapeTools>().preview_error.is_some(),
        "the preview explains the refusal before any click"
    );
    begin(app.world_mut(), blocked);
    dab(app.world_mut(), blocked);
    finish_stroke(app.world_mut());
    apply(&mut app);
    let tools = app.world().resource::<ShapeTools>();
    assert_eq!(tools.refused, 1);
    assert!(tools.status.starts_with("Not applied:"));
    assert!(tools.cursor_reason().is_some());
    assert!(
        !tools.can_undo() && !tools.can_redo(),
        "a stroke of refused dabs alone is no history"
    );
    assert!(
        tools.expected.is_empty(),
        "no stale planned height survives"
    );
    assert_eq!(heights(app.world(), &[blocked]), before);
}

#[test]
fn tool_requests_respect_the_commands_limits() {
    let mut app = fixture();
    for _ in 0..12 {
        apply_request(app.world_mut(), Action::Radius(1));
    }
    assert_eq!(
        app.world().resource::<ShapeTools>().brush.radius,
        Limits::LEGACY.max_radius
    );
    for _ in 0..12 {
        apply_request(app.world_mut(), Action::Radius(-1));
    }
    assert_eq!(app.world().resource::<ShapeTools>().brush.radius, 0);
    apply_request(app.world_mut(), Action::NextSurface);
    let tools = app.world().resource::<ShapeTools>();
    assert_eq!(
        (tools.brush.tool, tools.brush.surface),
        (Tool::Paint, TerrainSurface::Soil)
    );
    apply_request(app.world_mut(), Action::Undo);
    assert_eq!(
        app.world().resource::<ShapeTools>().status,
        "Nothing to undo."
    );
}

// The regional living world records each dab as one command.

use island_content::village::frontier;

fn regional() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    app.init_resource::<ShapeTools>()
        .insert_resource(IslandMode::Editor);
    app
}

fn regional_revision(world: &World) -> u64 {
    world.resource::<frontier::terrain::TerrainState>().revision
}

/// The centre of dry ground, clear of homes, sites and residents, where a
/// brush two cells wider than `radius` may raise every cell.
fn open_ground(world: &World, radius: u32) -> (i32, i32) {
    for z in (-90..90).step_by(4) {
        for x in (-90..90).step_by(4) {
            let cells: Vec<_> = model::disc((x, z), radius + 2)
                .filter_map(|at| picking::terrain_cell(world, at.0, at.1).map(|cell| (at, cell)))
                .collect();
            if cells.len() != model::disc((x, z), radius + 2).count()
                || cells
                    .iter()
                    .any(|(_, cell)| cell.surface == TerrainSurface::Water || cell.height_mm < 1000)
            {
                continue;
            }
            let raised = Command::Cells(
                cells
                    .into_iter()
                    .map(|(at, cell)| {
                        (
                            at,
                            Cell {
                                height_mm: cell.height_mm + 250,
                                ..cell
                            },
                        )
                    })
                    .collect(),
            );
            if check(world, &raised).is_ok() {
                return (x, z);
            }
        }
    }
    panic!("no open ground for a radius {radius} brush");
}

fn set_brush(world: &mut World, tool: Tool, radius: u32, falloff: Falloff) {
    let mut tools = world.resource_mut::<ShapeTools>();
    tools.brush.tool = tool;
    tools.brush.radius = radius;
    tools.brush.falloff = falloff;
    tools.brush.surface = TerrainSurface::Stone;
}

/// One dab of a fresh stroke, left in the outbox for inspection.
fn one_dab(world: &mut World, cell: (i32, i32)) -> Vec<Queued> {
    begin(world, cell);
    dab(world, cell);
    finish_stroke(world);
    world
        .resource::<ShapeTools>()
        .outbox
        .iter()
        .cloned()
        .collect()
}

fn recorded_kind(command: &Command) -> String {
    match sim_command(command) {
        SimCommand::Content { kind, .. } => kind,
        other => panic!("a content command, not {other:?}"),
    }
}

#[test]
fn every_tool_records_one_batch_per_dab_in_the_regional_world() {
    let mut app = regional();
    let centre = open_ground(app.world(), 2);
    let beside = (centre.0 + 1, centre.1);
    // Each dab works on what the one before left: a soft cone, flattened
    // off-centre, smoothed, lowered and finally paved.
    for (tool, at) in [
        (Tool::Raise, centre),
        (Tool::Flatten, beside),
        (Tool::Smooth, centre),
        (Tool::Lower, centre),
        (Tool::Paint, centre),
    ] {
        set_brush(app.world_mut(), tool, 2, Falloff::Soft);
        let revision = regional_revision(app.world());
        let queued = one_dab(app.world_mut(), at);
        assert_eq!(queued.len(), 1, "{tool:?}: one command per dab");
        assert_eq!(queued[0].origin, Origin::Dab);
        let Command::Cells(planned) = queued[0].command.clone() else {
            panic!("{tool:?}: a batch, not {:?}", queued[0].command);
        };
        assert!(
            !planned.is_empty() && planned.len() <= 13,
            "{tool:?}: bounded by the brush"
        );
        // The recorded command is the simulation's own `TerraformCells`.
        assert_eq!(recorded_kind(&queued[0].command), "TerraformCells");
        let recorded = batch_command(&planned);
        assert_eq!(recorded.cells.len(), planned.len());
        assert!(
            recorded
                .cells
                .iter()
                .zip(&planned)
                .all(|(cell, (at, value))| (cell.x, cell.z) == *at
                    && cell.height_mm == value.height_mm
                    && cell.surface == value.surface)
        );
        apply(&mut app);
        let tools = app.world().resource::<ShapeTools>();
        assert_eq!(tools.refused, 0, "{tool:?}: {}", tools.status);
        assert_eq!(
            regional_revision(app.world()),
            revision + 1,
            "{tool:?}: one command, one terrain revision"
        );
        for (at, value) in planned {
            assert_eq!(
                picking::terrain_cell(app.world(), at.0, at.1),
                Some(value),
                "{tool:?} at {at:?}"
            );
        }
    }
    // A hard brush whose cells all end at one value is still the single
    // disc `Terraform`: flatten a patch, then raise it.
    set_brush(app.world_mut(), Tool::Flatten, 1, Falloff::Hard);
    one_dab(app.world_mut(), centre);
    apply(&mut app);
    set_brush(app.world_mut(), Tool::Raise, 1, Falloff::Hard);
    let queued = one_dab(app.world_mut(), centre);
    assert_eq!(queued.len(), 1);
    assert!(
        matches!(queued[0].command, Command::Disc { radius: 1, .. }),
        "{:?}",
        queued[0].command
    );
    assert_eq!(recorded_kind(&queued[0].command), "Terraform");
    apply(&mut app);
    assert_eq!(app.world().resource::<ShapeTools>().refused, 0);
}

#[test]
fn a_regional_drag_and_its_undo_and_redo_are_one_command_per_step() {
    let mut app = regional();
    let centre = open_ground(app.world(), 2);
    set_brush(app.world_mut(), Tool::Raise, 2, Falloff::Soft);
    let cells: Vec<_> = model::disc(centre, 2)
        .chain(model::disc((centre.0 + 1, centre.1), 2))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let original = heights(app.world(), &cells);
    let revision = regional_revision(app.world());

    begin(app.world_mut(), centre);
    dab(app.world_mut(), centre);
    app.world_mut()
        .resource_mut::<ShapeTools>()
        .stroke
        .as_mut()
        .unwrap()
        .since_dab = DAB_SECS;
    dab(app.world_mut(), (centre.0 + 1, centre.1));
    assert_eq!(
        app.world().resource::<ShapeTools>().queued(),
        2,
        "two dabs are two commands, not one per cell"
    );
    finish_stroke(app.world_mut());
    apply(&mut app);
    let sculpted = heights(app.world(), &cells);
    assert!(
        sculpted
            .iter()
            .zip(&original)
            .all(|(after, before)| after.height_mm > before.height_mm)
    );
    assert_eq!(regional_revision(app.world()), revision + 2);

    for (action, expected, steps) in [
        (Action::Undo, &original, 3),
        (Action::Redo, &sculpted, 4),
        (Action::Undo, &original, 5),
    ] {
        apply_request(app.world_mut(), action);
        let outbox: Vec<Queued> = app
            .world()
            .resource::<ShapeTools>()
            .outbox
            .iter()
            .cloned()
            .collect();
        assert_eq!(outbox.len(), 1, "{action:?} is one command");
        assert!(matches!(
            outbox[0].origin,
            Origin::History { whole: true, .. }
        ));
        let Command::Cells(batch) = &outbox[0].command else {
            panic!("{action:?} is a batch, not {:?}", outbox[0].command);
        };
        assert_eq!(batch.len(), cells.len(), "every cell of the stroke");
        apply(&mut app);
        assert_eq!(
            &heights(app.world(), &cells),
            expected,
            "{action:?} is exact"
        );
        assert_eq!(regional_revision(app.world()), revision + steps);
        assert_eq!(app.world().resource::<ShapeTools>().refused, 0);
    }
    assert!(app.world().resource::<ShapeTools>().can_redo());
}

#[test]
fn a_refused_batch_changes_nothing_and_shows_the_simulations_reason() {
    let mut app = regional();
    let settlement = frontier::settlements::all(app.world())[0].0;
    let door = frontier::settlements::home_def(app.world(), settlement, 0)
        .expect("home")
        .door;
    let at = (door.x.div_euclid(1000), door.z.div_euclid(1000));
    set_brush(app.world_mut(), Tool::Raise, 2, Falloff::Soft);
    let cells: Vec<_> = model::disc(at, 2).collect();
    let before = heights(app.world(), &cells);
    let revision = regional_revision(app.world());
    refresh_preview(app.world_mut(), Some(at));
    let preview = app
        .world()
        .resource::<ShapeTools>()
        .preview_error
        .clone()
        .expect("the preview validates the whole batch before any click");
    assert!(preview.contains("overlaps"), "{preview}");

    let queued = one_dab(app.world_mut(), at);
    assert_eq!(queued.len(), 1);
    apply(&mut app);
    let tools = app.world().resource::<ShapeTools>();
    assert_eq!(tools.refused, 1);
    assert!(tools.status.starts_with("Not applied:"), "{}", tools.status);
    assert!(
        tools
            .cursor_reason()
            .is_some_and(|reason| reason.contains("overlaps")),
        "{:?}",
        tools.cursor_reason()
    );
    assert!(
        tools.expected.is_empty(),
        "no stale planned height survives"
    );
    assert_eq!(heights(app.world(), &cells), before, "all or nothing");
    assert_eq!(regional_revision(app.world()), revision);
}

#[test]
fn a_refused_undo_keeps_the_stroke_undoable() {
    let mut app = regional();
    let centre = open_ground(app.world(), 2);
    let beside = (centre.0 + 1, centre.1);
    set_brush(app.world_mut(), Tool::Raise, 0, Falloff::Hard);
    let original = heights(app.world(), &[centre])[0];
    one_dab(app.world_mut(), centre);
    apply(&mut app);
    assert_eq!(
        heights(app.world(), &[centre])[0].height_mm,
        original.height_mm + 250
    );
    // Fixture shortcut: a neighbour now stands so high that lowering the
    // cell again would open a cliff, which the simulation refuses.
    let index = app
        .world()
        .resource::<frontier::terrain::TerrainState>()
        .iter()
        .position(|(cell, _)| (cell.x, cell.z) == beside)
        .expect("neighbour");
    let neighbour = app
        .world()
        .resource::<frontier::terrain::TerrainState>()
        .cells[index]
        .height_mm;
    app.world_mut()
        .resource_mut::<frontier::terrain::TerrainState>()
        .cells[index]
        .height_mm = original.height_mm + 1700;
    apply_request(app.world_mut(), Action::Undo);
    apply(&mut app);
    {
        let tools = app.world().resource::<ShapeTools>();
        assert_eq!(tools.refused, 1);
        assert!(
            tools.status.starts_with("Undo not applied:"),
            "{}",
            tools.status
        );
        assert!(
            tools
                .cursor_reason()
                .is_some_and(|reason| reason.contains("cliff"))
        );
        assert!(
            tools.can_undo() && !tools.can_redo(),
            "the step was taken back"
        );
    }
    assert_eq!(
        heights(app.world(), &[centre])[0].height_mm,
        original.height_mm + 250,
        "nothing changed"
    );
    // With the obstacle gone the same undo applies.
    app.world_mut()
        .resource_mut::<frontier::terrain::TerrainState>()
        .cells[index]
        .height_mm = neighbour;
    apply_request(app.world_mut(), Action::Undo);
    apply(&mut app);
    assert_eq!(heights(app.world(), &[centre])[0], original);
    let tools = app.world().resource::<ShapeTools>();
    assert!(tools.can_redo() && !tools.can_undo());
}

#[test]
fn a_stroke_stays_small_enough_for_one_undo_batch() {
    let mut app = regional();
    set_brush(app.world_mut(), Tool::Raise, 8, Falloff::Soft);
    let limit = Limits::REGIONAL.max_batch;
    begin(app.world_mut(), (0, 0));
    // Sweep rows across the island; only the bound matters here.
    let mut full = false;
    for z in (-80..80).step_by(12) {
        for x in (-100..100).step_by(6) {
            if full {
                break;
            }
            app.world_mut()
                .resource_mut::<ShapeTools>()
                .stroke
                .as_mut()
                .unwrap()
                .since_dab = DAB_SECS;
            dab(app.world_mut(), (x, z));
            let mut tools = app.world_mut().resource_mut::<ShapeTools>();
            assert!(tools.stroke.as_ref().unwrap().cells.len() <= limit);
            let drained: Vec<_> = tools.outbox.drain(..).collect();
            assert!(drained.len() <= 1, "one command per dab");
            full = tools.status.contains("size limit");
        }
    }
    let tools = app.world().resource::<ShapeTools>();
    assert!(full, "{}", tools.status);
    assert!(tools.stroke.as_ref().unwrap().cells.len() > limit - 197);
}

#[test]
fn the_comparison_switch_records_cell_by_cell_and_latency_is_summarised() {
    let mut app = regional();
    let centre = open_ground(app.world(), 2);
    app.world_mut().resource_mut::<ShapeTools>().per_cell = true;
    set_brush(app.world_mut(), Tool::Raise, 2, Falloff::Soft);
    let queued = one_dab(app.world_mut(), centre);
    assert_eq!(queued.len(), 13, "the old shape: one command per cell");
    assert!(
        queued
            .iter()
            .all(|queued| matches!(queued.command, Command::Disc { radius: 0, .. }))
    );
    assert_eq!(latency_summary(&[]), None);
    assert_eq!(latency_summary(&[40.0]), Some((40.0, 40.0, 40.0)));
    let samples: Vec<f32> = (1..=100).rev().map(|ms| ms as f32).collect();
    assert_eq!(latency_summary(&samples), Some((51.0, 95.0, 100.0)));
}
