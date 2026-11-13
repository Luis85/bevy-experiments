//! Interface icons, looked up by concept.
//!
//! Callers name what they mean ([`Icon::Planks`], [`Icon::Morale`]) and get a
//! tinted node; whether it comes from the Maki or Temaki atlas or is drawn
//! from a few nodes is this module's business. The two atlases are embedded,
//! white on transparent, so a packaged game needs no loose files and any
//! token colour can tint them. Sources, licences and the atlas cell of every
//! file: `assets/ui-icons-manifest.json` (written by `scripts/fetch_ui_icons.py`).
use super::tokens::Status;
use super::widgets::{Role, Tone};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::ui::UiScale;
use bevy::window::PrimaryWindow;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// Atlas cells in order, 10 to a row: `set/file` without `.svg`.
pub const ATLAS: [&str; 92] = [
    "maki/aquarium",
    "maki/arrow",
    "maki/bakery",
    "maki/beer",
    "maki/caution",
    "maki/circle",
    "maki/circle-stroked",
    "maki/clothing-store",
    "maki/cross",
    "maki/diamond",
    "maki/farm",
    "maki/fence",
    "maki/ferry",
    "maki/garden",
    "maki/harbor",
    "maki/hardware",
    "maki/heart",
    "maki/home",
    "maki/hospital",
    "maki/information",
    "maki/library",
    "maki/lodging",
    "maki/logging",
    "maki/marker",
    "maki/mountain",
    "maki/paint",
    "maki/park",
    "maki/restaurant",
    "maki/roadblock",
    "maki/shoe",
    "maki/shop",
    "maki/slaughterhouse",
    "maki/square",
    "maki/star",
    "maki/triangle",
    "maki/viewpoint",
    "maki/village",
    "maki/warehouse",
    "maki/waste-basket",
    "maki/water",
    "maki/windmill",
    "temaki/anvil",
    "temaki/anvil_and_hammer",
    "temaki/barn",
    "temaki/binoculars",
    "temaki/boulder1",
    "temaki/bow_and_arrow",
    "temaki/bread",
    "temaki/brick_trowel",
    "temaki/briefcase",
    "temaki/bulb",
    "temaki/bulletin_board",
    "temaki/cabin",
    "temaki/campfire",
    "temaki/clock",
    "temaki/compass",
    "temaki/dagger",
    "temaki/fireplace",
    "temaki/fishing_pier",
    "temaki/garden_bed",
    "temaki/grapes",
    "temaki/grass",
    "temaki/handbag",
    "temaki/hotpot",
    "temaki/ice_fishing",
    "temaki/island_trees_building",
    "temaki/lock",
    "temaki/meat",
    "temaki/mineshaft_cage",
    "temaki/money_hand",
    "temaki/needle_and_spool",
    "temaki/pedestrian",
    "temaki/pick_hammer",
    "temaki/plant",
    "temaki/portrait",
    "temaki/room",
    "temaki/row_houses",
    "temaki/scaffold",
    "temaki/silo",
    "temaki/sleep_shelter",
    "temaki/snow",
    "temaki/speaker",
    "temaki/spice_bottle",
    "temaki/storage",
    "temaki/tools",
    "temaki/toolbox",
    "temaki/town_hall",
    "temaki/tree_needleleaved",
    "temaki/tree_stump",
    "temaki/vase",
    "temaki/well_pump_manual",
    "temaki/windsock",
];
pub const ATLAS_COLUMNS: usize = 10;

/// The glyphs with no suitable pictogram, drawn from plain nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Drawn {
    Pause,
    Menu,
    Tick,
    Coin,
    Sun,
    Cloud,
    Fog,
    Flatten,
    Smooth,
}

impl Drawn {
    pub const ALL: [Self; 9] = [
        Self::Pause,
        Self::Menu,
        Self::Tick,
        Self::Coin,
        Self::Sun,
        Self::Cloud,
        Self::Fog,
        Self::Flatten,
        Self::Smooth,
    ];
}

