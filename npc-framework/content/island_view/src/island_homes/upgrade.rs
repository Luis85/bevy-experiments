//! The played character's homestead upgrade, drawn from `view::upgrade_site`
//! (work left against the tier's `work_ticks`, materials delivered):
//! - while materials come in, they are stacked beside the cottage and the
//!   new part's sill beams lie on the ground behind it;
//! - once any work is done or everything is in, a scaffold stands round
//!   cottage and new part, its working rails climbing with the work;
//! - the new part rises with the work: its frame posts go up, then the
//!   plank walls climb them, then its roof boards go on;
//! - when the upgrade is finished the site reports nothing and all of it
//!   is taken down, and the cottage is fitted again when the simulation
//!   grows its body (the `larger_cottage` tier turns the 4 by 5 m body into
//!   5 by 6 m on the same entity).
//!
//! View entities only.
use super::*;

/// Seconds between two looks at the upgrade and the cottage's body.
const LOOK_SECS: f32 = 0.5;
/// The scaffold stands this far out from the walls.
const SCAFFOLD_OUT_M: f32 = 0.5;
/// Heights of the scaffold with no work done and with all of it.
const SCAFFOLD_LOW_M: f32 = 1.2;
const SCAFFOLD_HIGH_M: f32 = 5.0;
/// At most this many logs and planks are drawn in the stack.
const STACK_MAX: u32 = 10;
/// The new part's depth behind the cottage's back wall, and its walls'
/// height.
const EXTENSION_M: f32 = 1.4;
const WALL_M: f32 = 2.3;

/// What the scaffold shows: the cottage's root, work done in tenths and
/// logs and planks delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shown {
    root: Entity,
    tenths: u32,
    logs: u32,
    planks: u32,
    supplied: bool,
}

/// The scaffold drawn now.
#[derive(Default)]
pub(super) struct ScaffoldState {
    since: f32,
    shown: Option<(Shown, Entity)>,
}

/// How far along a site's work is, 0 to 1.
pub(crate) fn progress(work_ticks: u32, remaining_ticks: u64) -> f32 {
    if work_ticks == 0 {
        return 0.0;
    }
    let done = u64::from(work_ticks).saturating_sub(remaining_ticks);
    (done as f32 / work_ticks as f32).clamp(0.0, 1.0)
}

/// Whether the scaffold stands: once any work is done or all the
/// materials are in.
pub(crate) fn scaffold_up(done: f32, supplied: bool) -> bool {
    supplied || done > 0.0
}

/// The new part behind the cottage's back wall (local +Z beyond `depth`/2),
/// `width` across, at work `done` (0 to 1): `(centre, size, is_plank)` of
/// its sill beams (always), frame posts (rising over the first fifth of
/// the work, more of them as it goes), plank walls (climbing the posts
/// from a fifth to four fifths) and roof boards (the last fifth).
pub(crate) fn extension_parts(width: f32, depth: f32, done: f32) -> Vec<(Vec3, Vec3, bool)> {
    let done = done.clamp(0.0, 1.0);
    let back = depth * 0.5;
    let (z0, z1) = (back, back + EXTENSION_M);
    let half = width * 0.5;
    let mut parts = Vec::new();
    // Sill beams: the footprint laid out on the ground.
    for z in [z0 + 0.06, z1 - 0.06] {
        parts.push((Vec3::new(0.0, 0.06, z), Vec3::new(width, 0.12, 0.12), false));
    }
    for x in [-half + 0.06, half - 0.06] {
        parts.push((
            Vec3::new(x, 0.06, (z0 + z1) * 0.5),
            Vec3::new(0.12, 0.12, EXTENSION_M),
            false,
        ));
    }
    // Frame posts: six of them along the outer walls, raised in turn.
    let posts = [
        (-half + 0.06, z1 - 0.06),
        (half - 0.06, z1 - 0.06),
        (-half + 0.06, (z0 + z1) * 0.5),
        (half - 0.06, (z0 + z1) * 0.5),
        (-half * 0.33, z1 - 0.06),
        (half * 0.33, z1 - 0.06),
    ];
    let raised = ((done / 0.2) * posts.len() as f32).floor() as usize;
    for (x, z) in posts.iter().take(raised.min(posts.len())) {
        parts.push((
            Vec3::new(*x, WALL_M * 0.5, *z),
            Vec3::new(0.12, WALL_M, 0.12),
            false,
        ));
    }
    // Plank walls climbing the frame on the three outer sides.
    let wall = ((done - 0.2) / 0.6).clamp(0.0, 1.0) * WALL_M;
    if wall > 0.05 {
        let y = 0.12 + wall * 0.5;
        parts.push((
            Vec3::new(0.0, y, z1 - 0.03),
            Vec3::new(width, wall, 0.05),
            true,
        ));
        for x in [-half + 0.03, half - 0.03] {
            parts.push((
                Vec3::new(x, y, (z0 + z1) * 0.5),
                Vec3::new(0.05, wall, EXTENSION_M),
                true,
            ));
        }
    }
    // Roof boards across the top, laid from the old wall outwards.
    let roof = ((done - 0.8) / 0.2).clamp(0.0, 1.0);
    let boards = (roof * 5.0).floor() as usize;
    for board in 0..boards {
        let z = z0 + (board as f32 + 0.5) * EXTENSION_M / 5.0;
        parts.push((
            Vec3::new(0.0, WALL_M + 0.18, z),
            Vec3::new(width + 0.3, 0.05, EXTENSION_M / 5.0 - 0.02),
            true,
        ));
    }
    parts
}

