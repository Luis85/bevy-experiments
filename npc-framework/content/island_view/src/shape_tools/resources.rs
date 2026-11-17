//! Shape's resources: place and remove resource patches, and export the
//! shaped world.
//!
//! Two tools beside the terrain brushes: Resource (N) places a patch of the
//! chosen good at the chosen richness where the ground is clicked, Remove (X)
//! takes away the patch under the pointer. Both are recorded commands
//! (`PlaceResourcePatch`, `RemoveResourcePatch`); the simulation's own check
//! is the verdict beside the cursor before the click, and its refusal is the
//! notice after it. The goods come from the world's patch list, never from a
//! list here. While either tool is in hand the existing patches carry a
//! marker and a label.
//!
//! Both take part in Undo and Redo (`placing::track`): the inverse of a
//! placement is the removal of the patch the simulation made, found again
//! by good and position; the inverse of a removal places the same good at
//! the same spot and richness again.
//!
//! "Export world" asks the host to write the world file
//! ([`WorldExportRequest`]); the view does no file work.
use super::ShapeTools;
use super::picking;
use super::placing::{Kind as PlaceKind, Thing};
use crate::IslandMode;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Tone};
use crate::island_shell::focus::FocusRegion;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use island_content::village::frontier::shape::PlaceMode;
use island_content::village::{
    ResourcePatch,
    frontier::{
        self, Settlement,
        shape::{PlaceResourcePatch, RemoveResourcePatch},
    },
};
use npc_framework::kernel::commands::{CommandQueue, SkippedCommands, submit_deferred};
use npc_framework::prelude::{SimCommand, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{ViewSystems, command_ui::DragBox};

/// Richness the stepper starts at and its bounds, in percent.
pub(crate) const RICHNESS_DEFAULT: u32 = 100;
const RICHNESS_STEP: u32 = 25;
const RICHNESS_MIN: u32 = 25;
const RICHNESS_MAX: u32 = 400;
/// The Remove tool finds a patch this near the pointer, in millimetres.
const PICK_MM: i64 = 2_000;
/// Labels drawn at most, nearest the camera's focus first.
const LABELS: usize = 40;
/// Width of the resources panel.
pub(crate) const PANEL_WIDTH: f32 = 248.0;

/// The two resource tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Place,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResAction {
    Mode(Mode),
    /// Back to the terrain brush in hand.
    Leave,
    Good(String),
    /// Steps of 25%, up or down.
    Richness(i32),
    Export,
}

/// The host writes the world file when this is set, and clears it.
#[derive(Resource, Default)]
pub struct WorldExportRequest(pub bool);

/// Whether "Export world" was chosen since the last call (the host's door).
pub fn take_world_export(world: &mut World) -> bool {
    world
        .get_resource_mut::<WorldExportRequest>()
        .is_some_and(|mut request| std::mem::take(&mut request.0))
}

#[derive(Resource)]
pub(crate) struct Resources {
    pub(crate) mode: Option<Mode>,
    pub(crate) good: Option<String>,
    pub(crate) richness: u32,
    /// The ground under the pointer, in millimetres.
    pub(crate) hover: Option<SimPos>,
    /// The patch under the pointer while removing: its id and its words.
    pub(crate) hovered: Option<(SimId, String)>,
    /// The simulation's answer for a click here now.
    pub(crate) verdict: Result<(), String>,
    pub(crate) requests: Vec<ResAction>,
    /// Recorded commands waiting for their tick, with what they would do.
    inflight: Vec<((u64, u32), String)>,
}

impl Default for Resources {
    fn default() -> Self {
        Self {
            mode: None,
            good: None,
            richness: RICHNESS_DEFAULT,
            hover: None,
            hovered: None,
            verdict: Ok(()),
            requests: Vec::new(),
            inflight: Vec::new(),
        }
    }
}

/// Whether a resource tool is in hand (then the terrain brush rests).
/// Patches placed or removed and not applied yet.
pub(crate) fn waiting(world: &World) -> usize {
    world
        .get_resource::<Resources>()
        .map_or(0, |state| state.inflight.len())
}

pub(crate) fn active(world: &World) -> bool {
    world
        .get_resource::<Resources>()
        .is_some_and(|state| state.mode.is_some())
}

/// The goods that grow in patches in this world, in the order the world's
/// data lists them, each once.
pub(crate) fn patch_goods(world: &World) -> Vec<String> {
    let mut goods: Vec<String> = Vec::new();
    if let Some(registry) = frontier::economy_spec::registry(world) {
        for patch in &registry.spec.patches {
            if !goods.contains(&patch.good) {
                goods.push(patch.good.clone());
            }
        }
    }
    goods
}

/// "Poor · 50%", "Normal · 100%", "Rich · 150%", "Abundant · 300%".
pub(crate) fn richness_words(richness: u32) -> String {
    let word = match richness {
        0..=75 => "Poor",
        76..=100 => "Normal",
        101..=200 => "Rich",
        _ => "Abundant",
    };
    format!("{word} \u{b7} {richness}%")
}

/// The richness after `steps` presses of + or −, inside 25 to 400.
pub(crate) fn step_richness(richness: u32, steps: i32) -> u32 {
    let next = richness as i64 + i64::from(steps) * i64::from(RICHNESS_STEP);
    (next.clamp(i64::from(RICHNESS_MIN), i64::from(RICHNESS_MAX)) as u32) / RICHNESS_STEP
        * RICHNESS_STEP
}

/// "Grain patch of Harbour" as "grain patch of Harbour": the first letter
/// only, so a village keeps its capital.
fn lower_first(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}