/// Where a concept's picture comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    /// A cell of the atlas, by `set/file`.
    Atlas(&'static str),
    Drawn(Drawn),
    /// A cloud with a small caution triangle.
    Storm,
}

/// One concept's picture: its source, a clockwise turn and a mirror.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub source: Source,
    pub turn: f32,
    pub flip: bool,
    /// May stand without a text label beside or beneath it.
    pub alone: bool,
}

const fn atlas(name: &'static str) -> Glyph {
    Glyph {
        source: Source::Atlas(name),
        turn: 0.0,
        flip: false,
        alone: true,
    }
}
/// A closest-match glyph: always shown with its name.
const fn labelled(name: &'static str) -> Glyph {
    Glyph {
        alone: false,
        ..atlas(name)
    }
}
const fn turned(name: &'static str, turn: f32) -> Glyph {
    Glyph {
        turn,
        ..atlas(name)
    }
}
const fn drawn(kind: Drawn) -> Glyph {
    Glyph {
        source: Source::Drawn(kind),
        turn: 0.0,
        flip: false,
        alone: true,
    }
}

macro_rules! icons {
    ($($name:ident => $glyph:expr,)*) => {
        /// Every concept the interface has a picture for.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Icon { $($name,)* }
        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$name,)*];
            pub fn glyph(self) -> Glyph {
                match self { $(Icon::$name => $glyph,)* }
            }
            pub fn name(self) -> &'static str {
                match self { $(Icon::$name => stringify!($name),)* }
            }
        }
    };
}

