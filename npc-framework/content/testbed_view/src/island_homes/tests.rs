use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::run_ticks;
use testbed_content::village::Villager;

use super::model::*;
use super::*;
use crate::visuals::Motion;

const FACINGS: [u8; 4] = [0, 1, 2, 3];

/// A body `width` across and `depth` behind an entrance cell at (10, 20),
/// lying towards quarter turn `facing`, as `civic::placement` lays plots out.
fn body(width: i32, depth: i32, facing: u8) -> BodyFrame {
    let (x, z) = (10, 20);
    let (left, right) = ((width - 1) / 2, width / 2);
    let (lo, hi) = match facing % 4 {
        0 => ((x - left, z + 1), (x + right, z + depth)),
        1 => ((x + 1, z - left), (x + depth, z + right)),
        2 => ((x - right, z - depth), (x + left, z - 1)),
        _ => ((x - depth, z - right), (x - 1, z + left)),
    };
    BodyFrame::of_cells(lo, hi, facing, SimPos::new(x * 1000 + 500, z * 1000 + 500))
}

#[test]
fn a_resident_fits_through_every_door() {
    // The door is between 1.1 and 1.3 times as high as whoever uses it.
    for resident in [RESIDENT_HEIGHT_M, RESIDENT_HEIGHT_MIN_M] {
        let ratio = DOOR_HEIGHT_M / resident;
        assert!((1.1..=1.3).contains(&ratio), "{resident} m: {ratio}");
    }
    assert!((RESIDENT_HEIGHT_MIN_M - 1.725).abs() < 1e-4);
    // The walls stand above the door, and a resident clears the lintel.
    const { assert!(WALL_HEIGHT_M > DOOR_HEIGHT_M && DOOR_HEIGHT_M > RESIDENT_HEIGHT_M + 0.3) };
    // Whatever the body's size, a fitted model's own door is that high and
    // wide enough to walk through.
    for (width, depth, model) in [
        (4, 5, HouseModel::Cottage),
        (5, 6, HouseModel::Cottage),
        (5, 5, HouseModel::Hall),
    ] {
        let fit = model.fit(&body(width, depth, 0));
        assert!((fit.door_height() - DOOR_HEIGHT_M).abs() < 1e-4);
        assert!(
            fit.door_opening() > 0.8 && fit.door_opening() < 1.4,
            "{model:?} {width}x{depth}: {}",
            fit.door_opening()
        );
        assert!(fit.door_width() > fit.door_opening());
    }
}

#[test]
fn a_body_is_the_cell_rectangle_with_its_entrance_in_front() {
    for facing in FACINGS {
        for (width, depth) in [(4, 5), (5, 6), (5, 5)] {
            let frame = body(width, depth, facing);
            assert_eq!((frame.width, frame.depth), (width as f32, depth as f32));
            // The entrance cell touches the entrance wall: half a metre out.
            let entrance = frame.local_of(frame.entrance);
            assert!(
                (entrance.y + frame.depth * 0.5 + 0.5).abs() < 1e-4,
                "facing {facing}: {entrance}"
            );
            // An odd width has it in the middle, an even one half a cell off.
            let off = if width % 2 == 1 { 0.0 } else { 0.5 };
            assert!((entrance.x.abs() - off).abs() < 1e-4, "{entrance}");
            // The frame's corners are the rectangle's.
            let (lo, hi) = frame.bounds();
            let size = hi - lo;
            let expected = if facing % 2 == 0 {
                Vec2::new(width as f32, depth as f32)
            } else {
                Vec2::new(depth as f32, width as f32)
            };
            assert!((size - expected).length() < 1e-4, "facing {facing}");
            assert!(frame.contains(frame.centre, 0.0));
            assert!(!frame.contains(frame.entrance, 0.0));
            assert!(frame.contains(frame.entrance, 0.6));
            // Local and world agree both ways.
            let point = Vec3::new(0.7, 1.5, -1.1);
            let world = frame.to_world(point, 3.0);
            assert!((world.y - 4.5).abs() < 1e-5);
            let back = frame.local_of(Vec2::new(world.x, world.z));
            assert!((back - Vec2::new(0.7, -1.1)).length() < 1e-4);
            // The turn points local +Z from the entrance into the body.
            let along = frame.rotation() * Vec3::Z;
            assert!((Vec2::new(along.x, along.z) - frame.dir).length() < 1e-5);
            let across = frame.rotation() * Vec3::X;
            assert!((Vec2::new(across.x, across.z) - frame.across()).length() < 1e-5);
        }
    }
}

#[test]
fn a_fitted_model_covers_the_body_with_its_door_on_the_entrance_side() {
    for (width, depth, model, walls, threshold) in [
        (4, 5, HouseModel::Cottage, (0.28, -0.315, 0.315), 0.0),
        (5, 6, HouseModel::Cottage, (0.28, -0.315, 0.315), 0.0),
        (5, 5, HouseModel::Hall, (0.35, -0.35, 0.245), 0.21),
        (4, 5, HouseModel::Hall, (0.35, -0.35, 0.245), 0.21),
    ] {
        let frame = body(width, depth, 2);
        let fit = model.fit(&frame);
        let (half_x, back, front) = walls;
        let (half_w, half_d) = (frame.width * 0.5, frame.depth * 0.5);
        // The four corners of the model's wall box land on the body's.
        for (x, z, want) in [
            (-half_x, front, Vec3::new(half_w, 0.0, -half_d)),
            (half_x, front, Vec3::new(-half_w, 0.0, -half_d)),
            (-half_x, back, Vec3::new(half_w, 0.0, half_d)),
            (half_x, back, Vec3::new(-half_w, 0.0, half_d)),
        ] {
            let got = fit.point(Vec3::new(x, threshold, z));
            assert!((got - want).length() < 1e-4, "{model:?}: {got} for {want}");
        }
        // The same through the transform the model entity gets.
        let placed = fit
            .transform()
            .transform_point(Vec3::new(half_x, threshold, front));
        assert!((placed - Vec3::new(-half_w, 0.0, -half_d)).length() < 1e-4);
        // The door stands on the ground in the entrance wall.
        let door = fit.point(Vec3::new(
            if model == HouseModel::Hall { 0.14 } else { 0.0 },
            threshold,
            front,
        ));
        assert!((door - Vec3::new(fit.door_x(), 0.0, -half_d)).length() < 1e-4);
        assert!(fit.door_x().abs() + fit.door_width() * 0.5 < half_w);
        // The chimney is on the back half and above the walls.
        let chimney = fit.chimney();
        assert!(
            chimney.z > 0.0 && chimney.y > WALL_HEIGHT_M * 2.0,
            "{chimney}"
        );
        // No axis is stretched by more than half against another.
        let scales = [fit.scale.x, fit.scale.y, fit.scale.z];
        let (least, most) = (
            scales.iter().copied().fold(f32::MAX, f32::min),
            scales.iter().copied().fold(f32::MIN, f32::max),
        );
        if (width, depth, model) != (4, 5, HouseModel::Hall) {
            assert!(most / least < 1.45, "{model:?} {width}x{depth}: {scales:?}");
        }
    }
    // The cottage's door is in the middle; the hall's is a metre to a side.
    assert_eq!(HouseModel::Cottage.fit(&body(4, 5, 0)).door_x(), 0.0);
    let hall = HouseModel::Hall.fit(&body(5, 5, 0));
    assert!((hall.door_x() + 1.0).abs() < 1e-3, "{}", hall.door_x());
    assert_eq!(house_model(false), HouseModel::Cottage);
    assert_eq!(house_model(true), HouseModel::Hall);
}

fn bed_rects(layout: &Interior) -> Vec<FloorRect> {
    layout.beds.iter().map(|bed| bed.rect).collect()
}

