//! Geometry and rules of the island's homes as the view draws them: where a
//! body stands, how a house model is fitted to it, what the inside holds,
//! when a home is cut away and who is hidden indoors. Pure: no ECS access.
use bevy::prelude::*;
use island_content::village::Role;
use island_content::village::frontier::settlements::{HOME_OFFSET_MM, WORKPLACE_OFFSET_MM};
use island_content::village::frontier::world_spec::LivingWorldSpec;
use island_content::village::view::HomeView;
use npc_framework::prelude::{SimId, SimPos};

/// Height of the tallest shipped resident at its placed scale: the
/// Quaternius civilians import at about 1.8 m and are drawn at scale 1.
#[cfg(test)]
pub(crate) const RESIDENT_HEIGHT_M: f32 = 1.8;
/// The shortest: a KayKit adventurer, about 2.3 m imported, drawn at 0.75.
#[cfg(test)]
pub(crate) const RESIDENT_HEIGHT_MIN_M: f32 = 2.3 * crate::visuals::V;
/// Height of a home's door, the crown of its frame: 1.22 times the tallest
/// resident and 1.28 times the shortest. Every house model is scaled
/// vertically so its own door is this high.
pub(crate) const DOOR_HEIGHT_M: f32 = 2.2;
/// Top of the ground-floor walls: under the cottage's eaves beam.
pub(crate) const WALL_HEIGHT_M: f32 = 2.35;
/// Thickness of the walls the cutaway draws.
pub(crate) const WALL_THICKNESS_M: f32 = 0.18;
/// What is left of a wall on the camera's side of a cut-away home.
pub(crate) const LOW_WALL_HEIGHT_M: f32 = 0.35;

/// Half extents of a founding workplace's body, as its blocker is spawned
/// (`frontier::settlements`): the simulation has no accessor for it.
const FOUNDING_WORKPLACE_HALF_M: f32 = 2.5;

/// Half extents of a founding home's body, likewise.
const FOUNDING_HOME_HALF_MM: (i32, i32) = (1_900, 2_300);

/// Whether a point is on the body of a founding home or solid workplace,
/// or within `margin` metres of it: where the fitted models stand, so no
/// scenery may. The well and the fishing hut have no body.
pub(crate) fn on_founding_body(spec: &LivingWorldSpec, at: SimPos, margin: f32) -> bool {
    let point = metres(at);
    spec.settlements.iter().any(|settlement| {
        settlement
            .homes
            .iter()
            .any(|home| BodyFrame::of_founding_home(home.door).contains(point, margin))
            || settlement.sites.iter().any(|site| {
                !matches!(site.role, Role::WaterKeeper | Role::Fisher)
                    && BodyFrame::of_founding_workplace(site.position).contains(point, margin)
            })
    })
}

/// Where a building's body stands on the ground plane, in world metres.
/// Its own axes: +Z runs from the entrance side to the back, +X across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BodyFrame {
    /// Centre of the body, world (x, z).
    pub(crate) centre: Vec2,
    /// Across the entrance side.
    pub(crate) width: f32,
    /// From the entrance side to the back.
    pub(crate) depth: f32,
    /// Unit vector from the entrance into the body, world (x, z).
    pub(crate) dir: Vec2,
    /// Where residents stand to go in, world (x, z).
    pub(crate) entrance: Vec2,
}

fn metres(pos: SimPos) -> Vec2 {
    Vec2::new(pos.x as f32 / 1000.0, pos.z as f32 / 1000.0)
}

impl BodyFrame {
    /// The body of an inclusive cell rectangle that lies towards quarter
    /// turn `facing` of its entrance (+z, +x, -z, -x), as `HomeView` and
    /// `civic::Plot` describe it.
    pub(crate) fn of_cells(lo: (i32, i32), hi: (i32, i32), facing: u8, entrance: SimPos) -> Self {
        let size = Vec2::new((hi.0 - lo.0 + 1) as f32, (hi.1 - lo.1 + 1) as f32);
        let dir = match facing % 4 {
            0 => Vec2::Y,
            1 => Vec2::X,
            2 => Vec2::NEG_Y,
            _ => Vec2::NEG_X,
        };
        let (width, depth) = if facing.is_multiple_of(2) {
            (size.x, size.y)
        } else {
            (size.y, size.x)
        };
        Self {
            centre: Vec2::new(
                (lo.0 + hi.0 + 1) as f32 * 0.5,
                (lo.1 + hi.1 + 1) as f32 * 0.5,
            ),
            width,
            depth,
            dir,
            entrance: metres(entrance),
        }
    }

    /// A body given in millimetres, as `HomeView::body_min..body_max` and
    /// `InnView` give it: a founding workplace's is not on whole cells.
    pub(crate) fn of_bounds(min: SimPos, max: SimPos, facing: u8, entrance: SimPos) -> Self {
        let (lo, hi) = (metres(min), metres(max));
        let dir = match facing % 4 {
            0 => Vec2::Y,
            1 => Vec2::X,
            2 => Vec2::NEG_Y,
            _ => Vec2::NEG_X,
        };
        let size = hi - lo;
        let (width, depth) = if facing.is_multiple_of(2) {
            (size.x, size.y)
        } else {
            (size.y, size.x)
        };
        Self {
            centre: (lo + hi) * 0.5,
            width,
            depth,
            dir,
            entrance: metres(entrance),
        }
    }

    pub(crate) fn of_home(home: &HomeView) -> Self {
        Self::of_cells(home.body_lo, home.body_hi, home.facing, home.entrance)
    }

    /// A founding home, from its door alone: the cells of the blocker the
    /// simulation spawns behind it, as `view::home` reports them. For scenery
    /// that is laid out before any home has a visual.
    pub(crate) fn of_founding_home(door: SimPos) -> Self {
        let (x, z) = (door.x, door.z + HOME_OFFSET_MM);
        let cell = |at: i32| at.div_euclid(1000);
        Self::of_cells(
            (
                cell(x - FOUNDING_HOME_HALF_MM.0),
                cell(z - FOUNDING_HOME_HALF_MM.1),
            ),
            (
                cell(x + FOUNDING_HOME_HALF_MM.0 - 1),
                cell(z + FOUNDING_HOME_HALF_MM.1 - 1),
            ),
            if HOME_OFFSET_MM < 0 { 2 } else { 0 },
            door,
        )
    }