icons! {
    // Goods.
    Bread => atlas("temaki/bread"),
    CookedMeals => labelled("temaki/hotpot"),
    Berries => labelled("temaki/grapes"),
    Grain => labelled("temaki/plant"),
    Flour => labelled("temaki/spice_bottle"),
    Fish => atlas("maki/aquarium"),
    RawMeat => atlas("temaki/meat"),
    CleanWater => atlas("maki/water"),
    Logs => atlas("temaki/tree_stump"),
    Planks => labelled("maki/fence"),
    Ore => labelled("temaki/boulder1"),
    Ingots => labelled("temaki/anvil"),
    Fiber => labelled("temaki/grass"),
    Thread => labelled("temaki/needle_and_spool"),
    Hides => labelled("maki/slaughterhouse"),
    Leather => labelled("maki/shoe"),
    WarmClothes => atlas("maki/clothing-store"),
    Bowls => labelled("temaki/vase"),
    Axes => labelled("maki/logging"),
    Pickaxes => labelled("temaki/pick_hammer"),
    Hammers => labelled("temaki/tools"),
    Hoes => labelled("temaki/tools"),
    HuntingSpears => labelled("temaki/dagger"),
    FishingRods => labelled("temaki/ice_fishing"),
    Coins => drawn(Drawn::Coin),
    // Trades.
    Farmer => atlas("maki/farm"),
    Miller => atlas("maki/windmill"),
    Baker => atlas("maki/bakery"),
    WellKeeper => atlas("temaki/well_pump_manual"),
    Woodcutter => atlas("maki/logging"),
    Carpenter => atlas("temaki/tools"),
    Miner => atlas("temaki/pick_hammer"),
    Smith => atlas("temaki/anvil_and_hammer"),
    Innkeeper => atlas("maki/beer"),
    Hunter => atlas("temaki/bow_and_arrow"),
    Tanner => atlas("maki/shoe"),
    Tailor => atlas("temaki/needle_and_spool"),
    Fisher => atlas("temaki/fishing_pier"),
    // Buildings.
    Farmstead => atlas("maki/farm"),
    Field => atlas("temaki/garden_bed"),
    Mill => atlas("maki/windmill"),
    Bakery => atlas("maki/bakery"),
    Well => atlas("temaki/well_pump_manual"),
    TimberYard => atlas("maki/logging"),
    CarpenterWorkshop => atlas("temaki/tools"),
    MineHead => atlas("temaki/mineshaft_cage"),
    Smithy => atlas("temaki/anvil_and_hammer"),
    Inn => atlas("maki/beer"),
    HuntingLodge => atlas("temaki/bow_and_arrow"),
    Tannery => atlas("maki/shoe"),
    TailorShop => atlas("temaki/needle_and_spool"),
    FishingHut => atlas("temaki/fishing_pier"),
    Cottage => atlas("temaki/cabin"),
    House => atlas("maki/home"),
    Stockpile => atlas("temaki/storage"),
    Barn => atlas("temaki/barn"),
    Granary => atlas("temaki/silo"),
    MarketStall => atlas("maki/shop"),
    TradingPost => atlas("maki/harbor"),
    BuildingSite => atlas("temaki/scaffold"),
    // Needs and totals.
    Health => atlas("maki/hospital"),
    Food => atlas("maki/restaurant"),
    Water => atlas("maki/water"),
    Rest => atlas("maki/lodging"),
    Warmth => atlas("temaki/campfire"),
    LitHearth => atlas("temaki/fireplace"),
    Morale => atlas("maki/heart"),
    Residents => atlas("temaki/pedestrian"),
    Resident => atlas("temaki/portrait"),
    Households => atlas("temaki/row_houses"),
    Homeless => atlas("temaki/sleep_shelter"),
    Jobs => atlas("temaki/briefcase"),
    CivicFund => atlas("temaki/town_hall"),
    // Modes, navigation, build categories.
    Live => atlas("temaki/pedestrian"),
    Oversee => atlas("temaki/binoculars"),
    Follow => atlas("temaki/binoculars"),
    Shape => atlas("maki/mountain"),
    Homes => atlas("maki/home"),
    Work => atlas("temaki/anvil_and_hammer"),
    Storage => atlas("maki/warehouse"),
    Market => atlas("maki/shop"),
    Farms => atlas("temaki/garden_bed"),
    Ledger => atlas("maki/library"),
    Guide => atlas("maki/library"),
    VillagePage => atlas("maki/village"),
    EconomyPage => atlas("temaki/storage"),
    PeoplePage => atlas("temaki/portrait"),
    ProjectsPage => atlas("temaki/scaffold"),
    JournalPage => atlas("temaki/bulletin_board"),
    Overlays => atlas("maki/viewpoint"),
    Places => atlas("maki/marker"),
    Locate => atlas("maki/marker"),
    WholeIsland => atlas("temaki/island_trees_building"),
    ResourcePatches => atlas("temaki/tree_needleleaved"),
    HouseInterior => atlas("temaki/room"),
    MerchantBoat => atlas("maki/ferry"),
    // Live workspaces.
    Pouch => atlas("temaki/handbag"),
    Craft => atlas("temaki/toolbox"),
    Build => atlas("temaki/brick_trowel"),
    Garden => atlas("temaki/garden_bed"),
    Trade => atlas("temaki/money_hand"),
    People => atlas("temaki/portrait"),
    Explore => atlas("temaki/compass"),
    Stop => atlas("maki/roadblock"),
    Goal => atlas("temaki/bulb"),
    // Status shapes. Colour never speaks alone: these carry the meaning.
    Fine => atlas("maki/circle"),
    Low => atlas("maki/triangle"),
    Caution => atlas("maki/caution"),
    Danger => atlas("maki/diamond"),
    Note => atlas("maki/square"),
    Hint => atlas("maki/information"),
    Unchanged => atlas("maki/circle-stroked"),
    // The arrow points right as drawn.
    TrendUp => turned("maki/arrow", -FRAC_PI_4),
    TrendDown => turned("maki/arrow", FRAC_PI_4),
    SortUp => turned("maki/arrow", -FRAC_PI_2),
    SortDown => turned("maki/arrow", FRAC_PI_2),
    GoTo => atlas("maki/arrow"),
    Back => turned("maki/arrow", PI),
    Raise => turned("maki/arrow", -FRAC_PI_2),
    Lower => turned("maki/arrow", FRAC_PI_2),
    Undo => Glyph { flip: true, ..turned("maki/arrow", FRAC_PI_4) },
    Redo => turned("maki/arrow", -FRAC_PI_4),
    // The triangle points up as drawn.
    Collapsed => turned("maki/triangle", FRAC_PI_2),
    Expanded => turned("maki/triangle", PI),
    Play => turned("maki/triangle", FRAC_PI_2),
    Close => atlas("maki/cross"),
    Cannot => atlas("maki/cross"),
    GoalReached => atlas("maki/star"),
    Done => drawn(Drawn::Tick),
    // Weather, time, actions.
    Clear => drawn(Drawn::Sun),
    Rain => atlas("maki/water"),
    Storm => Glyph { source: Source::Storm, turn: 0.0, flip: false, alone: true },
    Snow => atlas("temaki/snow"),
    Cloudy => drawn(Drawn::Cloud),
    Fog => drawn(Drawn::Fog),
    Wind => atlas("temaki/windsock"),
    Spring => atlas("maki/garden"),
    Summer => drawn(Drawn::Sun),
    Autumn => atlas("maki/park"),
    Winter => atlas("temaki/snow"),
    Time => atlas("temaki/clock"),
    Settings => atlas("maki/hardware"),
    Sound => atlas("temaki/speaker"),
    Abandon => atlas("maki/waste-basket"),
    Locked => atlas("temaki/lock"),
    Paint => atlas("maki/paint"),
    Pause => drawn(Drawn::Pause),
    Menu => drawn(Drawn::Menu),
    Flatten => Glyph { alone: false, ..drawn(Drawn::Flatten) },
    Smooth => Glyph { alone: false, ..drawn(Drawn::Smooth) },
}