#[test]
fn beds_lie_inside_the_body_for_every_capacity_and_facing() {
    for width in 4..=6 {
        for depth in 4..=7 {
            for door_x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                for beds in 0..=12 {
                    let layout = interior(width as f32, depth as f32, door_x, beds);
                    let what = format!("{width}x{depth}, door {door_x}, {beds} beds");
                    assert_eq!(layout.beds.len(), beds, "{what}");
                    let rects = bed_rects(&layout);
                    for (index, bed) in layout.beds.iter().enumerate() {
                        assert!(bed.rect.inside(&layout.room), "{what}: bed {index} leaves");
                        assert!(bed.scale > 0.0 && bed.scale <= 1.0, "{what}");
                        for other in &rects[index + 1..] {
                            assert!(!bed.rect.overlaps(other), "{what}: beds overlap");
                        }
                        for (name, kept) in [
                            ("hearth", layout.hearth),
                            ("table", layout.table),
                            ("door", layout.door),
                        ] {
                            assert!(
                                !bed.rect.overlaps(&kept),
                                "{what}: bed {index} on the {name}"
                            );
                        }
                    }
                    assert!(layout.hearth.inside(&FloorRect {
                        centre: Vec2::ZERO,
                        size: Vec2::new(width as f32, depth as f32),
                    }));
                    assert!(layout.table.inside(&layout.room), "{what}");
                    assert!(!layout.table.overlaps(&layout.hearth), "{what}");
                    assert!(!layout.table.overlaps(&layout.door), "{what}");
                    // Everybody who is up stands on free floor.
                    for spot in &layout.spots {
                        let feet = FloorRect {
                            centre: spot.at,
                            size: Vec2::splat(0.3),
                        };
                        assert!(feet.inside(&layout.room), "{what}: {spot:?}");
                        assert!(!feet.overlaps(&layout.hearth), "{what}: {spot:?}");
                        assert!(
                            rects.iter().all(|bed| !feet.overlaps(bed)),
                            "{what}: {spot:?} in a bed"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_shipped_homes_have_full_beds_against_the_side_walls() {
    // A founding home and a cottage (two beds), a family house (four) and
    // every bed is full size, head to the back.
    for (width, depth, door_x, beds) in [
        (4, 5, 0.0, 2),
        (4, 5, -0.5, 2),
        (4, 5, 0.5, 2),
        (5, 6, 0.0, 4),
    ] {
        let layout = interior(width as f32, depth as f32, door_x, beds);
        assert_eq!(layout.beds.len(), beds);
        for bed in &layout.beds {
            assert_eq!(bed.scale, 1.0, "{width}x{depth} door {door_x}");
            assert!(
                (bed.rect.size - Vec2::new(BED_WIDTH_M, BED_LENGTH_M)).length() < 1e-5,
                "{:?}",
                bed.rect
            );
            // Against a side wall.
            let wall = layout.room.size.x * 0.5;
            assert!((bed.rect.centre.x.abs() + BED_WIDTH_M * 0.5 - wall).abs() < 1e-4);
        }
        // The first two stand at the back, one a side.
        let back = layout.room.size.y * 0.5;
        for bed in layout.beds.iter().take(2) {
            assert!((bed.rect.hi().y - back).abs() < 1e-4);
        }
        assert!(layout.beds[0].rect.centre.x * layout.beds[1].rect.centre.x < 0.0);
        // The hearth is on the back wall, between them.
        assert!((layout.hearth.hi().y - (depth as f32 * 0.5 - WALL_THICKNESS_M)).abs() < 1e-4);
        assert_eq!(layout.hearth.centre.x, 0.0);
    }
    // The homestead chest fits in a front corner of the standard and the
    // larger cottage, clear of the beds, the door's floor and the table.
    for (width, depth, beds) in [(4, 5, 2), (5, 6, 4)] {
        for door_x in [-0.5, 0.0, 0.5] {
            let layout = interior(width as f32, depth as f32, door_x, beds);
            let chest = chest_spot(&layout, door_x);
            assert!(chest.inside(&layout.room), "{width}x{depth}");
            assert!(!chest.overlaps(&layout.door) && !chest.overlaps(&layout.table));
            for bed in bed_rects(&layout) {
                assert!(!chest.overlaps(&bed), "{width}x{depth} door {door_x}");
            }
        }
    }
    // In the world, for every way a body can face, the beds are on its cells.
    for facing in FACINGS {
        for (width, depth, beds) in [(4, 5, 2), (5, 6, 4)] {
            let frame = body(width, depth, facing);
            let fit = HouseModel::Cottage.fit(&frame);
            let layout = interior(frame.width, frame.depth, fit.door_x(), beds);
            let (lo, hi) = frame.bounds();
            for bed in &layout.beds {
                for corner in [bed.rect.lo(), bed.rect.hi()] {
                    let at = frame.to_world(Vec3::new(corner.x, 0.0, corner.y), 0.0);
                    assert!(
                        at.x > lo.x && at.x < hi.x && at.z > lo.y && at.z < hi.y,
                        "facing {facing}: {at} outside {lo}..{hi}"
                    );
                }
            }
        }
    }
    // A bed is a bed's size for whoever lies in it.
    const { assert!(BED_LENGTH_M > RESIDENT_HEIGHT_M && BED_LENGTH_M < RESIDENT_HEIGHT_M + 0.4) };
    assert!((BED_MODEL_SCALE * 3.0 - BED_LENGTH_M).abs() < 1e-4);
    assert!((BED_MODEL_SCALE * 1.6 - BED_WIDTH_M).abs() < 0.01);
}

#[test]
fn the_inn_has_a_bed_for_every_guest_the_simulation_rents_to() {
    // Four guest beds on five by five metres with the hall's door a metre
    // to one side: the two on the far side are full size, the two by the
    // door a little shorter, all inside the walls.
    let beds = view::INN_BEDS as usize;
    let fit = HouseModel::Hall.fit(&BodyFrame::of_founding_workplace(SimPos::new(0, 0)));
    let layout = interior(5.0, 5.0, fit.door_x(), beds);
    assert_eq!(layout.beds.len(), beds);
    assert_eq!(layout.beds[0].scale, 1.0);
    for bed in &layout.beds {
        assert!(bed.scale > 0.8 && bed.rect.inside(&layout.room), "{bed:?}");
        assert!(!bed.rect.overlaps(&layout.door));
    }
}

#[test]
fn an_open_door_is_drawn_over_the_models_own_and_swings_outwards() {
    for (width, depth, model) in [
        (4, 5, HouseModel::Cottage),
        (5, 6, HouseModel::Cottage),
        (5, 5, HouseModel::Hall),
    ] {
        let frame = body(width, depth, 0);
        let fit = model.fit(&frame);
        let parts = door_parts(&fit, frame.depth);
        let wall = -frame.depth * 0.5;
        // The dark of the doorway: in front of the wall, within the door's
        // frame, from the ground to under its crown.
        let dark: Vec<&DoorPart> = parts
            .iter()
            .filter(|part| part.tone == Tone::Dark)
            .collect();
        assert_eq!(dark.len(), 2);
        for part in &dark {
            assert!(
                part.centre.z < wall && part.centre.z > wall - 0.5,
                "{part:?}"
            );
            assert!((part.centre.x - fit.door_x()).abs() < 1e-5);
            assert!(part.size.x <= fit.door_opening());
            assert!(part.centre.y + part.size.y * 0.5 <= fit.door_height() + 1e-4);
            assert_eq!(part.turn, 0.0);
        }
        assert!(
            dark[0].centre.y - dark[0].size.y * 0.5 < 1e-4,
            "from the ground"
        );
        assert!(dark[1].size.x < dark[0].size.x, "narrower under the arch");
        // The leaf: as wide as the opening, hinged on the jamb, standing
        // out from the wall on the entrance side.
        let leaf = parts
            .iter()
            .find(|part| part.tone == Tone::Timber)
            .expect("a leaf");
        assert!((leaf.size.x - fit.door_opening()).abs() < 1e-5);
        assert!(leaf.centre.z < dark[0].centre.z - 0.3, "{leaf:?}");
        assert!(leaf.centre.x < fit.door_x(), "on the hinge side");
        assert!(leaf.size.y < fit.door_height());
    }
    const { assert!(DOOR_LINGER_SECS > 0.3 && DOOR_LINGER_SECS < 1.5) };
}

#[test]
fn walls_stand_on_the_body_and_leave_the_door_open() {
    for (width, depth, door_x, opening) in [(4.0, 5.0, 0.0, 1.03), (5.0, 5.0, -1.0, 1.03)] {
        for lowered in [false, true] {
            let mut highest = 0.0_f32;
            for wall in Wall::ALL {
                let parts = wall_parts(wall, width, depth, door_x, opening, lowered);
                assert!(!parts.is_empty());
                for part in &parts {
                    let (lo, hi) = (part.centre - part.size * 0.5, part.centre + part.size * 0.5);
                    // On the body, a timber's thickness proud at most.
                    assert!(
                        lo.x >= -width * 0.5 - 0.03
                            && hi.x <= width * 0.5 + 0.03
                            && lo.z >= -depth * 0.5 - 0.03
                            && hi.z <= depth * 0.5 + 0.03,
                        "{wall:?}: {part:?}"
                    );
                    assert!(lo.y >= -1e-5, "{part:?}");
                    highest = highest.max(hi.y);
                    // Nothing stands in the doorway below the lintel.
                    if wall == Wall::Front && lo.y < DOOR_HEIGHT_M - 1e-3 {
                        assert!(
                            hi.x <= door_x - opening * 0.5 + 1e-4
                                || lo.x >= door_x + opening * 0.5 - 1e-4,
                            "{part:?} blocks the door"
                        );
                    }
                    // A wall is on its own side of the room.
                    let normal = wall.normal();
                    let out = part.centre.x * normal.x + part.centre.z * normal.y;
                    assert!(out > 0.0, "{wall:?}: {part:?}");
                }
                if !lowered {
                    assert!(parts.iter().any(|part| part.tone == Tone::Plaster));
                    assert!(parts.iter().any(|part| part.tone == Tone::Timber));
                }
                assert!(parts.iter().any(|part| part.tone == Tone::Stone));
            }
            if lowered {
                assert!(highest < 0.6, "a lowered wall hides nobody: {highest}");
            } else {
                assert!((highest - WALL_HEIGHT_M).abs() < 1e-4);
            }
        }
    }
}

#[test]
fn the_walls_between_the_camera_and_the_room_come_down() {
    // A founding home: its body lies towards -z, its door looks to +z.
    let frame = body(4, 5, 2);
    let down = |yaw: f32| Wall::ALL.map(|wall| wall_lowered(wall, &frame, yaw, false));
    // The player's default view, from +z: the entrance wall is in the way.
    assert_eq!(down(0.0), [true, false, false, false]);
    // From behind (the capture scripts' yaw pi): the back wall.
    assert_eq!(down(PI), [false, true, false, false]);
    // From +x: the wall on that side. The frame's +X is world -x here.
    assert_eq!(down(FRAC_PI_2), [false, false, true, false]);
    // From a corner, the two walls that meet there.
    assert_eq!(down(PI / 4.0), [true, false, true, false]);
    for facing in FACINGS {
        let frame = body(5, 6, facing);
        for step in 0..32 {
            let yaw = step as f32 * PI / 16.0;
            let down = Wall::ALL.map(|wall| wall_lowered(wall, &frame, yaw, false));
            let count = down.iter().filter(|down| **down).count();
            assert!((1..=2).contains(&count), "facing {facing}, yaw {yaw}");
            // Never a wall and the one opposite it.
            assert!(!(down[0] && down[1]) && !(down[2] && down[3]));
            // A wall that is down stays down a little past the point where
            // it would come down.
            for wall in Wall::ALL {
                if down[wall.index()] {
                    assert!(wall_lowered(wall, &frame, yaw, true));
                }
            }
        }
    }
    // Edge on, a standing wall stays up and a lowered one stays down.
    let edge = (0.06_f32).asin();
    assert!(!wall_lowered(Wall::Right, &frame, PI + edge, false));
    assert!(wall_lowered(Wall::Right, &frame, PI + edge, true));
}

#[test]
fn a_cutaway_follows_the_selection_the_card_and_a_close_camera() {
    let (a, b, c, d, e) = (SimId(1), SimId(2), SimId(3), SimId(4), SimId(5));
    let all = |_| true;
    assert!(wanted(Signals::default(), all).is_empty());
    let one = Signals {
        card: Some(a),
        ..Signals::default()
    };
    assert_eq!(wanted(one, all), vec![a]);
    // A home the fog does not show live keeps its roof.
    assert!(wanted(one, |_| false).is_empty());
    // The played character's home first, then the selection, the card and
    // the camera; a home asked for twice is cut away once.
    let many = Signals {
        card: Some(c),
        selected: Some(b),
        played: Some(a),
        near: Some(b),
        ..Signals::default()
    };
    assert_eq!(wanted(many, all), vec![a, b, c]);
    let four = Signals {
        card: Some(c),
        selected: Some(b),
        played: Some(a),
        near: Some(d),
        ..Signals::default()
    };
    assert_eq!(wanted(four, all), vec![a, b, c], "at most three at once");
    assert_eq!(wanted(four, |id| id != b), vec![a, c, d]);
    // A home in the camera's way is cut away after the played character's,
    // before the close camera's; the same home asked for twice is one
    // cutaway.
    let blocking = Signals {
        played: Some(a),
        blocking: [Some(e), Some(a)],
        near: Some(d),
        ..Signals::default()
    };
    assert_eq!(wanted(blocking, all), vec![a, e, d]);
    const { assert!(MAX_CUTAWAYS == 3) };

    // The camera: close enough, and looking at the body or just beside it.
    let near = body(4, 5, 2);
    let far = BodyFrame {
        centre: near.centre + Vec2::new(8.0, 0.0),
        entrance: near.entrance + Vec2::new(8.0, 0.0),
        ..near
    };
    let bodies = [(d, near), (e, far)];
    assert_eq!(near_home(14.0, near.centre, None, bodies), Some(d));
    assert_eq!(near_home(14.0, far.centre, None, bodies), Some(e));
    assert_eq!(
        near_home(14.0, near.centre + Vec2::new(4.0, 0.0), None, bodies),
        None,
        "the street between two homes opens neither"
    );
    assert_eq!(near_home(30.0, near.centre, None, bodies), None);
    assert_eq!(near_home(30.0, near.centre, Some(d), bodies), None);
    // Zooming out a little keeps an open home open; opening needs the
    // closer distance.
    assert_eq!(near_home(17.0, near.centre, None, bodies), None);
    assert_eq!(near_home(17.0, near.centre, Some(d), bodies), Some(d));
    // The entrance is beside the body: looking at the door opens the home.
    assert_eq!(near_home(12.0, near.entrance, None, bodies), Some(d));
    const {
        assert!(CLOSE_DISTANCE_M < CLOSE_RELEASE_M);
        assert!(CLOSE_MARGIN_M < 2.0);
    };
}

#[test]
fn a_resident_indoors_is_hidden_after_one_step_into_the_doorway() {
    // Seen first where the simulation has them, without a step.
    assert_eq!(Phase::first(true), Phase::Inside);
    assert_eq!(Phase::first(false), Phase::Outside);
    let frame = 1.0 / 60.0;
    // Going in: a short step, then indoors.
    let mut phase = Phase::Outside.advance(true, frame);
    assert_eq!(phase, Phase::Entering(0.0));
    let mut steps = 0;
    let mut last = -1.0;
    while let Phase::Entering(_) = phase {
        let Drawn::Step(along) = drawn(phase, true, false, false) else {
            panic!("a resident on the way in is drawn stepping");
        };
        assert!(along >= last && (0.0..=1.0).contains(&along));
        last = along;
        phase = phase.advance(true, frame);
        steps += 1;
    }
    assert_eq!(phase, Phase::Inside);
    assert!((steps as f32 * frame - ENTER_SECS).abs() < 2.0 * frame);
    assert!(last > 0.9, "the step reaches the doorway: {last}");
    // Indoors: hidden, selected or not, unless the home is cut away.
    assert_eq!(drawn(phase, true, false, false), Drawn::Hidden);
    assert_eq!(drawn(phase, true, false, true), Drawn::Hidden);
    assert_eq!(drawn(phase, true, true, false), Drawn::Indoors);
    assert_eq!(phase.advance(true, 10.0), Phase::Inside);
    // Coming out: the reverse, from the doorway to where they walk.
    phase = phase.advance(false, frame);
    assert_eq!(phase, Phase::Leaving(0.0));
    assert_eq!(drawn(phase, true, false, false), Drawn::Step(1.0));
    let mut last = 2.0;
    while let Phase::Leaving(_) = phase {
        let Drawn::Step(along) = drawn(phase, true, true, false) else {
            panic!("a resident on the way out is drawn stepping");
        };
        assert!(along <= last);
        last = along;
        phase = phase.advance(false, frame);
    }
    assert_eq!(phase, Phase::Outside);
    assert!(last < 0.1);
    assert_eq!(drawn(phase, true, true, true), Drawn::Outdoors);
    // Turning round in the doorway does not jump.
    let half = Phase::Entering(ENTER_SECS * 0.5);
    let Drawn::Step(before) = drawn(half, true, false, false) else {
        panic!()
    };
    let Drawn::Step(after) = drawn(half.advance(false, 0.0), true, false, false) else {
        panic!()
    };
    assert!((before - after).abs() < 1e-4);
    let Phase::Entering(resumed) = Phase::Leaving(EXIT_SECS * 0.25).advance(true, 0.0) else {
        panic!()
    };
    assert!((resumed - ENTER_SECS * 0.75).abs() < 1e-4);
    // A building without an inside to show: its occupant is hidden, except
    // the one the player follows, who stays at the door.
    assert_eq!(drawn(Phase::Inside, false, false, false), Drawn::Hidden);
    assert_eq!(drawn(Phase::Inside, false, false, true), Drawn::Outdoors);
    const { assert!(ENTER_SECS <= 0.6 && EXIT_SECS <= 0.6) };
}

#[test]
fn the_hearth_lights_a_cut_away_home_at_night() {
    // By day an unheated hearth is dark; a fire shows in daylight too.
    assert_eq!(hearth_lumens(false, 0.0), 0.0);
    assert!(hearth_lumens(true, 0.0) > 0.0);
    // At night embers and a candle still show who sleeps there.
    assert!(hearth_lumens(false, 1.0) >= 3_000.0);
    assert!(hearth_lumens(true, 1.0) > 3.0 * hearth_lumens(false, 1.0));
    assert!(hearth_lumens(true, 1.0) <= 20_000.0);
    assert_eq!(hearth_lumens(true, 7.0), hearth_lumens(true, 1.0));
}

// The modules against the shipped world.

fn living_world() -> App {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    app.insert_resource(IslandMode::Inspector)
        .init_resource::<Selected>()
        .init_resource::<VisualIndex>()
        .init_resource::<Shells>()
        .init_resource::<Cutaways>()
        .init_resource::<IndoorViews>()
        .init_resource::<Stepping>()
        .init_resource::<OpenDoors>()
        // A door shuts at once after a step, so a test need not wait for it.
        .insert_resource(DoorHold(0.05))
        .init_resource::<Time>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>();
    app
}

/// Visual roots as the scene spawns them: at the door, or at the site.
fn spawn_roots(world: &mut World) {
    let mut statics = world
        .query_filtered::<(Entity, &SimId, &SimPos), Or<(With<Household>, With<VillagePlace>)>>();
    let found: Vec<(Entity, SimId, SimPos)> = statics
        .iter(world)
        .map(|(entity, id, at)| (entity, *id, *at))
        .collect();
    for (entity, id, at) in found {
        let root = world
            .spawn((
                VisualOf(entity),
                StaticId(id),
                Transform::from_xyz(at.x as f32 / 1000.0, 2.0, at.z as f32 / 1000.0),
                Visibility::default(),
                Fogged::default(),
            ))
            .id();
        world.resource_mut::<VisualIndex>().map.insert(entity, root);
    }
    let mut people = world.query_filtered::<(Entity, &SimPos), With<Villager>>();
    let found: Vec<(Entity, SimPos)> = people
        .iter(world)
        .map(|(entity, at)| (entity, *at))
        .collect();
    for (entity, at) in found {
        let root = world
            .spawn((
                VisualOf(entity),
                ActorVisual {
                    kind: crate::assets::ModelKind::VillageWoman,
                    motion: Motion::default(),
                    hand_r: None,
                    hand_l: None,
                },
                Transform::from_xyz(at.x as f32 / 1000.0, 2.0, at.z as f32 / 1000.0),
                Visibility::default(),
                Fogged::default(),
            ))
            .id();
        world.resource_mut::<VisualIndex>().map.insert(entity, root);
    }
}

/// One frame of the view: the visuals are put back where the simulation has
/// them, as the actor pass does, then the homes pass runs.
fn frame(app: &mut App, seconds: f32) {
    let world = app.world_mut();
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(seconds));
    let mut actors = world.query::<(&VisualOf, &mut Transform, &ActorVisual)>();
    let placed: Vec<(Entity, Entity)> = actors.iter(world).map(|(of, ..)| (of.0, of.0)).collect();
    for (sim, _) in placed {
        let at = *world.get::<SimPos>(sim).expect("position");
        let root = world.resource::<VisualIndex>().map[&sim];
        *world.get_mut::<Transform>(root).expect("root") =
            Transform::from_xyz(at.x as f32 / 1000.0, 2.0, at.z as f32 / 1000.0);
    }
    world.run_system_once(dress).expect("dress");
    world.run_system_once(cutaways).expect("cutaways");
    world.run_system_once(residents).expect("residents");
    world.run_system_once(doors).expect("doors");
}

/// Every entity there is: the count an inside adds to and gives back.
fn entities(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<Entity>().iter(world).count()
}

/// Set the clock to the first tick of the night and return it: the view only
/// reads the clock, and a rest in bed is sleep at night.
fn night(app: &mut App) -> u64 {
    let world = app.world_mut();
    let tick = {
        let config = world.resource::<SimConfig>();
        (0..config.day_length)
            .find(|tick| config.is_night(*tick))
            .expect("a night")
    };
    world.resource_mut::<SimClock>().tick = tick;
    tick
}

fn root_of(world: &World, sim: Entity) -> Entity {
    world.resource::<VisualIndex>().map[&sim]
}

#[test]
fn every_home_and_inn_of_the_shipped_world_gets_its_fitted_house() {
    let mut app = living_world();
    spawn_roots(app.world_mut());
    let before = state_hash(app.world_mut());
    frame(&mut app, 0.1);
    let world = app.world_mut();
    let homes = view::homes(world);
    assert_eq!(homes.len(), 42);
    let shells: BTreeMap<SimId, HomeShell> = world
        .query::<&HomeShell>()
        .iter(world)
        .map(|shell| (shell.building, shell.clone()))
        .collect();
    assert_eq!(shells.len(), 45, "42 homes and the three inns");
    for home in &homes {
        let shell = &shells[&home.house];
        assert!(!shell.inn);
        let frame = shell.frame;
        // Exactly the simulation's cells, which the door alone gives too.
        assert_eq!(frame, BodyFrame::of_founding_home(home.entrance));
        let (lo, hi) = frame.bounds();
        assert_eq!(
            (lo, hi),
            (
                Vec2::new(home.body_lo.0 as f32, home.body_lo.1 as f32),
                Vec2::new(home.body_hi.0 as f32 + 1.0, home.body_hi.1 as f32 + 1.0)
            ),
            "{}",
            home.label
        );
        assert_eq!((frame.width, frame.depth), (4.0, 5.0), "{}", home.label);
        // The entrance is a metre in front of the middle of the door wall,
        // which is where the cottage's door is.
        let entrance = frame.local_of(frame.entrance);
        assert!(
            (entrance - Vec2::new(0.0, -frame.depth * 0.5 - 1.0)).length() < 1e-4,
            "{}: {entrance}",
            home.label
        );
        assert_eq!(shell.fit.model, HouseModel::Cottage);
        assert_eq!(shell.fit.door_x(), 0.0);
        // The frame hangs under the root so that it stands on the body.
        let root = world.resource::<Shells>().0[&home.house];
        let at = world
            .get::<Transform>(root)
            .expect("root")
            .mul_transform(shell.local);
        assert!((at.translation - Vec3::new(frame.centre.x, 2.0, frame.centre.y)).length() < 1e-4);
        assert!((at.rotation * Vec3::Z - Vec3::new(frame.dir.x, 0.0, frame.dir.y)).length() < 1e-4);
    }
    // The inns: the five by five metres the simulation blocks behind the
    // site, the entrance at the site itself.
    let inns: Vec<&HomeShell> = shells.values().filter(|shell| shell.inn).collect();
    assert_eq!(inns.len(), 3);
    for shell in inns {
        assert!(testbed_content::village::indoors::is_inn(
            world,
            shell.building
        ));
        assert_eq!((shell.frame.width, shell.frame.depth), (5.0, 5.0));
        let entrance = shell.frame.local_of(shell.frame.entrance);
        assert!(
            (entrance - Vec2::new(0.0, -3.5)).length() < 1e-4,
            "{entrance}"
        );
        assert_eq!(shell.fit.model, HouseModel::Hall);
    }
    // Workplaces other than the inn keep their own models.
    let workplaces = world
        .query::<&VillagePlace>()
        .iter(world)
        .filter(|place| place.role != Role::Innkeeper)
        .count();
    assert!(workplaces > 30);
    // No scenery stands on a house or against its walls.
    let spec = world.resource::<LivingWorldSpec>().clone();
    for shell in shells.values() {
        let (lo, hi) = shell.frame.bounds();
        for (x, z) in [(lo.x, lo.y), (hi.x, hi.y), (lo.x, hi.y), (hi.x, lo.y)] {
            let corner = SimPos::new((x * 1000.0) as i32, (z * 1000.0) as i32);
            assert!(on_founding_body(&spec, corner, 0.01));
        }
        let beside = shell.frame.centre + shell.frame.across() * (shell.frame.width * 0.5 + 1.5);
        let beside = SimPos::new((beside.x * 1000.0) as i32, (beside.y * 1000.0) as i32);
        assert!(on_founding_body(&spec, beside, 2.2));
        assert!(!on_founding_body(&spec, beside, 1.0));
    }
    assert!(!on_founding_body(&spec, spec.settlements[0].center, 2.2));
    // A second pass adds nothing.
    let before_pass = entities(&mut app);
    frame(&mut app, 0.1);
    assert_eq!(entities(&mut app), before_pass);
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "the view wrote the sim"
    );
}

#[test]
fn a_selected_occupant_cuts_the_home_away_and_is_drawn_inside_it() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    // The test writes the state it wants to see drawn; the view only reads
    // it. First a home whose resident is out, and somebody from another home
    // who will be inside it with them.
    let homes = view::homes(app.world());
    let home = homes
        .iter()
        .find(|home| home.beds[0].resident.is_some())
        .expect("an occupied home")
        .clone();
    let house = home.house;
    let visitor = homes
        .iter()
        .filter(|other| other.house != house)
        .find_map(|other| other.beds[0].resident)
        .expect("a neighbour");
    let people: Vec<(SimId, Entity)> = [home.beds[0].resident.expect("resident"), visitor]
        .into_iter()
        .map(|id| {
            (
                id,
                app.world()
                    .resource::<SimIndex>()
                    .get(id)
                    .expect("resident"),
            )
        })
        .collect();
    // Night, so that a rest in bed is sleep.
    let now = night(&mut app);
    for (_, entity) in &people {
        let world = app.world_mut();
        *world.get_mut::<SimPos>(*entity).expect("position") = home.entrance;
        world.get_mut::<Indoors>(*entity).expect("indoors").building = None;
        world
            .get_mut::<Villager>(*entity)
            .expect("villager")
            .resting_until = 0;
    }
    spawn_roots(app.world_mut());
    frame(&mut app, 0.1);
    // Then both go inside, as the simulation has it when they stand at
    // their door: the first sleeps in its bed, the second is up.
    for (_, entity) in &people {
        app.world_mut()
            .get_mut::<Indoors>(*entity)
            .expect("indoors")
            .building = Some(house);
    }
    {
        let world = app.world_mut();
        let mut person = world.get_mut::<Villager>(people[0].1).expect("villager");
        person.resting_until = now + 5_000;
        person.rest_rate = testbed_content::village::HOME_REST_RATE;
        world
            .get_mut::<Indoors>(people[0].1)
            .expect("indoors")
            .asleep_until = now + 5_000;
    }
    assert_eq!(view::repose(app.world(), people[0].1), Repose::Asleep);
    assert_eq!(view::repose(app.world(), people[1].1), Repose::Awake);
    assert_eq!(view::occupants(app.world(), house).len(), 2);
    let before = state_hash(app.world_mut());
    let idle = entities(&mut app);
    let shell = {
        let world = app.world_mut();
        let root = world.resource::<Shells>().0[&house];
        world.get::<HomeShell>(root).expect("shell").clone()
    };

    // They step into the doorway, then nobody is drawn.
    frame(&mut app, 0.1);
    for (_, entity) in &people {
        let root = root_of(app.world(), *entity);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
    }
    let stepping = *app
        .world()
        .get::<Transform>(root_of(app.world(), people[0].1))
        .expect("root");
    // The home's door stands open while they step through it: three boxes
    // under its frame.
    let door = app
        .world()
        .resource::<OpenDoors>()
        .0
        .get(&house)
        .map(|door| door.0);
    let door = door.expect("an open door");
    assert_eq!(
        app.world().get::<Children>(door).map(|parts| parts.len()),
        Some(3)
    );
    assert_eq!(
        app.world()
            .get::<ChildOf>(door)
            .map(|parent| parent.parent()),
        Some(shell.frame_entity)
    );
    frame(&mut app, 0.2);
    let further = *app
        .world()
        .get::<Transform>(root_of(app.world(), people[0].1))
        .expect("root");
    let along = |at: Vec3| shell.frame.local_of(Vec2::new(at.x, at.z)).y;
    assert!(
        along(further.translation) > along(stepping.translation) + 0.2
            && along(stepping.translation)
                >= along(Vec3::new(
                    home.entrance.x as f32 / 1000.0,
                    0.0,
                    home.entrance.z as f32 / 1000.0
                )),
        "towards the door"
    );
    for _ in 0..6 {
        frame(&mut app, 0.1);
    }
    for (_, entity) in &people {
        let root = root_of(app.world(), *entity);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        // Out of a click's reach too.
        assert!(
            app.world()
                .get::<Transform>(root)
                .expect("root")
                .translation
                .y
                < -20.0
        );
    }
    assert!(!app.world().resource::<Cutaways>().is_open(house));
    assert_eq!(
        entities(&mut app),
        idle,
        "nothing is built for a home nobody looks into"
    );
    // The fog pass shows a resident it can see again on every tick; they
    // are hidden once more before markers and captions look.
    let root = root_of(app.world(), people[0].1);
    *app.world_mut().get_mut::<Visibility>(root).expect("root") = Visibility::Inherited;
    app.world_mut()
        .run_system_once(keep_hidden)
        .expect("keep hidden");
    assert_eq!(
        app.world().get::<Visibility>(root),
        Some(&Visibility::Hidden)
    );

    // Selecting one of them cuts the home away.
    app.world_mut().resource_mut::<Selected>().0 = Some(people[1].0);
    frame(&mut app, 0.1);
    assert!(app.world().resource::<Cutaways>().is_open(house));
    assert!(entities(&mut app) > idle + 40);
    let (inside_bed, inside_up) = {
        let world = app.world();
        (
            *world
                .get::<Transform>(root_of(world, people[0].1))
                .expect("root"),
            *world
                .get::<Transform>(root_of(world, people[1].1))
                .expect("root"),
        )
    };
    for (index, at) in [inside_bed, inside_up].into_iter().enumerate() {
        let root = root_of(app.world(), people[index].1);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
        assert!(
            shell.frame.contains(
                Vec2::new(at.translation.x, at.translation.z),
                -WALL_THICKNESS_M
            ),
            "inside the walls: {}",
            at.translation
        );
    }
    // The sleeper lies on its own bed, the first; the other stands by the
    // hearth, upright.
    let layout = interior(shell.frame.width, shell.frame.depth, 0.0, 2);
    let bed = layout.beds[0].rect;
    let lying = shell.frame.local_of(Vec2::new(
        inside_bed.translation.x,
        inside_bed.translation.z,
    ));
    assert!(
        lying.x > bed.lo().x && lying.x < bed.hi().x && lying.y > bed.lo().y - 0.01,
        "{lying} on {bed:?}"
    );
    assert!(inside_bed.translation.y > 2.0 + MATTRESS_HEIGHT_M);
    assert!((inside_bed.rotation * Vec3::Y).y.abs() < 1e-3, "laid down");
    assert!((inside_up.rotation * Vec3::Y).y > 0.99, "upright");
    let stands = shell
        .frame
        .local_of(Vec2::new(inside_up.translation.x, inside_up.translation.z));
    assert!((stands - layout.spots[0].at).length() < 1e-3, "{stands}");
    // The house model's place is taken by walls: standing ones away from
    // the camera only once a camera exists, so all four stand here.
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "the view wrote the sim"
    );

    // Deselecting closes it: the inside is gone and they are hidden again.
    app.world_mut().resource_mut::<Selected>().0 = None;
    frame(&mut app, 0.1);
    assert!(!app.world().resource::<Cutaways>().is_open(house));
    assert_eq!(entities(&mut app), idle, "the inside is gone");
    for (_, entity) in &people {
        let root = root_of(app.world(), *entity);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
    }
    // The home itself can be the selection (a card does the same).
    app.world_mut().resource_mut::<Selected>().0 = Some(house);
    frame(&mut app, 0.1);
    assert!(app.world().resource::<Cutaways>().is_open(house));
    // A home the fog only remembers is not opened.
    let root = app.world().resource::<Shells>().0[&house];
    app.world_mut().get_mut::<Fogged>(root).expect("fog").0 = Look::Dim;
    frame(&mut app, 0.1);
    assert!(!app.world().resource::<Cutaways>().is_open(house));
    app.world_mut().get_mut::<Fogged>(root).expect("fog").0 = Look::Live;
    app.world_mut().resource_mut::<Selected>().0 = None;

    // Leaving: they step out of the doorway and are drawn again.
    for (_, entity) in &people {
        app.world_mut()
            .get_mut::<Indoors>(*entity)
            .expect("indoors")
            .building = None;
    }
    frame(&mut app, 0.1);
    let root = root_of(app.world(), people[0].1);
    assert_eq!(
        app.world().get::<Visibility>(root),
        Some(&Visibility::Inherited)
    );
    for _ in 0..6 {
        frame(&mut app, 0.1);
    }
    let out = app
        .world()
        .get::<Transform>(root)
        .expect("root")
        .translation;
    assert!(
        (out - Vec3::new(
            home.entrance.x as f32 / 1000.0,
            2.0,
            home.entrance.z as f32 / 1000.0
        ))
        .length()
            < 1e-4,
        "back where the simulation has them: {out}"
    );
}

#[test]
fn an_inn_shows_its_guests_in_rented_beds() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    spawn_roots(app.world_mut());
    frame(&mut app, 0.1);
    let (inn, shell) = {
        let world = app.world_mut();
        let shell = world
            .query::<&HomeShell>()
            .iter(world)
            .find(|shell| shell.inn)
            .expect("an inn")
            .clone();
        (shell.building, shell)
    };
    let beds = view::INN_BEDS as usize;
    assert_eq!(bed_kinds(app.world(), inn, true), vec![false; beds]);
    // The body is the one the simulation's own view of the inn gives.
    let seen = view::inn(app.world(), inn).expect("an inn");
    assert_eq!(
        shell.frame,
        BodyFrame::of_bounds(seen.body_min, seen.body_max, seen.facing, seen.entrance)
    );
    assert_eq!(seen.beds.len(), beds);
    // A guest asleep in the inn, at night.
    let now = night(&mut app);
    let guest = {
        let world = app.world_mut();
        let (entity, id) = world
            .query_filtered::<(Entity, &SimId), With<Villager>>()
            .iter(world)
            .map(|(entity, id)| (entity, *id))
            .min_by_key(|(_, id)| *id)
            .expect("somebody");
        let mut indoors = world.get_mut::<Indoors>(entity).expect("indoors");
        indoors.building = Some(inn);
        indoors.bed = 0;
        indoors.asleep_until = now + 5_000;
        let mut person = world.get_mut::<Villager>(entity).expect("villager");
        person.resting_until = now + 5_000;
        person.rest_rate = testbed_content::village::HOME_REST_RATE;
        (entity, id)
    };
    assert_eq!(view::repose(app.world(), guest.0), Repose::Asleep);
    for _ in 0..8 {
        frame(&mut app, 0.1);
    }
    let root = root_of(app.world(), guest.0);
    assert_eq!(
        app.world().get::<Visibility>(root),
        Some(&Visibility::Hidden)
    );
    // The inn's own selection opens it; the guest lies in the first bed.
    app.world_mut().resource_mut::<Selected>().0 = Some(inn);
    frame(&mut app, 0.1);
    assert!(app.world().resource::<Cutaways>().is_open(inn));
    let at = app
        .world()
        .get::<Transform>(root)
        .expect("root")
        .translation;
    let layout = interior(5.0, 5.0, shell.fit.door_x(), beds);
    let lying = shell.frame.local_of(Vec2::new(at.x, at.z));
    let bed = layout.beds[0].rect;
    assert!(
        lying.x > bed.lo().x && lying.x < bed.hi().x && lying.y > bed.lo().y - 0.01,
        "{lying} on {bed:?}"
    );
    assert_eq!(
        app.world().get::<Visibility>(root),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn a_child_has_a_small_bed_and_a_home_keeps_one_bed_per_place() {
    let mut app = living_world();
    spawn_roots(app.world_mut());
    frame(&mut app, 0.1);
    for home in view::homes(app.world()) {
        let kinds = bed_kinds(app.world(), home.house, false);
        assert_eq!(kinds.len(), home.capacity as usize, "{}", home.label);
        assert_eq!(
            kinds,
            home.beds
                .iter()
                .map(|bed| bed.child.is_some())
                .collect::<Vec<_>>()
        );
    }
    const { assert!(CHILD_BED_SCALE < 0.8) };
}

#[test]
fn a_caption_follows_whoever_is_drawn_indoors_and_a_house_has_a_box_to_click() {
    let app = living_world();
    let world = app.world();
    // Nobody is drawn indoors until a home is cut away.
    let mut open = Cutaways::default();
    let somebody = World::new().spawn_empty().id();
    assert_eq!(open.caption_anchor(somebody, 2.1), None);
    open.seats.insert(
        somebody,
        Seat::Bed {
            centre: Vec3::new(10.0, 0.6, -4.0),
            head_yaw: 0.0,
        },
    );
    assert_eq!(
        open.caption_anchor(somebody, 2.1),
        Some(Vec3::new(10.0, 0.6 + BED_CAPTION_LIFT_M, -4.0)),
        "over the bed, not over the entrance point"
    );
    open.seats.insert(
        somebody,
        Seat::Floor {
            at: Vec3::new(11.0, 0.0, -3.0),
            yaw: 0.0,
        },
    );
    assert_eq!(
        open.caption_anchor(somebody, 2.1),
        Some(Vec3::new(11.0, 2.1, -3.0))
    );
    // Every home's box is its whole body, roof included.
    let index = world.resource::<npc_framework::prelude::SimIndex>();
    let homes = view::homes(world);
    assert!(!homes.is_empty());
    for home in homes {
        let sim = index.get(home.house).unwrap();
        let (lo, hi, height) = body_box(world, sim, home.house).expect("a home has a body");
        assert_eq!(lo, Vec2::new(home.body_lo.0 as f32, home.body_lo.1 as f32));
        assert_eq!(
            hi,
            Vec2::new(home.body_hi.0 as f32 + 1.0, home.body_hi.1 as f32 + 1.0)
        );
        assert!(height > 4.5, "to the chimney: {height}");
    }
}

#[test]
fn a_body_too_small_for_a_house_gets_a_lower_door_and_a_bunk() {
    // The homestead cottage: 2 by 2 m.
    let frame = BodyFrame::of_bounds(
        SimPos::new(-1_000, -2_800),
        SimPos::new(1_000, -800),
        2,
        SimPos::new(0, 0),
    );
    assert!((Vec2::new(frame.width, frame.depth) - Vec2::splat(2.0)).length() < 1e-4);
    assert!((frame.centre - Vec2::new(0.0, -1.8)).length() < 1e-4);
    let fit = HouseModel::Cottage.fit(&frame);
    // The walls still cover the body: the door wall is on the entrance side.
    let corner = fit.point(Vec3::new(0.28, 0.0, 0.315));
    assert!(
        (corner - Vec3::new(-1.0, 0.0, -1.0)).length() < 1e-4,
        "{corner}"
    );
    // Not a tower: 1.4 times the level scale, which leaves a door a
    // resident stoops through.
    assert!((fit.scale.y / fit.scale.x - 1.4).abs() < 1e-4);
    assert!(
        (fit.door_height() - 1.575).abs() < 1e-3,
        "{}",
        fit.door_height()
    );
    let ratio = fit.door_height() / RESIDENT_HEIGHT_M;
    assert!(ratio > 0.85 && ratio < 0.9, "{ratio}");
    assert!((fit.door_opening() - 0.518).abs() < 1e-3);
    assert!(fit.chimney().y < 4.7, "{}", fit.chimney());
    // A house-sized body is fitted as before.
    assert_eq!(
        HouseModel::Cottage.fit(&body(4, 5, 0)).door_height(),
        DOOR_HEIGHT_M
    );

    // Inside: 1.52 m between the walls, and a bed is 1.98 m. The three
    // beds are the berths of one bunk across the back wall.
    let layout = interior(frame.width, frame.depth, fit.door_x(), 3);
    assert!((layout.room.size.x - 1.52).abs() < 1e-4);
    assert_eq!(layout.beds.len(), 3);
    for (tier, bed) in layout.beds.iter().enumerate() {
        assert!(bed.across);
        assert!(bed.rect.inside(&layout.room), "berth {tier}");
        assert!((bed.rect.size.x - layout.room.size.x).abs() < 1e-4);
        assert!((bed.scale - 1.52 / BED_LENGTH_M).abs() < 1e-4);
        assert_eq!(bed.lift, tier as f32 * BUNK_TIER_M);
        // The sleeper on the top berth is under the top of the walls.
        assert!(bed.lift + MATTRESS_HEIGHT_M * bed.scale + 0.3 < WALL_HEIGHT_M);
        assert!(!bed.rect.overlaps(&layout.door), "the way in is free");
    }
    // No hearth and no table are drawn: there is no floor for them.
    assert_eq!(layout.hearth.size, Vec2::ZERO);
    assert_eq!(layout.table.size, Vec2::ZERO);
    assert!(
        layout.room.lo().x < layout.hearth.centre.x && layout.hearth.centre.x < layout.room.hi().x
    );
    // Whoever is up stands between the bunk and the door, inside the room.
    assert_eq!(layout.spots.len(), 3);
    for spot in &layout.spots {
        let feet = FloorRect {
            centre: spot.at,
            size: Vec2::splat(0.3),
        };
        assert!(feet.inside(&layout.room), "{spot:?}");
        assert!(!feet.overlaps(&layout.beds[0].rect), "{spot:?}");
    }
    // A room a bed fits in keeps its beds on the floor.
    assert!(
        interior(4.0, 5.0, 0.0, 3)
            .beds
            .iter()
            .all(|bed| !bed.across && bed.lift == 0.0)
    );
}

#[test]
fn a_homestead_cottage_is_fitted_to_its_small_body_and_cut_away_with_a_bunk() {
    let mut app = living_world();
    spawn_roots(app.world_mut());
    let world = app.world_mut();
    let centre = SimPos::new(3_000, 3_200);
    let (id, sim) = npc_framework::prelude::spawn_sim(
        world,
        (
            centre,
            Household {
                label: "Cottage".into(),
                capacity: 3,
                residents: vec![],
                door: SimPos::new(centre.x, centre.z + 1_800),
                heated_until: 0,
                logs_burned: 0,
            },
            Cottage {
                plot: SimId(u32::MAX),
                plot_id: "plot".into(),
            },
        ),
    );
    // Its root as the homestead pass spawns it: at the body's centre, with
    // a small model of its own.
    let root = world
        .spawn((
            VisualOf(sim),
            StaticId(id),
            Transform::from_xyz(3.0, 2.0, 3.2),
            Visibility::default(),
            Fogged(Look::Live),
        ))
        .id();
    let own = world
        .spawn((ChildOf(root), WorldAssetRoot(Handle::default())))
        .id();
    world.resource_mut::<VisualIndex>().map.insert(sim, root);
    let before = state_hash(world);
    frame(&mut app, 0.1);
    let world = app.world_mut();
    assert!(
        world.get_entity(own).is_err(),
        "the fitted model takes its place"
    );
    let shell = world.get::<HomeShell>(root).expect("a body").clone();
    let size = Vec2::new(shell.frame.width, shell.frame.depth);
    assert!((size - Vec2::splat(2.0)).length() < 1e-4, "{size}");
    assert!((shell.frame.centre - Vec2::new(3.0, 3.2)).length() < 1e-4);
    // The door faces the plot, 0.8 m in front of the wall.
    let entrance = shell.frame.local_of(shell.frame.entrance);
    assert!(
        (entrance - Vec2::new(0.0, -1.8)).length() < 1e-4,
        "{entrance}"
    );
    let (lo, hi, height) = body_box(world, sim, id).expect("a box to click");
    assert!((lo - Vec2::new(2.0, 2.2)).length() < 1e-4, "{lo}");
    assert!((hi - Vec2::new(4.0, 4.2)).length() < 1e-4, "{hi}");
    assert!(height > 4.0 && height < 4.7, "{height}");

    // Selected, it is cut away: three berths, one above the other.
    let closed = entities(&mut app);
    app.world_mut().resource_mut::<Selected>().0 = Some(id);
    frame(&mut app, 0.1);
    let world = app.world_mut();
    let open = world.resource::<Cutaways>();
    assert!(open.is_open(id));
    let beds = &open.open[&id].layout.beds;
    assert_eq!(beds.len(), 3);
    assert!(beds.iter().all(|bed| bed.across));
    assert_eq!(beds[2].lift, 2.0 * BUNK_TIER_M);
    assert!(entities(&mut app) > closed);
    app.world_mut().resource_mut::<Selected>().0 = None;
    frame(&mut app, 0.1);
    assert_eq!(entities(&mut app), closed, "the inside is given back");
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "the view wrote nothing"
    );
}

#[test]
fn a_resident_who_is_gone_is_no_longer_tracked() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    spawn_roots(app.world_mut());
    frame(&mut app, 0.1);
    let world = app.world_mut();
    let tracked = world.resource::<IndoorViews>().0.len();
    assert!(tracked > 0);
    // A day after a death the simulation removes the entity; an arrival in
    // the same tick keeps the count the same.
    let gone = *world.resource::<IndoorViews>().0.keys().next().unwrap();
    let root = root_of(world, gone);
    world.entity_mut(root).despawn();
    world.resource_mut::<VisualIndex>().map.remove(&gone);
    world.entity_mut(gone).despawn();
    let stranger = world.spawn_empty().id();
    world.resource_mut::<IndoorViews>().0.insert(
        stranger,
        Track {
            phase: Phase::Inside,
            building: None,
            hidden: true,
        },
    );
    world.entity_mut(stranger).despawn();
    frame(&mut app, 0.1);
    let views = &app.world().resource::<IndoorViews>().0;
    assert_eq!(views.len(), tracked - 1);
    assert!(!views.contains_key(&gone) && !views.contains_key(&stranger));
}

#[test]
fn a_building_blocks_the_line_from_the_eye_to_somebody_behind_it() {
    // A 4 by 5 m home 6.5 m high, standing on ground at 2 m.
    let frame = body(4, 5, 2);
    let (lo, hi) = frame.bounds();
    let middle = (lo + hi) * 0.5;
    let (ground, height) = (2.0, 6.5);
    let at = |x: f32, y: f32, z: f32| Vec3::new(middle.x + x, ground + y, middle.y + z);
    // The camera high in front, somebody close behind the back wall.
    let eye = at(0.0, 14.0, -20.0);
    assert!(blocks(&frame, ground, height, eye, at(0.0, 1.2, 4.0)));
    // In front of the house, or well beside it, they are in plain view.
    assert!(!blocks(&frame, ground, height, eye, at(0.0, 1.2, -4.0)));
    assert!(!blocks(&frame, ground, height, eye, at(6.0, 1.2, 4.0)));
    // Far enough behind, the line passes over the roof.
    assert!(!blocks(&frame, ground, height, eye, at(0.0, 1.2, 30.0)));
    // A camera straight overhead is blocked by nothing beside it.
    assert!(!blocks(
        &frame,
        ground,
        height,
        at(4.0, 30.0, 0.0),
        at(4.0, 1.2, 0.0)
    ));
    // A lower building lets the same line pass.
    assert!(!blocks(&frame, ground, 1.0, eye, at(0.0, 1.2, 4.0)));
}

#[test]
fn a_home_between_the_camera_and_the_selected_resident_is_taken_out_of_the_way() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    spawn_roots(app.world_mut());
    app.init_resource::<Unblocked>()
        .init_resource::<HomeBlocks>();
    frame(&mut app, 0.1);
    let world = app.world_mut();
    // A home and somebody who lives elsewhere, put behind its back wall.
    let home = view::homes(world).remove(0);
    let sim = world.resource::<SimIndex>().get(home.house).unwrap();
    let root = root_of(world, sim);
    let shell = world.get::<HomeShell>(root).unwrap().clone();
    let occluder = *world.get::<Occluder>(root).expect("a box");
    assert_eq!(occluder.drawn, shell.frame_entity);
    // To the ridge, not the chimney.
    assert!(
        occluder.height > 5.0 && occluder.height < 6.0,
        "{}",
        occluder.height
    );
    let somebody = view::homes(world)[5].beds[0].resident.expect("a resident");
    let actor = world.resource::<SimIndex>().get(somebody).unwrap();
    world.get_mut::<Indoors>(actor).unwrap().building = None;
    let ground = world.get::<Transform>(root).unwrap().translation.y;
    let behind = shell
        .frame
        .to_world(Vec3::new(0.0, 0.0, shell.frame.depth * 0.5 + 1.5), ground);
    let before = shell.frame.to_world(Vec3::new(0.0, 14.0, -22.0), ground);
    let actor_root = root_of(world, actor);
    world.get_mut::<Transform>(actor_root).unwrap().translation = behind;
    world.spawn((
        Camera3d::default(),
        GlobalTransform::from_translation(before),
    ));
    let rig = world
        .spawn(RtsCamera {
            focus: behind,
            yaw: 0.0,
            pitch: 0.7,
            distance: 26.0,
        })
        .id();
    // No asset server here: a stand-in for the house model, at the fit's
    // scale, under the frame as the real one hangs.
    let model = world
        .spawn((
            ChildOf(shell.frame_entity),
            shell.fit.transform(),
            Visibility::default(),
        ))
        .id();
    world.get_mut::<HomeShell>(root).unwrap().model_entity = Some(model);
    let hidden = |world: &World| world.get::<Visibility>(model) == Some(&Visibility::Hidden);
    let height = |world: &World| world.get::<Transform>(model).unwrap().scale.y;
    let standing = height(world);
    let run = |world: &mut World, seconds: f32| {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        world.run_system_once(unblock).unwrap();
        world.run_system_once(cutaways).unwrap();
    };
    let open = |world: &World| world.resource::<Cutaways>().is_open(home.house);
    // Nobody selected: nothing is looked for, nothing opened.
    run(world, 0.1);
    assert!(!open(world) && world.resource::<HomeBlocks>().0.is_empty());
    // Selected at a close camera: the home is cut away, not squashed to its
    // footing. Its roof and walls sink over a fifth of a second, uncovering
    // the room, and the frame (floor, furniture, far walls) stays.
    world.resource_mut::<Selected>().0 = Some(somebody);
    run(world, 0.01);
    assert!(world.resource::<HomeBlocks>().0.contains_key(&home.house));
    assert!(world.resource::<Unblocked>().0.is_empty(), "no squash");
    run(world, UNBLOCK_EASE_SECS * 0.5);
    assert!(open(world));
    assert!(!hidden(world), "still sinking");
    let sinking = height(world);
    assert!(
        sinking < standing * 0.9 && sinking > standing * 0.05,
        "{sinking}"
    );
    run(world, UNBLOCK_EASE_SECS * 0.6);
    assert!(hidden(world));
    assert_eq!(
        world.get::<Visibility>(shell.frame_entity),
        Some(&Visibility::Inherited),
        "the room stays"
    );
    // The back wall, between the room and the resident behind it, is down
    // too, so they are seen whole; the side walls stand.
    assert!(wall_hides(
        Wall::Back,
        &shell.frame,
        Vec2::new(behind.x, behind.z)
    ));
    assert!(!wall_hides(
        Wall::Left,
        &shell.frame,
        Vec2::new(behind.x, behind.z)
    ));
    assert!(!wall_hides(
        Wall::Front,
        &shell.frame,
        Vec2::new(behind.x, behind.z)
    ));
    // From far out the home is left alone: a moment after it stops
    // blocking the cutaway closes and the roof rises to its own height.
    world.get_mut::<RtsCamera>(rig).unwrap().distance = 60.0;
    run(world, 0.1);
    assert!(open(world), "it lingers");
    run(world, UNBLOCK_LINGER_SECS);
    assert!(!open(world));
    assert!(!hidden(world), "rising");
    run(world, UNBLOCK_EASE_SECS + 0.05);
    assert!((height(world) - standing).abs() < 1e-4);
    // The played character is kept in view at any distance.
    world.resource_mut::<Selected>().0 = None;
    world.insert_resource(IslandMode::Character);
    world.resource_mut::<VillageState>().played = somebody;
    run(world, 0.01);
    run(world, UNBLOCK_EASE_SECS + 0.05);
    assert!(open(world) && hidden(world));
    // Seen from the other side, nothing is in the way.
    let mut eyes = world.query_filtered::<&mut GlobalTransform, With<Camera3d>>();
    *eyes.single_mut(world).unwrap() =
        GlobalTransform::from_translation(shell.frame.to_world(Vec3::new(0.0, 14.0, 22.0), ground));
    run(world, UNBLOCK_LINGER_SECS + 0.05);
    run(world, UNBLOCK_EASE_SECS + 0.05);
    assert!(!open(world) && !hidden(world));
    assert!((height(world) - standing).abs() < 1e-4);
}