fn good_words(world: &World, kind: &str) -> String {
    let name = crate::island_build::good_name(world, kind);
    let mut letters = name.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

fn settlement_label(world: &World, entity: Entity) -> Option<String> {
    let settlement = frontier::settlements::member(world, entity)?;
    let place = world.resource::<SimIndex>().get(settlement)?;
    world.get::<Settlement>(place).map(|s| s.label.clone())
}

/// Every resource patch: id, entity, good and position, ascending by id.
fn patches(world: &World) -> Vec<(SimId, Entity, String, SimPos)> {
    let Some(index) = world.get_resource::<SimIndex>() else {
        return Vec::new();
    };
    let mut out: Vec<_> = index
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let patch = world.get::<ResourcePatch>(*entity)?;
            let pos = world.get::<SimPos>(*entity)?;
            Some((*id, *entity, patch.kind.clone(), *pos))
        })
        .collect();
    out.sort_unstable_by_key(|(id, ..)| *id);
    out
}

/// The patch of `good` standing at `(x, z)` millimetres, if any.
pub(crate) fn patch_at(world: &World, good: &str, x: i32, z: i32) -> Option<SimId> {
    patches(world)
        .into_iter()
        .find(|(_, _, kind, pos)| kind == good && (pos.x, pos.z) == (x, z))
        .map(|(id, ..)| id)
}

/// A placed patch's richness in percent: its capacity against the world's
/// own patch of that good, in the stepper's steps.
fn richness_of(world: &World, entity: Entity) -> u32 {
    let Some(patch) = world.get::<ResourcePatch>(entity) else {
        return RICHNESS_DEFAULT;
    };
    let authored = frontier::economy_spec::registry(world).and_then(|registry| {
        registry
            .spec
            .patches
            .iter()
            .find(|def| def.good == patch.kind)
            .map(|def| def.capacity)
    });
    match authored {
        Some(capacity) if capacity > 0 => {
            let percent = u64::from(patch.capacity) * 100 / u64::from(capacity);
            let stepped = (percent + u64::from(RICHNESS_STEP) / 2) / u64::from(RICHNESS_STEP)
                * u64::from(RICHNESS_STEP);
            (stepped as u32).clamp(RICHNESS_MIN, RICHNESS_MAX)
        }
        _ => RICHNESS_DEFAULT,
    }
}

/// The patch nearest `at` within reach of the Remove tool.
pub(crate) fn nearest(patches: &[(SimId, SimPos)], at: SimPos, reach_mm: i64) -> Option<SimId> {
    patches
        .iter()
        .map(|(id, pos)| (pos.dist2(at), *id))
        .filter(|(distance, _)| *distance <= reach_mm * reach_mm)
        .min()
        .map(|(_, id)| id)
}

#[derive(Component, Clone)]
struct ResourceControl(ResAction);
#[derive(Component)]
struct PanelRoot;
#[derive(Component)]
struct GoodsGrid;
#[derive(Component)]
struct RichnessText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct TagRoot;
#[derive(Component)]
struct TagLine;
#[derive(Component)]
struct TagReason;
#[derive(Component)]
struct PatchLabel(usize);
/// A patch chip's picture slot and text, and the good its picture shows.
#[derive(Component)]
struct ChipParts {
    picture: Entity,
    text: Entity,
    kind: Option<String>,
}
#[derive(Component)]
struct ChipIcon;
#[derive(Component)]
struct ChipText;

/// A chip's picture, and the chip's height, in UI pixels.
const CHIP_ICON: f32 = 14.0;
const CHIP_HEIGHT: f32 = 22.0;
/// Closer than this (metres from the camera), every chip names its good.
const NAMES_WITHIN_M: f32 = 45.0;
/// The pointer this close to a chip (UI pixels) names it.
const HOVER_PX: f32 = 28.0;

/// One patch that wants a chip: where on screen, its good and its words.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChipWant {
    pub(crate) at: Vec2,
    pub(crate) kind: String,
    pub(crate) name: String,
    /// The pointer is on it, or the camera is close: the name shows.
    pub(crate) named: bool,
    pub(crate) hovered: bool,
}

/// A chip as laid out: its place (top left), good, the words beside the
/// picture ("Logs", "+2", "Logs +1" or nothing) and its rectangle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChipPlaced {
    pub(crate) left_top: Vec2,
    pub(crate) kind: String,
    pub(crate) name: Option<String>,
    pub(crate) more: u32,
    pub(crate) rect: Rect,
}

impl ChipPlaced {
    pub(crate) fn words(&self) -> String {
        match (&self.name, self.more) {
            (Some(name), 0) => name.clone(),
            (Some(name), more) => format!("{name} +{more}"),
            (None, 0) => String::new(),
            (None, more) => format!("+{more}"),
        }
    }
}