impl Icon {
    /// The status shape: circle, triangle, diamond, square or ring.
    pub const fn status(status: Status) -> Self {
        match status {
            Status::Good => Self::Fine,
            Status::Warn => Self::Low,
            Status::Crit => Self::Danger,
            Status::Info => Self::Note,
            Status::Idle => Self::Unchanged,
        }
    }

    /// The icon of a good by its catalogue key, where one is mapped.
    pub fn of_good(kind: &str) -> Option<Self> {
        let kind = kind.strip_prefix("village_").unwrap_or(kind);
        Some(match kind {
            "bread" => Self::Bread,
            "meal" | "cooked_meal" | "stew" => Self::CookedMeals,
            "berries" | "berry" => Self::Berries,
            "grain" => Self::Grain,
            "flour" => Self::Flour,
            "fish" => Self::Fish,
            "meat" | "raw_meat" => Self::RawMeat,
            "water" | "clean_water" => Self::CleanWater,
            "log" => Self::Logs,
            "plank" => Self::Planks,
            "ore" => Self::Ore,
            "ingot" => Self::Ingots,
            "fiber" => Self::Fiber,
            "thread" => Self::Thread,
            "hide" => Self::Hides,
            "leather" => Self::Leather,
            "coat" | "warm_clothes" | "woven_coat" => Self::WarmClothes,
            "bowl" => Self::Bowls,
            "axe" => Self::Axes,
            "pickaxe" => Self::Pickaxes,
            "hammer" => Self::Hammers,
            "hoe" => Self::Hoes,
            "spear" | "hunting_spear" => Self::HuntingSpears,
            "fishing_rod" | "rod" => Self::FishingRods,
            "coin" | "coins" => Self::Coins,
            _ => return None,
        })
    }

    /// The icon of a trade by its role name ("well keeper").
    pub fn of_trade(role: &str) -> Option<Self> {
        Some(match role.to_ascii_lowercase().replace('_', " ").as_str() {
            "farmer" => Self::Farmer,
            "miller" => Self::Miller,
            "baker" => Self::Baker,
            "well keeper" | "wellkeeper" => Self::WellKeeper,
            "woodcutter" => Self::Woodcutter,
            "carpenter" => Self::Carpenter,
            "miner" => Self::Miner,
            "smith" | "blacksmith" => Self::Smith,
            "innkeeper" => Self::Innkeeper,
            "hunter" => Self::Hunter,
            "tanner" => Self::Tanner,
            "tailor" => Self::Tailor,
            "fisher" => Self::Fisher,
            _ => return None,
        })
    }