    /// A founding workplace: its blocker stands behind the site's own
    /// position, which is where its workers and guests stand.
    pub(crate) fn of_founding_workplace(site: SimPos) -> Self {
        let entrance = metres(site);
        Self {
            centre: entrance + Vec2::new(0.0, WORKPLACE_OFFSET_MM as f32 / 1000.0),
            width: FOUNDING_WORKPLACE_HALF_M * 2.0,
            depth: FOUNDING_WORKPLACE_HALF_M * 2.0,
            dir: Vec2::Y,
            entrance,
        }
    }

    /// The turn about the vertical that points local +Z along `dir`.
    pub(crate) fn yaw(&self) -> f32 {
        self.dir.x.atan2(self.dir.y)
    }

    pub(crate) fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw())
    }

    /// The body's +X axis in world (x, z).
    pub(crate) fn across(&self) -> Vec2 {
        Vec2::new(self.dir.y, -self.dir.x)
    }

    /// A point of the body's own frame in the world; `ground` is the height
    /// the body stands at.
    pub(crate) fn to_world(self, local: Vec3, ground: f32) -> Vec3 {
        let flat = self.centre + self.across() * local.x + self.dir * local.z;
        Vec3::new(flat.x, ground + local.y, flat.y)
    }

    /// A world (x, z) point in the body's own frame: (across, along).
    pub(crate) fn local_of(&self, point: Vec2) -> Vec2 {
        let offset = point - self.centre;
        Vec2::new(offset.dot(self.across()), offset.dot(self.dir))
    }

    /// Whether a world point lies on the body or within `margin` of it.
    pub(crate) fn contains(&self, point: Vec2, margin: f32) -> bool {
        let local = self.local_of(point);
        local.x.abs() <= self.width * 0.5 + margin && local.y.abs() <= self.depth * 0.5 + margin
    }

    /// World (x, z) corners of the body: low and high.
    #[cfg(test)]
    pub(crate) fn bounds(&self) -> (Vec2, Vec2) {
        let half = (self.across() * self.width * 0.5).abs() + (self.dir * self.depth * 0.5).abs();
        (self.centre - half, self.centre + half)
    }
}

/// The two KayKit house meshes, measured in their own units (the kit's grid
/// is 0.07): the box of the ground-floor walls, the door and the chimney
/// mouth. A model's door is on its +Z wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HouseModel {
    /// `building_home_A`: one storey, the door in the middle of the gable
    /// wall, the chimney on the back wall.
    Cottage,
    /// `building_home_B`: two storeys on a stone base, the door to one side.
    Hall,
}

impl HouseModel {
    pub(crate) fn path(self) -> &'static str {
        match self {
            Self::Cottage => "hex/building_home_A_blue.gltf",
            Self::Hall => "living/buildings/building_home_B_blue.gltf",
        }
    }
    /// Half the width of the wall box.
    fn half_x(self) -> f32 {
        match self {
            Self::Cottage => 0.28,
            Self::Hall => 0.35,
        }
    }
    /// The back and the front wall plane.
    fn z_range(self) -> (f32, f32) {
        match self {
            Self::Cottage => (-0.315, 0.315),
            Self::Hall => (-0.35, 0.245),
        }
    }
    /// Height of the door's threshold: the hall stands on a base, which the
    /// view sinks into the ground.
    fn threshold(self) -> f32 {
        match self {
            Self::Cottage => 0.0,
            Self::Hall => 0.21,
        }
    }
    /// Height of the door, threshold to the crown of its frame.
    const DOOR_UNITS: f32 = 0.315;
    /// How far the door's leaf stands proud of the wall plane.
    const DOOR_PROUD_UNITS: f32 = 0.03;
    /// Width of the door's frame, and of the leaf inside it.
    const DOOR_FRAME_UNITS: f32 = 0.21;
    const DOOR_LEAF_UNITS: f32 = 0.145;
    /// Where the door's middle is along the front wall.
    fn door_x(self) -> f32 {
        match self {
            Self::Cottage => 0.0,
            Self::Hall => 0.14,
        }
    }
    /// The ridge of the roof, the highest the body stands without its
    /// chimney (read off the meshes: 0.77 and 1.225 units).
    fn ridge(self) -> f32 {
        match self {
            Self::Cottage => 0.77,
            Self::Hall => 1.225,
        }
    }
    /// The chimney's mouth.
    fn chimney(self) -> Vec3 {
        match self {
            Self::Cottage => Vec3::new(0.0, 0.93, -0.35),
            Self::Hall => Vec3::new(0.14, 1.28, -0.42),
        }
    }

    /// A body smaller than a house (the homestead cottage's 2 by 2 m) is not
    /// stretched further upwards than this against its wider wall: a door
    /// of full height would make it a tower.
    const MAX_STRETCH: f32 = 1.4;

    /// Scale and place the model so its wall box covers the body exactly,
    /// its door is [`DOOR_HEIGHT_M`] high (lower on a body too small for
    /// that) and stands on the ground, on the entrance side.
    pub(crate) fn fit(self, frame: &BodyFrame) -> ModelFit {
        let (back, front) = self.z_range();
        let across = frame.width / (self.half_x() * 2.0);
        let along = frame.depth / (front - back);
        let scale = Vec3::new(
            across,
            (DOOR_HEIGHT_M / Self::DOOR_UNITS).min(across.max(along) * Self::MAX_STRETCH),
            along,
        );
        ModelFit {
            scale,
            // The model is turned half round: its +Z, the door, faces the
            // entrance, which is the body's -Z.
            translation: Vec3::new(
                0.0,
                -self.threshold() * scale.y,
                (front + back) * 0.5 * scale.z,
            ),
            model: self,
        }
    }
}

/// A house model fitted to one body, in the body's own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ModelFit {
    pub(crate) scale: Vec3,
    pub(crate) translation: Vec3,
    pub(crate) model: HouseModel,
}