#[test]
fn a_tree_blocks_only_where_its_trunk_or_crown_stands_between_eye_and_somebody() {
    // A single pine at scale 3.6: crown 1.3 m round at 0.54 m, 4 m high.
    let shape = TreeShape::of_model(1, 3.6);
    let root = Vec3::new(5.0, 1.0, 5.0);
    let at = |x: f32, y: f32, z: f32| root + Vec3::new(x, y, z);
    let eye = at(0.0, 14.0, -20.0);
    // Behind the tree: blocked. Beside it, or in front: not.
    assert!(blocks_tree(root, &shape, eye, at(0.0, 1.2, 3.0)));
    assert!(!blocks_tree(root, &shape, eye, at(4.0, 1.2, 3.0)));
    assert!(!blocks_tree(root, &shape, eye, at(0.0, 1.2, -3.0)));
    // Under the crown, the crown hides them.
    assert!(blocks_tree(root, &shape, eye, at(0.0, 1.2, 0.5)));
    // Slightly off the line: clear of the crown by more than the margin.
    assert!(!blocks_tree(root, &shape, eye, at(2.4, 1.2, 3.0)));
    // The square column of the last pass (1.3 m each side of the trunk up
    // to the top) took this one away; the line passes beside the tapering
    // crown, more than the margin clear of it.
    let low_eye = at(-1.2, 8.0, -10.0);
    assert!(!blocks_tree(root, &shape, low_eye, at(-1.2, 1.2, 4.0)));
    // A level line through the trunk blocks; one past it does not.
    assert!(blocks_tree(
        root,
        &shape,
        at(0.0, 0.3, -5.0),
        at(0.0, 0.3, 5.0)
    ));
    assert!(!blocks_tree(
        root,
        &shape,
        at(1.0, 0.3, -5.0),
        at(1.0, 0.3, 5.0)
    ));
    // The squash eases from standing to almost flat.
    assert_eq!(squash(0.0), 1.0);
    assert!(squash(0.5) < 0.6 && squash(0.5) > 0.4);
    assert!(squash(1.0) < 0.05);
}