    /// The icon of a season by its calendar name.
    pub fn of_season(season: &str) -> Option<Self> {
        Some(match season {
            "Spring" => Self::Spring,
            "Summer" => Self::Summer,
            "Autumn" => Self::Autumn,
            "Winter" => Self::Winter,
            _ => return None,
        })
    }
}

/// Cell index of an atlas file, by `set/file`.
pub fn cell(name: &str) -> Option<usize> {
    ATLAS.iter().position(|entry| *entry == name)
}

fn rect(cell: usize, size: f32) -> Rect {
    let (column, row) = ((cell % ATLAS_COLUMNS) as f32, (cell / ATLAS_COLUMNS) as f32);
    Rect::new(
        column * size,
        row * size,
        (column + 1.0) * size,
        (row + 1.0) * size,
    )
}

/// Which raster a size is drawn from: the 32 px cells up to 32 physical
/// pixels on screen, the 64 px cells above.
pub fn atlas_cell_size(size: f32, physical_scale: f32) -> f32 {
    if size * physical_scale <= 32.0 {
        32.0
    } else {
        64.0
    }
}

#[derive(Resource, Clone)]
pub struct IconAtlases {
    small: Handle<Image>,
    large: Handle<Image>,
}

/// An atlas-drawn icon: enough to pick its raster again when the scale changes.
#[derive(Component, Clone, Copy, Debug)]
pub struct IconSprite {
    cell: usize,
    size: f32,
}

/// A node of a code-drawn glyph that takes the icon's tint.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphPart {
    Fill,
    Ring,
}

fn decode(bytes: &[u8]) -> Option<Image> {
    Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .ok()
}

/// Load the embedded atlases once. Without an image store (headless tests)
/// icons stay empty nodes; no layout depends on an icon being present.
fn atlases(world: &mut World) -> Option<IconAtlases> {
    if let Some(atlases) = world.get_resource::<IconAtlases>() {
        return Some(atlases.clone());
    }
    let small = decode(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/ui/icons/icons-32.png"
    )))?;
    let large = decode(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/ui/icons/icons-64.png"
    )))?;
    let mut images = world.get_resource_mut::<Assets<Image>>()?;
    let atlases = IconAtlases {
        small: images.add(small),
        large: images.add(large),
    };
    world.insert_resource(atlases.clone());
    Some(atlases)
}

fn physical_scale(world: &mut World) -> f32 {
    let ui = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
    let window = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map_or(1.0, Window::scale_factor);
    ui * window
}