impl ModelFit {
    /// The model's transform under the body's frame.
    pub(crate) fn transform(&self) -> Transform {
        Transform::from_translation(self.translation)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
            .with_scale(self.scale)
    }
    /// A point of the model, in the body's frame.
    pub(crate) fn point(&self, model: Vec3) -> Vec3 {
        self.translation + Vec3::new(-model.x, model.y, -model.z) * self.scale
    }
    /// A size along the model's axes, in metres.
    pub(crate) fn size(&self, model: Vec3) -> Vec3 {
        model * self.scale
    }
    pub(crate) fn door_height(&self) -> f32 {
        HouseModel::DOOR_UNITS * self.scale.y
    }
    pub(crate) fn door_width(&self) -> f32 {
        HouseModel::DOOR_FRAME_UNITS * self.scale.x
    }
    /// Width of the opening between the jambs: the door's leaf.
    pub(crate) fn door_opening(&self) -> f32 {
        HouseModel::DOOR_LEAF_UNITS * self.scale.x
    }
    /// Where the door's middle is along the entrance wall.
    pub(crate) fn door_x(&self) -> f32 {
        -self.model.door_x() * self.scale.x
    }
    /// The roof's ridge over the ground, in metres: how high the body
    /// stands for taking it out of the way of the camera.
    pub(crate) fn ridge(&self) -> f32 {
        self.point(Vec3::new(0.0, self.model.ridge(), 0.0)).y
    }
    /// The chimney's mouth, in the body's frame.
    pub(crate) fn chimney(&self) -> Vec3 {
        self.point(self.model.chimney())
    }
    /// Lit panes of a cottage, in the body's frame: centre and size. The
    /// two windows of each side wall, the gable window over the door and a
    /// lantern beside the door, so a lit home shows from every side.
    pub(crate) fn panes(&self) -> Vec<(Vec3, Vec3)> {
        if self.model != HouseModel::Cottage {
            return Vec::new();
        }
        let window = Vec3::new(0.012, 0.096, 0.07);
        let mut panes: Vec<(Vec3, Vec3)> = [-1.0_f32, 1.0]
            .into_iter()
            .flat_map(|side| {
                [-0.105_f32, 0.105].map(move |z| (Vec3::new(side * 0.262, 0.214, z), window))
            })
            .map(|(at, size)| (self.point(at), self.size(size)))
            .collect();
        // The gable window, a little proud of its glass.
        panes.push((
            self.point(Vec3::new(0.0, 0.51, 0.326)),
            self.size(Vec3::new(0.066, 0.09, 0.012)),
        ));
        // A lantern a hand's width off the wall, beside the door.
        let door = self.door_x() + self.door_width() * 0.5 + 0.42;
        panes.push((
            self.point(Vec3::new(0.0, 0.0, 0.315)) + Vec3::new(door, 1.75, -0.16),
            Vec3::new(0.2, 0.3, 0.2),
        ));
        panes
    }
}

/// Which house model a building wears. Every home is the cottage, whose
/// door is in the middle of its wall; the inn is the two-storey hall.
pub(crate) fn house_model(inn: bool) -> HouseModel {
    if inn {
        HouseModel::Hall
    } else {
        HouseModel::Cottage
    }
}

/// Length and width of a full bed (the shipped `bed_single_A` at
/// [`BED_MODEL_SCALE`]) and the height of its mattress.
pub(crate) const BED_LENGTH_M: f32 = 1.98;
pub(crate) const BED_WIDTH_M: f32 = 1.06;
pub(crate) const BED_MODEL_SCALE: f32 = 0.66;
pub(crate) const MATTRESS_HEIGHT_M: f32 = 0.33;
/// A child's bed, against a full one.
pub(crate) const CHILD_BED_SCALE: f32 = 0.72;

/// Free floor kept inside the door: half its width and its depth.
const DOOR_CLEAR_HALF_M: f32 = 0.6;
const DOOR_CLEAR_DEPTH_M: f32 = 0.95;
/// Gap between a wall and the furniture, and between two beds in a row.
const WALL_GAP_M: f32 = 0.06;
const ROW_GAP_M: f32 = 0.14;
/// The hearth against the back wall, and the table.
pub(crate) const HEARTH_SIZE_M: Vec2 = Vec2::new(1.2, 0.6);
pub(crate) const TABLE_SIZE_M: Vec2 = Vec2::new(0.9, 0.6);

/// An axis-aligned rectangle of the floor, in the body's frame (x, z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FloorRect {
    pub(crate) centre: Vec2,
    pub(crate) size: Vec2,
}

impl FloorRect {
    pub(crate) fn lo(&self) -> Vec2 {
        self.centre - self.size * 0.5
    }
    pub(crate) fn hi(&self) -> Vec2 {
        self.centre + self.size * 0.5
    }
    pub(crate) fn overlaps(&self, other: &FloorRect) -> bool {
        const TOUCH: f32 = 1e-4;
        let (a, b) = ((self.lo(), self.hi()), (other.lo(), other.hi()));
        a.0.x < b.1.x - TOUCH
            && b.0.x < a.1.x - TOUCH
            && a.0.y < b.1.y - TOUCH
            && b.0.y < a.1.y - TOUCH
    }
    #[cfg(test)]
    pub(crate) fn inside(&self, outer: &FloorRect) -> bool {
        const TOUCH: f32 = 1e-4;
        self.lo().x >= outer.lo().x - TOUCH
            && self.lo().y >= outer.lo().y - TOUCH
            && self.hi().x <= outer.hi().x + TOUCH
            && self.hi().y <= outer.hi().y + TOUCH
    }
}

/// One bed of the layout: its floor rectangle (the head is at the back, +Z)
/// and its size against a full bed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BedSpot {
    pub(crate) rect: FloorRect,
    pub(crate) scale: f32,
    /// How far above the floor the bed stands: a berth of a bunk.
    pub(crate) lift: f32,
    /// Lying across the room, the head at +X, not at the back.
    pub(crate) across: bool,
}

/// From one berth of a bunk to the next above it.
pub(crate) const BUNK_TIER_M: f32 = 0.75;

/// Where somebody who is up stands or sits: a point of the floor and the
/// way they face (a turn about the vertical in the body's frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FloorSpot {
    pub(crate) at: Vec2,
    pub(crate) yaw: f32,
}

/// The inside of a home, in the body's frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Interior {
    /// The floor inside the walls.
    pub(crate) room: FloorRect,
    pub(crate) beds: Vec<BedSpot>,
    pub(crate) hearth: FloorRect,
    pub(crate) table: FloorRect,
    pub(crate) stools: [Vec2; 2],
    /// Floor kept free inside the door.
    pub(crate) door: FloorRect,
    /// By the hearth first, then at the table, then inside the door.
    pub(crate) spots: Vec<FloorSpot>,
}

/// Where a homestead's chest stands: the front corner away from the door,
/// clear of the walls, a footprint of about 0.75 by 0.5 m.
pub(crate) fn chest_spot(layout: &Interior, door_x: f32) -> FloorRect {
    chest_rect(&layout.room, door_x)
}

fn chest_rect(room: &FloorRect, door_x: f32) -> FloorRect {
    let size = Vec2::new(0.75, 0.5);
    let side = if door_x > 0.0 { -1.0 } else { 1.0 };
    FloorRect {
        centre: Vec2::new(
            side * (room.size.x * 0.5 - size.x * 0.5 - 0.05),
            -room.size.y * 0.5 + size.y * 0.5 + 0.05,
        ),
        size,
    }
}