#[test]
fn a_founding_workplace_is_drawn_on_the_body_view_workplace_reports() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    let world = app.world();
    let all = view::workplaces(world);
    // The well and the fishing hut are open and keep their own scale.
    let founding: Vec<_> = all
        .into_iter()
        .filter(|place| !place.built && !matches!(place.role, Role::WaterKeeper | Role::Fisher))
        .collect();
    assert!(!founding.is_empty());
    for place in founding {
        let seen =
            BodyFrame::of_bounds(place.body_min, place.body_max, place.facing, place.entrance);
        let drawn = BodyFrame::of_founding_workplace(place.entrance);
        let what = format!("{} ({:?})", place.label, place.role);
        assert!(
            (seen.centre - drawn.centre).length() < 0.01,
            "{what}: {seen:?} {drawn:?}"
        );
        assert!(
            (seen.width - drawn.width).abs() < 0.01,
            "{what}: {seen:?} {drawn:?}"
        );
        assert!(
            (seen.depth - drawn.depth).abs() < 0.01,
            "{what}: {seen:?} {drawn:?}"
        );
        assert!(seen.dir.dot(drawn.dir) > 0.99, "{what}: {seen:?} {drawn:?}");
    }
}

#[test]
fn everybody_up_indoors_stands_clear_of_the_furniture() {
    let check = |what: &str, layout: &Interior, door_x: f32| {
        let furniture = layout.furniture(door_x);
        for (index, spot) in layout.spots.iter().enumerate() {
            assert!(
                clear_to_stand(spot.at, &layout.room, &furniture, &[]),
                "{what}: place {index} at {:?} touches furniture or a wall",
                spot.at
            );
            for other in &layout.spots[index + 1..] {
                assert!(other.at.distance(spot.at) > 0.5, "{what}: two on one place");
            }
        }
    };
    // The homestead cottage (4 by 5, two beds) and its larger body (5 by 6,
    // four), a founding home and a family house, and the inn (5 by 5, four).
    for (width, depth, beds) in [(4, 5, 1), (4, 5, 2), (5, 6, 3), (5, 6, 4), (5, 5, 4)] {
        for door_x in [-0.5, 0.0, 0.5] {
            let layout = interior(width as f32, depth as f32, door_x, beds);
            check(
                &format!("{width}x{depth}, {beds} beds, door {door_x}"),
                &layout,
                door_x,
            );
        }
    }
    // Every home and inn of the shipped world, as the cutaway lays it out.
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    let world = app.world();
    let mut seen = 0;
    let buildings: Vec<SimId> = view::homes(world)
        .into_iter()
        .map(|home| home.house)
        .chain(view::inns(world).into_iter().map(|inn| inn.inn))
        .collect();
    for id in buildings {
        let sim = world.resource::<SimIndex>().get(id).expect("a building");
        let Some((frame, inn)) = subject(world, sim, id) else {
            continue;
        };
        let fit = model::house_model(inn).fit(&frame);
        let beds = bed_kinds(world, id, inn).len();
        let layout = interior(frame.width, frame.depth, fit.door_x(), beds);
        check(&format!("building {id:?}"), &layout, fit.door_x());
        seen += 1;
    }
    assert!(seen > 20, "{seen}");
}