/// The scaffold's parts in the cottage's frame (local +Z from the entrance
/// into the body): `(centre, size)` of each pole and rail, for a body
/// `width` by `depth` and work `done` (0 to 1). It stands round the
/// cottage and its new part behind it.
pub(crate) fn scaffold_parts(width: f32, depth: f32, done: f32) -> Vec<(Vec3, Vec3)> {
    let mut parts = scaffold_round(width, depth + EXTENSION_M, done);
    for (centre, _) in &mut parts {
        centre.z += EXTENSION_M * 0.5;
    }
    parts
}

fn scaffold_round(width: f32, depth: f32, done: f32) -> Vec<(Vec3, Vec3)> {
    let (hx, hz) = (width * 0.5 + SCAFFOLD_OUT_M, depth * 0.5 + SCAFFOLD_OUT_M);
    let tall = SCAFFOLD_LOW_M + (SCAFFOLD_HIGH_M - SCAFFOLD_LOW_M) * done.clamp(0.0, 1.0);
    // The poles stand full height from the start; the rails (the working
    // platforms) climb with the work.
    let pole = SCAFFOLD_HIGH_M + 0.4;
    let mut parts = Vec::new();
    for (x, z) in [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)] {
        parts.push((Vec3::new(x, pole * 0.5, z), Vec3::new(0.12, pole, 0.12)));
    }
    // Midway poles along the long sides.
    for x in [-hx, hx] {
        parts.push((Vec3::new(x, pole * 0.5, 0.0), Vec3::new(0.1, pole, 0.1)));
    }
    // A rail round all four sides every 1.4 m of height, the top one too.
    let mut y = 1.0_f32;
    let mut levels = Vec::new();
    while y < tall - 0.2 {
        levels.push(y);
        y += 1.4;
    }
    levels.push(tall - 0.05);
    for y in levels {
        parts.push((Vec3::new(0.0, y, -hz), Vec3::new(hx * 2.0, 0.07, 0.07)));
        parts.push((Vec3::new(0.0, y, hz), Vec3::new(hx * 2.0, 0.07, 0.07)));
        parts.push((Vec3::new(-hx, y, 0.0), Vec3::new(0.07, 0.07, hz * 2.0)));
        parts.push((Vec3::new(hx, y, 0.0), Vec3::new(0.07, 0.07, hz * 2.0)));
    }
    parts
}

/// The stack of delivered goods beside the cottage's +X wall, logs below
/// and planks on top: `(centre, size, is_plank)`.
pub(crate) fn stack_parts(width: f32, logs: u32, planks: u32) -> Vec<(Vec3, Vec3, bool)> {
    let x = width * 0.5 + SCAFFOLD_OUT_M + 1.0;
    let mut parts = Vec::new();
    let mut top = 0.0_f32;
    for index in 0..logs.min(STACK_MAX) {
        let (row, across) = (index / 4, index % 4);
        let y = 0.13 + row as f32 * 0.24;
        parts.push((
            Vec3::new(x + across as f32 * 0.26 - 0.39, y, 0.0),
            Vec3::new(0.22, 0.22, 1.6),
            false,
        ));
        top = top.max(y + 0.11);
    }
    for index in 0..planks.min(STACK_MAX) {
        let y = top + 0.03 + index as f32 * 0.06;
        parts.push((Vec3::new(x, y, 0.0), Vec3::new(0.9, 0.05, 1.8), true));
    }
    parts
}

fn count(goods: &[(String, u32)], kind: &str) -> u32 {
    goods
        .iter()
        .filter(|(name, _)| name == kind)
        .map(|(_, units)| *units)
        .sum()
}