/// How much room somebody standing indoors takes: a 1.8 m adult's
/// shoulders and elbows, round the point they stand on.
pub(crate) const PERSON_RADIUS_M: f32 = 0.45;
/// How far the hearth's fire and its stone slab stand out in front of its
/// surround, and half a stool's seat.
pub(crate) const HEARTH_APRON_M: f32 = 0.32;
const STOOL_HALF_M: f32 = 0.17;
/// Two people standing are at least this far apart, where the room allows.
const SHOULDER_GAP_M: f32 = 0.7;

/// The shortest distance from `point` to `rect` (0 inside it).
pub(crate) fn rect_distance(rect: &FloorRect, point: Vec2) -> f32 {
    let outside = (point - rect.centre).abs() - rect.size * 0.5;
    outside.max(Vec2::ZERO).length()
}

impl Interior {
    /// Everything on the floor somebody standing must keep clear of: the
    /// beds, the hearth with its apron, the table, the stools and the chest's
    /// corner (kept free whether or not the chest stands there yet).
    pub(crate) fn furniture(&self, door_x: f32) -> Vec<FloorRect> {
        let mut out: Vec<FloorRect> = self.beds.iter().map(|bed| bed.rect).collect();
        if self.hearth.size.x > 0.0 {
            out.push(FloorRect {
                centre: self.hearth.centre - Vec2::new(0.0, HEARTH_APRON_M * 0.5),
                size: self.hearth.size + Vec2::new(0.0, HEARTH_APRON_M),
            });
        }
        if self.table.size.x > 0.0 {
            out.push(self.table);
            for stool in self.stools {
                out.push(FloorRect {
                    centre: stool,
                    size: Vec2::splat(STOOL_HALF_M * 2.0),
                });
            }
        }
        out.push(chest_rect(&self.room, door_x));
        out
    }
}

/// Whether somebody standing at `point` is clear of the walls and of every
/// rectangle by [`PERSON_RADIUS_M`], and of the people at `others`.
#[cfg(test)]
pub(crate) fn clear_to_stand(
    point: Vec2,
    room: &FloorRect,
    furniture: &[FloorRect],
    others: &[Vec2],
) -> bool {
    clear_by(point, PERSON_RADIUS_M, room, furniture, others)
}

fn clear_by(
    point: Vec2,
    radius: f32,
    room: &FloorRect,
    furniture: &[FloorRect],
    others: &[Vec2],
) -> bool {
    let inner = room.size * 0.5 - Vec2::splat(radius);
    if inner.min_element() < 0.0 || ((point - room.centre).abs() - inner).max_element() > 0.0 {
        return false;
    }
    furniture
        .iter()
        .all(|rect| rect_distance(rect, point) >= radius)
        && others
            .iter()
            .all(|other| other.distance(point) >= SHOULDER_GAP_M)
}

/// The nearest point to `want` where somebody stands clear of everything
/// (searched on a 5 cm grid over the room). A room too full for that (more
/// beds than a home is built for) gives up the gap to the others, then
/// keeps only the feet (0.15 m) off the furniture; `want` if even that fails.
fn place_standing(want: Vec2, room: &FloorRect, furniture: &[FloorRect], others: &[Vec2]) -> Vec2 {
    const STEP: f32 = 0.05;
    let (lo, hi) = (room.lo(), room.hi());
    let (columns, rows) = (((hi.x - lo.x) / STEP) as i32, ((hi.y - lo.y) / STEP) as i32);
    for (radius, others) in [
        (PERSON_RADIUS_M, others),
        (PERSON_RADIUS_M, &[][..]),
        (0.15, &[][..]),
    ] {
        if clear_by(want, radius, room, furniture, others) {
            return want;
        }
        let mut best: Option<(f32, Vec2)> = None;
        for row in 0..=rows {
            for column in 0..=columns {
                let point = lo + Vec2::new(column as f32, row as f32) * STEP;
                let away = point.distance_squared(want);
                if best.is_some_and(|(nearest, _)| away >= nearest) {
                    continue;
                }
                if clear_by(point, radius, room, furniture, others) {
                    best = Some((away, point));
                }
            }
        }
        if let Some((_, point)) = best {
            return point;
        }
    }
    want
}

/// Move each place to stand to the nearest point clear of the furniture
/// and of the places before it, keeping its facing.
fn clear_spots(layout: &mut Interior, door_x: f32) {
    let furniture = layout.furniture(door_x);
    let mut placed: Vec<Vec2> = Vec::new();
    for spot in &mut layout.spots {
        spot.at = place_standing(spot.at, &layout.room, &furniture, &placed);
        placed.push(spot.at);
    }
}