fn chip_rect(at: Vec2, name: Option<&str>) -> Rect {
    // Picture, padding and, with a name, about 7 pixels a letter.
    let width =
        6.0 + CHIP_ICON + 6.0 + name.map_or(0.0, |name| 4.0 + 7.0 * name.chars().count() as f32);
    let min = Vec2::new(at.x - (6.0 + CHIP_ICON / 2.0), at.y - CHIP_HEIGHT / 2.0);
    Rect::from_corners(min, min + Vec2::new(width, CHIP_HEIGHT))
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// Chips that never lie under one another or under `blocked` (the first-run
/// tip, the panel): the hovered patch first, then from the top of the
/// screen down. A named chip that has no room drops its name; a chip that
/// has no room at all is counted on the chip it would cover ("+1").
pub(crate) fn layout_chips(wants: &[ChipWant], blocked: &[Rect], most: usize) -> Vec<ChipPlaced> {
    let mut order: Vec<&ChipWant> = wants.iter().collect();
    order.sort_by(|a, b| {
        b.hovered
            .cmp(&a.hovered)
            .then(a.at.y.total_cmp(&b.at.y))
            .then(a.at.x.total_cmp(&b.at.x))
    });
    let mut placed: Vec<ChipPlaced> = Vec::new();
    for want in order {
        let free = |rect: Rect, placed: &[ChipPlaced]| {
            !placed.iter().any(|chip| overlaps(chip.rect, rect))
                && !blocked.iter().any(|block| overlaps(*block, rect))
        };
        let named = want.named.then(|| chip_rect(want.at, Some(&want.name)));
        let bare = chip_rect(want.at, None);
        let choice = match named {
            Some(rect) if free(rect, &placed) => Some((rect, Some(want.name.clone()))),
            _ if free(bare, &placed) => Some((bare, None)),
            _ => None,
        };
        match choice {
            Some((rect, name)) if placed.len() < most => placed.push(ChipPlaced {
                left_top: rect.min,
                kind: want.kind.clone(),
                name,
                more: 0,
                rect,
            }),
            _ => {
                // Under a block, or too many: it is not drawn; under a chip,
                // that chip counts it.
                if let Some(chip) = placed.iter_mut().find(|chip| overlaps(chip.rect, bare)) {
                    chip.more += 1;
                }
            }
        }
    }
    placed
}
/// The goods the grid was built from.
#[derive(Resource, Default)]
struct Built(Vec<String>);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Resources>()
        .init_resource::<WorldExportRequest>()
        .init_resource::<Built>()
        .init_resource::<Marks>()
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, pointer)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .before(crate::island_shell::ShellInput)
                .before(super::pointer)
                .run_if(not(crate::player_hud::active)),
        )
        .add_systems(
            Update,
            (process, refresh, mark, gizmo)
                .chain()
                .in_set(ViewSystems::Present)
                .after(super::process),
        );
}

fn editing(world: &World) -> bool {
    world.get_resource::<IslandMode>() == Some(&IslandMode::Editor)
}

/// The rail's three resource tiles: Resource, Remove and Export world.
pub(super) fn rail_tiles(world: &mut World, rail: Entity) {
    for (name, picture, key, action, sentence) in [
        (
            "Resource",
            Icon::ResourcePatches,
            "N",
            ResAction::Mode(Mode::Place),
            "Place a patch of a good where you click: berries, trees, stone. A village within 60 m works it.",
        ),
        (
            "Remove",
            Icon::Cannot,
            "X",
            ResAction::Mode(Mode::Remove),
            "Take away the patch under the pointer. A village keeps its last patch of each good.",
        ),
    ] {
        let tile = w::dock_tile(
            world,
            rail,
            name,
            picture,
            Some(key),
            ResourceControl(action),
        );
        world.entity_mut(tile.entity).insert(
            Tooltip::new(name, sentence)
                .icon(picture)
                .key(key)
                .detail([UNDO_SENTENCE]),
        );
    }
}

/// The rail's footer tile: write the shaped world to a world file.
pub(super) fn export_tile(world: &mut World, rail: Entity) {
    let tile = w::dock_tile(
        world,
        rail,
        "Export",
        Icon::GoTo,
        None,
        ResourceControl(ResAction::Export),
    );
    world.entity_mut(tile.entity).insert(
        Tooltip::new(
            "Export world",
            "Write this island as a new world file in your worlds folder: its land, patches, buildings, plots and props.",
        )
        .icon(Icon::GoTo)
        .detail(["The file keeps the world's rules; the island you play is not changed."]),
    );
    // Its name for automation and the Tab ring.
    world
        .entity_mut(tile.entity)
        .insert(Name::new("Game: Export world"));
}

/// Placing and removing are not strokes: Undo does not take them back.
pub(crate) const UNDO_SENTENCE: &str =
    "Ctrl+Z takes back a placed patch or brings back a removed one.";