/// The played character's cottage: its visual root and shell.
fn played_cottage(world: &World) -> Option<(SimId, Entity, HomeShell)> {
    let played = world.get_resource::<VillageState>()?.played;
    let sims = world.get_resource::<SimIndex>()?;
    let shells: Vec<(SimId, Entity)> = world
        .get_resource::<Shells>()?
        .0
        .iter()
        .map(|(id, root)| (*id, *root))
        .collect();
    shells.into_iter().find_map(|(id, root)| {
        let sim = sims.get(id)?;
        world.get::<Cottage>(sim)?;
        world
            .get::<Household>(sim)?
            .residents
            .contains(&played)
            .then(|| world.get::<HomeShell>(root).cloned())
            .flatten()
            .map(|shell| (played, root, shell))
    })
}

/// Fit a homestead cottage again when the simulation has grown its body:
/// its drawing is taken down and `dress` builds it anew on the next pass.
pub(super) fn refit(world: &mut World, mut since: Local<f32>) {
    *since += world.resource::<Time>().delta_secs();
    if *since < LOOK_SECS {
        return;
    }
    *since = 0.0;
    let Some(sims) = world.get_resource::<SimIndex>() else {
        return;
    };
    let shells: Vec<(SimId, Entity)> = world
        .resource::<Shells>()
        .0
        .iter()
        .map(|(id, root)| (*id, *root))
        .collect();
    let mut stale = Vec::new();
    for (id, root) in shells {
        let Some(sim) = sims.get(id) else { continue };
        if world.get::<Cottage>(sim).is_none() {
            continue;
        }
        let (Some(shell), Some((frame, _))) =
            (world.get::<HomeShell>(root), subject(world, sim, id))
        else {
            continue;
        };
        let was = shell.frame;
        if (was.centre - frame.centre).length() > 0.05
            || (was.width - frame.width).abs() > 0.05
            || (was.depth - frame.depth).abs() > 0.05
        {
            stale.push((id, root, shell.frame_entity));
        }
    }
    for (id, root, frame_entity) in stale {
        world.resource_mut::<Cutaways>().open.remove(&id);
        world.resource_mut::<Shells>().0.remove(&id);
        if world.get_entity(frame_entity).is_ok() {
            world.entity_mut(frame_entity).despawn();
        }
        let footing: Vec<Entity> = world
            .get::<Children>(root)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| {
                        world
                            .get::<Name>(*child)
                            .is_some_and(|name| name.as_str() == "home footing")
                    })
                    .collect()
            })
            .unwrap_or_default();
        for entity in footing {
            world.entity_mut(entity).despawn();
        }
        world
            .entity_mut(root)
            .remove::<(HomeShell, Occluder, ShellChecked)>();
    }
}

