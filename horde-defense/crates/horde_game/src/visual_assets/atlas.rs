//! Atlas geometry: measured sheet sizes, cell rectangles and the cut icon grid.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct RigSheet {
    pub texture: TextureId,
    pub atlas: RigAtlas,
}
/// Icon cells cut from the legacy defender rig sheet: 4..=9 firearms, 10..=13 ordnance.
/// The body cells (0..=3) are retained only because the grid is cut uniformly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigAtlas {
    pub(super) size: [f32; 2],
    pub(super) cells: [[f32; 4]; 16],
}
impl RigAtlas {
    pub(super) fn from_image(image: &Image) -> Self {
        let width = image.width();
        let height = image.height();
        let pixels = image
            .data
            .as_deref()
            .expect("RGBA rig pixels remain available");
        // The authored rows are deliberately unequal: standing boots extend
        // farther than the isolated guns. Equal quarters would amputate feet.
        let rows = [0., 0.34, 0.51, 0.75, 1.];
        let cells = std::array::from_fn(|index| {
            let left = index as u32 % 4 * width / 4;
            let right = (index as u32 % 4 + 1) * width / 4;
            let top = (rows[index / 4] * height as f32) as u32;
            let bottom = (rows[index / 4 + 1] * height as f32) as u32;
            let mut bounds = [right, bottom, left, top];
            for y in top..bottom {
                for x in left..right {
                    if pixels[((y * width + x) * 4 + 3) as usize] > 8 {
                        bounds[0] = bounds[0].min(x);
                        bounds[1] = bounds[1].min(y);
                        bounds[2] = bounds[2].max(x + 1);
                        bounds[3] = bounds[3].max(y + 1);
                    }
                }
            }
            if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
                [left as f32, top as f32, right as f32, bottom as f32]
            } else {
                [
                    bounds[0].saturating_sub(2).max(left) as f32,
                    bounds[1].saturating_sub(2).max(top) as f32,
                    (bounds[2] + 2).min(right) as f32,
                    (bounds[3] + 2).min(bottom) as f32,
                ]
            }
        });
        Self {
            size: [width as f32, height as f32],
            cells,
        }
    }
    /// Uniform 4x4 cells over a `size` sheet: a stand-in rig sheet for
    /// headless painter tests that need the ordnance icons textured.
    #[cfg(test)]
    pub(crate) fn grid(size: [f32; 2]) -> Self {
        let [width, height] = size;
        Self {
            size,
            cells: std::array::from_fn(|index| {
                let (column, row) = ((index % 4) as f32, (index / 4) as f32);
                [
                    column * width / 4.,
                    row * height / 4.,
                    (column + 1.) * width / 4.,
                    (row + 1.) * height / 4.,
                ]
            }),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WalkSheet {
    pub texture: TextureId,
    pub size: [u32; 2],
}

// Metadata uses untouched source pixels; final values are measured from each
// generated sheet's alpha, with transparent padding around every silhouette.
// Alpha <=8 is only the generator's invisible fringe/noise, not figure content;
// all more-opaque hands, boots and equipment are enclosed with 3-4px padding.
// Hips are the median solid pixels in the 51-64% torso band. Each gait keeps
// its row's deepest boot baseline; passing feet are never pulled to the floor.
/// Survivor sheet size; startup requires `enemy_visuals.json` to declare the same.
pub(crate) const ACTOR_SIZE: [f32; 2] = [1774., 887.];
pub(super) const PROP_SIZE: [f32; 2] = [1261., 1247.];
pub(super) const GROUND_SIZE: [f32; 2] = [2172., 724.];
pub(super) const KEYART_RATIO: f32 = 1672. / 941.;
// Enemy cells, anchors and gaits live in `assets/art/enemy_visuals.json`.
pub(super) const SURVIVOR_RECTS: [[f32; 4]; 2] = [[72., 47., 396., 430.], [506., 58., 826., 432.]];
// Solid torso median, deepest boot and occupied height for each standing survivor.
pub(super) const SURVIVOR_ANCHORS: [[f32; 3]; 2] = [[195., 427., 377.], [622., 429., 368.]];
pub(super) const PORTRAIT_RECTS: [[f32; 4]; 2] =
    [[496., 513., 816., 837.], [925., 517., 1257., 835.]];
pub(super) const CORPSE_RECT: [f32; 4] = [1342., 619., 1768., 758.];
pub(super) const PROP_RECTS: [[f32; 4]; 16] = [
    [25., 54., 295., 287.],
    [325., 89., 613., 277.],
    [658., 57., 933., 291.],
    [951., 110., 1243., 241.],
    [38., 343., 292., 605.],
    [341., 340., 595., 600.],
    [644., 341., 919., 600.],
    [998., 395., 1202., 564.],
    [15., 628., 305., 915.],
    [362., 624., 580., 926.],
    [649., 734., 907., 901.],
    [952., 677., 1247., 902.],
    [12., 969., 322., 1222.],
    [336., 994., 612., 1190.],
    [644., 984., 935., 1197.],
    [1006., 925., 1205., 1233.],
];
pub(super) fn region_for(px: [f32; 4], size: [f32; 2]) -> Rect {
    Rect::from_min_max(
        pos2(px[0] / size[0], px[1] / size[1]),
        pos2(px[2] / size[0], px[3] / size[1]),
    )
}
pub(super) fn region(px: [f32; 4]) -> Rect {
    region_for(px, PROP_SIZE)
}
pub(super) fn fit(center: Pos2, size: Vec2, source: [f32; 4]) -> Rect {
    let ratio = (source[2] - source[0]) / (source[3] - source[1]);
    let height = size.y.min(size.x / ratio);
    Rect::from_center_size(center, vec2(height * ratio, height))
}