/// Spawn the icon of `concept` under `parent`, `size` logical pixels square,
/// tinted by `tone`. Callers do not know which kind of glyph they get.
pub fn icon(world: &mut World, parent: Entity, concept: Icon, size: f32, tone: Tone) -> Entity {
    let glyph = concept.glyph();
    let entity = world
        .spawn((
            ChildOf(parent),
            Name::new(format!("Icon: {}", concept.name())),
            tone,
            bevy::picking::Pickable::IGNORE,
            Node {
                width: px(size),
                height: px(size),
                min_width: px(size),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    match glyph.source {
        Source::Atlas(name) => raster(world, entity, name, size, glyph),
        Source::Drawn(kind) => draw(world, entity, kind, size),
        Source::Storm => {
            draw(world, entity, Drawn::Cloud, size);
            let badge = world
                .spawn((
                    ChildOf(entity),
                    Tone::new(tone.surface, Role::Status(Status::Warn)),
                    bevy::picking::Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(0),
                        bottom: px(0),
                        width: px(size * 0.5),
                        height: px(size * 0.5),
                        ..default()
                    },
                ))
                .id();
            raster(
                world,
                badge,
                "maki/caution",
                size * 0.5,
                atlas("maki/caution"),
            );
        }
    }
    entity
}

fn raster(world: &mut World, entity: Entity, name: &str, size: f32, glyph: Glyph) {
    let (Some(cell), Some(atlases)) = (cell(name), atlases(world)) else {
        return;
    };
    let cells = atlas_cell_size(size, physical_scale(world));
    world.entity_mut(entity).insert((
        IconSprite { cell, size },
        ImageNode {
            image: if cells > 32.0 {
                atlases.large
            } else {
                atlases.small
            },
            rect: Some(rect(cell, cells)),
            flip_x: glyph.flip,
            ..default()
        },
    ));
    if glyph.turn != 0.0 {
        world.entity_mut(entity).insert(UiTransform {
            rotation: Rot2::radians(glyph.turn),
            ..UiTransform::IDENTITY
        });
    }
}

/// One plain node of a drawn glyph, in sixteenths of the icon's size.
fn part(
    world: &mut World,
    parent: Entity,
    unit: f32,
    [x, y, w, h]: [f32; 4],
    radius: f32,
    kind: GlyphPart,
) -> Entity {
    let mut node = Node {
        position_type: PositionType::Absolute,
        left: px(x * unit),
        top: px(y * unit),
        width: px(w * unit),
        height: px(h * unit),
        border_radius: BorderRadius::all(px(radius * unit)),
        ..default()
    };
    if kind == GlyphPart::Ring {
        node.border = UiRect::all(px((2.0 * unit).max(1.0)));
    }
    world
        .spawn((
            ChildOf(parent),
            kind,
            bevy::picking::Pickable::IGNORE,
            node,
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id()
}

/// A bar from one point to another, two sixteenths thick.
fn stroke(world: &mut World, parent: Entity, unit: f32, from: (f32, f32), to: (f32, f32)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt() + 1.0;
    let middle = ((from.0 + to.0) * 0.5, (from.1 + to.1) * 0.5);
    let bar = part(
        world,
        parent,
        unit,
        [middle.0 - length * 0.5, middle.1 - 1.0, length, 2.0],
        1.0,
        GlyphPart::Fill,
    );
    world.entity_mut(bar).insert(UiTransform {
        rotation: Rot2::radians(dy.atan2(dx)),
        ..UiTransform::IDENTITY
    });
}

/// Half of a ring: the upper or the lower arc of a wave.
fn arc(world: &mut World, parent: Entity, unit: f32, x: f32, y: f32, upper: bool) {
    let clip = world
        .spawn((
            ChildOf(parent),
            bevy::picking::Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(x * unit),
                top: px(y * unit),
                width: px(8.0 * unit),
                height: px(4.0 * unit),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    part(
        world,
        clip,
        unit,
        [0.0, if upper { 0.0 } else { -4.0 }, 8.0, 8.0],
        4.0,
        GlyphPart::Ring,
    );
}

fn draw(world: &mut World, entity: Entity, kind: Drawn, size: f32) {
    let unit = size / 16.0;
    let fill = GlyphPart::Fill;
    match kind {
        Drawn::Pause => {
            part(world, entity, unit, [4.0, 3.0, 3.0, 10.0], 0.5, fill);
            part(world, entity, unit, [9.0, 3.0, 3.0, 10.0], 0.5, fill);
        }
        Drawn::Menu => {
            for y in [3.0, 7.0, 11.0] {
                part(world, entity, unit, [2.0, y, 12.0, 2.0], 1.0, fill);
            }
        }
        Drawn::Tick => {
            stroke(world, entity, unit, (3.5, 8.5), (6.5, 11.5));
            stroke(world, entity, unit, (6.5, 11.5), (12.5, 4.5));
        }
        Drawn::Coin => {
            part(
                world,
                entity,
                unit,
                [2.0, 2.0, 12.0, 12.0],
                6.0,
                GlyphPart::Ring,
            );
            part(world, entity, unit, [6.5, 6.5, 3.0, 3.0], 1.5, fill);
        }
        Drawn::Sun => {
            part(world, entity, unit, [5.0, 5.0, 6.0, 6.0], 3.0, fill);
            for step in 0..8 {
                let angle = step as f32 * FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                stroke(
                    world,
                    entity,
                    unit,
                    (8.0 + cos * 5.5, 8.0 + sin * 5.5),
                    (8.0 + cos * 6.5, 8.0 + sin * 6.5),
                );
            }
        }
        Drawn::Cloud => {
            part(world, entity, unit, [1.5, 7.0, 6.0, 6.0], 3.0, fill);
            part(world, entity, unit, [4.5, 3.5, 7.5, 7.5], 3.75, fill);
            part(world, entity, unit, [9.0, 7.0, 5.5, 6.0], 3.0, fill);
            part(world, entity, unit, [4.0, 9.0, 8.0, 4.0], 1.0, fill);
        }
        Drawn::Fog => {
            part(world, entity, unit, [3.0, 3.5, 10.0, 2.0], 1.0, fill);
            part(world, entity, unit, [1.0, 7.0, 10.0, 2.0], 1.0, fill);
            part(world, entity, unit, [5.0, 10.5, 10.0, 2.0], 1.0, fill);
        }
        Drawn::Flatten => {
            // A bar under two small triangles.
            part(world, entity, unit, [2.0, 11.0, 12.0, 2.0], 1.0, fill);
            for x in [2.0, 8.5] {
                let peak = world
                    .spawn((
                        ChildOf(entity),
                        bevy::picking::Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(x * unit),
                            top: px(4.0 * unit),
                            width: px(5.5 * unit),
                            height: px(5.5 * unit),
                            ..default()
                        },
                    ))
                    .id();
                raster(
                    world,
                    peak,
                    "maki/triangle",
                    5.5 * unit,
                    atlas("maki/triangle"),
                );
                world.entity_mut(peak).insert(GlyphPart::Fill);
            }
        }
        Drawn::Smooth => {
            arc(world, entity, unit, 0.5, 4.0, true);
            arc(world, entity, unit, 7.5, 8.0, false);
        }
    }
}

/// Keep icons sharp: swap rasters when the interface or window scale changes.
pub(super) fn pick_raster(
    atlases: Option<Res<IconAtlases>>,
    scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last: Local<f32>,
    mut sprites: Query<(&IconSprite, &mut ImageNode)>,
) {
    let Some(atlases) = atlases else { return };
    let physical = scale.0 * windows.iter().next().map_or(1.0, Window::scale_factor);
    if *last == physical {
        return;
    }
    *last = physical;
    for (sprite, mut image) in &mut sprites {
        let cells = atlas_cell_size(sprite.size, physical);
        let wanted = if cells > 32.0 {
            &atlases.large
        } else {
            &atlases.small
        };
        if image.image != *wanted {
            image.image = wanted.clone();
            image.rect = Some(rect(sprite.cell, cells));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_ui::tokens::Surface;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn every_concept_resolves_to_a_file_that_exists_or_a_drawn_glyph() {
        let mut used = BTreeSet::new();
        let mut drawn = BTreeSet::new();
        for concept in Icon::ALL {
            match concept.glyph().source {
                Source::Atlas(name) => {
                    assert!(
                        cell(name).is_some(),
                        "{concept:?}: {name} is not in the atlas"
                    );
                    let path = assets().join("ui/icons/src").join(format!("{name}.svg"));
                    assert!(path.is_file(), "{concept:?}: {} is missing", path.display());
                    used.insert(name);
                }
                Source::Drawn(kind) => {
                    drawn.insert(format!("{kind:?}"));
                }
                Source::Storm => {
                    assert!(cell("maki/caution").is_some());
                }
            }
        }
        // All 92 fetched files are mapped, and all nine drawn glyphs are used.
        let unused: Vec<_> = ATLAS.iter().filter(|name| !used.contains(*name)).collect();
        assert!(unused.is_empty(), "unmapped atlas files: {unused:?}");
        assert_eq!(drawn.len(), Drawn::ALL.len());
        let names: BTreeSet<_> = Icon::ALL.iter().map(|icon| icon.name()).collect();
        assert_eq!(names.len(), Icon::ALL.len());
    }

    #[test]
    fn the_atlas_table_is_the_manifest() {
        let text = std::fs::read_to_string(assets().join("ui-icons-manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
        let cells = manifest["cells"].as_object().unwrap();
        assert_eq!(cells.len(), ATLAS.len());
        for (index, name) in ATLAS.iter().enumerate() {
            assert_eq!(cells[*name].as_u64(), Some(index as u64), "{name}");
        }
        for set in ["maki", "temaki"] {
            assert_eq!(manifest["sets"][set]["license"], "CC0-1.0");
            assert!(
                manifest["sets"][set]["license_line"]
                    .as_str()
                    .unwrap()
                    .contains("CC0 1.0 Universal")
            );
        }
        let sizes: Vec<(u64, u64, u64)> = manifest["atlases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|atlas| {
                (
                    atlas["cell"].as_u64().unwrap(),
                    atlas["width"].as_u64().unwrap(),
                    atlas["height"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(sizes, [(32, 320, 320), (64, 640, 640)]);
        assert_eq!(
            manifest["atlases"][0]["columns"].as_u64(),
            Some(ATLAS_COLUMNS as u64)
        );
        // Both licence texts are kept beside the other licences.
        for file in ["Maki-8.0.0-CC0.txt", "Temaki-5.13.0-CC0.txt"] {
            let text = std::fs::read_to_string(assets().join("licenses").join(file)).unwrap();
            assert!(text.contains("CC0 1.0 Universal"), "{file}");
        }
    }

    #[test]
    fn rasters_and_cells_follow_the_scale() {
        assert_eq!(atlas_cell_size(16.0, 1.0), 32.0);
        assert_eq!(atlas_cell_size(32.0, 1.0), 32.0);
        assert_eq!(atlas_cell_size(16.0, 2.0), 32.0);
        assert_eq!(atlas_cell_size(20.0, 2.0), 64.0);
        assert_eq!(atlas_cell_size(32.0, 2.5), 64.0);
        assert_eq!(rect(0, 32.0), Rect::new(0.0, 0.0, 32.0, 32.0));
        assert_eq!(rect(11, 32.0), Rect::new(32.0, 32.0, 64.0, 64.0));
        assert_eq!(rect(91, 64.0), Rect::new(64.0, 576.0, 128.0, 640.0));
    }

    #[test]
    fn concepts_look_up_by_good_season_and_status() {
        assert_eq!(Icon::of_good("village_plank"), Some(Icon::Planks));
        assert_eq!(Icon::of_good("log"), Some(Icon::Logs));
        assert_eq!(Icon::of_good("unknown_thing"), None);
        assert_eq!(Icon::of_season("Autumn"), Some(Icon::Autumn));
        assert_eq!(Icon::of_trade("Well keeper"), Some(Icon::WellKeeper));
        assert_eq!(Icon::of_trade("visitor"), None);
        assert_eq!(Icon::status(Status::Crit), Icon::Danger);
        // Closest-match glyphs are marked as needing their name.
        assert!(!Icon::Planks.glyph().alone && Icon::Bread.glyph().alone);
        // Without an image store an icon is an empty node of the right size.
        let mut world = World::new();
        let parent = world.spawn(Node::default()).id();
        let entity = icon(
            &mut world,
            parent,
            Icon::Planks,
            16.0,
            Tone::new(Surface::Chrome, Role::Text),
        );
        assert_eq!(world.get::<Node>(entity).unwrap().width, px(16.0));
        assert!(world.get::<ImageNode>(entity).is_none());
        let coin = icon(
            &mut world,
            parent,
            Icon::Coins,
            16.0,
            Tone::new(Surface::Chrome, Role::Text),
        );
        assert_eq!(world.get::<Children>(coin).unwrap().len(), 2);
    }
}