/// Draw, redraw or take down the scaffold and the stack.
pub(super) fn scaffold(world: &mut World, mut state: Local<ScaffoldState>) {
    state.since += world.resource::<Time>().delta_secs();
    if state.since < LOOK_SECS {
        return;
    }
    state.since = 0.0;
    let wanted = played_cottage(world).and_then(|(played, root, shell)| {
        let site = view::upgrade_site(world, played)?;
        let done = progress(site.work_ticks, site.remaining_ticks);
        Some((
            Shown {
                root,
                tenths: (done * 20.0).round() as u32,
                logs: count(&site.delivered, "log"),
                planks: count(&site.delivered, "plank"),
                supplied: site.supplied,
            },
            shell,
        ))
    });
    if state.shown.map(|(shown, _)| shown) == wanted.as_ref().map(|(shown, _)| *shown) {
        return;
    }
    if let Some((_, entity)) = state.shown.take()
        && world.get_entity(entity).is_ok()
    {
        world.entity_mut(entity).despawn();
    }
    let Some((shown, shell)) = wanted else { return };
    if !shell_assets(world) {
        return;
    }
    let (cube, timber, wood) = {
        let assets = world.resource::<ShellAssets>();
        (
            assets.cube.clone(),
            assets.timber.clone(),
            assets.floor.clone(),
        )
    };
    let holder = world
        .spawn((
            Name::new("homestead upgrade scaffold"),
            ChildOf(shown.root),
            shell.local,
            Visibility::default(),
        ))
        .id();
    let done = shown.tenths as f32 / 20.0;
    if scaffold_up(done, shown.supplied) {
        for (centre, size) in scaffold_parts(shell.frame.width, shell.frame.depth, done) {
            block(world, holder, &timber, &cube, centre, size);
        }
    }
    for (centre, size, plank) in extension_parts(shell.frame.width, shell.frame.depth, done) {
        block(
            world,
            holder,
            if plank { &wood } else { &timber },
            &cube,
            centre,
            size,
        );
    }
    for (centre, size, plank) in stack_parts(shell.frame.width, shown.logs, shown.planks) {
        block(
            world,
            holder,
            if plank { &wood } else { &timber },
            &cube,
            centre,
            size,
        );
    }
    state.shown = Some((shown, holder));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_part_rises_with_the_work_and_the_scaffold_waits_for_the_materials() {
        assert!(!scaffold_up(0.0, false), "nothing in yet: no scaffold");
        assert!(scaffold_up(0.0, true) && scaffold_up(0.3, false));
        let at = |done: f32| extension_parts(4.0, 5.0, done);
        let posts = |parts: &[(Vec3, Vec3, bool)]| {
            parts
                .iter()
                .filter(|(_, size, plank)| !plank && size.y > 1.0)
                .count()
        };
        let wall = |parts: &[(Vec3, Vec3, bool)]| {
            parts
                .iter()
                .filter(|(_, size, plank)| *plank && size.y > 0.1)
                .map(|(_, size, _)| size.y)
                .fold(0.0_f32, f32::max)
        };
        let roof = |parts: &[(Vec3, Vec3, bool)]| {
            parts
                .iter()
                .filter(|(c, _, plank)| *plank && c.y > WALL_M)
                .count()
        };
        // Sills only at the start; posts over the first fifth; walls
        // climbing; the roof last.
        assert_eq!(
            (posts(&at(0.0)), wall(&at(0.0)), roof(&at(0.0))),
            (0, 0.0, 0)
        );
        assert!(posts(&at(0.1)) > 0 && posts(&at(0.1)) < posts(&at(0.25)));
        assert!(wall(&at(0.4)) > 0.0 && wall(&at(0.4)) < wall(&at(0.7)));
        assert_eq!(roof(&at(0.7)), 0);
        assert!(roof(&at(1.0)) > roof(&at(0.9)));
        assert!(at(1.0).len() > at(0.5).len() && at(0.5).len() > at(0.0).len());
        // All of it behind the back wall, within the body's width.
        for (centre, size, _) in at(1.0) {
            assert!(centre.z - size.z * 0.5 >= 2.5 - 1e-4, "{centre:?}");
            assert!(centre.z + size.z * 0.5 <= 2.5 + EXTENSION_M + 1e-4);
            assert!(centre.x.abs() + size.x * 0.5 <= 2.0 + 0.2);
        }
    }

    #[test]
    fn the_scaffold_rises_with_the_work_and_stands_clear_of_the_walls() {
        assert_eq!(progress(900, 900), 0.0);
        assert_eq!(progress(900, 0), 1.0);
        assert!((progress(900, 450) - 0.5).abs() < 1e-6);
        assert_eq!(progress(0, 0), 0.0);
        // The highest rail (a part longer than it is tall).
        let height = |parts: &[(Vec3, Vec3)]| {
            parts
                .iter()
                .filter(|(_, size)| size.x.max(size.z) > 1.0)
                .map(|(centre, size)| centre.y + size.y * 0.5)
                .fold(0.0_f32, f32::max)
        };
        let (low, high) = (scaffold_parts(4.0, 5.0, 0.0), scaffold_parts(4.0, 5.0, 1.0));
        assert!((height(&low) - SCAFFOLD_LOW_M).abs() < 0.05);
        assert!((height(&high) - SCAFFOLD_HIGH_M).abs() < 0.05);
        assert!(high.len() > low.len());
        // Every pole and rail is outside the 4 by 5 m body and its new part.
        for (centre, size) in &high {
            let (lo, hi) = (*centre - *size * 0.5, *centre + *size * 0.5);
            let inside = lo.x > -2.0 && hi.x < 2.0 && lo.z > -2.5 && hi.z < 2.5 + EXTENSION_M;
            assert!(!inside, "{centre:?} {size:?}");
            assert!(hi.x.abs().max(lo.x.abs()) <= 2.0 + SCAFFOLD_OUT_M + 0.1);
        }
        // The stack beside the +X wall, logs under planks, capped.
        let stack = stack_parts(4.0, 3, 2);
        assert_eq!(stack.len(), 5);
        assert!(
            stack
                .iter()
                .all(|(centre, size, _)| centre.x - size.x * 0.5 > 2.0)
        );
        let log_top = stack
            .iter()
            .filter(|part| !part.2)
            .map(|(c, s, _)| c.y + s.y * 0.5)
            .fold(0.0_f32, f32::max);
        assert!(
            stack
                .iter()
                .filter(|part| part.2)
                .all(|(c, _, _)| c.y > log_top)
        );
        assert_eq!(stack_parts(4.0, 40, 40).len(), (STACK_MAX * 2) as usize);
    }
}