/// Lay out a home's inside: `beds` beds along the two side walls, heads to
/// the back, filled from the back; the hearth against the middle of the back
/// wall; the table between the hearth and the door. `door_x` is where the
/// door is along the entrance wall. Beds shrink when a wall is too short for
/// its share, so any number fits.
pub(crate) fn interior(width: f32, depth: f32, door_x: f32, beds: usize) -> Interior {
    let inset = WALL_THICKNESS_M + WALL_GAP_M;
    let room = FloorRect {
        centre: Vec2::ZERO,
        size: Vec2::new(width - inset * 2.0, depth - inset * 2.0),
    };
    let (half_x, half_z) = (room.size.x * 0.5, room.size.y * 0.5);
    if room.size.x.min(room.size.y) < BED_LENGTH_M {
        return bunk_room(room, door_x, beds);
    }
    let door = FloorRect {
        centre: Vec2::new(
            door_x.clamp(-half_x + DOOR_CLEAR_HALF_M, half_x - DOOR_CLEAR_HALF_M),
            -depth * 0.5 + WALL_THICKNESS_M + DOOR_CLEAR_DEPTH_M * 0.5,
        ),
        size: Vec2::new(DOOR_CLEAR_HALF_M * 2.0, DOOR_CLEAR_DEPTH_M),
    };
    // The middle aisle holds the hearth, the table and the way in; the beds
    // keep to a strip along each side wall, never wider than a third of
    // the room.
    let strip = BED_WIDTH_M.min(room.size.x / 3.0);
    let aisle = room.size.x - strip * 2.0;
    let hearth_size = Vec2::new(
        HEARTH_SIZE_M.x.min(aisle - 0.04),
        HEARTH_SIZE_M.y.min(room.size.y / 4.0),
    );
    let hearth = FloorRect {
        centre: Vec2::new(0.0, depth * 0.5 - WALL_THICKNESS_M - hearth_size.y * 0.5),
        size: hearth_size,
    };
    // The side the door is on gets the smaller share and is filled last.
    let first = if door_x < -0.05 { 1.0 } else { -1.0 };
    let counts = [beds.div_ceil(2), beds / 2];
    let mut spots = vec![None; beds];
    for (order, side) in [first, -first].into_iter().enumerate() {
        let rows = counts[order];
        if rows == 0 {
            continue;
        }
        let lane = FloorRect {
            centre: Vec2::new(side * (half_x - strip * 0.5), 0.0),
            size: Vec2::new(strip, room.size.y),
        };
        let front = if lane.overlaps(&door) {
            door.hi().y
        } else {
            -half_z
        };
        let pitch = (half_z - front) / rows as f32;
        let scale = ((pitch - ROW_GAP_M) / BED_LENGTH_M)
            .min(strip / BED_WIDTH_M)
            .clamp(0.05, 1.0);
        let size = Vec2::new(BED_WIDTH_M, BED_LENGTH_M) * scale;
        for row in 0..rows {
            spots[row * 2 + order] = Some(BedSpot {
                rect: FloorRect {
                    centre: Vec2::new(
                        side * (half_x - size.x * 0.5),
                        half_z - pitch * row as f32 - size.y * 0.5,
                    ),
                    size,
                },
                scale,
                lift: 0.0,
                across: false,
            });
        }
    }
    let table_size = Vec2::new(
        TABLE_SIZE_M.x.min(aisle - 0.3),
        TABLE_SIZE_M.y.min(room.size.y / 5.0),
    );
    let table = FloorRect {
        centre: Vec2::new(0.0, (hearth.lo().y - 0.5 + door.hi().y) * 0.5),
        size: table_size,
    };
    let reach = table.size.y * 0.5 + 0.3;
    let stools = [
        table.centre - Vec2::new(0.0, reach),
        table.centre + Vec2::new(0.0, reach),
    ];
    // Wanted places; moved clear of the furniture below.
    let warm = hearth.lo().y - HEARTH_APRON_M - PERSON_RADIUS_M - 0.02;
    let beside = (hearth.size.x * 0.5 - 0.1).max(0.35);
    let spots_up = vec![
        FloorSpot {
            at: Vec2::new(-beside, warm),
            yaw: 0.3,
        },
        FloorSpot {
            at: Vec2::new(beside, warm),
            yaw: -0.3,
        },
        FloorSpot {
            at: Vec2::new(table.lo().x - PERSON_RADIUS_M - 0.02, table.centre.y),
            yaw: std::f32::consts::FRAC_PI_2,
        },
        FloorSpot {
            at: Vec2::new(door.centre.x, door.centre.y + 0.1),
            yaw: 0.0,
        },
    ];
    let mut layout = Interior {
        room,
        beds: spots.into_iter().flatten().collect(),
        hearth,
        table,
        stools,
        door,
        spots: spots_up,
    };
    clear_spots(&mut layout, door_x);
    layout
}

/// The inside of a room no bed fits along a wall of: the homestead cottage's
/// 2 by 2 m body leaves 1.52 m between its walls, and a bed is 1.98 m long.
/// What fits is drawn and nothing else: the beds as the berths of one bunk
/// across the back wall, as long as the room is wide, one above the other;
/// a brazier in a front corner for the hearth; no table. Whoever is up
/// stands on the strip of floor between the bunk and the door.
fn bunk_room(room: FloorRect, door_x: f32, beds: usize) -> Interior {
    let (half_x, half_z) = (room.size.x * 0.5, room.size.y * 0.5);
    let scale = (room.size.x / BED_LENGTH_M).min(1.0);
    let berth = FloorRect {
        centre: Vec2::new(0.0, half_z - BED_WIDTH_M * scale * 0.5),
        size: Vec2::new(BED_LENGTH_M, BED_WIDTH_M) * scale,
    };
    let front = berth.lo().y;
    let door = FloorRect {
        centre: Vec2::new(
            door_x.clamp(-half_x * 0.5, half_x * 0.5),
            (front - half_z) * 0.5,
        ),
        size: Vec2::new(half_x, front + half_z),
    };
    // No hearth and no table: a rectangle without a size is not drawn.
    let none = |at: Vec2| FloorRect {
        centre: at,
        size: Vec2::ZERO,
    };
    let brazier = Vec2::new(half_x - 0.2, -half_z + 0.2);
    let stand = (front - half_z) * 0.5;
    Interior {
        room,
        beds: (0..beds)
            .map(|tier| BedSpot {
                rect: berth,
                scale,
                lift: tier as f32 * BUNK_TIER_M,
                across: true,
            })
            .collect(),
        hearth: none(brazier),
        table: none(Vec2::new(0.0, stand)),
        stools: [Vec2::new(0.0, stand); 2],
        door,
        spots: vec![
            FloorSpot {
                at: Vec2::new(-half_x * 0.6, stand + 0.08),
                yaw: 0.4,
            },
            FloorSpot {
                at: Vec2::new(half_x * 0.45, stand + 0.14),
                yaw: -0.4,
            },
            FloorSpot {
                at: Vec2::new(door.centre.x, stand - 0.1),
                yaw: 0.0,
            },
        ],
    }
}

/// The four walls of a cut-away home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wall {
    /// The entrance side.
    Front,
    Back,
    Left,
    Right,
}

impl Wall {
    pub(crate) const ALL: [Wall; 4] = [Wall::Front, Wall::Back, Wall::Left, Wall::Right];
    pub(crate) fn index(self) -> usize {
        self as usize
    }
    /// Outward normal in the body's frame (x, z).
    pub(crate) fn normal(self) -> Vec2 {
        match self {
            Wall::Front => Vec2::NEG_Y,
            Wall::Back => Vec2::Y,
            Wall::Left => Vec2::NEG_X,
            Wall::Right => Vec2::X,
        }
    }
}

/// Whether a wall of a cut-away home is lowered: it stands between the
/// camera and the room. `camera_yaw` is `RtsCamera::yaw`, the side of its
/// focus the camera looks from. A wall that is already down stays down a
/// little longer, so an orbiting camera does not make it flicker.
pub(crate) fn wall_lowered(wall: Wall, frame: &BodyFrame, camera_yaw: f32, was: bool) -> bool {
    let to_camera = Vec2::new(camera_yaw.sin(), camera_yaw.cos());
    let normal = frame.across() * wall.normal().x + frame.dir * wall.normal().y;
    normal.dot(to_camera) > if was { 0.02 } else { 0.12 }
}

