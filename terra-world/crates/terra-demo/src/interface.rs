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

pub const TRADE_ITEMS: [(&str, &str); 18] = [
    ("bread", "Bread"),
    ("water", "Water"),
    ("wheat_seed", "Seeds"),
    ("wood", "Wood"),
    ("planks", "Planks"),
    ("stone", "Stone"),
    ("wheat", "Wheat"),
    ("flour", "Flour"),
    ("tool", "Tools"),
    ("berries", "Berries"),
    ("fiber", "Fiber"),
    ("fish", "Fish"),
    ("cooked_fish", "Cooked fish"),
    ("iron_ore", "Iron ore"),
    ("iron_ingot", "Iron ingots"),
    ("fishing_rod", "Fishing rod"),
    ("iron_tool", "Iron tool"),
    ("stone_blocks", "Stone blocks"),
];
pub struct InterfacePlugin;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InterfaceLayout;
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
            .add_plugins(crate::command_input::CommandInputPlugin)
            .add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (
                    edit_text.after(crate::accessibility::ControllerNavigation),
                    sync_layout,
                    adapt_inspector_text,
                    align_inspector_scroll_track,
                    handle_context_request,
                )
                    .in_set(InterfaceLayout)
                    .before(ControlSet::Input)
                    .before(WorldSet::Stream),
            )
            .add_systems(
                Update,
                (publish_control_status, update_immersion)
                    .after(ControlSet::Input)
                    .before(WorldSet::Sandbox),
            )
            .add_systems(
                Update,
                (
                    crate::frontier::refresh_guidance
                        .after(WorldSet::Persistence)
                        .before(button_availability)
                        .before(update_labels),
                    scroll_panels,
                    update_scrollbars.after(scroll_panels),
                    button_availability
                        .after(WorldSet::Persistence)
                        .after(update_labels),
                    button_style.after(button_availability),
                    update_labels.after(WorldSet::Persistence),
                    update_needs.after(WorldSet::Persistence),
                ),
            )
            .add_systems(
                PostUpdate,
                reserve_source_banner.after(bevy::ui::UiSystems::Layout),
            );
    }
}
#[derive(Resource, Default)]
pub struct UiQueue(pub VecDeque<Action>);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Coordinates,
    FactionName,
    CompanyName,
    DriverName,
    RouteName,
    SettlementName,
    InvitePlayer,
    PromotePlayer,
    DemotePlayer,
    RevokeInvite,
    RemoveMember,
    TransferLeader,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    Activities,
    Command,
    Build,
    Market,
    Industry,
    Factions,
    Nature,
    Territory,
    Driving,
    Garage,
    Fleet,
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
    pub performance_panel: bool,
    pub immersive_controls: bool,
    pub panel: Panel,
    pub panel_navigation_open: bool,
    pub trade_quantity: u32,
    pub market: crate::market::MarketUi,
    pub selected_site: Option<u64>,
    pub(crate) industry: crate::industry::IndustryUi,
    pub(crate) building_review: crate::building_review::BuildingReviewUi,
    pub frontier: crate::frontier::FrontierUi,
    pub driving: crate::driving::DrivingUi,
    pub fleet: crate::fleet::FleetUi,
    pub communities: crate::communities::CommunityUi,
    pub(crate) displayed_consume_target: Option<crate::actor_summary::ConsumeTarget>,
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
            performance_panel: false,
            immersive_controls: true,
            panel: Panel::Activities,
            panel_navigation_open: false,
            trade_quantity: 1,
            market: crate::market::MarketUi::default(),
            selected_site: None,
            industry: Default::default(),
            building_review: Default::default(),
            frontier: crate::frontier::FrontierUi::default(),
            driving: crate::driving::DrivingUi::default(),
            fleet: crate::fleet::FleetUi::default(),
            communities: crate::communities::CommunityUi::default(),
            displayed_consume_target: None,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Action {
    GraphicsQuality,
    Atmosphere,
    Diagnostics,
    ResetDiagnostics,
    ImmersiveControls,
    Frontier(crate::frontier::FrontierAction),
    FrontierReviewed {
        action: crate::frontier::FrontierAction,
        token: u64,
    },
    Driving(crate::driving::DrivingAction),
    Fleet(crate::fleet::FleetAction),
    FleetReviewed {
        action: crate::fleet::FleetAction,
        token: u64,
    },
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
    ReviewDamage(u64),
    ConfirmDamage,
    ConfirmDamageReviewed(u64),
    CancelDamage,
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
    Community(crate::communities::CommunityAction),
    ToggleWorld,
    ToggleInspector,
    TogglePanelNavigation,
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
    NextMarket,
    Eat,
    Drink,
    ConsumeSelected {
        target: crate::actor_summary::ConsumeTarget,
        water: bool,
    },
    GoToMarket,
    NextSite,
    GoToSite,
    ToggleProduction,
    IndustryReviewed {
        token: u64,
        travel: bool,
    },
}
#[derive(Component)]
pub(crate) enum Label {
    GraphicsQuality,
    Atmosphere,
    ImmersiveControls,
    Diagnostics,
    Persistence,
    Save,
    Load,
    PauseClock,
    Frontier(crate::frontier::FrontierLabel),
    Driving(crate::driving::DrivingLabel),
    Fleet(crate::fleet::FleetLabel),
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
    Community(crate::communities::CommunityLabel),
    Controls,
    Mode,
    Selected,
    SelectionActions,
    Wallet,
    Population,
    Performance,
    Owned(usize),
    Shop,
    Offer(usize),
    Quantity,
    Industry,
    ProductionOperation,
    ProductionProblem,
    DamageTarget,
    DamageReview,
    Logistics,
    PanelTitle,
    PanelNavigation,
}
#[derive(Component)]
pub(crate) struct ActionButton(pub(crate) Action);
#[derive(Component)]
struct SidePanel(bool);
#[derive(Component)]
struct PanelPage(Panel);
#[derive(Component)]
struct WorldDrawer;
#[derive(Component)]
struct PerformancePanel;
#[derive(Component)]
struct Inspector;
#[derive(Component)]
struct InspectorTabCell;
#[derive(Component)]
struct ScrollThumb(ScrollTarget);
#[derive(Component)]
struct ScrollTrack(ScrollTarget);
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScrollTarget {
    World,
    Inspector,
    Performance,
}
#[derive(Component)]
struct AtlasControls;
#[derive(Component)]
struct SourceBanner;
#[derive(Component)]
struct InspectorNavigation;
#[derive(Component)]
struct InspectorNavigationToggle;
#[derive(Component)]
struct AtlasCaption;
#[derive(Component)]
struct HudFooter;
/// Bottom spacing shared with presentation panels that float above the footer.
#[derive(Component)]
pub(crate) struct AboveFooter(pub f32);
#[derive(Component)]
struct NeedFill(usize);
#[derive(Component)]
struct NeedLabel(usize);
pub(crate) const PANEL: Color = Color::srgb(0.055, 0.095, 0.083);
const SURFACE: Color = Color::srgb(0.105, 0.188, 0.16);
pub(crate) const TEXT: Color = Color::srgb(0.933, 0.945, 0.894);
pub(crate) const MUTED: Color = Color::srgb(0.698, 0.769, 0.710);
pub(crate) const ACCENT: Color = Color::srgb(0.702, 0.863, 0.557);
pub(crate) const SAND: Color = Color::srgb(0.902, 0.765, 0.545);
pub(crate) fn text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
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
pub(crate) fn button(parent: &mut ChildSpawnerCommands, title: &str, action: Action) {
    let label = match action {
        Action::Save => Some(Label::Save),
        Action::Load => Some(Label::Load),
        Action::Pause => Some(Label::PauseClock),
        _ => None,
    };
    button_with_label(parent, title, action, label);
}
pub(crate) fn button_with_label(
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
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(SURFACE),
            BorderColor::all(Color::NONE),
            observe(
                move |_: On<Activate>,
                      mut q: ResMut<UiQueue>,
                      keyboard: Option<Res<crate::accessibility::ControllerKeyboard>>,
                      ui: Option<Res<UiState>>| {
                    if keyboard
                        .as_ref()
                        .is_some_and(|keyboard| keyboard.blocks_parent_actions())
                    {
                        return;
                    }
                    if q.0.len() < 64 {
                        if matches!(action, Action::Eat | Action::Drink) {
                            if let Some(target) =
                                ui.as_ref().and_then(|ui| ui.displayed_consume_target)
                            {
                                q.0.push_back(Action::ConsumeSelected {
                                    target,
                                    water: matches!(action, Action::Drink),
                                });
                            }
                        } else if matches!(action, Action::GoToSite | Action::ToggleProduction) {
                            if let Some(bound) = ui.as_ref().and_then(|ui| {
                                crate::industry::bind(ui, matches!(action, Action::GoToSite))
                            }) {
                                q.0.push_back(bound);
                            }
                        } else if matches!(action, Action::Damage | Action::ConfirmDamage) {
                            if let Some(bound) = ui.as_ref().and_then(|ui| {
                                crate::building_review::bind(
                                    ui,
                                    matches!(action, Action::ConfirmDamage),
                                )
                            }) {
                                q.0.push_back(bound);
                            }
                        } else if let Action::Fleet(action) = action {
                            if let Some(bound) = ui
                                .as_ref()
                                .and_then(|ui| crate::fleet::bind_operation(ui, action))
                            {
                                q.0.push_back(bound);
                            }
                        } else if let Action::Community(action) = action {
                            if let Some(bound) = ui
                                .as_ref()
                                .and_then(|ui| crate::communities::bind_review_action(ui, action))
                            {
                                q.0.push_back(Action::Community(bound));
                            }
                        } else if let Action::Frontier(action) = action {
                            if let Some(bound) = ui
                                .as_ref()
                                .and_then(|ui| crate::frontier::bind_action(ui, action))
                            {
                                q.0.push_back(bound);
                            }
                        } else {
                            q.0.push_back(action);
                        }
                    }
                },
            ),
        ))
        .with_children(|p| {
            let mut e = p.spawn(text(title, 12., TEXT));
            if let Some(label) = label {
                e.insert(label);
            }
        });
}
pub(crate) fn heading(parent: &mut ChildSpawnerCommands, title: &str) {
    parent.spawn(text(title, 13., ACCENT)).insert(Node {
        flex_shrink: 0.,
        margin: UiRect::top(px(14)),
        ..default()
    });
}
pub(crate) fn pair(parent: &mut ChildSpawnerCommands, a: (&str, Action), b: (&str, Action)) {
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
/// Keep navigation inside the inspector's existing 300px hit-test boundary.
/// Enlarged text uses two columns without rebuilding buttons or losing focus.
pub(crate) fn inspector_tab_cell() -> impl Bundle {
    (
        InspectorTabCell,
        Node {
            width: percent(31.8),
            min_width: px(0),
            flex_shrink: 0.,
            ..default()
        },
    )
}
pub(crate) fn compact_inspector_navigation(height: f32, text_scale: f32) -> bool {
    let available = height - 76. - (96. * text_scale + 26.);
    available < if text_scale > 1. { 520. } else { 400. }
}
fn adapt_inspector_text(
    settings: Res<crate::player_input::InputSettings>,
    ui: Res<UiState>,
    windows: Query<&Window>,
    mut nodes: Query<
        (
            &mut Node,
            Has<InspectorTabCell>,
            Has<InspectorNavigation>,
            Has<InspectorNavigationToggle>,
        ),
        Or<(
            With<InspectorTabCell>,
            With<InspectorNavigation>,
            With<InspectorNavigationToggle>,
        )>,
    >,
) {
    let width = percent(if settings.text_scale > 1. { 49. } else { 31.8 });
    let compact = windows
        .iter()
        .next()
        .is_some_and(|window| compact_inspector_navigation(window.height(), settings.text_scale));
    for (mut node, cell, navigation, toggle) in &mut nodes {
        if cell && node.width != width {
            node.width = width;
        }
        if navigation {
            node.display = if !compact || ui.panel_navigation_open {
                Display::Flex
            } else {
                Display::None
            };
        } else if toggle {
            node.display = if compact {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}

/// Text scale and a longer active objective can make the source banner wrap.
/// Use its measured edge so clicking that overlay cannot order terrain behind it.
fn reserve_source_banner(
    banners: Query<(&ComputedNode, &UiGlobalTransform), With<SourceBanner>>,
    politics: Res<PoliticalWorld>,
    mut viewport: ResMut<WorldViewport>,
) {
    let top = if politics.atlas {
        76.
    } else {
        banners
            .iter()
            .next()
            .filter(|(node, _)| !node.is_empty())
            .map_or(76., |(node, transform)| {
                ((transform.translation.y + node.size().y * 0.5) * node.inverse_scale_factor() + 8.)
                    .max(76.)
            })
    };
    if top.is_finite() && viewport.top != top {
        viewport.top = top;
    }
}
fn align_inspector_scroll_track(
    panels: Query<(&SidePanel, &ComputedNode, &UiGlobalTransform)>,
    mut tracks: Query<(&ScrollTrack, &mut Node)>,
) {
    let Some((_, panel, transform)) = panels.iter().find(|(side, _, _)| side.0) else {
        return;
    };
    if panel.is_empty() {
        return;
    }
    // Header wrapping and text scale change the body's actual top. Following
    // its measured viewport keeps the scrollbar out of the navigation rows.
    let top = (transform.translation.y - panel.size().y * 0.5) * panel.inverse_scale_factor();
    if !top.is_finite() || top < 0. {
        return;
    }
    for (track, mut node) in &mut tracks {
        if track.0 == ScrollTarget::Inspector && node.top != px(top) {
            node.top = px(top);
        }
    }
}
pub(crate) fn page(
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
            button_with_label(p, "", Action::GraphicsQuality, Some(Label::GraphicsQuality));
            button_with_label(p, "", Action::Atmosphere, Some(Label::Atmosphere));
            button_with_label(
                p,
                "",
                Action::ImmersiveControls,
                Some(Label::ImmersiveControls),
            );
            button(p, "Performance [F12]", Action::Diagnostics);
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
            heading(p, "Simulation diagnostics");
            p.spawn((Label::Performance, text("", 12., MUTED)));
            heading(p, "Local session");
            pair(p, ("Save [F5]", Action::Save), ("Load [F9]", Action::Load));
            p.spawn((Label::Persistence, text("", 11., MUTED)));
        });
    commands
        .spawn((
            PerformancePanel,
            ScrollPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(76),
                left: px(14),
                bottom: px(122),
                width: px(300),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                border_radius: BorderRadius::all(px(8)),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            GlobalZIndex(20),
        ))
        .with_children(|p| {
            p.spawn(text("Performance", 20., TEXT));
            pair(
                p,
                ("Close [F12]", Action::Diagnostics),
                ("Reset timings", Action::ResetDiagnostics),
            );
            p.spawn((
                Label::Diagnostics,
                text("Collecting measured samples…", 12., TEXT),
            ));
        });
    commands.spawn((Inspector,Node {position_type:PositionType::Absolute,top:px(76),right:px(14),bottom:px(122),
        width:px(300),padding:UiRect::all(px(14)),flex_direction:FlexDirection::Column,row_gap:px(10),
        border_radius:BorderRadius::all(px(8)),..default()},BackgroundColor(PANEL),GlobalZIndex(10)))
        .with_children(|p| {
            p.spawn(Node {width:percent(100),justify_content:JustifyContent::SpaceBetween,align_items:AlignItems::Center,flex_wrap:FlexWrap::Wrap,flex_shrink:0.,column_gap:px(8),row_gap:px(4),..default()}).with_children(|p| {
                p.spawn((Label::PanelTitle,text("Command",19.,TEXT)));
                p.spawn((Label::Wallet,text("",13.,SAND)));
            });
            p.spawn((InspectorNavigationToggle,Node {width:percent(100),flex_shrink:0.,display:Display::None,..default()})).with_children(|p| {
                button_with_label(p,"Choose activity",Action::TogglePanelNavigation,Some(Label::PanelNavigation));
            });
            p.spawn((InspectorNavigation,Node {width:percent(100),column_gap:px(5),row_gap:px(5),flex_wrap:FlexWrap::Wrap,flex_shrink:0.,..default()})).with_children(|p| {
                for (title,panel) in [("Activities",Panel::Activities),("Driving",Panel::Driving),("Garage",Panel::Garage),("Fleet",Panel::Fleet),("Command",Panel::Command),("Nature",Panel::Nature),("Territory",Panel::Territory),("Build",Panel::Build),("Market",Panel::Market),("Industry",Panel::Industry),("Factions",Panel::Factions)] {
                    p.spawn(inspector_tab_cell()).with_children(|p|button(p,title,Action::Panel(panel)));
                }
            });
            p.spawn((SidePanel(true),ScrollPosition::default(),Node {width:percent(100),flex_grow:1.,min_height:px(0),
                flex_direction:FlexDirection::Column,row_gap:px(6),overflow:Overflow::scroll_y(),padding:UiRect::right(px(5)),..default()}))
                .with_children(|p| {
                    crate::frontier::panels(p);
                    crate::driving::panels(p);
                    crate::fleet::panels(p);
                    page(p,Panel::Command,|p| {
                        p.spawn((Label::Mode,text("",12.,ACCENT)));
                        p.spawn((Label::Selected,text("Select an owned actor",16.,TEXT)));
                        for (i,name) in ["Food","Water","Energy"].into_iter().enumerate() {
                            p.spawn((NeedLabel(i),text(name,12.,MUTED)));
                            p.spawn((Node {width:percent(100),height:px(5),flex_shrink:0.,border_radius:BorderRadius::all(px(2)),..default()},BackgroundColor(SURFACE)))
                                .with_children(|p|{p.spawn((NeedFill(i),Node {height:percent(100),width:percent(100),border_radius:BorderRadius::all(px(2)),..default()},BackgroundColor(ACCENT)));});
                        }
                        p.spawn((Label::SelectionActions,text("",12.,MUTED)));
                        pair(p,("Possess selected",Action::Possess),("Release [F1]",Action::Release));
                        pair(p,("Select crew",Action::SelectAll),("Focus [Tab]",Action::Follow));
                        button(p,"Select next owned actor",Action::CycleActor);
                        pair(p,("Stop orders",Action::StopUnits),("Rest units",Action::RestUnits));
                        pair(p,("Eat bread",Action::Eat),("Drink water",Action::Drink));
                        heading(p,"Your crew");
                        for i in 0..8 {button_with_label(p,"",Action::SelectOwned(i),Some(Label::Owned(i)));}
                        heading(p,"Living settlement");
                        p.spawn((Label::Population,text("",12.,MUTED)));
                        button(p,"Open market",Action::Panel(Panel::Market));
                        p.spawn(text("RTS: select actors, then right-click the ground to move. Hold Shift to add actors to a selection.",12.,MUTED));
                    });
                    page(p,Panel::Build,|p| {
                        p.spawn((Label::DamageReview,text("",12.,SAND)));
                        button(p,"Cancel building damage",Action::CancelDamage);
                        button(p,"Confirm damage",Action::ConfirmDamage);
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
                        p.spawn((Label::DamageTarget,text("",12.,MUTED)));
                        button(p,"Review damage",Action::Damage);
                        pair(p,("Save [F5]",Action::Save),("Load [F9]",Action::Load));
                    });
                    page(p,Panel::Market,|p| {
                        p.spawn((Label::Shop,text("",12.,TEXT)));
                        pair(p,("Choose market",Action::NextMarket),("Go to market",Action::GoToMarket));
                        button_with_label(p,"Quantity: 1",Action::TradeQuantity,Some(Label::Quantity));
                        p.spawn(text("Totals for the selected quantity. Trade within 20 m.",12.,MUTED));
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
                        button_with_label(p,"",Action::ToggleProduction,Some(Label::ProductionOperation));
                        p.spawn((Label::ProductionProblem,text("",12.,MUTED)));
                        heading(p,"Logistics");
                        p.spawn((Label::Logistics,text("",12.,MUTED)));
                        p.spawn(text("Couriers reserve source stock, then carry it along road routes. Blocked routes retry; undeliverable cargo returns physically to its supplier. Production waits for inputs; moving vehicles yield to traffic ahead.",12.,MUTED));
                    });
                    page(p,Panel::Factions,|p| {
                        crate::communities::build(p);
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
            HudFooter,
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
    for target in [
        ScrollTarget::World,
        ScrollTarget::Inspector,
        ScrollTarget::Performance,
    ] {
        let right = target == ScrollTarget::Inspector;
        commands
            .spawn((
                ScrollTrack(target),
                Node {
                    position_type: PositionType::Absolute,
                    top: if right { px(242) } else { px(90) },
                    bottom: px(136),
                    left: if right {
                        Val::Auto
                    } else if target == ScrollTarget::Performance {
                        px(308)
                    } else {
                        px(254)
                    },
                    right: if right { px(19) } else { Val::Auto },
                    width: px(3),
                    ..default()
                },
                BackgroundColor(SURFACE),
                GlobalZIndex(21),
            ))
            .with_children(|p| {
                p.spawn((
                    ScrollThumb(target),
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
    settings: Res<crate::player_input::InputSettings>,
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
        Option<&PerformancePanel>,
        Option<&HudFooter>,
        Option<&AboveFooter>,
    )>,
    mut previous: Local<
        Option<(
            bool,
            bool,
            Panel,
            bool,
            bool,
            u32,
            crate::fleet::FleetTab,
            u64,
            u64,
            u64,
        )>,
    >,
    mut scrolls: Query<(&SidePanel, &mut ScrollPosition)>,
) {
    let current = (
        ui.world_panel,
        ui.context_panel,
        ui.panel,
        politics.atlas,
        ui.performance_panel,
        settings.text_scale.to_bits(),
        ui.fleet.tab,
        ui.communities.scroll_revision,
        ui.frontier.scroll_revision,
        ui.building_review.scroll_revision,
    );
    if *previous == Some(current) {
        return;
    }
    if previous.is_some_and(|old| {
        old.2 != ui.panel
            || old.6 != ui.fleet.tab
            || old.7 != ui.communities.scroll_revision
            || old.8 != ui.frontier.scroll_revision
            || old.9 != ui.building_review.scroll_revision
    }) {
        for (side, mut scroll) in &mut scrolls {
            if side.0 {
                scroll.y = 0.;
            }
        }
    }
    *previous = Some(current);
    viewport.left = if ui.performance_panel {
        326.
    } else if ui.world_panel {
        272.
    } else {
        14.
    };
    viewport.right = if ui.context_panel { 314. } else { 14. };
    viewport.top = 76.;
    let footer_height = 96. * settings.text_scale;
    viewport.bottom = footer_height + 26.;
    for (
        mut node,
        world,
        inspector,
        page,
        atlas,
        source,
        caption,
        track,
        performance,
        footer,
        above,
    ) in &mut nodes
    {
        if footer.is_some() {
            node.height = px(footer_height);
        }
        if world.is_some() || inspector.is_some() || performance.is_some() {
            node.bottom = px(viewport.bottom);
        } else if caption.is_some() {
            node.bottom = px(viewport.bottom + 6.);
        } else if track.is_some() {
            node.bottom = px(viewport.bottom + 14.);
        } else if let Some(above) = above {
            node.bottom = px(viewport.bottom + above.0);
        }
        let shown = if world.is_some() {
            Some(ui.world_panel)
        } else if inspector.is_some() {
            Some(ui.context_panel)
        } else if performance.is_some() {
            Some(ui.performance_panel)
        } else if let Some(page) = page {
            Some(page.0 == ui.panel)
        } else if atlas.is_some() || caption.is_some() {
            Some(politics.atlas)
        } else {
            track.map(|t| match t.0 {
                ScrollTarget::Inspector => ui.context_panel,
                ScrollTarget::World => ui.world_panel,
                ScrollTarget::Performance => ui.performance_panel,
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
    mut queue: ResMut<UiQueue>,
) {
    if !control.interact_requested {
        return;
    }
    control.interact_requested = false;
    ui.context_panel = true;
    let Some(position) = sandbox
        .state
        .living
        .active_unit(sandbox.player_id)
        .map(|u| u.position)
    else {
        return;
    };
    if crate::driving::handle_vehicle_interaction(&mut sandbox, &mut ui, &mut queue) {
        return;
    }
    let shop = sandbox
        .state
        .living
        .nearest_shop(position)
        .map(|s| (s.name.clone(), reach_distance(position, s.position)));
    let node = sandbox
        .state
        .resource_nodes
        .values()
        .filter(|n| {
            n.remaining > 0
                && crate::frontier::RESOURCE_ITEMS
                    .iter()
                    .any(|(item, _)| *item == n.item)
        })
        .min_by(|a, b| {
            reach_distance(position, a.position).total_cmp(&reach_distance(position, b.position))
        });
    if let Some(node) = node.filter(|n| {
        reach_distance(position, n.position) <= 20.
            && shop
                .as_ref()
                .is_none_or(|(_, d)| reach_distance(position, n.position) < *d)
    }) {
        ui.panel = Panel::Nature;
        ui.frontier.resource_kind = crate::frontier::RESOURCE_ITEMS
            .iter()
            .position(|(item, _)| *item == node.item)
            .unwrap_or(0);
        ui.frontier.resource = Some(node.id.clone());
        sandbox.last_message = format!(
            "{} patch is nearby. Inspect its yield and gather in Nature.",
            node.item
        );
    } else {
        ui.panel = Panel::Market;
        sandbox.last_message = match shop {
            Some((name, distance)) if distance <= 20. => {
                format!("{name} is open. Choose an item to buy or sell.")
            }
            Some((name, distance)) => format!(
                "{name} is {distance:.0} m away. Order your actor there, or use N to find resources nearby."
            ),
            None => "No market is loaded. Open Activities with J or Nature with N.".into(),
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

#[derive(Default)]
struct ImmersivePanels(Option<(bool, bool)>);
impl ImmersivePanels {
    fn update(&mut self, mode: ControlMode, ui: &mut UiState) {
        if mode.is_possessed() && ui.immersive_controls {
            if self.0.is_none() && !ui.editing_gps {
                self.0 = Some((ui.world_panel, ui.context_panel));
                ui.world_panel = false;
                ui.context_panel = false;
            }
        } else if let Some((world, context)) = self.0.take() {
            // Preserve a panel the player deliberately opened while possessing.
            ui.world_panel |= world && !ui.performance_panel;
            ui.context_panel |= context;
        }
    }
}
fn update_immersion(
    control: Res<PlayerControl>,
    mut ui: ResMut<UiState>,
    mut panels: Local<ImmersivePanels>,
) {
    let transition = if control.mode.is_possessed() && ui.immersive_controls {
        panels.0.is_none() && !ui.editing_gps
    } else {
        panels.0.is_some()
    };
    if transition {
        panels.update(control.mode, &mut ui);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn switching_fleet_section_reveals_its_start_and_keeps_other_scroll_positions() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiState>()
            .init_resource::<WorldViewport>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .add_systems(Update, sync_layout);
        let inspector = app
            .world_mut()
            .spawn((SidePanel(true), ScrollPosition(Vec2::new(0., 170.))))
            .id();
        let world = app
            .world_mut()
            .spawn((SidePanel(false), ScrollPosition(Vec2::new(0., 95.))))
            .id();
        app.world_mut().resource_mut::<UiState>().panel = Panel::Fleet;
        app.update();
        app.world_mut().resource_mut::<UiState>().fleet.tab = crate::fleet::FleetTab::Finances;
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(inspector).unwrap().y, 0.);
        assert_eq!(app.world().get::<ScrollPosition>(world).unwrap().y, 95.);
        app.world_mut()
            .get_mut::<ScrollPosition>(inspector)
            .unwrap()
            .y = 60.;
        app.world_mut()
            .resource_mut::<crate::player_input::InputSettings>()
            .text_scale = 1.5;
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(inspector).unwrap().y,
            60.,
            "text scaling preserves the reader's place"
        );
    }

    #[test]
    fn community_review_reveals_confirmation_above_a_scrolled_roster() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiState>()
            .init_resource::<WorldViewport>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .add_systems(Update, sync_layout);
        let inspector = app
            .world_mut()
            .spawn((SidePanel(true), ScrollPosition(Vec2::new(0., 900.))))
            .id();
        let world = app
            .world_mut()
            .spawn((SidePanel(false), ScrollPosition(Vec2::new(0., 95.))))
            .id();
        app.world_mut().resource_mut::<UiState>().panel = Panel::Factions;
        app.update();
        app.world_mut()
            .resource_mut::<UiState>()
            .communities
            .scroll_revision += 1;
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(inspector).unwrap().y, 0.);
        assert_eq!(app.world().get::<ScrollPosition>(world).unwrap().y, 95.);
    }

    #[test]
    fn goal_review_reveals_its_start_without_resetting_other_panels_or_later_refresh() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiState>()
            .init_resource::<WorldViewport>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .add_systems(Update, sync_layout);
        let inspector = app
            .world_mut()
            .spawn((SidePanel(true), ScrollPosition(Vec2::new(0., 740.))))
            .id();
        let world = app
            .world_mut()
            .spawn((SidePanel(false), ScrollPosition(Vec2::new(0., 95.))))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<UiState>()
            .frontier
            .scroll_revision += 1;
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(inspector).unwrap().y, 0.);
        assert_eq!(app.world().get::<ScrollPosition>(world).unwrap().y, 95.);

        app.world_mut()
            .get_mut::<ScrollPosition>(inspector)
            .unwrap()
            .y = 160.;
        let state = terra_bevy::rules::SandboxState::default();
        app.world_mut().resource_mut::<UiState>().frontier.guide = Some(state.activity_guidance(
            1,
            &terra_bevy::rules::Catalog::builtin(),
            terra_bevy::rules::GuideTrack::Suggested,
            None,
        ));
        app.world_mut()
            .resource_mut::<crate::player_input::InputSettings>()
            .text_scale = 1.5;
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(inspector).unwrap().y,
            160.,
            "a refreshed guide and text scaling preserve the reader's new place"
        );
        assert_eq!(app.world().get::<ScrollPosition>(world).unwrap().y, 95.);
    }

    #[test]
    fn small_window_enlarged_text_keeps_navigation_available_without_squeezing_content() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiState>()
            .init_resource::<UiQueue>()
            .add_systems(Startup, setup_ui)
            .add_systems(Update, adapt_inspector_text);
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: (1100, 700).into(),
                ..default()
            })
            .id();
        app.world_mut()
            .resource_mut::<crate::player_input::InputSettings>()
            .text_scale = 1.5;
        app.update();
        let world = app.world_mut();
        let navigation = world
            .query_filtered::<Entity, With<InspectorNavigation>>()
            .single(world)
            .unwrap();
        let toggle = world
            .query_filtered::<Entity, With<InspectorNavigationToggle>>()
            .single(world)
            .unwrap();
        assert_eq!(
            world.get::<Node>(navigation).unwrap().display,
            Display::None
        );
        assert_eq!(world.get::<Node>(toggle).unwrap().display, Display::Flex);
        world.resource_mut::<UiState>().panel_navigation_open = true;
        app.update();
        assert_eq!(
            app.world().get::<Node>(navigation).unwrap().display,
            Display::Flex
        );
        app.world_mut()
            .resource_mut::<UiState>()
            .panel_navigation_open = false;
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set(1440., 900.);
        app.update();
        assert_eq!(
            app.world().get::<Node>(navigation).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Node>(toggle).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn wrapped_guidance_banner_is_excluded_from_world_clicks_at_scaled_dpi() {
        let mut app = App::new();
        app.init_resource::<WorldViewport>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .add_systems(Update, reserve_source_banner);
        app.world_mut().spawn((
            SourceBanner,
            ComputedNode {
                size: Vec2::new(1000., 80.),
                inverse_scale_factor: 0.5,
                ..default()
            },
            UiGlobalTransform::from_xy(700., 200.),
        ));
        app.update();
        let viewport = *app.world().resource::<WorldViewport>();
        let window = Window {
            resolution: (1100, 700).into(),
            ..default()
        };
        assert!(!viewport.contains(&window, Vec2::new(400., 120.), false));
        assert!(viewport.contains(&window, Vec2::new(400., 140.), false));
        app.world_mut().resource_mut::<PoliticalWorld>().atlas = true;
        app.update();
        assert_eq!(app.world().resource::<WorldViewport>().top, 76.);
    }

    #[test]
    fn scaled_footer_reserves_the_same_world_panel_and_floating_hud_bounds() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiState>()
            .init_resource::<UiQueue>()
            .init_resource::<WorldViewport>()
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .add_systems(Startup, setup_ui)
            .add_systems(Update, sync_layout);
        let floating = app
            .world_mut()
            .spawn((Node::default(), AboveFooter(15.)))
            .id();
        for scale in [1., 1.5, 1.] {
            app.world_mut()
                .resource_mut::<crate::player_input::InputSettings>()
                .text_scale = scale;
            app.update();
            let height = 96. * scale;
            let viewport = *app.world().resource::<WorldViewport>();
            assert_eq!(viewport.bottom, height + 26.);
            assert_eq!(
                app.world().get::<Node>(floating).unwrap().bottom,
                px(height + 41.)
            );
            let world = app.world_mut();
            let footer = world
                .query_filtered::<&Node, With<HudFooter>>()
                .single(world)
                .unwrap();
            assert_eq!(footer.height, px(height));
            let inspector = world
                .query_filtered::<&Node, With<Inspector>>()
                .single(world)
                .unwrap();
            assert_eq!(inspector.bottom, px(viewport.bottom));
            let window = Window {
                resolution: (1100, 700).into(),
                ..default()
            };
            let usable = viewport.rect(&window, false);
            assert!(
                usable.max.y < 700. - 12. - height,
                "world hit testing must stop before the enlarged footer"
            );
        }
    }

    #[test]
    fn enlarged_inspector_tabs_keep_actions_scroll_state_and_outer_bounds() {
        let mut app = App::new();
        app.init_resource::<crate::player_input::InputSettings>()
            .init_resource::<UiQueue>()
            .init_resource::<UiState>()
            .add_systems(Startup, setup_ui)
            .add_systems(Update, (adapt_inspector_text, align_inspector_scroll_track));
        app.update();
        let world = app.world_mut();
        let inspector = world
            .query_filtered::<Entity, With<Inspector>>()
            .single(world)
            .unwrap();
        let body = world
            .query::<(Entity, &SidePanel)>()
            .iter(world)
            .find(|(_, side)| side.0)
            .unwrap()
            .0;
        world.entity_mut(body).insert((
            ComputedNode {
                size: Vec2::new(534., 240.),
                content_size: Vec2::new(534., 800.),
                inverse_scale_factor: 0.5,
                ..default()
            },
            UiGlobalTransform::from_xy(2200., 800.),
            ScrollPosition(Vec2::new(0., 77.)),
        ));
        let mut tabs: Vec<_> = world
            .query_filtered::<Entity, With<InspectorTabCell>>()
            .iter(world)
            .collect();
        tabs.sort();
        assert_eq!(
            tabs.len(),
            17,
            "all eleven inspector and six Fleet tabs must adapt"
        );
        for scale in [1., 1.15, 1.3, 1.5, 1.] {
            app.world_mut()
                .resource_mut::<crate::player_input::InputSettings>()
                .text_scale = scale;
            app.update();
            let world = app.world_mut();
            let outer = world.get::<Node>(inspector).unwrap();
            assert_eq!(
                (outer.width, outer.right, outer.top, outer.bottom),
                (px(300), px(14), px(76), px(122))
            );
            let mut current: Vec<_> = world
                .query_filtered::<Entity, With<InspectorTabCell>>()
                .iter(world)
                .collect();
            current.sort();
            assert_eq!(
                current, tabs,
                "rescaling must retain button entities and focus"
            );
            for &cell in &tabs {
                assert_eq!(
                    world.get::<Node>(cell).unwrap().width,
                    percent(if scale > 1. { 49. } else { 31.8 })
                );
                let button = world.get::<Children>(cell).unwrap()[0];
                assert!(matches!(
                    world.get::<ActionButton>(button).unwrap().0,
                    Action::Panel(_) | Action::Fleet(crate::fleet::FleetAction::Tab(_))
                ));
            }
            assert_eq!(world.get::<ScrollPosition>(body).unwrap().y, 77.);
            assert_eq!(
                world.get::<Node>(body).unwrap().overflow,
                Overflow::scroll_y()
            );
            let (_, track) = world
                .query::<(&ScrollTrack, &Node)>()
                .iter(world)
                .find(|(track, _)| track.0 == ScrollTarget::Inspector)
                .unwrap();
            assert_eq!(
                track.top,
                px(340),
                "scrollbar follows the measured body at 2x DPI"
            );
            assert_eq!(track.bottom, px(136));
        }
        let world = app.world_mut();
        let title = world
            .query::<(Entity, &Label)>()
            .iter(world)
            .find(|(_, label)| matches!(label, Label::PanelTitle))
            .unwrap()
            .0;
        let header = world
            .get::<Node>(world.get::<ChildOf>(title).unwrap().parent())
            .unwrap();
        assert_eq!(header.flex_wrap, FlexWrap::Wrap);
        assert_eq!(header.flex_shrink, 0.);
        let (_, release) = world
            .query::<(&ActionButton, &Children)>()
            .iter(world)
            .find(|(action, _)| matches!(action.0, Action::Release))
            .unwrap();
        assert_eq!(world.get::<Text>(release[0]).unwrap().0, "Release [F1]");
    }
}

#[cfg(test)]
mod immersion_tests {
    use super::*;
    #[test]
    fn persistence_recovery_keys_replace_resume_until_the_block_is_cleared() {
        for paused in [false, true] {
            let blocked = controls_footer("WASD move", paused, true, false, true);
            assert!(blocked.contains("F5 retry checkpoint"));
            assert!(blocked.contains("F9 restore last checkpoint"));
            assert!(!blocked.contains("Space"));
            assert!(!blocked.contains("WASD"));
            let terminal = controls_footer("WASD move", paused, true, true, true);
            assert!(terminal.contains("Esc menu → Close failed world"));
            assert!(!terminal.contains("F5") && !terminal.contains("F9"));
            assert!(!terminal.contains("Space") && !terminal.contains("WASD"));
        }
        let recovered = controls_footer("WASD move", true, false, false, true);
        assert!(recovered.contains("Space to resume"));
        assert!(!recovered.contains("retry checkpoint"));
        assert_eq!(
            controls_footer("WASD move", false, false, false, true),
            "WASD move"
        );
        let remapped = controls_footer("↑ move", true, false, false, false);
        assert!(remapped.contains("World [F10]"));
        assert!(!remapped.contains("Space"));
    }

    #[test]
    fn possession_recedes_once_and_preserves_deliberate_context_opening() {
        let mut ui = UiState {
            world_panel: true,
            ..default()
        };
        let mut panels = ImmersivePanels::default();
        panels.update(ControlMode::ThirdPerson, &mut ui);
        assert!(!ui.world_panel && !ui.context_panel);
        ui.context_panel = true;
        ui.panel = Panel::Market;
        panels.update(ControlMode::FirstPerson, &mut ui);
        assert!(ui.context_panel && ui.panel == Panel::Market);
        panels.update(ControlMode::Rts, &mut ui);
        assert!(ui.world_panel && ui.context_panel);
    }
    #[test]
    fn text_entry_and_explicit_keep_panels_preference_are_respected() {
        let mut ui = UiState {
            editing_gps: true,
            ..default()
        };
        let mut panels = ImmersivePanels::default();
        panels.update(ControlMode::ThirdPerson, &mut ui);
        assert!(ui.context_panel);
        ui.editing_gps = false;
        panels.update(ControlMode::ThirdPerson, &mut ui);
        assert!(!ui.context_panel);
        ui.immersive_controls = false;
        panels.update(ControlMode::ThirdPerson, &mut ui);
        assert!(ui.context_panel);
    }
}
fn update_scrollbars(
    panels: Query<(&SidePanel, &ScrollPosition, &ComputedNode)>,
    performance: Query<(&ScrollPosition, &ComputedNode), With<PerformancePanel>>,
    tracks: Query<(&ScrollTrack, &ComputedNode)>,
    mut thumbs: Query<(&ScrollThumb, &mut Node)>,
) {
    for (thumb, mut node) in &mut thumbs {
        let measured = match thumb.0 {
            ScrollTarget::Performance => performance.single().ok(),
            target => panels
                .iter()
                .find(|(p, _, _)| p.0 == (target == ScrollTarget::Inspector))
                .map(|(_, scroll, computed)| (scroll, computed)),
        };
        if let Some((scroll, computed)) = measured {
            let view = computed.size().y * computed.inverse_scale_factor();
            let content = computed.content_size().y * computed.inverse_scale_factor();
            let extent = (content - view).max(0.);
            let track = tracks
                .iter()
                .find(|(target, _)| target.0 == thumb.0)
                .map(|(_, track)| track.size().y * track.inverse_scale_factor())
                .unwrap_or(view)
                .max(0.);
            let height = (track * view / content.max(1.)).clamp(24_f32.min(track), track);
            node.height = px(height);
            node.top = px(if extent > 0. {
                (scroll.y / extent).clamp(0., 1.) * (track - height)
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
    quality: Res<terra_bevy::render::RenderQualitySettings>,
    capabilities: Res<terra_bevy::render::RenderCapabilities>,
    scenes: Option<Res<terra_bevy::render::ActorSceneOverrides>>,
) {
    let player = sandbox.state.players.get(&sandbox.player_id);
    let living = &sandbox.state.living;
    let unit = living.active_unit(sandbox.player_id);
    let actor = crate::actor_summary::ActorSummary::resolve(&sandbox, &control);
    let owns = living
        .units
        .values()
        .filter(|u| u.owner == Some(sandbox.player_id))
        .count();
    for (entity, button, disabled) in &buttons {
        let unavailable = match button.0 {
            Action::Atmosphere => {
                quality.quality == terra_bevy::render::RenderQuality::Performance
                    || !capabilities.atmospheric_sky
            }
            Action::Frontier(action) => crate::frontier::unavailable(
                action,
                &sandbox,
                &ui,
                net.as_ref().is_some_and(|n| n.sandbox_snapshot_truncated),
            ),
            Action::Driving(action) => {
                crate::driving::unavailable(action, &sandbox, &ui, scenes.as_deref())
            }
            Action::Fleet(action) => crate::fleet::unavailable(action, &sandbox, &ui),
            Action::Community(action) => {
                if matches!(
                    action,
                    crate::communities::CommunityAction::Confirm
                        | crate::communities::CommunityAction::Cancel
                ) && let Ok(mut node) = nodes.get_mut(entity)
                {
                    node.display = if crate::communities::has_review(&ui) {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
                crate::communities::unavailable(action, &sandbox, &ui)
            }
            Action::CreateFaction
            | Action::NextFaction
            | Action::AcceptFaction
            | Action::InviteFaction
            | Action::PromoteFaction
            | Action::DemoteFaction
            | Action::SharePlot => crate::communities::legacy_unavailable(button.0, &sandbox, &ui),
            Action::Buy(index) | Action::Sell(index) => {
                crate::market::unavailable(&sandbox, &ui, index, matches!(button.0, Action::Buy(_)))
            }
            Action::NextMarket => living.shops.len() < 2,
            Action::Eat | Action::Drink => {
                sandbox.state.is_player_dormant(sandbox.player_id)
                    || unit.is_none_or(|u| {
                        !u.is_person()
                            || if matches!(button.0, Action::Eat) {
                                u.needs.food >= 0.98
                            } else {
                                u.needs.water >= 0.98
                            }
                            || actor.consume_body != Some(u.id)
                    })
                    || player.is_none_or(|p| {
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
            Action::StopUnits | Action::RestUnits => actor.owned_selected == 0,
            Action::SelectAll | Action::CycleActor | Action::GoToMarket => !living.enabled,
            Action::ToggleProduction => crate::industry::problem(&sandbox, &ui).is_some(),
            Action::GoToSite => selected_site(&sandbox, &ui).is_none(),
            Action::NextSite => living.producers.is_empty(),
            Action::Damage => crate::building_review::unavailable(&sandbox, &ui, false),
            Action::ConfirmDamage | Action::CancelDamage => {
                if let Ok(mut node) = nodes.get_mut(entity) {
                    node.display = if crate::building_review::has_review(&ui) {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
                matches!(button.0, Action::ConfirmDamage)
                    && crate::building_review::unavailable(&sandbox, &ui, true)
            }
            _ => false,
        };
        let offline = net.as_ref().is_some_and(|n| !n.connected)
            && !matches!(
                button.0,
                Action::ToggleWorld
                    | Action::ToggleInspector
                    | Action::GraphicsQuality
                    | Action::Atmosphere
                    | Action::Diagnostics
                    | Action::ResetDiagnostics
                    | Action::ImmersiveControls
                    | Action::TogglePanelNavigation
                    | Action::Frontier(crate::frontier::FrontierAction::ChooseJourney)
                    | Action::Frontier(crate::frontier::FrontierAction::SelectJourney(_))
                    | Action::Frontier(crate::frontier::FrontierAction::ReviewAcceptedSupply)
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
    focus: Res<bevy::input_focus::InputFocus>,
    ui: Res<UiState>,
    control: Res<PlayerControl>,
    sandbox: Res<SandboxRuntime>,
    mut buttons: Query<
        (
            Entity,
            &ActionButton,
            &Hovered,
            Has<Pressed>,
            Has<InteractionDisabled>,
            &Children,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
    mut labels: Query<&mut TextColor>,
) {
    for (entity, action, hover, pressed, disabled, children, mut color, mut border) in &mut buttons
    {
        *border = BorderColor::all(if !disabled && focus.get() == Some(entity) {
            ACCENT
        } else {
            Color::NONE
        });
        let active = match action.0 {
            Action::Panel(panel) => panel == ui.panel,
            Action::Fleet(crate::fleet::FleetAction::Tab(tab)) => tab == ui.fleet.tab,
            Action::Fleet(crate::fleet::FleetAction::ToggleNavigation) => ui.fleet.navigation_open,
            Action::Mode(mode) => mode == control.mode,
            Action::ToggleWorld => ui.world_panel,
            Action::ToggleInspector => ui.context_panel,
            Action::Diagnostics => ui.performance_panel,
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
    viewport: Res<WorldViewport>,
    mut panels: Query<(&SidePanel, &mut ScrollPosition, &ComputedNode)>,
    mut performance: Query<
        (&mut ScrollPosition, &ComputedNode),
        (With<PerformancePanel>, Without<SidePanel>),
    >,
) {
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.y < viewport.top || cursor.y > window.height() - viewport.bottom {
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
    if ui.performance_panel && (14. ..314.).contains(&cursor.x) {
        for (mut position, node) in &mut performance {
            let max = (node.content_size().y - node.size().y).max(0.) * node.inverse_scale_factor();
            position.y = (position.y - delta).clamp(0., max);
        }
    }
}
fn edit_text(
    mut events: MessageReader<KeyboardInput>,
    mut ui: ResMut<UiState>,
    mut queue: ResMut<UiQueue>,
    keyboard: Option<Res<crate::accessibility::ControllerKeyboard>>,
) {
    for event in events.read() {
        if !ui.editing_gps
            || keyboard
                .as_ref()
                .is_some_and(|keyboard| keyboard.is_open() || keyboard.consumed)
            || event.state != ButtonState::Pressed
        {
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                if queue.0.len() < 64 {
                    queue.0.push_back(Action::JumpGps);
                    ui.editing_gps = false;
                }
            }
            Key::Escape => ui.editing_gps = false,
            Key::Backspace => {
                ui.gps_text.pop();
            }
            Key::Space
                if matches!(
                    ui.input_mode,
                    InputMode::Coordinates
                        | InputMode::FactionName
                        | InputMode::SettlementName
                        | InputMode::CompanyName
                        | InputMode::DriverName
                        | InputMode::RouteName
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
                        InputMode::FactionName
                        | InputMode::SettlementName
                        | InputMode::CompanyName
                        | InputMode::DriverName
                        | InputMode::RouteName => !c.is_control(),
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
    crate::industry::selected(sandbox, ui)
}
fn selected_body<'a>(
    sandbox: &'a SandboxRuntime,
    control: &PlayerControl,
) -> Option<&'a terra_bevy::rules::LivingUnit> {
    let id = control
        .possessed
        .or_else(|| control.selected.iter().next().copied());
    id.and_then(|id| sandbox.state.living.units.get(&id))
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
    let actor = crate::actor_summary::ActorSummary::resolve(&sandbox, &control);
    let needs = actor.needs.unwrap_or([0.; 3]);
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
        let value = actor.need_label(label.0);
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
    ui: (
        ResMut<UiState>,
        Res<crate::player_input::InputSettings>,
        Res<crate::command_input::CommandFocus>,
        Res<crate::command_input::WorldPointer>,
    ),
    control: Res<PlayerControl>,
    render: (
        Res<terra_bevy::render::RenderTelemetry>,
        Option<Res<terra_bevy::render::AssetPackDiagnostics>>,
        Option<Res<terra_bevy::render::ActorSceneOverrides>>,
        Res<crate::DemoOptions>,
        Option<Res<crate::performance::CaptureStatus>>,
    ),
    diagnostics: Option<Res<terra_bevy::LivingDiagnostics>>,
    net: Option<Res<NetworkStatus>>,
    mut texts: Query<(&Label, &mut Text)>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
    quality: Res<terra_bevy::render::RenderQualitySettings>,
    capabilities: Res<terra_bevy::render::RenderCapabilities>,
    timings: Res<terra_bevy::WorldDiagnostics>,
    persistence: Res<terra_bevy::RegionalPersistence>,
) {
    let (mut ui, input_settings, command_focus, pointer) = ui;
    let (render, asset_pack, scenes, options, capture) = render;
    *elapsed += time.delta_secs();
    if *elapsed < 0.2 && !ui.is_changed() {
        return;
    }
    *elapsed = 0.;
    crate::market::refresh_displayed(&sandbox, &mut ui);
    crate::fleet::refresh_displayed_quotes(&sandbox, &mut ui);
    crate::fleet::refresh_displayed_operations(&sandbox, &mut ui);
    crate::industry::refresh_displayed(&sandbox, &mut ui);
    crate::building_review::refresh_displayed(&sandbox, &mut ui);
    crate::communities::refresh_displayed(&sandbox, &mut ui);
    crate::driving::refresh_displayed(&sandbox, &mut ui, scenes.as_deref());
    crate::frontier::refresh_displayed_actions(&sandbox, &mut ui);
    let player = sandbox.state.players.get(&sandbox.player_id);
    let life = &sandbox.state.living;
    let actor = crate::actor_summary::ActorSummary::resolve(&sandbox, &control);
    let active_body = life.active_unit(sandbox.player_id);
    ui.displayed_consume_target = actor.consume_target(&sandbox);
    let shop = crate::market::shop(&sandbox, &ui);
    let account = life.accounts.get(&sandbox.player_id);
    let weather = runtime.effective_weather();
    let stats = &runtime.stream_stats;
    for (label, mut text) in &mut texts {
        let value=match label {
            Label::GraphicsQuality=>format!("Graphics: {} · change",quality.quality.label()),
            Label::Atmosphere=>format!("Sky scattering: {}",if quality.quality==terra_bevy::render::RenderQuality::Performance{"off · Performance"}else if !capabilities.atmospheric_sky{"adapter fallback"}else if quality.uses_atmosphere(){"on"}else{"off"}),
            Label::ImmersiveControls=>format!("Focus view: {}",if ui.immersive_controls{"auto-hide panels"}else{"keep panels"}),
            Label::Diagnostics=>{
                let mut report = crate::performance::describe(&timings,&runtime,diagnostics.as_deref(),&render,net.as_deref(),&quality,&persistence,asset_pack.as_deref());
                if let Some(capture) = capture.as_deref() { report.push_str(&format!("\n{}", capture.describe(&options))); }
                report
            },
            Label::Save=>if persistence.enabled{"Checkpoint [F5]"}else{"Save [F5]"}.into(),
            Label::Load=>if persistence.enabled{"Restore [F9]"}else{"Load [F9]"}.into(),
            Label::PauseClock=>format!("{} clock{}",if runtime.clock.paused {"Resume"} else {"Pause"},if !control.mode.is_possessed() && !input_settings.space_has_rts_action() {" [Space]"} else {""}),
            Label::Persistence=>{
                let contract = crate::flow::save_contract_description(runtime.network_authority,persistence.enabled,options.world_profile_path.is_some());
                if persistence.enabled && !runtime.network_authority {format!("{}\n{contract}\nAutomatic checkpoint every {:.0} s and on region changes. Dormant records freeze.",persistence.status_line(),runtime.config.region_checkpoint_seconds)}else{contract.into()}
            },
            Label::Frontier(label)=>crate::frontier::label(*label,&sandbox,&ui,runtime.network_authority,net.as_ref().is_some_and(|n| n.sandbox_snapshot_truncated)),
            Label::Driving(label)=>crate::driving::label(*label,&sandbox,&ui,scenes.as_deref(),&input_settings),
            Label::Fleet(label)=>crate::fleet::label(*label,&sandbox,&ui),
            Label::PanelTitle=>match ui.panel{Panel::Activities=>"Activities",Panel::Nature=>"Nature",Panel::Territory=>"Territory",Panel::Command=>"Command",Panel::Build=>"Build and farm",Panel::Market=>"Market",Panel::Industry=>"Industry",Panel::Factions=>"Factions",Panel::Driving=>"Driving",Panel::Garage=>"Garage",Panel::Fleet=>"Fleet company"}.into(),
            Label::PanelNavigation=>if ui.panel_navigation_open {"Close activity chooser"} else {"Choose activity"}.into(),
            Label::Location=>ui.location.clone(),
            Label::Coordinates=>if ui.editing_gps&&matches!(ui.input_mode,InputMode::Coordinates){format!("{}|  Enter",ui.gps_text)}else{format!("{:.5}°, {:.5}°",runtime.focus.latitude,runtime.focus.longitude)},
            Label::Clock=>format!("{}\n{:?} · {:.0}× {}{}",runtime.clock.label(),season(&runtime.focus,&runtime.clock),runtime.clock.time_scale,if runtime.clock.paused{"PAUSED"}else{""},if runtime.network_authority{" SERVER"}else{""}),
            Label::Weather=>format!("{:.1}°C · wind {:.1} m/s\nCloud {:.0}% · rain {:.1} mm/h\n{} weather",weather.temperature_c,weather.wind_speed_m_s,weather.cloud_cover*100.,weather.precipitation_mm_h,if !runtime.config.layers.contains(Layer::Weather){"Disabled"}else if weather.synthetic{"Synthetic"}else{"Current model"}),
            Label::Streaming=>{
                let context=actor.footer(runtime.network_authority,net.as_ref().is_some_and(|n|n.sandbox_snapshot_truncated));
                if persistence.blocked(){persistence.status_line()}else{
                    let pending=stats.in_flight+stats.pending+render.pending_meshes+render.building_meshes;
                    if pending>0{format!("Loading region · {pending} pending · {context}")}
                    else if control.mode.is_possessed(){format!("{context} · {} cr · J activities",account.map(|a|credits(a.money)).unwrap_or_default())}
                    else{format!("{context} · {}",if runtime.network_authority{"Host saves"}else if persistence.enabled{"F5 checkpoint · F9 review"}else{"F5 save · F9 review"})}
                }
            },
            Label::Inventory=>player.map(|p|format!("Shared inventory\n{}",inventory_line(&p.inventory))).unwrap_or_else(||"Waiting for authoritative inventory…".into()),
            Label::Sandbox=>format!("{} plots · {} structures · {} crops\nHome: {}",sandbox.state.plots.len(),sandbox.state.structures.len(),sandbox.state.farms.len(),player.and_then(|p|p.home).map(|id|id.to_string()).unwrap_or_else(||"not set".into())),
            Label::Toast=>if ui.editing_gps{let prompt=match ui.input_mode{InputMode::Coordinates=>"Latitude, longitude",InputMode::FactionName=>"Faction name",InputMode::CompanyName=>"Company name (up to 32 characters)",InputMode::DriverName=>"Employee name (up to 32 characters)",InputMode::RouteName=>"Route name (up to 32 characters)",InputMode::SettlementName=>"Settlement name",InputMode::InvitePlayer=>"Player ID to invite",InputMode::PromotePlayer=>"Player ID for builder role",InputMode::DemotePlayer=>"Player ID for member role",InputMode::RevokeInvite=>"Player ID whose invitation to revoke",InputMode::RemoveMember=>"Player ID to remove (review follows)",InputMode::TransferLeader=>"Next leader's player ID (review follows)"};format!("{prompt}: {}|  Enter submits · Esc cancels",ui.gps_text)}else{sandbox.last_message.clone()},
            Label::Source=>if politics.atlas{"POLITICAL ATLAS · source claims and authored territories".into()}
                else if runtime.config.heightmap_manifest.is_some(){if resident.chunks.is_empty(){"HEIGHTMAP · loading terrain".into()}else if resident.chunks.values().all(|c|c.is_synthetic()){"SYNTHETIC HEIGHTMAP · authored settlement".into()}else{"IMPORTED HEIGHTMAP · authored settlement".into()}}
                else if runtime.config.historical_pack.is_some(){format!("ERA {} · {}",source_date(runtime.config.data_epoch_unix),if resident.chunks.values().all(|c|c.is_synthetic()){"fictional example"}else{"dated source pack"})}
                else if runtime.config.live_data{format!("LIVE SOURCES · {} sourced / {} fallback chunks",resident.chunks.values().filter(|c|!c.is_synthetic()).count(),resident.chunks.values().filter(|c|c.sources.iter().any(|s|s.synthetic)).count())}
                else{"SYNTHETIC LANDSCAPE · real GPS · authored living settlement".into()},
            Label::Attribution=>attribution(&runtime,&resident,&politics),
            Label::Connection=>net.as_ref().map(|n|format!("{}\nPlayer #{}{}",n.message.chars().take(28).collect::<String>(),sandbox.player_id,if n.resync_pending{" · synchronizing"}else{""})).unwrap_or_else(||format!("LOCAL WORLD · Terra {}\nPlayer #{}",env!("CARGO_PKG_VERSION"),sandbox.player_id)),
            Label::Layer(layer)=>format!("{}  {layer:?}",if runtime.config.layers.contains(*layer){"ON"}else{"OFF"}),
            Label::Politics=>politics.describe(runtime.focus,&runtime.clock),
            Label::Faction=>crate::communities::summary(&sandbox,&ui),
            Label::Community(label)=>crate::communities::describe(*label,&sandbox,&ui),
            Label::Controls=>if command_focus.active{"COMMAND FOCUS · D-pad / arrows / Tab navigate · A / Enter choose · RB/LB or Page Up/Down change page · B / Esc return".into()}else if pointer.active{"RTS · Left stick cursor · Hold A drag-select · RB add · X order · B stop · Y possess · LB next actor · Right stick pan · Triggers zoom · R3 commands".into()}else if politics.drawing{"Click vertices · Enter closes · Backspace undo · Esc cancels".into()}else if politics.atlas{"Wheel zoom · Right-drag pan · Click inspect · M returns to the world".into()}else if control.mode.is_possessed() && active_body.is_some_and(|u|sandbox.state.transport.vehicles.contains_key(&u.id)){input_settings.movement_hint(true)}else{match control.mode{
                ControlMode::Rts=>input_settings.rts_hint(),
                ControlMode::ThirdPerson=>input_settings.movement_hint(false),
                ControlMode::FirstPerson=>input_settings.movement_hint(false),
            }},
            Label::Mode=>format!("{} · {} selected",control.mode.label().to_uppercase(),control.selected.len()),
            Label::Selected=>actor.inspector(runtime.network_authority,net.as_ref().is_some_and(|n|n.sandbox_snapshot_truncated)),
            Label::SelectionActions=>actor.action_hint.clone(),
            Label::Wallet=>account.map(|a|format!("{} cr",credits(a.money))).unwrap_or_else(||"— cr".into()),
            Label::Performance=>if runtime.network_authority {"Simulation runs on the host. Local render statistics appear in the footer; host timings are not replicated.".into()}else{diagnostics.as_ref().map(|d|format!("Living fixed step: {:.2} ms\nPeak since start: {:.2} ms\n{} moving bodies · {} steps\n{}",d.simulation_ms,d.peak_simulation_ms,d.moving,d.fixed_steps,d.last_error.as_deref().unwrap_or("No simulation error"))).unwrap_or_else(||"Living simulation not installed".into())},
            Label::Population=>format!("{}{} inhabitants and vehicles\n{} production facilities · {} markets\n{} cycles · {} deliveries completed\n{} resident purchases",life.units.len(),if runtime.network_authority {" received"}else{""},life.producers.len(),life.shops.len(),life.metrics.production_cycles,life.metrics.deliveries_completed,life.metrics.npc_purchases),
            Label::Owned(i)=>life.units.values().filter(|u|u.owner==Some(sandbox.player_id)).nth(*i).map(|u|format!("{} · {:?}",u.name,u.kind)).unwrap_or_default(),
            Label::Shop=>match(shop,active_body){(Some(s),Some(u))=>format!("{}\n{:.0} m from active actor · till {} cr\n{}",s.name,reach_distance(u.position,s.position),credits(s.money),if s.source_feature.is_some(){"Authored economy at a map feature"}else{"Authored demonstration market"}),_=>if ui.market.selected_shop.is_some(){"The selected market is outside the current view. Explore its location or choose a loaded market.".into()}else{"Waiting for a living settlement and market…".into()}},
            Label::Quantity=>format!("Quantity: {} · cycle 1 / 5 / 10",ui.trade_quantity),
            Label::Offer(i)=>crate::market::offer_label(&ui,*i),
            Label::ProductionOperation=>crate::industry::operation_label(&sandbox,&ui),
            Label::ProductionProblem=>crate::industry::problem(&sandbox,&ui).unwrap_or_else(||"The named operation applies to this facility. Stock and work in progress are retained.".into()),
            Label::DamageTarget=>crate::building_review::target_label(&sandbox,&ui),
            Label::DamageReview=>crate::building_review::review_label(&sandbox,&ui),
            Label::Industry=>selected_site(&sandbox,&ui).map(|s|format!("{}\n{:?} · {} · {}\n{} → {}\nCycle {:.0}/{:.0} s · {} completed\nStock: {}\n{:.0} m from active actor · manage within 20 m\n{}",s.name,s.kind,if runtime.clock.paused||runtime.clock.time_scale==0.{"time paused"}else{production_status(s)},if s.owner==Some(sandbox.player_id){"your facility"}else{"other owner"},if s.inputs.is_empty(){"finite source".into()}else{inventory_line(&s.inputs)},inventory_line(&s.outputs),s.progress_seconds,s.cycle_seconds,s.cycles_completed,inventory_line(&s.stock),player.map(|p|reach_distance(p.position,s.position)).unwrap_or(0.),if s.source_feature.is_some(){"Authored process at a map feature"}else{"Authored demonstration facility"})).unwrap_or_else(|| match ui.selected_site { Some(id)=>format!("Selected facility #{id} is unavailable in this view. Choose Next facility to review another."), None=>"No production facilities loaded.".into() }),
            Label::Logistics=>{
                let site_name=|id|life.producers.get(&id).map(|s|s.name.clone()).or_else(||life.shops.get(&id).map(|s|s.name.clone())).unwrap_or_else(||format!("Regional site #{id}"));
                let mut rows=vec![format!("{} active deliveries · {} completed\n{} replans · {} returned",life.deliveries.len(),life.metrics.deliveries_completed,life.metrics.delivery_replans,life.metrics.deliveries_returned)];
                for d in life.deliveries.iter().take(6){
                    rows.push(format!("\n{} × {} · {}\n{} → {}{}",d.amount,d.item.replace('_'," "),match d.phase{terra_bevy::rules::DeliveryPhase::ToPickup=>"collecting",terra_bevy::rules::DeliveryPhase::ToDropoff=>"delivering",terra_bevy::rules::DeliveryPhase::Returning=>"returning cargo"},if d.phase==terra_bevy::rules::DeliveryPhase::Returning {"Courier".into()}else{site_name(d.from_site)},site_name(if d.phase==terra_bevy::rules::DeliveryPhase::Returning {d.from_site}else{d.to_site}),if d.stalled_seconds>1. {format!("\nWaiting {:.0} s · {} route retries",d.stalled_seconds,d.route_retries)}else{String::new()}));
                }
                rows.join("\n")
            },
        };
        let value = if matches!(label, Label::Source) && !politics.atlas {
            if let Some(guide) = &ui.frontier.guide {
                format!("{value}\nNext: {} · J activities", guide.title)
            } else {
                value
            }
        } else if matches!(label, Label::Controls) {
            controls_footer(
                &value,
                runtime.clock.paused,
                persistence.blocked(),
                !runtime.network_authority && persistence.requires_reopen(),
                !control.mode.is_possessed() && !input_settings.space_has_rts_action(),
            )
        } else {
            value
        };
        if **text != value {
            **text = value;
        }
    }
}

fn controls_footer(
    value: &str,
    paused: bool,
    persistence_blocked: bool,
    requires_reopen: bool,
    space_pauses: bool,
) -> String {
    if requires_reopen {
        "STORAGE STOPPED · Esc menu → Close failed world · Reopen to validate saved progress".into()
    } else if persistence_blocked {
        "PAUSED · F5 retry checkpoint · F9 restore last checkpoint".into()
    } else if paused {
        format!(
            "PAUSED · {}   |   {value}",
            if space_pauses {
                "Space to resume"
            } else {
                "World [F10] → Resume clock"
            }
        )
    } else {
        value.into()
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

#[cfg(test)]
mod actor_context_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::{collections::BTreeSet, time::Duration};
    use terra_bevy::{
        WorldConfig, WorldDiagnostics,
        render::{RenderCapabilities, RenderQualitySettings, RenderTelemetry},
        rules::{Command, LivingCommand, Needs, UnitOrder},
    };

    pub(super) fn app() -> (App, u64, u64, u64) {
        let mut app = App::new();
        let config = WorldConfig::default();
        let position = config.focus;
        let mut sandbox = SandboxRuntime::new(position);
        sandbox
            .state
            .seed_living_demo(position, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let person = sandbox.state.living.accounts[&1].avatar_unit;
        let other = sandbox
            .state
            .living
            .units
            .values()
            .find(|u| u.owner == Some(1) && u.is_person() && u.id != person)
            .unwrap()
            .id;
        let vehicle = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == 1)
            .unwrap()
            .unit_id;
        sandbox
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("bread".into(), 2);
        sandbox
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("water".into(), 2);
        sandbox.state.living.units.get_mut(&person).unwrap().needs = Needs {
            food: 0.2,
            water: 0.5,
            energy: 0.7,
        };
        sandbox.state.living.units.get_mut(&other).unwrap().needs = Needs {
            food: 0.1,
            water: 0.8,
            energy: 0.9,
        };
        let mut control = PlayerControl::default();
        control.selected.insert(person);
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_millis(250));
        app.insert_resource(WorldRuntime {
            focus: position,
            frame: terra_core::LocalFrame::new(position),
            config,
            origin_revision: 0,
            clock: Default::default(),
            weather: Default::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        })
        .insert_resource(sandbox)
        .insert_resource(control)
        .insert_resource(time)
        .init_resource::<Time<Real>>()
        .insert_resource(PoliticalWorld::load(None).unwrap())
        .init_resource::<UiState>()
        .init_resource::<UiQueue>()
        .init_resource::<WorldViewport>()
        .init_resource::<crate::player_input::InputSettings>()
        .init_resource::<crate::command_input::CommandFocus>()
        .init_resource::<crate::command_input::WorldPointer>()
        .init_resource::<ResidentChunks>()
        .init_resource::<RenderTelemetry>()
        .init_resource::<RenderQualitySettings>()
        .init_resource::<RenderCapabilities>()
        .init_resource::<crate::DemoOptions>()
        .init_resource::<WorldDiagnostics>()
        .init_resource::<terra_bevy::RegionalPersistence>()
        .init_resource::<terra_bevy::SandboxQueue>()
        .init_resource::<crate::control::ControlQueue>()
        .init_resource::<crate::interaction::Placement>()
        .insert_resource(crate::camera::CameraRig {
            position,
            yaw: 0.,
            pitch: 0.,
            needs_grounding: false,
            teleport: None,
            altitude_offset: 0.,
            follow_distance: 4.,
            last_mode: ControlMode::Rts,
        })
        .add_systems(Startup, setup_ui)
        .add_systems(
            Update,
            (
                sync_layout,
                crate::frontier::refresh_guidance,
                update_labels,
                update_needs,
                button_availability,
            )
                .chain(),
        );
        (app, person, other, vehicle)
    }

    pub(super) fn label(app: &mut App, select: fn(&Label) -> bool) -> String {
        let world = app.world_mut();
        world
            .query::<(&Label, &Text)>()
            .iter(world)
            .find(|(label, _)| select(label))
            .unwrap()
            .1
            .0
            .clone()
    }

    fn need(app: &mut App, index: usize) -> (String, Val) {
        let world = app.world_mut();
        let text = world
            .query::<(&NeedLabel, &Text)>()
            .iter(world)
            .find(|(label, _)| label.0 == index)
            .unwrap()
            .1
            .0
            .clone();
        let width = world
            .query::<(&NeedFill, &Node)>()
            .iter(world)
            .find(|(fill, _)| fill.0 == index)
            .unwrap()
            .1
            .width;
        (text, width)
    }

    pub(super) fn disabled(app: &mut App, select: fn(Action) -> bool) -> bool {
        let world = app.world_mut();
        world
            .query::<(&ActionButton, Has<InteractionDisabled>)>()
            .iter(world)
            .find(|(action, _)| select(action.0))
            .unwrap()
            .1
    }

    pub(super) fn activate(app: &mut App, select: fn(Action) -> bool) {
        let world = app.world_mut();
        let entity = world
            .query::<(Entity, &ActionButton)>()
            .iter(world)
            .find(|(_, action)| select(action.0))
            .unwrap()
            .0;
        world.trigger(Activate { entity });
        world.flush();
    }

    #[test]
    fn native_community_confirmation_cannot_accept_an_unshown_or_replacement_review() {
        use crate::communities::CommunityAction;
        let (mut app, _, _, _) = app();
        let (first, second) = {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let catalog = sandbox.catalog.clone();
            let first = sandbox
                .state
                .apply(
                    1,
                    Command::CreateFaction {
                        name: "First review".into(),
                    },
                    &catalog,
                )
                .unwrap()
                .created_id
                .unwrap();
            let second = sandbox
                .state
                .apply(
                    1,
                    Command::CreateFaction {
                        name: "Second review".into(),
                    },
                    &catalog,
                )
                .unwrap()
                .created_id
                .unwrap();
            (first, second)
        };
        app.world_mut().resource_mut::<UiState>().selected_faction = Some(first);
        app.update();
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Dissolve))
        });
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Confirm))
        });
        assert!(
            app.world().resource::<UiQueue>().0.is_empty(),
            "new review has not been displayed"
        );
        app.update();
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Confirm))
        });
        let old_confirmation = app
            .world_mut()
            .resource_mut::<UiQueue>()
            .0
            .pop_front()
            .unwrap();
        assert!(matches!(
            old_confirmation,
            Action::Community(CommunityAction::ConfirmReviewed(_))
        ));
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Cancel))
        });
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        app.world_mut().resource_mut::<UiState>().selected_faction = Some(second);
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Dissolve))
        });
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        app.world_mut()
            .resource_mut::<UiQueue>()
            .0
            .push_back(old_confirmation);
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        assert!(
            app.world()
                .resource::<terra_bevy::SandboxQueue>()
                .0
                .is_empty()
        );
        assert!(crate::communities::has_review(
            app.world().resource::<UiState>()
        ));
        app.update();
        activate(&mut app, |action| {
            matches!(action, Action::Community(CommunityAction::Confirm))
        });
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        let intent = app
            .world_mut()
            .resource_mut::<terra_bevy::SandboxQueue>()
            .0
            .pop_front()
            .unwrap();
        let terra_bevy::SandboxIntent::Command(command) = intent else {
            panic!("expected reviewed community command")
        };
        assert!(
            matches!(&command, Command::DissolveFaction { faction_id } if *faction_id == second)
        );
        let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = sandbox.catalog.clone();
        sandbox.state.apply(1, command, &catalog).unwrap();
        assert!(sandbox.state.factions.contains_key(&first));
        assert!(!sandbox.state.factions.contains_key(&second));
    }

    #[test]
    fn native_consume_activation_binds_displayed_body_and_rejects_changed_context_before_dispatch()
    {
        for changed in 0..5 {
            let (mut app, person, other, vehicle) = app();
            app.update();
            activate(&mut app, |action| matches!(action, Action::Eat));
            assert!(matches!(
                app.world().resource::<UiQueue>().0.front(),
                Some(Action::ConsumeSelected { water: false, .. })
            ));
            match changed {
                0 => app
                    .world_mut()
                    .resource_mut::<PlayerControl>()
                    .selected
                    .clear(),
                1 => {
                    app.world_mut().resource_mut::<PlayerControl>().selected =
                        BTreeSet::from([other]);
                    app.world_mut()
                        .resource_mut::<SandboxRuntime>()
                        .state
                        .living
                        .accounts
                        .get_mut(&1)
                        .unwrap()
                        .active_unit = other;
                }
                2 => {
                    app.world_mut()
                        .resource_mut::<SandboxRuntime>()
                        .restore_revision += 1
                }
                3 => {
                    app.world_mut()
                        .resource_mut::<SandboxRuntime>()
                        .state
                        .living
                        .units
                        .get_mut(&person)
                        .unwrap()
                        .owner = Some(2)
                }
                _ => {
                    app.world_mut().resource_mut::<PlayerControl>().selected =
                        BTreeSet::from([vehicle]);
                    let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
                    let account = sandbox.state.living.accounts.get_mut(&1).unwrap();
                    account.active_unit = vehicle;
                    account.possessed = true;
                }
            }
            app.world_mut()
                .run_system_once(crate::interaction::actions)
                .unwrap();
            assert!(
                app.world()
                    .resource::<terra_bevy::SandboxQueue>()
                    .0
                    .is_empty()
            );
            assert_eq!(
                app.world().resource::<SandboxRuntime>().state.players[&1].inventory["bread"],
                2
            );
            assert!(
                app.world()
                    .resource::<SandboxRuntime>()
                    .last_message
                    .contains("Review it again")
            );
        }
        let (mut app, person, _, _) = app();
        app.update();
        activate(&mut app, |action| matches!(action, Action::Drink));
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        let intent = app
            .world_mut()
            .resource_mut::<terra_bevy::SandboxQueue>()
            .0
            .pop_front()
            .unwrap();
        let terra_bevy::SandboxIntent::Command(command) = intent else {
            panic!("expected consumption command")
        };
        let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = sandbox.catalog.clone();
        sandbox.state.apply(1, command, &catalog).unwrap();
        assert_eq!(sandbox.state.players[&1].inventory["water"], 1);
        assert!(sandbox.state.living.units[&person].needs.water > 0.5);
    }

    #[test]
    fn every_native_frontier_consumption_retains_the_displayed_active_person() {
        use crate::frontier::FrontierAction;
        for action in [
            FrontierAction::EatBerries,
            FrontierAction::EatFish,
            FrontierAction::DrinkWater,
            FrontierAction::ContinueJourney,
        ] {
            for changed in 0..7 {
                let (mut app, person, other, _) = app();
                {
                    let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
                    let inventory = &mut sandbox.state.players.get_mut(&1).unwrap().inventory;
                    inventory.insert("berries".into(), 2);
                    inventory.insert("cooked_fish".into(), 2);
                }
                app.update();
                let entity = {
                    let world = app.world_mut();
                    world
                        .query::<(Entity, &ActionButton)>()
                        .iter(world)
                        .find(|(_, button)| match (button.0, action) {
                            (
                                Action::Frontier(FrontierAction::EatBerries),
                                FrontierAction::EatBerries,
                            )
                            | (
                                Action::Frontier(FrontierAction::EatFish),
                                FrontierAction::EatFish,
                            )
                            | (
                                Action::Frontier(FrontierAction::DrinkWater),
                                FrontierAction::DrinkWater,
                            )
                            | (
                                Action::Frontier(FrontierAction::ContinueJourney),
                                FrontierAction::ContinueJourney,
                            ) => true,
                            _ => false,
                        })
                        .unwrap()
                        .0
                };
                app.world_mut().trigger(Activate { entity });
                assert!(matches!(
                    app.world().resource::<UiQueue>().0.front(),
                    Some(Action::FrontierReviewed { .. })
                ));
                match changed {
                    0 => {
                        app.world_mut()
                            .resource_mut::<SandboxRuntime>()
                            .state
                            .living
                            .accounts
                            .get_mut(&1)
                            .unwrap()
                            .active_unit = other;
                    }
                    1 => {
                        app.world_mut()
                            .resource_mut::<SandboxRuntime>()
                            .restore_revision += 1
                    }
                    2 => app.world_mut().resource_mut::<SandboxRuntime>().player_id = 2,
                    3 => {
                        app.world_mut()
                            .resource_mut::<SandboxRuntime>()
                            .state
                            .living
                            .units
                            .get_mut(&person)
                            .unwrap()
                            .owner = Some(2);
                    }
                    4 => app
                        .world_mut()
                        .resource_mut::<crate::control::ControlQueue>()
                        .0
                        .push_back(crate::control::ControlIntent::CycleActor),
                    5 => app
                        .world_mut()
                        .resource_mut::<terra_bevy::SandboxQueue>()
                        .0
                        .push_back(terra_bevy::SandboxIntent::Command(Command::Living {
                            action: LivingCommand::SelectActive { unit_id: other },
                        })),
                    _ => app
                        .world_mut()
                        .resource_mut::<SandboxRuntime>()
                        .state
                        .set_player_dormant(1, true)
                        .unwrap(),
                }
                let state = app.world().resource::<SandboxRuntime>().state.clone();
                let pending = app.world().resource::<terra_bevy::SandboxQueue>().0.len();
                app.world_mut()
                    .run_system_once(crate::interaction::actions)
                    .unwrap();
                assert_eq!(
                    app.world().resource::<terra_bevy::SandboxQueue>().0.len(),
                    pending
                );
                assert_eq!(app.world().resource::<SandboxRuntime>().state, state);
                assert!(
                    app.world()
                        .resource::<SandboxRuntime>()
                        .last_message
                        .contains("Review")
                );
            }
        }
    }

    #[test]
    fn nature_and_suggested_consume_for_the_named_active_person_while_inspecting_another_body() {
        use crate::frontier::FrontierAction;
        for (action, item) in [
            (FrontierAction::EatBerries, "berries"),
            (FrontierAction::EatFish, "cooked_fish"),
            (FrontierAction::DrinkWater, "water"),
            (FrontierAction::ContinueJourney, "bread"),
        ] {
            let (mut app, person, other, _) = app();
            app.world_mut().resource_mut::<PlayerControl>().selected = BTreeSet::from([other]);
            app.world_mut()
                .resource_mut::<SandboxRuntime>()
                .state
                .players
                .get_mut(&1)
                .unwrap()
                .inventory
                .insert(item.into(), 2);
            app.update();
            let active_name = app.world().resource::<SandboxRuntime>().state.living.units[&person]
                .name
                .clone();
            assert!(
                label(&mut app, |value| matches!(
                    value,
                    Label::Frontier(crate::frontier::FrontierLabel::Survival)
                ))
                .contains(&active_name)
            );
            assert!(
                label(&mut app, |value| matches!(
                    value,
                    Label::Frontier(crate::frontier::FrontierLabel::Journey)
                ))
                .contains(&active_name)
            );
            let bound =
                crate::frontier::bind_action(app.world().resource::<UiState>(), action).unwrap();
            app.world_mut().resource_mut::<UiQueue>().0.push_back(bound);
            app.world_mut()
                .run_system_once(crate::interaction::actions)
                .unwrap();
            let intent = app
                .world_mut()
                .resource_mut::<terra_bevy::SandboxQueue>()
                .0
                .pop_front()
                .unwrap();
            let terra_bevy::SandboxIntent::Command(command) = intent else {
                panic!("expected consumption command")
            };
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let other_needs = sandbox.state.living.units[&other].needs;
            let before = sandbox.state.living.units[&person].needs;
            let catalog = sandbox.catalog.clone();
            sandbox.state.apply(1, command, &catalog).unwrap();
            assert_eq!(sandbox.state.players[&1].inventory[item], 1);
            assert_eq!(sandbox.state.living.units[&other].needs, other_needs);
            assert_ne!(sandbox.state.living.units[&person].needs, before);
        }
    }

    #[test]
    fn native_frontier_activation_cannot_cross_a_queued_patch_choice_or_a_refreshed_display() {
        use crate::frontier::FrontierAction;
        for refresh_after_click in [false, true] {
            let (mut app, _, _, _) = app();
            {
                let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
                let origin = sandbox.state.players[&1].position;
                for id in ["review_patch_a", "review_patch_b"] {
                    sandbox
                        .state
                        .seed_resource(id.into(), origin, "berries", 10, 0.)
                        .unwrap();
                }
            }
            app.world_mut().resource_mut::<UiState>().frontier.resource =
                Some("review_patch_a".into());
            app.update();
            if refresh_after_click {
                activate(&mut app, |action| {
                    matches!(action, Action::Frontier(FrontierAction::GatherResource))
                });
                app.world_mut().resource_mut::<UiState>().frontier.resource =
                    Some("review_patch_b".into());
                app.update();
            } else {
                activate(&mut app, |action| {
                    matches!(action, Action::Frontier(FrontierAction::NextResource))
                });
                activate(&mut app, |action| {
                    matches!(action, Action::Frontier(FrontierAction::GatherResource))
                });
            }
            let state = app.world().resource::<SandboxRuntime>().state.clone();
            app.world_mut()
                .run_system_once(crate::interaction::actions)
                .unwrap();
            assert!(
                app.world()
                    .resource::<terra_bevy::SandboxQueue>()
                    .0
                    .is_empty()
            );
            assert_eq!(app.world().resource::<SandboxRuntime>().state, state);
            assert_eq!(
                app.world()
                    .resource::<UiState>()
                    .frontier
                    .resource
                    .as_deref(),
                Some("review_patch_b")
            );
            app.update();
            activate(&mut app, |action| {
                matches!(action, Action::Frontier(FrontierAction::GatherResource))
            });
            app.world_mut()
                .run_system_once(crate::interaction::actions)
                .unwrap();
            let intent = app
                .world_mut()
                .resource_mut::<terra_bevy::SandboxQueue>()
                .0
                .pop_front()
                .unwrap();
            assert!(
                matches!(intent, terra_bevy::SandboxIntent::Command(Command::Gather { node_id, amount: 5 }) if node_id == "review_patch_b")
            );
        }
    }

    #[test]
    fn disconnected_and_unbound_frontier_actions_cannot_submit_native_consumption() {
        use crate::frontier::FrontierAction;
        for bound in [false, true] {
            let (mut app, _, _, _) = app();
            app.update();
            if bound {
                activate(&mut app, |action| {
                    matches!(action, Action::Frontier(FrontierAction::DrinkWater))
                });
                app.insert_resource(NetworkStatus::default());
            } else {
                app.world_mut()
                    .resource_mut::<UiQueue>()
                    .0
                    .push_back(Action::Frontier(FrontierAction::DrinkWater));
            }
            let state = app.world().resource::<SandboxRuntime>().state.clone();
            app.world_mut()
                .run_system_once(crate::interaction::actions)
                .unwrap();
            assert!(
                app.world()
                    .resource::<terra_bevy::SandboxQueue>()
                    .0
                    .is_empty()
            );
            assert_eq!(app.world().resource::<SandboxRuntime>().state, state);
        }
    }

    #[test]
    fn consume_waits_for_queued_body_transitions_and_group_orders_snapshot_owned_ids_at_dispatch() {
        let (mut app, person, other, vehicle) = app();
        app.update();
        app.world_mut()
            .resource_mut::<terra_bevy::SandboxQueue>()
            .0
            .push_back(terra_bevy::SandboxIntent::Command(Command::Living {
                action: LivingCommand::SelectActive { unit_id: other },
            }));
        activate(&mut app, |action| matches!(action, Action::Eat));
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        assert_eq!(
            app.world().resource::<terra_bevy::SandboxQueue>().0.len(),
            1
        );
        app.world_mut()
            .resource_mut::<terra_bevy::SandboxQueue>()
            .0
            .clear();
        app.world_mut()
            .resource_mut::<crate::control::ControlQueue>()
            .0
            .push_back(crate::control::ControlIntent::CycleActor);
        activate(&mut app, |action| matches!(action, Action::Eat));
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        assert!(
            app.world()
                .resource::<terra_bevy::SandboxQueue>()
                .0
                .is_empty()
        );
        app.world_mut()
            .resource_mut::<crate::control::ControlQueue>()
            .0
            .clear();

        app.world_mut()
            .resource_mut::<PlayerControl>()
            .selected
            .clear();
        activate(&mut app, |action| matches!(action, Action::RestUnits));
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        assert!(
            app.world()
                .resource::<terra_bevy::SandboxQueue>()
                .0
                .is_empty()
        );
        app.world_mut().resource_mut::<PlayerControl>().selected =
            BTreeSet::from([person, vehicle]);
        activate(&mut app, |action| matches!(action, Action::RestUnits));
        app.world_mut()
            .run_system_once(crate::interaction::actions)
            .unwrap();
        assert!(
            app.world()
                .resource::<crate::control::ControlQueue>()
                .0
                .is_empty()
        );
        let intent = app
            .world_mut()
            .resource_mut::<terra_bevy::SandboxQueue>()
            .0
            .pop_front()
            .unwrap();
        let terra_bevy::SandboxIntent::Command(command) = intent else {
            panic!("expected group command")
        };
        assert!(
            matches!(&command, Command::Living { action: LivingCommand::Rest { unit_ids } } if unit_ids == &vec![person, vehicle])
        );
        app.world_mut().resource_mut::<PlayerControl>().selected = BTreeSet::from([other]);
        let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = sandbox.catalog.clone();
        let old_other = sandbox.state.living.units[&other].order.clone();
        sandbox.state.apply(1, command, &catalog).unwrap();
        assert_eq!(sandbox.state.living.units[&person].order, UnitOrder::Rest);
        assert_eq!(sandbox.state.living.units[&vehicle].order, UnitOrder::Rest);
        assert_eq!(sandbox.state.living.units[&other].order, old_other);
    }

    #[test]
    fn activities_hud_and_command_need_systems_follow_single_group_empty_and_missing_selection() {
        let (mut app, person, other, vehicle) = app();
        app.update();
        assert!(app.world().resource::<UiState>().panel == Panel::Activities);
        assert!(label(&mut app, |l| matches!(l, Label::Streaming)).contains("Food 20%"));
        assert_eq!(need(&mut app, 0), ("Food   20% · Low".into(), percent(20)));
        assert!(!disabled(&mut app, |a| matches!(a, Action::Eat)));
        app.world_mut().resource_mut::<PlayerControl>().selected =
            BTreeSet::from([person, other, vehicle]);
        app.update();
        let footer = label(&mut app, |l| matches!(l, Label::Streaming));
        assert!(footer.contains("3 selected · 3 owned"));
        assert!(footer.contains("Lowest (2 bodies): Food 10%"));
        assert_eq!(
            need(&mut app, 0),
            ("Lowest Food   10% · Low".into(), percent(10))
        );
        assert!(disabled(&mut app, |a| matches!(a, Action::Eat)));
        assert!(!disabled(&mut app, |a| matches!(a, Action::RestUnits)));
        for selection in [BTreeSet::new(), BTreeSet::from([u64::MAX])] {
            app.world_mut().resource_mut::<PlayerControl>().selected = selection;
            app.update();
            assert_eq!(need(&mut app, 0), ("Food   —".into(), percent(0)));
            assert!(disabled(&mut app, |a| matches!(a, Action::Eat)));
            assert!(disabled(&mut app, |a| matches!(a, Action::RestUnits)));
            assert!(disabled(&mut app, |a| matches!(a, Action::Possess)));
            assert!(!label(&mut app, |l| matches!(l, Label::Streaming)).contains("Food 20%"));
        }
    }

    #[test]
    fn actual_need_labels_wait_for_driver_admission_and_name_partial_remote_scope() {
        let (mut app, _, _, vehicle) = app();
        {
            let mut control = app.world_mut().resource_mut::<PlayerControl>();
            control.selected = BTreeSet::from([vehicle]);
            control.possessed = Some(vehicle);
            control.mode = ControlMode::ThirdPerson;
        }
        app.update();
        assert_eq!(need(&mut app, 0), ("Food   —".into(), percent(0)));
        assert!(label(&mut app, |l| matches!(l, Label::Selected)).contains("Awaiting control"));
        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let account = sandbox.state.living.accounts.get_mut(&1).unwrap();
            account.active_unit = vehicle;
            account.possessed = true;
        }
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .network_authority = true;
        let mut network = NetworkStatus::default();
        network.connected = true;
        network.sandbox_snapshot_truncated = true;
        app.insert_resource(network);
        app.update();
        assert_eq!(
            need(&mut app, 0),
            ("Driver Food   20% · Low".into(), percent(20))
        );
        let footer = label(&mut app, |l| matches!(l, Label::Streaming));
        assert!(footer.contains("Driver:"));
        assert!(footer.contains("Partial received view"));
        assert!(
            label(&mut app, |l| matches!(l, Label::Selected)).contains("Partial received view")
        );
        assert!(disabled(&mut app, |a| matches!(a, Action::Eat)));
        app.world_mut()
            .resource_mut::<RenderTelemetry>()
            .pending_meshes = 2;
        app.update();
        let pending = label(&mut app, |l| matches!(l, Label::Streaming));
        assert!(pending.starts_with("Loading region · 2 pending"));
        assert!(pending.contains("Driver:"));
    }

    #[test]
    fn in_world_actor_text_wraps_without_changing_the_scaled_input_boundary() {
        let (mut app, person, _, _) = app();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .living
            .units
            .get_mut(&person)
            .unwrap()
            .name = "LongUnbrokenActorName".repeat(3);
        app.world_mut()
            .resource_mut::<crate::player_input::InputSettings>()
            .text_scale = 1.5;
        app.update();
        let detail = label(&mut app, |l| matches!(l, Label::Selected));
        assert!(detail.starts_with(&"LongUnbrokenActorName".repeat(3)));
        let world = app.world_mut();
        for (label, layout) in world.query::<(&Label, &TextLayout)>().iter(world) {
            if matches!(
                label,
                Label::Selected | Label::Streaming | Label::SelectionActions
            ) {
                assert_eq!(layout.linebreak, bevy::text::LineBreak::WordOrCharacter);
            }
        }
        let footer = world
            .query_filtered::<&Node, With<HudFooter>>()
            .single(world)
            .unwrap();
        assert_eq!(footer.height, px(144));
        let viewport = world.resource::<WorldViewport>();
        assert_eq!(viewport.bottom, 170.);
        let window = Window {
            resolution: (1100, 700).into(),
            ..default()
        };
        assert!(!viewport.contains(&window, Vec2::new(500., 600.), false));
        assert!(viewport.contains(&window, Vec2::new(500., 500.), false));
    }
}

#[cfg(test)]
#[path = "native_operation_tests.rs"]
mod operation_tests;