#[test]
fn founding_workplace_bodies_are_solid_and_nobody_is_drawn_inside_one() {
    let mut app = living_world();
    run_ticks(app.world_mut(), 3);
    let world = app.world();
    let solid: Vec<_> = view::workplaces(world)
        .into_iter()
        .filter(|place| !place.built && !matches!(place.role, Role::WaterKeeper | Role::Fisher))
        .collect();
    assert!(!solid.is_empty());
    // The simulation now says so itself; the view's own rule agrees.
    for place in &solid {
        assert!(place.solid, "{} ({:?})", place.label, place.role);
    }
    // Over a working morning, nobody outdoors stands inside a solid body
    // (where the view would draw them, inside a model).
    let mut app = living_world();
    for _ in 0..12 {
        run_ticks(app.world_mut(), 50);
        let world = app.world_mut();
        let bodies: Vec<(SimPos, SimPos, String)> = view::workplaces(world)
            .into_iter()
            .filter(|place| place.solid)
            .map(|place| (place.body_min, place.body_max, place.label))
            .collect();
        let mut people = world.query::<(&SimPos, &Villager, Option<&Indoors>)>();
        for (at, person, indoors) in people.iter(world) {
            if indoors.is_some_and(|indoors| indoors.building.is_some()) {
                continue;
            }
            for (lo, hi, label) in &bodies {
                let inside = at.x > lo.x + 300
                    && at.x < hi.x - 300
                    && at.z > lo.z + 300
                    && at.z < hi.z - 300;
                assert!(!inside, "{} stands inside {label} at {at:?}", person.name);
            }
        }
    }
}