/// Whether `wall` stands between the room and somebody at `beyond` (world
/// x, z) outside it: they are past that wall's plane. A home cut away for
/// being in the way also lowers that wall, so the person it was cut away
/// for is seen whole.
pub(crate) fn wall_hides(wall: Wall, frame: &BodyFrame, beyond: Vec2) -> bool {
    let local = frame.local_of(beyond);
    let half = match wall {
        Wall::Front | Wall::Back => frame.depth * 0.5,
        Wall::Left | Wall::Right => frame.width * 0.5,
    };
    wall.normal().dot(local) > half
}

/// What a drawn piece of a cut-away home is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Stone,
    Plaster,
    Timber,
    /// The dark of a doorway.
    Dark,
}

/// One box of a cut-away home, in the body's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Part {
    pub(crate) tone: Tone,
    pub(crate) centre: Vec3,
    pub(crate) size: Vec3,
}

/// One box of an open door, in the body's frame, turned about the vertical.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DoorPart {
    pub(crate) tone: Tone,
    pub(crate) centre: Vec3,
    pub(crate) size: Vec3,
    pub(crate) turn: f32,
}

/// Seconds a door stays open after the last step through it.
pub(crate) const DOOR_LINGER_SECS: f32 = 1.0;
/// How far the leaf swings out, radians.
const DOOR_SWING: f32 = 1.83;

/// The boxes that show a closed house's door open: the dark of the doorway
/// over the model's painted leaf (a lower piece the width of the leaf and a
/// narrower one under the arch) and a timber leaf swung outwards on the
/// jamb. `depth` is the body's.
pub(crate) fn door_parts(fit: &ModelFit, depth: f32) -> Vec<DoorPart> {
    let (opening, tall) = (fit.door_opening(), fit.door_height());
    // The model's leaf stands this far proud of the wall plane.
    let proud = HouseModel::DOOR_PROUD_UNITS * fit.scale.z;
    let face = -depth * 0.5 - proud - 0.02;
    let spring = tall * 0.7;
    let hinge = Vec3::new(fit.door_x() - opening * 0.5, 0.0, face - 0.03);
    let leaf_tall = tall * 0.86;
    vec![
        DoorPart {
            tone: Tone::Dark,
            centre: Vec3::new(fit.door_x(), spring * 0.5, face),
            size: Vec3::new(opening * 0.98, spring, 0.03),
            turn: 0.0,
        },
        DoorPart {
            tone: Tone::Dark,
            centre: Vec3::new(fit.door_x(), spring + tall * 0.1, face),
            size: Vec3::new(opening * 0.68, tall * 0.2, 0.03),
            turn: 0.0,
        },
        DoorPart {
            tone: Tone::Timber,
            centre: hinge
                + Vec3::new(DOOR_SWING.cos(), 0.0, -DOOR_SWING.sin()) * opening * 0.5
                + Vec3::Y * leaf_tall * 0.5,
            size: Vec3::new(opening, leaf_tall, 0.06),
            turn: DOOR_SWING,
        },
    ]
}

/// Timber posts and beams stand this far proud of the plaster.
const TIMBER_PROUD_M: f32 = 0.03;
/// Width of a post, and of a door jamb.
const POST_M: f32 = 0.2;
const JAMB_M: f32 = 0.12;

/// The boxes of one wall of a cut-away home: a stone footing, plaster,
/// timber posts and a beam at [`WALL_HEIGHT_M`]; lowered, only the stone
/// footing, one unbroken kerb with no post stubs, so the walls that come
/// down read as one outline. The front wall leaves the door open: `door_x` is
/// its middle and `door_opening` its width. The front and back walls run
/// the whole width and own the corner posts; the side walls fit between.
pub(crate) fn wall_parts(
    wall: Wall,
    width: f32,
    depth: f32,
    door_x: f32,
    door_opening: f32,
    lowered: bool,
) -> Vec<Part> {
    let thick = WALL_THICKNESS_M;
    let across = matches!(wall, Wall::Front | Wall::Back);
    // Along the wall: its ends and where it stands across.
    let (reach, line) = if across {
        (width * 0.5, (depth - thick) * 0.5 * wall.normal().y)
    } else {
        (depth * 0.5 - thick, (width - thick) * 0.5 * wall.normal().x)
    };
    let place = |run: f32, height: f32, along: f32, tall: f32, proud: f32, tone: Tone| Part {
        tone,
        centre: if across {
            Vec3::new(run, height, line)
        } else {
            Vec3::new(line, height, run)
        },
        size: if across {
            Vec3::new(along, tall, thick + proud)
        } else {
            Vec3::new(thick + proud, tall, along)
        },
    };
    let spans: Vec<(f32, f32)> = if wall == Wall::Front {
        let half = door_opening * 0.5;
        vec![(-reach, door_x - half), (door_x + half, reach)]
    } else {
        vec![(-reach, reach)]
    };
    let top = if lowered {
        LOW_WALL_HEIGHT_M
    } else {
        WALL_HEIGHT_M
    };
    let mut parts = Vec::new();
    for (from, to) in &spans {
        let (middle, length) = ((from + to) * 0.5, to - from);
        if length <= 0.0 {
            continue;
        }
        parts.push(place(
            middle,
            LOW_WALL_HEIGHT_M * 0.5,
            length,
            LOW_WALL_HEIGHT_M,
            0.02,
            Tone::Stone,
        ));
        if !lowered {
            parts.push(place(
                middle,
                (LOW_WALL_HEIGHT_M + DOOR_HEIGHT_M) * 0.5,
                length,
                DOOR_HEIGHT_M - LOW_WALL_HEIGHT_M,
                0.0,
                Tone::Plaster,
            ));
        }
    }
    if !lowered {
        // The beam runs over the door as its lintel.
        parts.push(place(
            0.0,
            (DOOR_HEIGHT_M + WALL_HEIGHT_M) * 0.5,
            reach * 2.0,
            WALL_HEIGHT_M - DOOR_HEIGHT_M,
            TIMBER_PROUD_M,
            Tone::Timber,
        ));
    }
    let mut posts: Vec<(f32, f32, f32)> = Vec::new();
    let post_top = top;
    if lowered {
        // Down to the kerb: no stubs standing out of it.
    } else if across {
        // Corner posts.
        for end in [-1.0, 1.0] {
            posts.push((end * (reach - POST_M * 0.5), POST_M, post_top));
        }
    } else {
        posts.push((0.0, POST_M, post_top));
    }
    if wall == Wall::Front && !lowered {
        let jamb_top = DOOR_HEIGHT_M;
        for side in [-1.0, 1.0] {
            posts.push((
                door_x + side * (door_opening + JAMB_M) * 0.5,
                JAMB_M,
                jamb_top,
            ));
        }
    }
    for (run, along, tall) in posts {
        parts.push(place(
            run,
            tall * 0.5,
            along,
            tall,
            TIMBER_PROUD_M,
            Tone::Timber,
        ));
    }
    parts
}