fn spawn(world: &mut World) {
    let panel = world
        .spawn((
            Name::new("Shape resources"),
            PanelRoot,
            FocusRegion::Palette,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                    left: px(t::SCREEN_MARGIN + 76.0 + t::SCREEN_MARGIN),
                    width: px(PANEL_WIDTH),
                    padding: UiRect::all(px(10)),
                    row_gap: px(6),
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    w::label(
        world,
        panel,
        "Resources",
        t::SUBHEADING,
        Tone::text(Surface::Chrome),
    );
    w::label(
        world,
        panel,
        "Good",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    world.spawn((
        ChildOf(panel),
        GoodsGrid,
        Node {
            width: percent(100),
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(4),
            row_gap: px(4),
            ..default()
        },
    ));
    w::label(
        world,
        panel,
        "Richness",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    let row = world
        .spawn((
            ChildOf(panel),
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            },
        ))
        .id();
    for (text, steps, name, sentence) in [
        (
            "\u{2212}",
            -1,
            "Poorer",
            "A quarter less than the world's own patch.",
        ),
        (
            "+",
            1,
            "Richer",
            "A quarter more than the world's own patch.",
        ),
    ] {
        if steps > 0 {
            let value = w::label(world, row, "", t::BODY, Tone::text(Surface::Chrome));
            world.entity_mut(value).insert((
                RichnessText,
                Node {
                    width: px(130),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            if let Some(mut layout) = world.get_mut::<TextLayout>(value) {
                layout.justify = Justify::Center;
            }
        }
        let button = w::button(
            world,
            row,
            ButtonSpec::new(text, Kind::Secondary, Surface::Chrome).reach(Reach::Ring),
            ResourceControl(ResAction::Richness(steps)),
        );
        world.entity_mut(button).insert((
            Name::new(format!("Game: {name}")),
            Tooltip::new(name, sentence)
                .detail(["Richness is 25% to 400% of the world's own patch, in steps of 25%."]),
        ));
    }
    let hint = w::paragraph(world, panel, "", t::DENSE, Tone::secondary(Surface::Chrome));
    world.entity_mut(hint).insert(HintText);
    if let Some(mut node) = world.get_mut::<Node>(hint) {
        node.width = px(PANEL_WIDTH - 20.0);
    }

    // The tag beside the cursor: what a click does here, or why not.
    let tag = world
        .spawn((
            Name::new("Resource cursor tag"),
            TagRoot,
            Pickable::IGNORE,
            GlobalZIndex(t::layer::TOOLTIP),
            Outline::new(px(1), px(0), Color::NONE),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::axes(px(10), px(6)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let line = w::label(world, tag, "", t::BODY, Tone::text(Surface::Chrome));
    world.entity_mut(line).insert((TagLine, Pickable::IGNORE));
    let reason = w::paragraph(
        world,
        tag,
        "",
        t::DENSE,
        Tone::status(Surface::Chrome, Status::Crit),
    );
    world
        .entity_mut(reason)
        .insert((TagReason, Pickable::IGNORE));
    if let Some(mut node) = world.get_mut::<Node>(reason) {
        node.width = px(260);
    }
    // Labels over the existing patches.
    for slot in 0..LABELS {
        let label = world
            .spawn((
                Name::new("Patch label"),
                PatchLabel(slot),
                Pickable::IGNORE,
                GlobalZIndex(t::layer::CHROME - 2),
                w::chrome(
                    Node {
                        position_type: PositionType::Absolute,
                        padding: UiRect::axes(px(6), px(2)),
                        display: Display::None,
                        ..default()
                    },
                    t::RADIUS_SM,
                ),
            ))
            .id();
        if let Some(mut node) = world.get_mut::<Node>(label) {
            node.align_items = AlignItems::Center;
            node.column_gap = px(4);
        }
        // The good's picture (rebuilt when the slot's good changes), then
        // its name or "+2" when chips were folded into this one.
        let picture = world
            .spawn((
                ChildOf(label),
                ChipIcon,
                Pickable::IGNORE,
                Node {
                    width: px(CHIP_ICON),
                    height: px(CHIP_ICON),
                    ..default()
                },
            ))
            .id();
        let text = w::label(world, label, "", t::CAPTION, Tone::text(Surface::Chrome));
        world.entity_mut(text).insert((ChipText, Pickable::IGNORE));
        world.entity_mut(label).insert(ChipParts {
            picture,
            text,
            kind: None,
        });
    }
}

fn activate(
    event: On<Activate>,
    controls: Query<(&ResourceControl, Has<InteractionDisabled>)>,
    mut state: ResMut<Resources>,
) {
    if let Ok((control, false)) = controls.get(event.entity) {
        state.requests.push(control.0.clone());
    }
}

/// N takes up Resource, X takes up Remove; Esc puts either down (one step,
/// before the shell's own Esc ladder).
fn keys(world: &mut World) {
    if !editing(world) {
        return;
    }
    if world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some())
    {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
    ]) {
        return;
    }
    let holding = active(world);
    let pressed: Vec<(KeyCode, ResAction)> = [
        (KeyCode::KeyN, ResAction::Mode(Mode::Place)),
        (KeyCode::KeyX, ResAction::Mode(Mode::Remove)),
        (KeyCode::Escape, ResAction::Leave),
    ]
    .into_iter()
    .filter(|(key, action)| keys.just_pressed(*key) && (*action != ResAction::Leave || holding))
    .collect();
    if pressed.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for (key, _) in &pressed {
        keys.reset(*key);
    }
    world
        .resource_mut::<Resources>()
        .requests
        .extend(pressed.into_iter().map(|(_, action)| action));
}

/// The ground under the pointer, what a click would do there, and the click.
pub(super) fn pointer(world: &mut World) {
    let mode = world.resource::<Resources>().mode;
    if !editing(world) || mode.is_none() {
        let mut state = world.resource_mut::<Resources>();
        state.hover = None;
        state.hovered = None;
        return;
    }
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    let cell = if ui_hovered {
        None
    } else {
        picking::cursor_cell(world).filter(|(x, z)| picking::terrain_cell(world, *x, *z).is_some())
    };
    let at = cell.map(|(x, z)| SimPos::new(x * 1000 + 500, z * 1000 + 500));
    let clicked = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| mouse.just_pressed(MouseButton::Left));
    let (verdict, hovered, command) = match (mode, at) {
        (Some(Mode::Place), Some(at)) => {
            let state = world.resource::<Resources>();
            match state.good.clone() {
                None => (
                    Err("Choose a good in the Resources panel first.".to_owned()),
                    None,
                    None,
                ),
                Some(good) => {
                    let place = PlaceResourcePatch {
                        good: good.clone(),
                        x: at.x,
                        z: at.z,
                        richness: state.richness,
                    };
                    let verdict = place.check(world).map_err(|reason| sentence(&reason));
                    let words = format!(
                        "{} patch, {}",
                        good_words(world, &good),
                        richness_words(state.richness).to_lowercase()
                    );
                    let thing = Thing {
                        kind: PlaceKind::Patch,
                        key: good.clone(),
                        x: at.x,
                        z: at.z,
                        rotation: 0,
                        richness: state.richness,
                        mode: PlaceMode::Build,
                    };
                    (
                        verdict,
                        None,
                        Some((
                            SimCommand::content(&place),
                            format!("Placed a {}.", words.to_lowercase()),
                            thing,
                            true,
                        )),
                    )
                }
            }
        }
        (Some(Mode::Remove), Some(at)) => {
            let all = patches(world);
            let near = nearest(
                &all.iter()
                    .map(|(id, _, _, pos)| (*id, *pos))
                    .collect::<Vec<_>>(),
                at,
                PICK_MM,
            );
            match near.and_then(|id| all.iter().find(|row| row.0 == id)) {
                None => (Err("No resource patch here.".to_owned()), None, None),
                Some((id, entity, kind, pos)) => {
                    let thing = Thing {
                        kind: PlaceKind::Patch,
                        key: kind.clone(),
                        x: pos.x,
                        z: pos.z,
                        rotation: 0,
                        richness: richness_of(world, *entity),
                        mode: PlaceMode::Build,
                    };
                    let words = match settlement_label(world, *entity) {
                        Some(place) => format!("{} patch of {place}", good_words(world, kind)),
                        None => format!("{} patch", good_words(world, kind)),
                    };
                    let remove = RemoveResourcePatch { patch: *id };
                    let verdict = remove.check(world).map_err(|reason| sentence(&reason));
                    (
                        verdict,
                        Some((*id, words.clone())),
                        Some((
                            SimCommand::content(&remove),
                            format!("Removed the {}.", lower_first(&words)),
                            thing,
                            false,
                        )),
                    )
                }
            }
        }
        _ => (Ok(()), None, None),
    };
    {
        let mut state = world.resource_mut::<Resources>();
        state.hover = at;
        state.hovered = hovered;
        state.verdict = verdict.clone();
    }
    if clicked && at.is_some() {
        match (verdict, command) {
            // Applied, it joins Undo and Redo with strokes, plots and props.
            (Ok(()), Some((command, words, thing, present))) => {
                match submit_deferred(world, None, command) {
                    Ok(key) => super::placing::track(world, key, thing, present, words),
                    Err(error) => refused(world, mode, &error.to_string()),
                }
            }
            // A refused click says why in the notice too; nothing is recorded.
            (Err(reason), _) => refused(world, mode, &reason),
            _ => {}
        }
    }
    if clicked {
        // A click of a resource tool never also selects or orders anybody.
        if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            mouse.clear_just_pressed(MouseButton::Left);
            mouse.clear_just_released(MouseButton::Left);
        }
        if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
            drag.start = None;
        }
    }
}

/// The simulation's words as a sentence: capital first, full stop last.
pub(crate) fn sentence(reason: &str) -> String {
    crate::game_ui::words::sentence(reason)
}

fn refused(world: &mut World, mode: Option<Mode>, reason: &str) {
    let verb = if mode == Some(Mode::Remove) {
        "Not removed"
    } else {
        "Not placed"
    };
    world.resource_mut::<ShapeTools>().status = format!("{verb}: {reason}");
    crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Deny);
}

pub(super) fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Resources>().requests);
    for request in requests {
        match request {
            ResAction::Mode(mode) => {
                let mut state = world.resource_mut::<Resources>();
                // The same tile again puts the tool down.
                state.mode = if state.mode == Some(mode) {
                    None
                } else {
                    Some(mode)
                };
            }
            ResAction::Leave => world.resource_mut::<Resources>().mode = None,
            ResAction::Good(good) => {
                let mut state = world.resource_mut::<Resources>();
                state.good = Some(good);
                state.mode = Some(Mode::Place);
            }
            ResAction::Richness(steps) => {
                let mut state = world.resource_mut::<Resources>();
                state.richness = step_richness(state.richness, steps);
            }
            ResAction::Export => world.resource_mut::<WorldExportRequest>().0 = true,
        }
    }
    // A resource tool in hand puts a plot or prop down.
    if world.resource::<Resources>().mode.is_some()
        && let Some(mut placing) = world.get_resource_mut::<super::placing::Placing>()
    {
        placing.put_down();
    }
    // A good to start with: the first the world grows.
    if world.resource::<Resources>().good.is_none()
        && let Some(first) = patch_goods(world).into_iter().next()
    {
        world.resource_mut::<Resources>().good = Some(first);
    }
    settle(world);
}

/// Match recorded placements and removals with the simulation's outcome.
fn settle(world: &mut World) {
    if world.resource::<Resources>().inflight.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut world.resource_mut::<Resources>().inflight);
    let first = pending.iter().map(|(key, _)| key.0).min().unwrap_or(0);
    let skipped: Vec<((u64, u32), String)> = world
        .get_resource::<SkippedCommands>()
        .map(|log| {
            log.entries
                .iter()
                .rev()
                .take_while(|entry| entry.tick >= first)
                .map(|entry| ((entry.tick, entry.seq), entry.error.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut waiting = Vec::new();
    for (key, words) in pending {
        let queued = world
            .get_resource::<CommandQueue>()
            .is_some_and(|queue| queue.pending.contains_key(&key));
        if queued {
            waiting.push((key, words));
        } else if let Some((_, reason)) = skipped.iter().find(|(skip, _)| *skip == key) {
            world.resource_mut::<ShapeTools>().status =
                format!("Not applied: {}", sentence(reason));
            crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Deny);
        } else {
            let removed = words.contains(" of ") || words.ends_with(" patch");
            world.resource_mut::<ShapeTools>().status = if removed {
                format!("Removed the {}.", lower_first(&words))
            } else {
                format!("Placed a {}.", words.to_lowercase())
            };
            crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Place);
        }
    }
    world.resource_mut::<Resources>().inflight = waiting;
}

fn refresh(world: &mut World) {
    let shown = editing(world) && !crate::player_hud::active(world);
    let state = world.resource::<Resources>();
    let (mode, good, richness, verdict, hovered, hover) = (
        state.mode,
        state.good.clone(),
        state.richness,
        state.verdict.clone(),
        state.hovered.clone(),
        state.hover,
    );
    let open = shown && mode.is_some();
    let top = crate::island_shell::content_top(world);
    for mut node in world
        .query_filtered::<&mut Node, With<PanelRoot>>()
        .iter_mut(world)
    {
        let display = if open { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if node.top != px(top) {
            node.top = px(top);
        }
    }
    // Lit tiles: the tool in hand; the good chosen.
    let controls: Vec<(Entity, ResAction)> = world
        .query::<(Entity, &ResourceControl)>()
        .iter(world)
        .map(|(entity, control)| (entity, control.0.clone()))
        .collect();
    for (entity, action) in controls {
        let (on, enabled) = match &action {
            ResAction::Mode(tool) => (mode == Some(*tool), true),
            ResAction::Good(kind) => (good.as_deref() == Some(kind), mode != Some(Mode::Remove)),
            ResAction::Richness(steps) => (
                false,
                mode != Some(Mode::Remove)
                    && if *steps < 0 {
                        richness > RICHNESS_MIN
                    } else {
                        richness < RICHNESS_MAX
                    },
            ),
            ResAction::Leave | ResAction::Export => (false, true),
        };
        w::set_state(world, entity, on, false);
        w::set_enabled(world, entity, enabled);
    }
    if !shown {
        hide::<TagRoot>(world);
        // Only a real change: a written node is laid out again (the 40
        // patch chips every frame before, with the tools closed).
        hide::<PatchLabel>(world);
        return;
    }
    // The goods grid, built once the world's goods are known.
    let goods = patch_goods(world);
    if world.resource::<Built>().0 != goods {
        let grids: Vec<Entity> = world
            .query_filtered::<Entity, With<GoodsGrid>>()
            .iter(world)
            .collect();
        for grid in grids {
            world.entity_mut(grid).despawn_children();
            for kind in &goods {
                let name = good_words(world, kind);
                let tile = w::control(
                    world,
                    grid,
                    &name,
                    // Lit like a dock tile while it is the good in hand.
                    Kind::DockTile,
                    Surface::Chrome,
                    Reach::Ring,
                    Node {
                        width: px(70),
                        height: px(50),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: px(2),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                        ..default()
                    },
                    ResourceControl(ResAction::Good(kind.clone())),
                );
                if let Some(picture) = Icon::of_good(kind) {
                    icon::icon(world, tile, picture, 20.0, Tone::text(Surface::Chrome));
                }
                let caption = w::label(
                    world,
                    tile,
                    &name,
                    t::CAPTION,
                    Tone::secondary(Surface::Chrome),
                );
                world.entity_mut(caption).insert(w::Part::Label);
                world.entity_mut(tile).insert(Tooltip::new(
                    name.clone(),
                    format!("Place {} patches.", name.to_lowercase()),
                ));
            }
        }
        world.resource_mut::<Built>().0 = goods;
    }
    let texts: Vec<Entity> = world
        .query_filtered::<Entity, With<RichnessText>>()
        .iter(world)
        .collect();
    for text in texts {
        w::set_text(world, text, &richness_words(richness));
    }
    let hint = match mode {
        Some(Mode::Remove) => {
            "Click a marked patch to remove it. A village keeps its last patch of each good. Ctrl+Z brings it back."
        }
        _ => {
            "Click the ground to place: green where it can go, red with the reason where not. Ctrl+Z takes it back."
        }
    };
    let hints: Vec<Entity> = world
        .query_filtered::<Entity, With<HintText>>()
        .iter(world)
        .collect();
    for entity in hints {
        w::set_text(world, entity, hint);
    }
    // The cursor tag.
    let cursor = world
        .query::<&Window>()
        .iter(world)
        .find_map(Window::cursor_position);
    let scale = crate::game_ui::scale::layout(world).scale.max(0.1);
    let line = match (mode, hover) {
        (Some(Mode::Place), Some(_)) => good.as_deref().map(|good| {
            format!(
                "Place {} \u{b7} {}",
                good_words(world, good).to_lowercase(),
                richness_words(richness)
            )
        }),
        (Some(Mode::Remove), Some(_)) => Some(hovered.as_ref().map_or_else(
            || "Remove".to_owned(),
            |(_, words)| format!("Remove the {}", lower_first(words)),
        )),
        _ => None,
    };
    let tags: Vec<Entity> = world
        .query_filtered::<Entity, With<TagRoot>>()
        .iter(world)
        .collect();
    for tag in tags {
        let reason = verdict.as_ref().err().cloned();
        let lines: Vec<Entity> = world
            .query_filtered::<Entity, With<TagLine>>()
            .iter(world)
            .collect();
        for entity in lines {
            w::set_text(world, entity, line.as_deref().unwrap_or_default());
        }
        let reasons: Vec<Entity> = world
            .query_filtered::<Entity, With<TagReason>>()
            .iter(world)
            .collect();
        for entity in reasons {
            w::set_text(world, entity, reason.as_deref().unwrap_or_default());
            w::show(world, entity, reason.is_some());
        }
        let edge = if reason.is_some() {
            t::CRIT_LIGHT
        } else {
            Color::NONE
        };
        if let Some(mut outline) = world.get_mut::<Outline>(tag)
            && outline.color != edge
        {
            outline.color = edge;
        }
        if let Some(mut node) = world.get_mut::<Node>(tag) {
            match (cursor, line.is_some()) {
                (Some(cursor), true) => {
                    let (left, top) = (px(cursor.x / scale + 18.0), px(cursor.y / scale + 18.0));
                    if node.display != Display::Flex || node.left != left || node.top != top {
                        node.display = Display::Flex;
                        node.left = left;
                        node.top = top;
                    }
                }
                _ if node.display != Display::None => node.display = Display::None,
                _ => {}
            }
        }
    }
    labels(world, open, scale);
}

fn hide<T: Component>(world: &mut World) {
    for mut node in world.query_filtered::<&mut Node, With<T>>().iter_mut(world) {
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
}

/// Where a patch stands in the scene: on its cell's ground.
fn ground(world: &World, at: SimPos) -> Vec3 {
    let (x, z) = (at.x.div_euclid(1000), at.z.div_euclid(1000));
    let height = picking::terrain_cell(world, x, z)
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0);
    Vec3::new(at.x as f32 / 1000.0, height, at.z as f32 / 1000.0)
}

/// A chip over each patch while a resource tool is in hand: the good's
/// picture at island zoom, its name when the pointer is on it or the camera
/// is close. Chips never lie under one another or under the first-run tip.
fn labels(world: &mut World, open: bool, scale: f32) {
    let mut wants: Vec<ChipWant> = Vec::new();
    if open {
        let camera = world
            .query_filtered::<(&Camera, &GlobalTransform), With<crate::RtsCamera>>()
            .iter(world)
            .next()
            .map(|(camera, transform)| (camera.clone(), *transform));
        let cursor = world
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .iter(world)
            .next()
            .and_then(Window::cursor_position)
            .map(|at| at / scale);
        if let Some((camera, transform)) = camera {
            for (_, _, kind, at) in patches(world) {
                let point = ground(world, at) + Vec3::Y * 1.2;
                if let Ok(screen) = camera.world_to_viewport(&transform, point) {
                    let screen = screen / scale;
                    let hovered = cursor.is_some_and(|cursor| cursor.distance(screen) < HOVER_PX);
                    let near = transform.translation().distance(point) < NAMES_WITHIN_M;
                    wants.push(ChipWant {
                        at: screen,
                        name: good_words(world, &kind),
                        kind,
                        named: hovered || near,
                        hovered,
                    });
                }
            }
        }
    }
    let mut blocked = Vec::new();
    if open {
        if let Some(tip) = crate::island_shell::coach::tip_rect(world) {
            blocked.push(Rect::from_corners(tip.min / scale, tip.max / scale));
        }
        // The panel and the cursor tag: a chip never lies under either.
        let panels: Vec<Entity> = world
            .query_filtered::<(Entity, &Node), Or<(With<PanelRoot>, With<TagRoot>)>>()
            .iter(world)
            .filter(|(_, node)| node.display != Display::None)
            .map(|(entity, _)| entity)
            .collect();
        for panel in panels {
            if let Some(rect) = crate::game_ui::tooltip::anchor_rect(world, panel) {
                blocked.push(Rect::from_corners(rect.min / scale, rect.max / scale));
            }
        }
    }
    let placed = layout_chips(&wants, &blocked, LABELS);
    let slots: Vec<(Entity, usize)> = world
        .query::<(Entity, &PatchLabel)>()
        .iter(world)
        .map(|(entity, label)| (entity, label.0))
        .collect();
    for (entity, slot) in slots {
        match placed.get(slot) {
            Some(chip) => {
                let Some(parts) = world.get::<ChipParts>(entity) else {
                    continue;
                };
                let (picture, text, shown) = (parts.picture, parts.text, parts.kind.clone());
                if shown.as_deref() != Some(chip.kind.as_str()) {
                    world.entity_mut(picture).despawn_children();
                    if let Some(glyph) = Icon::of_good(&chip.kind) {
                        let image = icon::icon(
                            world,
                            picture,
                            glyph,
                            CHIP_ICON,
                            Tone::text(Surface::Chrome),
                        );
                        world.entity_mut(image).insert(Pickable::IGNORE);
                    }
                    if let Some(mut parts) = world.get_mut::<ChipParts>(entity) {
                        parts.kind = Some(chip.kind.clone());
                    }
                }
                // A good without a picture always shows its name.
                let words = if Icon::of_good(&chip.kind).is_none() && chip.name.is_none() {
                    ChipPlaced {
                        name: Some(good_words(world, &chip.kind)),
                        ..chip.clone()
                    }
                    .words()
                } else {
                    chip.words()
                };
                w::set_text(world, text, &words);
                w::show(world, text, !words.is_empty());
                // Written only when it moved: a written node is laid out
                // again (40 chips every frame before).
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    let (left, top) = (px(chip.left_top.x.round()), px(chip.left_top.y.round()));
                    if node.display != Display::Flex || node.left != left || node.top != top {
                        node.display = Display::Flex;
                        node.left = left;
                        node.top = top;
                    }
                }
            }
            None => {
                if let Some(mut node) = world.get_mut::<Node>(entity)
                    && node.display != Display::None
                {
                    node.display = Display::None;
                }
            }
        }
    }
}

/// What the gizmos draw this frame: the ghost (and whether it is refused)
/// and the patches (and whether one is the patch under the Remove tool).
#[derive(Resource, Default)]
struct Marks {
    ghost: Option<(Vec3, bool)>,
    patches: Vec<(Vec3, bool)>,
}

fn mark(world: &mut World) {
    let state = world.resource::<Resources>();
    let open = editing(world) && state.mode.is_some() && !crate::player_hud::active(world);
    let (mode, hover, refused, hovered) = (
        state.mode,
        state.hover,
        state.verdict.is_err(),
        state.hovered.as_ref().map(|(id, _)| *id),
    );
    let marks = if open {
        Marks {
            // The Remove tool marks the patch it would take, not the ground.
            ghost: hover
                .filter(|_| mode == Some(Mode::Place))
                .map(|at| (ground(world, at) + Vec3::Y * 0.08, refused)),
            patches: patches(world)
                .into_iter()
                .map(|(id, _, _, at)| (ground(world, at) + Vec3::Y * 0.08, hovered == Some(id)))
                .collect(),
        }
    } else {
        Marks::default()
    };
    *world.resource_mut::<Marks>() = marks;
}

fn gizmo(marks: Res<Marks>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for (at, picked) in &marks.patches {
        let color = if *picked { t::CRIT_LIGHT } else { t::HONEY_500 };
        gizmos.circle(Isometry3d::new(*at, flat), 0.6, color);
        if *picked {
            // The patch the click takes: a second ring, not colour alone.
            gizmos.circle(Isometry3d::new(*at, flat), 0.85, color);
        }
    }
    if let Some((at, refused)) = marks.ghost {
        if refused {
            // Refused: red and crossed, so the refusal is not colour alone.
            let color = t::CRIT_LIGHT;
            gizmos.circle(Isometry3d::new(at, flat), 1.0, color);
            gizmos.line(
                at + Vec3::new(-0.7, 0.0, -0.7),
                at + Vec3::new(0.7, 0.0, 0.7),
                color,
            );
            gizmos.line(
                at + Vec3::new(-0.7, 0.0, 0.7),
                at + Vec3::new(0.7, 0.0, -0.7),
                color,
            );
        } else {
            let color = t::GOOD_LIGHT;
            gizmos.circle(Isometry3d::new(at, flat), 1.0, color);
            gizmos.circle(Isometry3d::new(at, flat), 0.5, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want(x: f32, y: f32, kind: &str, named: bool, hovered: bool) -> ChipWant {
        ChipWant {
            at: Vec2::new(x, y),
            kind: kind.into(),
            name: kind.to_uppercase(),
            named,
            hovered,
        }
    }

    #[test]
    fn patch_chips_never_lie_under_each_other_or_the_tip() {
        // A cluster of three (Fiber, Grain, Logs at island zoom) and one apart.
        let wants = [
            want(100.0, 100.0, "fiber", false, false),
            want(108.0, 104.0, "grain", false, false),
            want(112.0, 98.0, "log", false, false),
            want(300.0, 300.0, "ore", false, false),
        ];
        let placed = layout_chips(&wants, &[], 40);
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!(!overlaps(a.rect, b.rect), "{a:?} under {b:?}");
            }
        }
        assert_eq!(placed.len(), 2, "the cluster folds into one chip");
        assert_eq!(placed[0].words(), "+2", "and counts what it holds");
        assert_eq!(placed[1].words(), "", "a lone chip is a picture only");
        // The first-run tip covers the lone patch: no chip under it.
        let tip = Rect::from_corners(Vec2::new(250.0, 250.0), Vec2::new(400.0, 400.0));
        let placed = layout_chips(&wants, &[tip], 40);
        assert!(placed.iter().all(|chip| !overlaps(chip.rect, tip)));
    }

    #[test]
    fn the_hovered_patch_names_its_good_first() {
        let wants = [
            want(100.0, 100.0, "fiber", false, false),
            want(108.0, 104.0, "grain", true, true),
        ];
        let placed = layout_chips(&wants, &[], 40);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].kind, "grain");
        assert_eq!(placed[0].words(), "GRAIN +1");
        // Close up, chips with room name their goods.
        let wants = [
            want(100.0, 100.0, "fiber", true, false),
            want(100.0, 200.0, "grain", true, false),
        ];
        let placed = layout_chips(&wants, &[], 40);
        assert_eq!(
            placed.iter().map(ChipPlaced::words).collect::<Vec<_>>(),
            ["FIBER", "GRAIN"]
        );
    }

    #[test]
    fn richness_steps_by_a_quarter_inside_its_bounds_and_says_its_word() {
        assert_eq!(step_richness(100, 1), 125);
        assert_eq!(step_richness(25, -1), 25);
        assert_eq!(step_richness(400, 2), 400);
        assert_eq!(richness_words(50), "Poor \u{b7} 50%");
        assert_eq!(richness_words(100), "Normal \u{b7} 100%");
        assert_eq!(richness_words(150), "Rich \u{b7} 150%");
        assert_eq!(richness_words(300), "Abundant \u{b7} 300%");
    }

    #[test]
    fn the_remove_tool_takes_the_nearest_patch_within_reach() {
        let patches = [
            (SimId(3), SimPos::new(0, 0)),
            (SimId(4), SimPos::new(1_500, 0)),
            (SimId(5), SimPos::new(9_000, 0)),
        ];
        assert_eq!(
            nearest(&patches, SimPos::new(1_000, 0), 2_000),
            Some(SimId(4))
        );
        assert_eq!(nearest(&patches, SimPos::new(5_000, 0), 2_000), None);
    }

    #[test]
    fn export_is_a_request_the_host_takes_once() {
        let mut world = World::new();
        world.init_resource::<WorldExportRequest>();
        world.resource_mut::<WorldExportRequest>().0 = true;
        assert!(take_world_export(&mut world));
        assert!(!take_world_export(&mut world));
    }
}