#[test]
fn a_cottage_whose_body_grows_is_fitted_again_to_the_larger_body() {
    use npc_framework::prelude::{Footprint, GridBlocker};
    let mut app = living_world();
    spawn_roots(app.world_mut());
    app.init_resource::<Unblocked>()
        .init_resource::<HomeBlocks>();
    let world = app.world_mut();
    // The standard homestead cottage: 4 by 5 m behind its door.
    let centre = SimPos::new(30_000, 30_000);
    let (id, sim) = npc_framework::prelude::spawn_sim(
        world,
        (
            centre,
            GridBlocker(Footprint {
                half_x_mm: 2_000,
                half_z_mm: 2_500,
            }),
            Household {
                label: "Cottage".into(),
                capacity: 2,
                residents: vec![],
                door: SimPos::new(centre.x, centre.z + 3_300),
                heated_until: 0,
                logs_burned: 0,
            },
            Cottage {
                plot: SimId(u32::MAX),
                plot_id: "plot".into(),
            },
        ),
    );
    let root = world
        .spawn((
            VisualOf(sim),
            StaticId(id),
            Transform::from_xyz(30.0, 2.0, 30.0),
            Visibility::default(),
            Fogged(Look::Live),
        ))
        .id();
    world.resource_mut::<VisualIndex>().map.insert(sim, root);
    frame(&mut app, 0.1);
    let size = |world: &World| {
        let shell = world.get::<HomeShell>(root).expect("a shell");
        let (a, b) = (shell.frame.width, shell.frame.depth);
        (a.min(b), a.max(b))
    };
    let first = size(app.world());
    assert!(
        (first.0 - 4.0).abs() < 0.01 && (first.1 - 5.0).abs() < 0.01,
        "{first:?}"
    );
    let old_frame = app.world().get::<HomeShell>(root).unwrap().frame_entity;
    // The upgrade finishes: the same entity grows to 5 by 6 m, the door
    // where it was.
    let world = app.world_mut();
    world.entity_mut(sim).insert((
        SimPos::new(centre.x, centre.z - 500),
        GridBlocker(Footprint {
            half_x_mm: 2_500,
            half_z_mm: 3_000,
        }),
    ));
    let before = state_hash(world);
    let run = |world: &mut World, seconds: f32| {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        world.run_system_once(upgrade::refit).unwrap();
        world.run_system_once(dress).unwrap();
    };
    run(world, 0.6);
    run(world, 0.6);
    let grown = size(world);
    assert!(
        (grown.0 - 5.0).abs() < 0.01 && (grown.1 - 6.0).abs() < 0.01,
        "{grown:?}"
    );
    assert!(
        world.get_entity(old_frame).is_err(),
        "the old drawing is gone"
    );
    assert_eq!(
        state_hash(world),
        before,
        "the view wrote no simulation state"
    );
    // Furnished for four in a 5 by 6 m room: full beds, a hearth, a table.
    let shell = world.get::<HomeShell>(root).unwrap().clone();
    let layout = interior(shell.frame.width, shell.frame.depth, shell.fit.door_x(), 4);
    assert!(layout.beds.iter().all(|bed| bed.scale == 1.0));
    assert!(layout.hearth.size.x > 0.0 && layout.table.size.x > 0.0);
}