/// A building is taken out of the way of the selected resident from this
/// zoom distance in; the played character's view is always kept clear.
pub(crate) const UNBLOCK_DISTANCE_M: f32 = 32.0;
/// Seconds a building stays out of the way after it last blocked the view,
/// so one at the edge of the line of sight does not flicker.
pub(crate) const UNBLOCK_LINGER_SECS: f32 = 0.35;
/// How far round the body the roof is taken to reach.
const BLOCK_MARGIN_M: f32 = 0.2;

/// Whether the straight line from `eye` to `target` passes through a
/// building: its body, a little wider for the eaves, from the `ground` it
/// stands on up to `height`.
pub(crate) fn blocks(frame: &BodyFrame, ground: f32, height: f32, eye: Vec3, target: Vec3) -> bool {
    let local = |point: Vec3| {
        let flat = frame.local_of(Vec2::new(point.x, point.z));
        Vec3::new(flat.x, point.y - ground - height * 0.5, flat.y)
    };
    let (from, to) = (local(eye), local(target));
    let half = Vec3::new(
        frame.width * 0.5 + BLOCK_MARGIN_M,
        height * 0.5,
        frame.depth * 0.5 + BLOCK_MARGIN_M,
    );
    // The stretch of the line inside all three slabs of the box.
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    for axis in 0..3 {
        let (start, run) = (from[axis], to[axis] - from[axis]);
        if run.abs() < 1e-6 {
            if start.abs() > half[axis] {
                return false;
            }
            continue;
        }
        let (a, b) = ((-half[axis] - start) / run, (half[axis] - start) / run);
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
        if enter > leave {
            return false;
        }
    }
    true
}

/// Real seconds a blocking building or tree takes to sink to its footing,
/// and to rise again.
pub(crate) const UNBLOCK_EASE_SECS: f32 = 0.2;
/// Real seconds a cut-away home's roof and walls take to sink away, and
/// to rise again when it closes: as a building taken out of the way.
pub(crate) const CUTAWAY_EASE_SECS: f32 = UNBLOCK_EASE_SECS;
/// What is left standing of a building or a tree taken out of the way.
const SQUASHED: f32 = 0.04;

/// The vertical scale of something `cut` (0 to 1) of the way out of the
/// way, eased in and out.
pub(crate) fn squash(cut: f32) -> f32 {
    let t = cut.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    1.0 - (1.0 - SQUASHED) * eased
}

/// A tree's silhouette, in metres over its root: a trunk, then a crown
/// tapering from `base_radius` at `base` to `top_radius` at `top`. Measured
/// on the living kit's tree models (see [`TreeShape::of_model`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TreeShape {
    pub(crate) trunk: f32,
    pub(crate) base: f32,
    pub(crate) base_radius: f32,
    pub(crate) top_radius: f32,
    pub(crate) top: f32,
}

impl TreeShape {
    /// The shape of tree model `index` of `living_scene::FOREST` (in model
    /// units: crown radius against height read off the meshes, or their
    /// bounds for the later ones) drawn at `scale`.
    pub(crate) fn of_model(index: usize, scale: f32) -> TreeShape {
        let [trunk, base, base_radius, top_radius, top] = match index {
            0 => [0.09, 0.2, 0.30, 0.02, 1.1],
            1 => [0.1, 0.15, 0.36, 0.07, 1.1],
            2 => [0.1, 0.1, 0.79, 0.55, 1.1],
            3 => [0.1, 0.1, 0.75, 0.55, 0.95],
            // The kit's medium clusters, from their bounds.
            4 => [0.1, 0.1, 0.86, 0.5, 1.3],
            5 => [0.1, 0.1, 0.9, 0.5, 1.18],
            // Kenney's pines (tall, default, round), oak, broadleaf, thin.
            6 => [0.03, 0.25, 0.19, 0.02, 1.53],
            7 => [0.04, 0.2, 0.26, 0.02, 1.55],
            8 => [0.04, 0.2, 0.33, 0.05, 1.37],
            9 => [0.05, 0.35, 0.36, 0.25, 1.23],
            10 => [0.05, 0.4, 0.37, 0.15, 1.71],
            _ => [0.04, 0.4, 0.32, 0.1, 1.49],
        };
        TreeShape {
            trunk: trunk * scale,
            base: base * scale,
            base_radius: base_radius * scale,
            top_radius: top_radius * scale,
            top: top * scale,
        }
    }

    /// The widest the tree is, for a quick test before the real one.
    pub(crate) fn reach(&self) -> f32 {
        self.base_radius.max(self.top_radius).max(self.trunk)
    }

    /// How far from its upright the tree reaches at `height` over its root;
    /// `None` above it or below its root.
    fn radius_at(&self, height: f32) -> Option<f32> {
        if !(0.0..=self.top).contains(&height) {
            return None;
        }
        if height < self.base {
            return Some(self.trunk);
        }
        let t = (height - self.base) / (self.top - self.base).max(1e-3);
        Some(self.base_radius + (self.top_radius - self.base_radius) * t)
    }
}

/// How far outside a tree's silhouette the line of sight must pass for the
/// tree to stay: about the half width of the person looked at.
pub(crate) const TREE_MARGIN_M: f32 = 0.5;

/// Whether the line from `eye` to `target` passes through a tree's trunk or
/// crown (`shape` over its root at `root`), with [`TREE_MARGIN_M`] round it.
pub(crate) fn blocks_tree(root: Vec3, shape: &TreeShape, eye: Vec3, target: Vec3) -> bool {
    // The stretch of the line between the tree's root and its top.
    let run = target - eye;
    let (mut from, mut to) = (0.0_f32, 1.0_f32);
    if run.y.abs() < 1e-6 {
        if !(root.y..=root.y + shape.top).contains(&eye.y) {
            return false;
        }
    } else {
        let (a, b) = (
            (root.y - eye.y) / run.y,
            (root.y + shape.top - eye.y) / run.y,
        );
        from = from.max(a.min(b));
        to = to.min(a.max(b));
        if from > to {
            return false;
        }
    }
    const SAMPLES: usize = 32;
    (0..=SAMPLES).any(|step| {
        let point = eye + run * (from + (to - from) * step as f32 / SAMPLES as f32);
        shape.radius_at(point.y - root.y).is_some_and(|radius| {
            Vec2::new(point.x - root.x, point.z - root.z).length() < radius + TREE_MARGIN_M
        })
    })
}

