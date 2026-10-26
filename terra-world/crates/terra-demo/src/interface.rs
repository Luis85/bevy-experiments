//! Contextual native game interface. World simulation and commands stay in their own modules.
use crate::control::{ControlMode, ControlSet, PlayerControl, WorldViewport};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::AccumulatedMouseScroll,
    },
    picking::hover::Hovered,
    prelude::*,
    ui::{InteractionDisabled, Pressed},
    ui_widgets::{Activate, Button, observe},
};
use std::collections::VecDeque;
use terra_bevy::{
    NetworkStatus, PoliticalWorld, ResidentChunks, SandboxRuntime, WorldRuntime, WorldSet,
};
use terra_core::{Layer, season};

pub const TRADE_ITEMS: [(&str, &str); 9] = [
    ("bread", "Bread"),
    ("water", "Water"),
    ("wheat_seed", "Seeds"),
    ("wood", "Wood"),
    ("planks", "Planks"),
    ("stone", "Stone"),
    ("wheat", "Wheat"),
    ("flour", "Flour"),
    ("tool", "Tools"),
];
pub struct InterfacePlugin;
impl Plugin for InterfacePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .insert(
                bevy::asset::AssetId::default(),
                Font::from_bytes(include_bytes!("../assets/DejaVuSans.ttf").to_vec()),
            )
            .expect("the default font has a stable asset ID");
        app.init_resource::<UiQueue>()
            .init_resource::<UiState>()
            .add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (edit_text, sync_layout, handle_context_request)
                    .before(ControlSet::Input)
                    .before(WorldSet::Stream),
            )
            .add_systems(
                Update,
                publish_control_status
                    .after(ControlSet::Input)
                    .before(WorldSet::Sandbox),
            )
            .add_systems(
                Update,
                (
                    scroll_panels,
                    update_scrollbars.after(scroll_panels),
                    button_availability,
                    button_style.after(button_availability),
                    update_labels,
                    update_needs,
                ),
            );
    }
}
#[derive(Resource, Default)]
pub struct UiQueue(pub VecDeque<Action>);
#[derive(Clone, Copy, Default)]
pub enum InputMode {
    #[default]
    Coordinates,
    FactionName,
    InvitePlayer,
    PromotePlayer,
    DemotePlayer,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Command,
    Build,
    Market,
    Industry,
    Factions,
}
#[derive(Resource)]
pub struct UiState {
    pub editing_gps: bool,
    pub gps_text: String,
    pub location: String,
    pub input_mode: InputMode,
    pub selected_faction: Option<u64>,
    pub world_panel: bool,
    pub context_panel: bool,
    pub panel: Panel,
    pub trade_quantity: u32,
    pub selected_site: Option<u64>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            editing_gps: false,
            gps_text: "48.137, 11.575".into(),
            location: "Geographic focus".into(),
            input_mode: InputMode::Coordinates,
            selected_faction: None,
            world_panel: false,
            context_panel: true,
            panel: Panel::Command,
            trade_quantity: 1,
            selected_site: None,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Action {
    Preset(usize),
    EditGps,
    JumpGps,
    Toggle(Layer),
    Pause,
    Speed,
    Season,
    Noon,
    Camp,
    Claim,
    GatherWood,
    GatherStone,
    Planks,
    Blocks,
    Flour,
    Bread,
    Tool,
    Seeds,
    House,
    Workbench,
    Fence,
    Plant,
    Harvest,
    Damage,
    Home,
    Save,
    Load,
    Follow,
    Overview,
    Atlas,
    FitAtlas,
    DrawTerritory,
    ExportPolitics,
    FlyToSelection,
    Borders,
    CreateFaction,
    InviteFaction,
    AcceptFaction,
    PromoteFaction,
    DemoteFaction,
    SharePlot,
    NextFaction,
    ToggleWorld,
    ToggleInspector,
    Panel(Panel),
    Mode(ControlMode),
    Possess,
    Release,
    SelectAll,
    CycleActor,
    SelectOwned(usize),
    StopUnits,
    RestUnits,
    Buy(usize),
    Sell(usize),
    TradeQuantity,
    Eat,
    Drink,
    GoToMarket,
    NextSite,
    GoToSite,
    ToggleProduction,
}
#[derive(Component)]
enum Label {
    Location,
    Coordinates,
    Clock,
    Weather,
    Streaming,
    Inventory,
    Sandbox,
    Toast,
    Attribution,
    Connection,
    Layer(Layer),
    Source,
    Politics,
    Faction,
    Controls,
    Mode,
    Selected,
    Wallet,
    Population,
    Owned(usize),
    Shop,
    Offer(usize),
    Quantity,
    Industry,
    Logistics,
    PanelTitle,
}
#[derive(Component)]
struct ActionButton(Action);
#[derive(Component)]
struct SidePanel(bool);
#[derive(Component)]
struct PanelPage(Panel);
#[derive(Component)]
struct WorldDrawer;
#[derive(Component)]
struct Inspector;
#[derive(Component)]
struct ScrollThumb(bool);
#[derive(Component)]
struct ScrollTrack(bool);
#[derive(Component)]
struct AtlasControls;
#[derive(Component)]
struct SourceBanner;
#[derive(Component)]
struct AtlasCaption;
#[derive(Component)]
struct NeedFill(usize);
#[derive(Component)]
struct NeedLabel(usize);
const PANEL: Color = Color::srgb(0.055, 0.095, 0.083);
const SURFACE: Color = Color::srgb(0.105, 0.188, 0.16);
const TEXT: Color = Color::srgb(0.933, 0.945, 0.894);
const MUTED: Color = Color::srgb(0.698, 0.769, 0.710);
const ACCENT: Color = Color::srgb(0.702, 0.863, 0.557);
const SAND: Color = Color::srgb(0.902, 0.765, 0.545);
fn text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size.max(11.)),
            ..default()
        },
        TextColor(color),
        Node {
            flex_shrink: 0.,
            max_width: percent(100),
            ..default()
        },
    )
}
fn button(parent: &mut ChildSpawnerCommands, title: &str, action: Action) {
    button_with_label(parent, title, action, None);
}
fn button_with_label(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    action: Action,
    label: Option<Label>,
) {
    parent
        .spawn((
            Button,
            ActionButton(action),
            Hovered::default(),
            Node {
                width: percent(100),
                min_height: px(33),
                flex_shrink: 0.,
                padding: UiRect::axes(px(10), px(8)),
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(SURFACE),
            observe(move |_: On<Activate>, mut q: ResMut<UiQueue>| {
                if q.0.len() < 64 {
                    q.0.push_back(action);
                }
            }),
        ))
        .with_children(|p| {
            let mut e = p.spawn(text(title, 12., TEXT));
            if let Some(label) = label {
                e.insert(label);
            }
        });
}
fn heading(parent: &mut ChildSpawnerCommands, title: &str) {
    parent.spawn(text(title, 13., ACCENT)).insert(Node {
        flex_shrink: 0.,
        margin: UiRect::top(px(14)),
        ..default()
    });
}
fn pair(parent: &mut ChildSpawnerCommands, a: (&str, Action), b: (&str, Action)) {
    parent
        .spawn(Node {
            width: percent(100),
            column_gap: px(6),
            flex_shrink: 0.,
            ..default()
        })
        .with_children(|p| {
            for (title, action) in [a, b] {
                p.spawn(Node {
                    width: percent(49),
                    ..default()
                })
                .with_children(|p| button(p, title, action));
            }
        });
}
fn page(
    parent: &mut ChildSpawnerCommands,
    which: Panel,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            PanelPage(which),
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.,
                ..default()
            },
        ))
        .with_children(build);
}
fn setup_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                right: px(0),
                height: px(64),
                padding: UiRect::axes(px(18), px(10)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(12),
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(30),
        ))
        .with_children(|p| {
            p.spawn(text("Terra", 25., TEXT));
            p.spawn(Node {
                width: px(310),
                column_gap: px(5),
                ..default()
            })
            .with_children(|p| {
                for (title, mode) in [
                    ("RTS  [F1]", ControlMode::Rts),
                    ("Third  [F2]", ControlMode::ThirdPerson),
                    ("First  [F3]", ControlMode::FirstPerson),
                ] {
                    p.spawn(Node {
                        width: percent(33),
                        ..default()
                    })
                    .with_children(|p| button(p, title, Action::Mode(mode)));
                }
            });
            p.spawn(Node {
                width: px(330),
                column_gap: px(5),
                ..default()
            })
            .with_children(|p| {
                for (title, action) in [
                    ("World  [F10]", Action::ToggleWorld),
                    ("Atlas  [M]", Action::Atlas),
                    ("Inspect [F11]", Action::ToggleInspector),
                ] {
                    p.spawn(Node {
                        width: percent(33),
                        ..default()
                    })
                    .with_children(|p| button(p, title, action));
                }
            });
            p.spawn((Label::Connection, text("LOCAL SESSION", 11., MUTED)))
                .insert(Node {
                    max_width: px(190),
                    ..default()
                });
        });
    commands
        .spawn((
            WorldDrawer,
            SidePanel(false),
            ScrollPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(76),
                left: px(14),
                bottom: px(122),
                width: px(246),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                border_radius: BorderRadius::all(px(8)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(10),
        ))
        .with_children(|p| {
            p.spawn(text("World", 20., TEXT));
            p.spawn((Label::Location, text("", 12., MUTED)));
            p.spawn((Label::Coordinates, text("", 12., TEXT)));
            button(p, "Enter GPS coordinates", Action::EditGps);
            button(p, "Move active actor to this region", Action::Camp);
            heading(p, "Regions");
            for (i, title) in [
                "Munich · settlement",
                "Zugspitze · alpine terrain",
                "Hamburg · harbor",
                "Cape Town · coastal region",
            ]
            .into_iter()
            .enumerate()
            {
                button(p, title, Action::Preset(i));
            }
            heading(p, "Visible layers");
            for layer in Layer::ALL {
                button_with_label(p, "", Action::Toggle(layer), Some(Label::Layer(layer)));
            }
            heading(p, "Time and conditions");
            p.spawn((Label::Clock, text("", 12., TEXT)));
            p.spawn((Label::Weather, text("", 12., MUTED)));
            pair(
                p,
                ("Pause [Space]", Action::Pause),
                ("Time speed", Action::Speed),
            );
            pair(
                p,
                ("Next season", Action::Season),
                ("Daytime", Action::Noon),
            );
            heading(p, "Geography and sources");
            p.spawn((Label::Attribution, text("", 11., MUTED)));
            p.spawn((Label::Politics, text("", 11., MUTED)));
            button(p, "Show / hide local borders", Action::Borders);
            button(p, "Draw a scenario territory", Action::DrawTerritory);
            button(p, "Export political GeoJSON", Action::ExportPolitics);
            heading(p, "Local session");
            pair(p, ("Save [F5]", Action::Save), ("Load [F9]", Action::Load));
            p.spawn(text(
                "Saving is explicit. Load with the same source and era.",
                11.,
                MUTED,
            ));
        });
    commands.spawn((Inspector,Node {position_type:PositionType::Absolute,top:px(76),right:px(14),bottom:px(122),
        width:px(300),padding:UiRect::all(px(14)),flex_direction:FlexDirection::Column,row_gap:px(10),
        border_radius:BorderRadius::all(px(8)),..default()},BackgroundColor(PANEL),GlobalZIndex(10)))
        .with_children(|p| {
            p.spawn(Node {width:percent(100),justify_content:JustifyContent::SpaceBetween,align_items:AlignItems::Center,..default()}).with_children(|p| {
                p.spawn((Label::PanelTitle,text("Command",19.,TEXT)));
                p.spawn((Label::Wallet,text("",13.,SAND)));
            });
            p.spawn(Node {width:percent(100),column_gap:px(5),row_gap:px(5),flex_wrap:FlexWrap::Wrap,flex_shrink:0.,..default()}).with_children(|p| {
                for (title,panel) in [("Command",Panel::Command),("Build",Panel::Build),("Market",Panel::Market),("Industry",Panel::Industry),("Factions",Panel::Factions)] {
                    p.spawn(Node {width:percent(31.8),..default()}).with_children(|p|button(p,title,Action::Panel(panel)));
                }
            });
            p.spawn((SidePanel(true),ScrollPosition::default(),Node {width:percent(100),flex_grow:1.,min_height:px(0),
                flex_direction:FlexDirection::Column,row_gap:px(6),overflow:Overflow::scroll_y(),padding:UiRect::right(px(5)),..default()}))
                .with_children(|p| {
                    page(p,Panel::Command,|p| {
                        p.spawn((Label::Mode,text("",12.,ACCENT)));
                        p.spawn((Label::Selected,text("Select an owned actor",16.,TEXT)));
                        for (i,name) in ["Food","Water","Energy"].into_iter().enumerate() {
                            p.spawn((NeedLabel(i),text(name,12.,MUTED)));
                            p.spawn((Node {width:percent(100),height:px(5),flex_shrink:0.,border_radius:BorderRadius::all(px(2)),..default()},BackgroundColor(SURFACE)))
                                .with_children(|p|{p.spawn((NeedFill(i),Node {height:percent(100),width:percent(100),border_radius:BorderRadius::all(px(2)),..default()},BackgroundColor(ACCENT)));});
                        }
                        pair(p,("Possess selected",Action::Possess),("Release [Esc]",Action::Release));
                        pair(p,("Select crew",Action::SelectAll),("Focus [Tab]",Action::Follow));
                        button(p,"Select next owned actor",Action::CycleActor);
                        pair(p,("Stop orders",Action::StopUnits),("Rest units",Action::RestUnits));
                        pair(p,("Eat bread",Action::Eat),("Drink water",Action::Drink));
                        heading(p,"Your crew");
                        for i in 0..8 {button_with_label(p,"",Action::SelectOwned(i),Some(Label::Owned(i)));}
                        heading(p,"Living settlement");
                        p.spawn((Label::Population,text("",12.,MUTED)));
                        button(p,"Open nearby market [E]",Action::Panel(Panel::Market));
                        p.spawn(text("RTS: select actors, then right-click the ground to move. Hold Shift to add actors to a selection.",12.,MUTED));
                    });
                    page(p,Panel::Build,|p| {
                        p.spawn((Label::Inventory,text("",12.,TEXT)));
                        p.spawn((Label::Sandbox,text("",12.,MUTED)));
                        heading(p,"Claim and gather");
                        button(p,"Claim a 40 m housing plot",Action::Claim);
                        pair(p,("Gather wood +5",Action::GatherWood),("Gather stone +5",Action::GatherStone));
                        heading(p,"Craft materials");
                        button(p,"Planks · 2 wood → 4 planks",Action::Planks);
                        button(p,"Blocks · 2 stone → 1 block",Action::Blocks);
                        button(p,"Tool · requires a workbench",Action::Tool);
                        button(p,"Seeds · 1 wheat → 3 seeds",Action::Seeds);
                        button(p,"Flour · 2 wheat → 3 flour",Action::Flour);
                        button(p,"Bread · 2 flour + water → 4 bread",Action::Bread);
                        p.spawn(text("Flour and bread crafting require a workbench.",11.,MUTED));
                        heading(p,"Place in your plot");
                        button(p,"House · 12 planks + 4 blocks",Action::House);
                        button(p,"Workbench · 4 planks + 2 stone",Action::Workbench);
                        button(p,"Fence · 2 planks",Action::Fence);
                        button(p,"Plant wheat · 1 seed",Action::Plant);
                        button(p,"Harvest ripe wheat",Action::Harvest);
                        heading(p,"Property");
                        button(p,"Set nearby house as home",Action::Home);
                        button(p,"Damage permitted structure −25",Action::Damage);
                        pair(p,("Save [F5]",Action::Save),("Load [F9]",Action::Load));
                    });
                    page(p,Panel::Market,|p| {
                        p.spawn((Label::Shop,text("",12.,TEXT)));
                        button(p,"Order active actor to market",Action::GoToMarket);
                        button_with_label(p,"Quantity: 1",Action::TradeQuantity,Some(Label::Quantity));
                        p.spawn(text("Buy pays the shop. Sell earns credits. Walk within 20 m to trade; stocks and tills are finite.",12.,MUTED));
                        for (i,(_,name)) in TRADE_ITEMS.iter().enumerate() {
                            heading(p,name);
                            p.spawn((Label::Offer(i),text("",12.,TEXT)));
                            pair(p,("Buy",Action::Buy(i)),("Sell",Action::Sell(i)));
                        }
                        heading(p,"Use supplies");
                        pair(p,("Eat bread",Action::Eat),("Drink water",Action::Drink));
                    });
                    page(p,Panel::Industry,|p| {
                        p.spawn(text("Production and deliveries",16.,TEXT));
                        p.spawn(text("Water + seeds → wheat → flour → bread\nWood → planks · wheat → seeds",12.,ACCENT));
                        p.spawn((Label::Industry,text("",12.,TEXT)));
                        pair(p,("Next facility",Action::NextSite),("Order actor here",Action::GoToSite));
                        button(p,"Pause / resume owned production",Action::ToggleProduction);
                        heading(p,"Logistics");
                        p.spawn((Label::Logistics,text("",12.,MUTED)));
                        p.spawn(text("Couriers carry real stock along road routes. Production waits for inputs; moving vehicles yield to traffic ahead.",12.,MUTED));
                    });
                    page(p,Panel::Factions,|p| {
                        p.spawn((Label::Faction,text("",12.,TEXT)));
                        button(p,"Create a named faction",Action::CreateFaction);
                        button(p,"Select next faction",Action::NextFaction);
                        button(p,"Invite a player by ID",Action::InviteFaction);
                        button(p,"Accept a pending invitation",Action::AcceptFaction);
                        button(p,"Grant builder role",Action::PromoteFaction);
                        button(p,"Set member role",Action::DemoteFaction);
                        button(p,"Transfer nearest plot to faction",Action::SharePlot);
                        p.spawn(text("Current faction roles control shared buildings, farms and resource access. Political borders are a separate geographic layer.",12.,MUTED));
                    });
                });
        });
    commands
        .spawn((
            SourceBanner,
            Node {
                position_type: PositionType::Absolute,
                top: px(77),
                left: px(14),
                right: px(326),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(5),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    padding: UiRect::axes(px(12), px(7)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((Label::Source, text("", 11., ACCENT)));
            });
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(12),
                left: px(14),
                right: px(14),
                height: px(96),
                padding: UiRect::axes(px(14), px(10)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(20),
        ))
        .with_children(|p| {
            p.spawn((
                Label::Toast,
                text("Loading terrain and the living settlement…", 13., ACCENT),
            ));
            p.spawn((Label::Controls, text("", 12., TEXT)));
            p.spawn((Label::Streaming, text("", 11., MUTED)));
        });
    commands
        .spawn((
            AtlasControls,
            Node {
                position_type: PositionType::Absolute,
                top: px(122),
                left: px(14),
                right: px(326),
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            GlobalZIndex(15),
        ))
        .with_children(|p| {
            for (title, action) in [
                ("Fit to data", Action::FitAtlas),
                ("Draw territory", Action::DrawTerritory),
                ("Export GeoJSON", Action::ExportPolitics),
                ("Explore selected", Action::FlyToSelection),
            ] {
                p.spawn(Node {
                    width: percent(24),
                    ..default()
                })
                .with_children(|p| button(p, title, action));
            }
        });
    commands
        .spawn((
            AtlasCaption,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(128),
                left: px(14),
                right: px(326),
                padding: UiRect::all(px(10)),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(5),
        ))
        .with_children(|p| {
            p.spawn((Label::Attribution, text("", 11., MUTED)));
        });
    for right in [false, true] {
        commands
            .spawn((
                ScrollTrack(right),
                Node {
                    position_type: PositionType::Absolute,
                    top: if right { px(204) } else { px(90) },
                    bottom: px(136),
                    left: if right { Val::Auto } else { px(254) },
                    right: if right { px(19) } else { Val::Auto },
                    width: px(3),
                    ..default()
                },
                BackgroundColor(SURFACE),
                GlobalZIndex(21),
            ))
            .with_children(|p| {
                p.spawn((
                    ScrollThumb(right),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        width: px(3),
                        height: px(40),
                        ..default()
                    },
                    BackgroundColor(ACCENT),
                ));
            });
    }
}

fn sync_layout(
    ui: Res<UiState>,
    politics: Res<PoliticalWorld>,
    mut viewport: ResMut<WorldViewport>,
    mut nodes: Query<(
        &mut Node,
        Option<&WorldDrawer>,
        Option<&Inspector>,
        Option<&PanelPage>,
        Option<&AtlasControls>,
        Option<&SourceBanner>,
        Option<&AtlasCaption>,
        Option<&ScrollTrack>,
    )>,
    mut previous: Local<Option<(bool, bool, Panel, bool)>>,
    mut scrolls: Query<(&SidePanel, &mut ScrollPosition)>,
) {
    let current = (ui.world_panel, ui.context_panel, ui.panel, politics.atlas);
    if *previous == Some(current) {
        return;
    }
    if previous.is_some_and(|old| old.2 != ui.panel) {
        for (side, mut scroll) in &mut scrolls {
            if side.0 {
                scroll.y = 0.;
            }
        }
    }
    *previous = Some(current);
    viewport.left = if ui.world_panel { 272. } else { 14. };
    viewport.right = if ui.context_panel { 314. } else { 14. };
    viewport.top = 76.;
    viewport.bottom = 122.;
    for (mut node, world, inspector, page, atlas, source, caption, track) in &mut nodes {
        let shown = if world.is_some() {
            Some(ui.world_panel)
        } else if inspector.is_some() {
            Some(ui.context_panel)
        } else if let Some(page) = page {
            Some(page.0 == ui.panel)
        } else if atlas.is_some() || caption.is_some() {
            Some(politics.atlas)
        } else {
            track.map(|t| {
                if t.0 {
                    ui.context_panel
                } else {
                    ui.world_panel
                }
            })
        };
        if let Some(shown) = shown {
            node.display = if shown { Display::Flex } else { Display::None };
        }
        if atlas.is_some() || source.is_some() || caption.is_some() {
            node.left = px(viewport.left);
            node.right = px(viewport.right + 12.);
        }
    }
}
fn handle_context_request(
    mut control: ResMut<PlayerControl>,
    mut ui: ResMut<UiState>,
    mut sandbox: ResMut<SandboxRuntime>,
) {
    if !control.interact_requested {
        return;
    }
    control.interact_requested = false;
    ui.context_panel = true;
    ui.panel = Panel::Market;
    if let Some(unit) = sandbox.state.living.active_unit(sandbox.player_id) {
        sandbox.last_message = if let Some(shop) = sandbox.state.living.nearest_shop(unit.position)
        {
            let distance = reach_distance(unit.position, shop.position);
            if distance <= 20. {
                format!("{} is open. Choose an item to buy or sell.", shop.name)
            } else {
                format!(
                    "{} is {:.0} m away. Order your actor to the market or walk there.",
                    shop.name, distance
                )
            }
        } else {
            "No market is loaded in this settlement.".into()
        };
    }
}
/// Publish input-layer feedback once. Domain/network command results run later
/// and retain priority, so selection errors never silently disappear in a field.
fn publish_control_status(
    control: Res<PlayerControl>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut previous: Local<String>,
) {
    if !control.status.is_empty() && *previous != control.status {
        sandbox.last_message = control.status.clone();
        previous.clone_from(&control.status);
    }
}
fn update_scrollbars(
    panels: Query<(&SidePanel, &ScrollPosition, &ComputedNode)>,
    mut thumbs: Query<(&ScrollThumb, &mut Node)>,
) {
    for (thumb, mut node) in &mut thumbs {
        if let Some((_, scroll, computed)) = panels.iter().find(|(p, _, _)| p.0 == thumb.0) {
            let view = computed.size().y * computed.inverse_scale_factor();
            let content = computed.content_size().y * computed.inverse_scale_factor();
            let extent = (content - view).max(0.);
            let height = (view * view / content.max(1.)).clamp(24., view.max(24.));
            node.height = px(height);
            node.top = px(if extent > 0. {
                (scroll.y / extent).clamp(0., 1.) * (view - height)
            } else {
                0.
            });
            node.display = if extent > 1. {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}
fn local_only(action: Action) -> bool {
    matches!(
        action,
        Action::Pause
            | Action::Speed
            | Action::Season
            | Action::Noon
            | Action::Camp
            | Action::Save
            | Action::Load
            | Action::DrawTerritory
    )
}
fn button_availability(
    mut commands: Commands,
    runtime: Res<WorldRuntime>,
    net: Option<Res<NetworkStatus>>,
    sandbox: Res<SandboxRuntime>,
    ui: Res<UiState>,
    control: Res<PlayerControl>,
    buttons: Query<(Entity, &ActionButton, Has<InteractionDisabled>)>,
    mut nodes: Query<&mut Node>,
) {
    let player = sandbox.state.players.get(&sandbox.player_id);
    let living = &sandbox.state.living;
    let unit = living.active_unit(sandbox.player_id);
    let shop = unit.and_then(|u| living.nearest_shop(u.position));
    let account = living.accounts.get(&sandbox.player_id);
    let owns = living
        .units
        .values()
        .filter(|u| u.owner == Some(sandbox.player_id))
        .count();
    for (entity, button, disabled) in &buttons {
        let unavailable = match button.0 {
            Action::Buy(index) | Action::Sell(index) => match (player, unit, shop, account) {
                (Some(p), Some(u), Some(s), Some(a)) => {
                    let item = TRADE_ITEMS[index].0;
                    let n = ui.trade_quantity;
                    let price = s.prices.get(item);
                    let far = u
                        .position
                        .surface_distance_m(s.position)
                        .hypot(u.position.altitude_m - s.position.altitude_m)
                        > 20.;
                    far || price.is_none_or(|price| {
                        if matches!(button.0, Action::Buy(_)) {
                            s.stock.get(item).copied().unwrap_or(0) < n
                                || a.money < price.buy * n as u64
                        } else {
                            p.inventory.get(item).copied().unwrap_or(0) < n
                                || s.money < price.sell * n as u64
                        }
                    })
                }
                _ => true,
            },
            Action::Eat | Action::Drink => {
                unit.is_none_or(|u| {
                    !u.is_person()
                        || selected_body(&sandbox, &control).is_some_and(|shown| shown.id != u.id)
                }) || player.is_none_or(|p| {
                    p.inventory
                        .get(if matches!(button.0, Action::Eat) {
                            "bread"
                        } else {
                            "water"
                        })
                        .copied()
                        .unwrap_or(0)
                        == 0
                })
            }
            Action::SelectOwned(index) => {
                if let Ok(mut node) = nodes.get_mut(entity) {
                    let display = if index < owns {
                        Display::Flex
                    } else {
                        Display::None
                    };
                    if node.display != display {
                        node.display = display;
                    }
                }
                index >= owns
            }
            Action::Possess => {
                !living.enabled
                    || selected_body(&sandbox, &control)
                        .is_none_or(|u| u.owner != Some(sandbox.player_id))
            }
            Action::SelectAll | Action::CycleActor | Action::GoToMarket => !living.enabled,
            Action::ToggleProduction => selected_site(&sandbox, &ui).is_none_or(|site| {
                site.owner != Some(sandbox.player_id)
                    || player.is_none_or(|p| reach_distance(p.position, site.position) > 20.)
            }),
            _ => false,
        };
        let offline = net.as_ref().is_some_and(|n| !n.connected)
            && !matches!(
                button.0,
                Action::ToggleWorld
                    | Action::ToggleInspector
                    | Action::Panel(_)
                    | Action::Atlas
                    | Action::Overview
                    | Action::Mode(_)
            );
        let should_disable =
            (local_only(button.0) && runtime.network_authority) || unavailable || offline;
        if should_disable && !disabled {
            commands.entity(entity).insert(InteractionDisabled);
        } else if !should_disable && disabled {
            commands.entity(entity).remove::<InteractionDisabled>();
        }
    }
}
fn button_style(
    ui: Res<UiState>,
    control: Res<PlayerControl>,
    sandbox: Res<SandboxRuntime>,
    mut buttons: Query<
        (
            &ActionButton,
            &Hovered,
            Has<Pressed>,
            Has<InteractionDisabled>,
            &Children,
            &mut BackgroundColor,
        ),
        With<Button>,
    >,
    mut labels: Query<&mut TextColor>,
) {
    for (action, hover, pressed, disabled, children, mut color) in &mut buttons {
        let active = match action.0 {
            Action::Panel(panel) => panel == ui.panel,
            Action::Mode(mode) => mode == control.mode,
            Action::ToggleWorld => ui.world_panel,
            Action::ToggleInspector => ui.context_panel,
            Action::SelectOwned(i) => sandbox
                .state
                .living
                .units
                .values()
                .filter(|u| u.owner == Some(sandbox.player_id))
                .nth(i)
                .is_some_and(|u| control.selected.contains(&u.id)),
            _ => false,
        };
        color.0 = if disabled {
            Color::srgb(0.075, 0.115, 0.095)
        } else if pressed || active {
            Color::srgb(0.22, 0.357, 0.263)
        } else if hover.get() {
            Color::srgb(0.165, 0.282, 0.231)
        } else {
            SURFACE
        };
        for child in children.iter() {
            if let Ok(mut text) = labels.get_mut(child) {
                text.0 = if disabled {
                    Color::srgb(0.47, 0.55, 0.49)
                } else if active {
                    ACCENT
                } else {
                    TEXT
                };
            }
        }
    }
}
fn scroll_panels(
    window: Single<&Window>,
    scroll: Res<AccumulatedMouseScroll>,
    ui: Res<UiState>,
    mut panels: Query<(&SidePanel, &mut ScrollPosition, &ComputedNode)>,
) {
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.y < 76. || cursor.y > window.height() - 122. {
        return;
    }
    let delta = scroll.delta.y
        * if scroll.unit == bevy::input::mouse::MouseScrollUnit::Line {
            34.
        } else {
            1.
        };
    for (panel, mut position, node) in &mut panels {
        if (!panel.0 && ui.world_panel && cursor.x < 260.)
            || (panel.0 && ui.context_panel && cursor.x > window.width() - 314.)
        {
            let max = (node.content_size().y - node.size().y).max(0.) * node.inverse_scale_factor();
            position.y = (position.y - delta).clamp(0., max);
        }
    }
}
fn edit_text(
    mut events: MessageReader<KeyboardInput>,
    mut ui: ResMut<UiState>,
    mut queue: ResMut<UiQueue>,
) {
    for event in events.read() {
        if !ui.editing_gps || event.state != ButtonState::Pressed {
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                if queue.0.len() < 64 {
                    queue.0.push_back(Action::JumpGps);
                }
            }
            Key::Escape => ui.editing_gps = false,
            Key::Backspace => {
                ui.gps_text.pop();
            }
            Key::Space
                if matches!(
                    ui.input_mode,
                    InputMode::Coordinates | InputMode::FactionName
                ) =>
            {
                if ui.gps_text.len() < 64 {
                    ui.gps_text.push(' ');
                }
            }
            Key::Character(s) => {
                for c in s.chars() {
                    let allowed = match ui.input_mode {
                        InputMode::Coordinates => {
                            c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+' | ' ')
                        }
                        InputMode::FactionName => !c.is_control(),
                        _ => c.is_ascii_digit(),
                    };
                    if ui.gps_text.len() + c.len_utf8() <= 64 && allowed {
                        ui.gps_text.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}
pub fn selected_site<'a>(
    sandbox: &'a SandboxRuntime,
    ui: &UiState,
) -> Option<&'a terra_bevy::rules::ProductionSite> {
    ui.selected_site
        .and_then(|id| sandbox.state.living.producers.get(&id))
        .or_else(|| sandbox.state.living.producers.values().next())
}
fn selected_body<'a>(
    sandbox: &'a SandboxRuntime,
    control: &PlayerControl,
) -> Option<&'a terra_bevy::rules::LivingUnit> {
    let id = control
        .possessed
        .or_else(|| control.selected.iter().next().copied());
    id.and_then(|id| sandbox.state.living.units.get(&id))
        .or_else(|| sandbox.state.living.active_unit(sandbox.player_id))
}
fn update_needs(
    sandbox: Res<SandboxRuntime>,
    control: Res<PlayerControl>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
    mut bars: Query<(&NeedFill, &mut Node, &mut BackgroundColor)>,
    mut labels: Query<(&NeedLabel, &mut Text)>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < 0.2 {
        return;
    }
    *elapsed = 0.;
    let body = selected_body(&sandbox, &control);
    let biological = body.is_some_and(|u| u.is_person());
    let needs = body
        .filter(|u| u.is_person())
        .map(|u| [u.needs.food, u.needs.water, u.needs.energy])
        .unwrap_or([0.; 3]);
    for (bar, mut node, mut color) in &mut bars {
        let value = needs[bar.0];
        let width = percent((value * 100.).round());
        if node.width != width {
            node.width = width;
        }
        color.0 = if value < 0.25 {
            Color::srgb(0.94, 0.61, 0.54)
        } else if value < 0.5 {
            SAND
        } else {
            ACCENT
        };
    }
    for (label, mut text) in &mut labels {
        let title = ["Food", "Water", "Energy"][label.0];
        let value = if biological {
            format!("{title}   {:.0}%", needs[label.0] * 100.)
        } else {
            format!("{title}   —")
        };
        if **text != value {
            **text = value;
        }
    }
}
fn reach_distance(a: terra_core::GeoPosition, b: terra_core::GeoPosition) -> f64 {
    a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m)
}
fn production_status(site: &terra_bevy::rules::ProductionSite) -> &'static str {
    if !site.enabled {
        return "paused";
    }
    if site.resource_remaining == Some(0) {
        return "source depleted";
    }
    if site
        .inputs
        .iter()
        .any(|(item, n)| site.stock.get(item).copied().unwrap_or(0) < *n)
    {
        return "waiting for inputs";
    }
    let stock: u64 = site.stock.values().map(|n| u64::from(*n)).sum();
    let inputs: u64 = site.inputs.values().map(|n| u64::from(*n)).sum();
    let outputs: u64 = site.outputs.values().map(|n| u64::from(*n)).sum();
    if stock.saturating_sub(inputs).saturating_add(outputs) > 2000 {
        return "storage full";
    }
    "working"
}
fn credits(value: u64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}
fn inventory_line(items: &terra_bevy::rules::Inventory) -> String {
    items
        .iter()
        .map(|(id, n)| format!("{} {id}", n))
        .collect::<Vec<_>>()
        .join(" · ")
}
fn update_labels(
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    sandbox: Res<SandboxRuntime>,
    politics: Res<PoliticalWorld>,
    ui: Res<UiState>,
    control: Res<PlayerControl>,
    render: Res<terra_bevy::render::RenderTelemetry>,
    net: Option<Res<NetworkStatus>>,
    mut texts: Query<(&Label, &mut Text)>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < 0.2 && !ui.is_changed() {
        return;
    }
    *elapsed = 0.;
    let player = sandbox.state.players.get(&sandbox.player_id);
    let life = &sandbox.state.living;
    let body = selected_body(&sandbox, &control);
    let active_body = life.active_unit(sandbox.player_id);
    let shop = active_body.and_then(|u| life.nearest_shop(u.position));
    let account = life.accounts.get(&sandbox.player_id);
    let weather = runtime.effective_weather();
    let stats = &runtime.stream_stats;
    let faction = ui
        .selected_faction
        .and_then(|id| sandbox.state.factions.get(&id))
        .or_else(|| {
            sandbox
                .state
                .factions
                .values()
                .find(|f| f.members.contains_key(&sandbox.player_id))
        });
    for (label, mut text) in &mut texts {
        let value=match label {
            Label::PanelTitle=>match ui.panel{Panel::Command=>"Command",Panel::Build=>"Build and farm",Panel::Market=>"Market",Panel::Industry=>"Industry",Panel::Factions=>"Factions"}.into(),
            Label::Location=>ui.location.clone(),
            Label::Coordinates=>if ui.editing_gps&&matches!(ui.input_mode,InputMode::Coordinates){format!("{}|  Enter",ui.gps_text)}else{format!("{:.5}°, {:.5}°",runtime.focus.latitude,runtime.focus.longitude)},
            Label::Clock=>format!("{}\n{:?} · {:.0}× {}{}",runtime.clock.label(),season(&runtime.focus,&runtime.clock),runtime.clock.time_scale,if runtime.clock.paused{"PAUSED"}else{""},if runtime.network_authority{" SERVER"}else{""}),
            Label::Weather=>format!("{:.1}°C · wind {:.1} m/s\nCloud {:.0}% · rain {:.1} mm/h\n{} weather",weather.temperature_c,weather.wind_speed_m_s,weather.cloud_cover*100.,weather.precipitation_mm_h,if !runtime.config.layers.contains(Layer::Weather){"Disabled"}else if weather.synthetic{"Synthetic"}else{"Current model"}),
            Label::Streaming=>format!("{} chunks · {} rendered · {} loading/meshing   |   {:.2}M source vertices   |   {} bodies · {} deliveries   |   F5 save · F9 load",stats.resident,render.chunks,stats.in_flight+stats.pending+render.pending_meshes+render.building_meshes,render.vertices as f64/1_000_000.,life.units.len(),life.deliveries.len()),
            Label::Inventory=>player.map(|p|format!("Shared inventory\n{}",inventory_line(&p.inventory))).unwrap_or_else(||"Waiting for authoritative inventory…".into()),
            Label::Sandbox=>format!("{} plots · {} structures · {} crops\nHome: {}",sandbox.state.plots.len(),sandbox.state.structures.len(),sandbox.state.farms.len(),player.and_then(|p|p.home).map(|id|id.to_string()).unwrap_or_else(||"not set".into())),
            Label::Toast=>if ui.editing_gps{let prompt=match ui.input_mode{InputMode::Coordinates=>"Latitude, longitude",InputMode::FactionName=>"Faction name",InputMode::InvitePlayer=>"Player ID to invite",InputMode::PromotePlayer=>"Player ID for builder role",InputMode::DemotePlayer=>"Player ID for member role"};format!("{prompt}: {}|  Enter confirms · Esc cancels",ui.gps_text)}else{sandbox.last_message.clone()},
            Label::Source=>if politics.atlas{"POLITICAL ATLAS · source claims and authored territories".into()}
                else if runtime.config.heightmap_manifest.is_some(){if resident.chunks.is_empty(){"HEIGHTMAP · loading terrain".into()}else if resident.chunks.values().all(|c|c.is_synthetic()){"SYNTHETIC HEIGHTMAP · authored settlement".into()}else{"IMPORTED HEIGHTMAP · authored settlement".into()}}
                else if runtime.config.historical_pack.is_some(){format!("ERA {} · {}",source_date(runtime.config.data_epoch_unix),if resident.chunks.values().all(|c|c.is_synthetic()){"fictional example"}else{"dated source pack"})}
                else if runtime.config.live_data{format!("LIVE SOURCES · {} sourced / {} fallback chunks",resident.chunks.values().filter(|c|!c.is_synthetic()).count(),resident.chunks.values().filter(|c|c.sources.iter().any(|s|s.synthetic)).count())}
                else{"SYNTHETIC LANDSCAPE · real GPS · authored living settlement".into()},
            Label::Attribution=>attribution(&runtime,&resident,&politics),
            Label::Connection=>net.as_ref().map(|n|format!("{}\nPlayer #{} · tick {}",n.message.chars().take(28).collect::<String>(),sandbox.player_id,n.tick)).unwrap_or_else(||format!("LOCAL WORLD · Terra 0.2\nPlayer #{}",sandbox.player_id)),
            Label::Layer(layer)=>format!("{}  {layer:?}",if runtime.config.layers.contains(*layer){"ON"}else{"OFF"}),
            Label::Politics=>politics.describe(runtime.focus,&runtime.clock),
            Label::Faction=>format!("You: player #{}\n{}",sandbox.player_id,faction.map(|f|format!("{} · #{}\nRole: {} · {} members\n{} shared plots",f.name,f.id,f.members.get(&sandbox.player_id).map(|r|format!("{r:?}")).unwrap_or_else(||"visitor".into()),f.members.len(),sandbox.state.plots.values().filter(|p|p.faction_owner==Some(f.id)).count())).unwrap_or_else(||"Create a faction or accept an invitation to share land, buildings and farms.".into())),
            Label::Controls=>if politics.drawing{"Click vertices · Enter closes · Backspace undo · Esc cancels".into()}else if politics.atlas{"Wheel zoom · Right-drag pan · Click inspect · M returns to the world".into()}else{match control.mode{
                ControlMode::Rts=>"RTS · Drag select · Shift add · Right-click move · WASD pan · Wheel zoom · Middle-drag orbit · F2 / F3 possess".into(),
                ControlMode::ThirdPerson=>"THIRD PERSON · WASD move · Right-drag look · Shift sprint · R / F flight altitude · E interact · C first person · F1 release".into(),
                ControlMode::FirstPerson=>"FIRST PERSON · WASD move · Right-drag look · Shift sprint · R / F flight altitude · E interact · C third person · F1 release".into(),
            }},
            Label::Mode=>format!("{} · {} selected",control.mode.label().to_uppercase(),control.selected.len()),
            Label::Selected=>body.map(|u|format!("{}\n{:?} · {}\n{}",u.name,u.kind,if u.owner==Some(sandbox.player_id){"Your actor"}else{"Inspect only"},match &u.order { terra_bevy::rules::UnitOrder::Idle=>"Ready".into(),terra_bevy::rules::UnitOrder::Rest=>"Resting".into(),terra_bevy::rules::UnitOrder::Patrol=>"On route".into(),terra_bevy::rules::UnitOrder::Move{destination}=>format!("Moving · {:.0} m",u.position.surface_distance_m(*destination)) })).unwrap_or_else(||"Select an owned actor to command or possess.".into()),
            Label::Wallet=>account.map(|a|format!("{} cr",credits(a.money))).unwrap_or_else(||"— cr".into()),
            Label::Population=>format!("{} inhabitants and vehicles\n{} production facilities · {} markets\n{} cycles · {} deliveries completed\n{} resident purchases",life.units.len(),life.producers.len(),life.shops.len(),life.metrics.production_cycles,life.metrics.deliveries_completed,life.metrics.npc_purchases),
            Label::Owned(i)=>life.units.values().filter(|u|u.owner==Some(sandbox.player_id)).nth(*i).map(|u|format!("{} · {:?}",u.name,u.kind)).unwrap_or_default(),
            Label::Shop=>match(shop,active_body){(Some(s),Some(u))=>format!("{}\n{:.0} m from active actor · till {} cr\n{}",s.name,reach_distance(u.position,s.position),credits(s.money),if s.source_feature.is_some(){"Authored economy at a map feature"}else{"Authored demonstration market"}),_=>"Waiting for a living settlement and market…".into()},
            Label::Quantity=>format!("Quantity: {} · cycle 1 / 5 / 10",ui.trade_quantity),
            Label::Offer(i)=>{let item=TRADE_ITEMS[*i].0;shop.and_then(|s|s.prices.get(item).map(|price|format!("Buy {} / sell {} cr each\nShop {} · you {}",credits(price.buy),credits(price.sell),s.stock.get(item).copied().unwrap_or(0),player.and_then(|p|p.inventory.get(item)).copied().unwrap_or(0)))).unwrap_or_else(||"Not stocked here".into())},
            Label::Industry=>selected_site(&sandbox,&ui).map(|s|format!("{}\n{:?} · {} · {}\n{} → {}\nCycle {:.0}/{:.0} s · {} completed\nStock: {}\n{:.0} m from active actor · manage within 20 m\n{}",s.name,s.kind,if runtime.clock.paused||runtime.clock.time_scale==0.{"time paused"}else{production_status(s)},if s.owner==Some(sandbox.player_id){"your facility"}else{"other owner"},if s.inputs.is_empty(){"finite source".into()}else{inventory_line(&s.inputs)},inventory_line(&s.outputs),s.progress_seconds,s.cycle_seconds,s.cycles_completed,inventory_line(&s.stock),player.map(|p|reach_distance(p.position,s.position)).unwrap_or(0.),if s.source_feature.is_some(){"Authored process at a map feature"}else{"Authored demonstration facility"})).unwrap_or_else(||"No production facilities loaded.".into()),
            Label::Logistics=>{let mut rows=vec![format!("{} active deliveries · {} completed",life.deliveries.len(),life.metrics.deliveries_completed)];for d in life.deliveries.iter().take(6){rows.push(format!("{} × {} · {}",d.amount,d.item,match d.phase{terra_bevy::rules::DeliveryPhase::ToPickup=>"collecting",terra_bevy::rules::DeliveryPhase::ToDropoff=>"delivering"}));}rows.join("\n")},
        };
        let value = if matches!(label, Label::Controls) && runtime.clock.paused {
            format!("PAUSED · Space to resume   |   {value}")
        } else {
            value
        };
        if **text != value {
            **text = value;
        }
    }
}
fn attribution(
    runtime: &WorldRuntime,
    resident: &ResidentChunks,
    politics: &PoliticalWorld,
) -> String {
    if politics.atlas {
        return politics
            .dataset
            .as_ref()
            .map(|d| {
                format!(
                    "{}\n{} · {}\n{}",
                    d.title,
                    d.attribution,
                    d.license,
                    if politics.hover.is_empty() {
                        "Teal: borders · amber: disputed · green: faction-linked"
                    } else {
                        &politics.hover
                    }
                )
            })
            .unwrap_or_else(|| "No political dataset. Load a GeoJSON source.".into());
    }
    let mut records = std::collections::BTreeSet::new();
    for chunk in resident.chunks.values() {
        for source in &chunk.sources {
            records.insert(format!("{} · {}", source.attribution, source.license));
        }
    }
    let mut result = if runtime.config.historical_pack.is_some() {
        format!(
            "Frozen data epoch: {}",
            source_date(runtime.config.data_epoch_unix)
        )
    } else {
        "Terrain and features retain their source records. Population and economy are authored simulation content.".into()
    };
    for record in records.iter().take(3) {
        result.push('\n');
        result.push_str(record);
    }
    if let Some(error) = &runtime.last_error {
        result.push_str(&format!(
            "\nProvider: {}",
            error.chars().take(150).collect::<String>()
        ));
    }
    result
}
fn source_date(epoch: Option<i64>) -> String {
    epoch
        .map(|value| {
            let c = terra_core::WorldClock {
                unix_seconds: value as f64,
                time_scale: 0.,
                paused: true,
            };
            let (y, m, d) = c.date();
            format!("{y:04}-{m:02}-{d:02}")
        })
        .unwrap_or_else(|| "unspecified".into())
}