/// Homes cut away at once.
pub(crate) const MAX_CUTAWAYS: usize = 3;
/// The camera opens the home under its focus from this zoom distance in,
/// and keeps it open until it is this far out again.
pub(crate) const CLOSE_DISTANCE_M: f32 = 15.0;
pub(crate) const CLOSE_RELEASE_M: f32 = 18.0;
/// How far outside a body the focus may be and still open it.
pub(crate) const CLOSE_MARGIN_M: f32 = 1.2;

/// What asks for a cutaway this frame, each already resolved to a building.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Signals {
    /// The building whose card is open.
    pub(crate) card: Option<SimId>,
    /// The selected building, or the building the selected resident is in.
    pub(crate) selected: Option<SimId>,
    /// The building the played character is in, while living as it.
    pub(crate) played: Option<SimId>,
    /// The building under a close camera.
    pub(crate) near: Option<SimId>,
    /// Homes standing between the camera and whoever is followed: cut
    /// away instead of squashed, so the room stays.
    pub(crate) blocking: [Option<SimId>; 2],
}

/// The buildings to cut away, most wanted first: the played character's,
/// the selection's, the open card's, then the one under a close camera.
/// `shown` says whether a building has an inside the fog shows live.
pub(crate) fn wanted(signals: Signals, shown: impl Fn(SimId) -> bool) -> Vec<SimId> {
    let mut out = Vec::new();
    for id in [signals.played, signals.selected, signals.card]
        .into_iter()
        .chain(signals.blocking)
        .chain([signals.near])
        .flatten()
    {
        if !out.contains(&id) && shown(id) {
            out.push(id);
        }
    }
    out.truncate(MAX_CUTAWAYS);
    out
}

/// The building a close camera opens: the nearest body the focus is on or
/// just beside. `open` is the one this rule already holds open.
pub(crate) fn near_home(
    distance: f32,
    focus: Vec2,
    open: Option<SimId>,
    bodies: impl IntoIterator<Item = (SimId, BodyFrame)>,
) -> Option<SimId> {
    if distance > CLOSE_RELEASE_M {
        return None;
    }
    let mut best: Option<(f32, SimId)> = None;
    for (id, body) in bodies {
        let held = open == Some(id);
        if distance > CLOSE_DISTANCE_M && !held {
            continue;
        }
        let margin = if held {
            CLOSE_MARGIN_M + 0.8
        } else {
            CLOSE_MARGIN_M
        };
        if !body.contains(focus, margin) {
            continue;
        }
        // The one already open wins a tie, then the lower identity.
        let reach = body.centre.distance_squared(focus) - if held { 1.0 } else { 0.0 };
        if best.is_none_or(|(least, other)| reach < least || (reach == least && id < other)) {
            best = Some((reach, id));
        }
    }
    best.map(|(_, id)| id)
}

/// Real seconds a resident takes from the entrance into the doorway, and
/// from the doorway back out.
pub(crate) const ENTER_SECS: f32 = 0.45;
pub(crate) const EXIT_SECS: f32 = 0.35;

/// Where a resident is between outdoors and indoors, as the view tracks it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    Outside,
    /// Walking the last step into the doorway, for this long so far.
    Entering(f32),
    Inside,
    /// Stepping out of the doorway, for this long so far.
    Leaving(f32),
}

impl Phase {
    /// A resident seen for the first time is where the simulation says,
    /// without a step in or out.
    pub(crate) fn first(inside: bool) -> Self {
        if inside { Self::Inside } else { Self::Outside }
    }

    /// One frame on. `inside` is what the simulation says now.
    pub(crate) fn advance(self, inside: bool, delta: f32) -> Self {
        match (self, inside) {
            (Self::Outside, true) => Self::Entering(0.0),
            (Self::Outside, false) | (Self::Inside, true) => self,
            (Self::Entering(spent), true) => {
                if spent + delta >= ENTER_SECS {
                    Self::Inside
                } else {
                    Self::Entering(spent + delta)
                }
            }
            // Turned round in the doorway: go back from where they got to.
            (Self::Entering(spent), false) => {
                Self::Leaving((1.0 - spent / ENTER_SECS).clamp(0.0, 1.0) * EXIT_SECS)
            }
            (Self::Inside, false) => Self::Leaving(0.0),
            (Self::Leaving(spent), false) => {
                if spent + delta >= EXIT_SECS {
                    Self::Outside
                } else {
                    Self::Leaving(spent + delta)
                }
            }
            (Self::Leaving(spent), true) => {
                Self::Entering((1.0 - spent / EXIT_SECS).clamp(0.0, 1.0) * ENTER_SECS)
            }
        }
    }
}

/// How a resident is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Drawn {
    /// Where the simulation has them.
    Outdoors,
    /// Between the entrance (0) and the doorway (1).
    Step(f32),
    /// Indoors and not drawn.
    Hidden,
    /// Indoors, in a cut-away home: in bed or on the floor.
    Indoors,
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How a resident in `phase` is drawn. `interior` says the building has an
/// inside the view can show, `cut_away` that it is shown now. A resident
/// inside a building without one (a workplace) is hidden unless `focused`:
/// selected, or the played character, who stays at the door.
pub(crate) fn drawn(phase: Phase, interior: bool, cut_away: bool, focused: bool) -> Drawn {
    match phase {
        Phase::Outside => Drawn::Outdoors,
        Phase::Entering(spent) => Drawn::Step(ease(spent / ENTER_SECS)),
        Phase::Leaving(spent) => Drawn::Step(1.0 - ease(spent / EXIT_SECS)),
        Phase::Inside if interior && cut_away => Drawn::Indoors,
        Phase::Inside if !interior && focused => Drawn::Outdoors,
        Phase::Inside => Drawn::Hidden,
    }
}

/// Light of the hearth inside a cut-away home, lumens: a fire when the home
/// is heated, banked embers and a candle otherwise, so sleepers show at
/// night. `lamp` is the lamp level, 0 by day and 1 at night.
pub(crate) fn hearth_lumens(heated: bool, lamp: f32) -> f32 {
    let lamp = lamp.clamp(0.0, 1.0);
    if heated {
        5_000.0 + 13_000.0 * lamp
    } else {
        3_500.0 * lamp
    }
}
