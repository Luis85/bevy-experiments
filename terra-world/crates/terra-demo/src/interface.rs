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
use terra_bevy::{NetworkStatus, PoliticalWorld, ResidentChunks, SandboxRuntime, WorldRuntime};
use terra_core::{Layer, season, solar_position};

pub struct InterfacePlugin;
impl Plugin for InterfacePlugin {
    fn build(&self, app: &mut App) {
        // The compact engine font omits geographic symbols and recipe arrows.
        // Embed the full licensed font for consistent offline rendering.
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .insert(
                bevy::asset::AssetId::default(),
                Font::from_bytes(include_bytes!("../assets/DejaVuSans.ttf").to_vec()),
            )
            .expect("the default UI font has a stable asset ID");
        app.init_resource::<UiQueue>()
            .init_resource::<UiState>()
            .add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (
                    edit_text,
                    scroll_panels,
                    update_scrollbars.after(scroll_panels),
                    button_availability,
                    button_style,
                    update_labels,
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
#[derive(Resource)]
pub struct UiState {
    pub editing_gps: bool,
    pub gps_text: String,
    pub location: String,
    pub input_mode: InputMode,
    pub selected_faction: Option<u64>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            editing_gps: false,
            gps_text: "48.137, 11.575".into(),
            location: "Geographic focus".into(),
            input_mode: InputMode::Coordinates,
            selected_faction: None,
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
}
#[derive(Component)]
struct SidePanel(bool);
#[derive(Component)]
struct ScrollThumb(bool);
#[derive(Component)]
struct AtlasControls;
#[derive(Component)]
struct ButtonPolicy {
    local_only: bool,
    requires_player: bool,
}
const PANEL: Color = Color::srgba(0.035, 0.060, 0.052, 0.97);
const TEXT: Color = Color::srgb(0.89, 0.92, 0.85);
const MUTED: Color = Color::srgb(0.60, 0.70, 0.63);
const ACCENT: Color = Color::srgb(0.64, 0.86, 0.57);
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
fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action) {
    parent
        .spawn((
            Button,
            ButtonPolicy {
                local_only: matches!(
                    action,
                    Action::Pause
                        | Action::Speed
                        | Action::Season
                        | Action::Noon
                        | Action::Camp
                        | Action::Save
                        | Action::Load
                        | Action::DrawTerritory
                ),
                requires_player: matches!(
                    action,
                    Action::Claim
                        | Action::GatherWood
                        | Action::GatherStone
                        | Action::Planks
                        | Action::Blocks
                        | Action::Tool
                        | Action::Seeds
                        | Action::House
                        | Action::Workbench
                        | Action::Fence
                        | Action::Plant
                        | Action::Harvest
                        | Action::Damage
                        | Action::Home
                        | Action::CreateFaction
                        | Action::InviteFaction
                        | Action::AcceptFaction
                        | Action::PromoteFaction
                        | Action::DemoteFaction
                        | Action::SharePlot
                ),
            },
            Hovered::default(),
            Node {
                width: percent(100),
                min_height: px(30),
                flex_shrink: 0.,
                padding: UiRect::axes(px(10), px(7)),
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.09, 0.14, 0.11)),
            observe(move |_: On<Activate>, mut q: ResMut<UiQueue>| {
                if q.0.len() < 64 {
                    q.0.push_back(action);
                }
            }),
        ))
        .with_children(|p| {
            p.spawn(text(label, 12., TEXT));
        });
}
fn heading(parent: &mut ChildSpawnerCommands, title: &str) {
    parent.spawn((
        Text::new(title),
        TextFont {
            font_size: FontSize::Px(10.),
            ..default()
        },
        TextColor(ACCENT),
        Node {
            flex_shrink: 0.,
            margin: UiRect::top(px(12)),
            ..default()
        },
    ));
}
fn setup_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                right: px(0),
                height: px(74),
                padding: UiRect::axes(px(24), px(14)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                ..default()
            })
            .with_children(|p| {
                p.spawn(text("T E R R A", 27., TEXT));
                p.spawn(text("A WORLD TO INHABIT", 9., ACCENT));
            });
            p.spawn(Node {
                width: px(350),
                column_gap: px(8),
                ..default()
            })
            .with_children(|p| {
                for (label, action) in [
                    ("World / atlas [M]", Action::Atlas),
                    ("Player / camp [Tab]", Action::Follow),
                ] {
                    p.spawn(Node {
                        width: px(171),
                        ..default()
                    })
                    .with_children(|p| button(p, label, action));
                }
            });
            p.spawn((Label::Connection, text("LOCAL SESSION", 11., ACCENT)));
        });
    commands
        .spawn((
            SidePanel(false),
            ScrollPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(88),
                left: px(14),
                bottom: px(92),
                width: px(254),
                padding: UiRect::all(px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                border_radius: BorderRadius::all(px(8)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(text("World explorer", 21., TEXT));
            p.spawn(text("SCROLL FOR LAYERS, TIME & POLITICS", 9., MUTED));
            p.spawn((Label::Location, text("", 12., ACCENT)));
            p.spawn((Label::Coordinates, text("", 11., MUTED)));
            button(p, "Enter coordinates", Action::EditGps);
            heading(p, "REGIONS");
            button(p, "Munich · temperate landscape", Action::Preset(0));
            button(p, "Zugspitze · alpine terrain", Action::Preset(1));
            button(p, "Hamburg · harbor", Action::Preset(2));
            button(p, "Cape Town · southern seasons", Action::Preset(3));
            heading(p, "WORLD LAYERS");
            for layer in Layer::ALL {
                p.spawn((
                    Button,
                    Hovered::default(),
                    Node {
                        width: percent(100),
                        min_height: px(27),
                        flex_shrink: 0.,
                        padding: UiRect::horizontal(px(9)),
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(px(3)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.09, 0.14, 0.11)),
                    observe(move |_: On<Activate>, mut q: ResMut<UiQueue>| {
                        if q.0.len() < 64 {
                            q.0.push_back(Action::Toggle(layer));
                        }
                    }),
                ))
                .with_children(|p| {
                    p.spawn((Label::Layer(layer), text("", 12., TEXT)));
                });
            }
            heading(p, "TIME & ATMOSPHERE");
            p.spawn((Label::Clock, text("", 11., TEXT)));
            p.spawn((Label::Weather, text("", 11., MUTED)));
            button(p, "Pause / resume [Space]", Action::Pause);
            button(p, "Change time speed", Action::Speed);
            button(p, "Advance to the next season", Action::Season);
            button(p, "Set daytime for this location", Action::Noon);
            heading(p, "POLITICS & SCENARIOS");
            p.spawn((Label::Politics, text("", 11., MUTED)));
            button(p, "World / political atlas [M]", Action::Atlas);
            button(p, "Fly to selected map location", Action::FlyToSelection);
            button(p, "Show / hide local borders", Action::Borders);
            button(p, "Draw a scenario territory", Action::DrawTerritory);
            button(p, "Export political GeoJSON", Action::ExportPolitics);
        });
    commands
        .spawn((
            SidePanel(true),
            ScrollPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(88),
                right: px(14),
                bottom: px(92),
                width: px(270),
                padding: UiRect::all(px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                border_radius: BorderRadius::all(px(8)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(text("Make it your world", 20., TEXT));
            p.spawn(text("SCROLL FOR BUILDING, FACTIONS & SAVES", 9., MUTED));
            p.spawn((Label::Inventory, text("", 11., TEXT)));
            p.spawn((Label::Sandbox, text("", 10., MUTED)));
            heading(p, "SETTLE");
            button(p, "Move player to explorer focus", Action::Camp);
            button(p, "Claim a 40 m housing plot", Action::Claim);
            heading(p, "GATHER & CRAFT");
            button(p, "Gather nearby wood +5", Action::GatherWood);
            button(p, "Gather nearby stone +5", Action::GatherStone);
            button(p, "Craft planks / 2 wood → 4 planks", Action::Planks);
            button(p, "Craft blocks / 2 stone → 1 block", Action::Blocks);
            button(p, "Craft tool / near a workbench", Action::Tool);
            button(p, "Craft seeds / 1 wheat → 3 seeds", Action::Seeds);
            heading(p, "BUILD & FARM");
            button(p, "Place house / 12 planks + 4 blocks", Action::House);
            button(p, "Place workbench / 4 planks + 2 stone", Action::Workbench);
            button(p, "Place fence / 2 planks", Action::Fence);
            button(p, "Plant wheat / 1 seed", Action::Plant);
            button(p, "Harvest ripe wheat", Action::Harvest);
            button(p, "Damage permitted structure −25", Action::Damage);
            button(p, "Set nearest permitted house as home", Action::Home);
            heading(p, "FACTIONS & SHARED OWNERSHIP");
            p.spawn((Label::Faction, text("", 11., MUTED)));
            button(p, "Create a named faction", Action::CreateFaction);
            button(p, "Select next faction", Action::NextFaction);
            button(p, "Invite a player by ID", Action::InviteFaction);
            button(p, "Accept first pending invitation", Action::AcceptFaction);
            button(p, "Grant builder role by player ID", Action::PromoteFaction);
            button(p, "Set member role by player ID", Action::DemoteFaction);
            button(p, "Transfer nearest plot to faction", Action::SharePlot);
            heading(p, "SESSION");
            button(p, "Save world changes [F5]", Action::Save);
            button(p, "Load saved world [F9]", Action::Load);
            button(p, "Focus player / camp [Tab]", Action::Follow);
            button(p, "Landscape overview [O]", Action::Overview);
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(12),
                left: px(14),
                right: px(14),
                height: px(66),
                padding: UiRect::axes(px(14), px(9)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn((Label::Toast, text("Loading your world…", 12., ACCENT)));
            p.spawn((Label::Streaming, text("", 10., MUTED)));
        });
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(94),
            left: px(286),
            right: px(300),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                Node {
                    padding: UiRect::axes(px(14), px(8)),
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
                bottom: px(102),
                left: px(286),
                right: px(300),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                padding: UiRect::all(px(9)),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn((Label::Attribution, text("", 10., Color::WHITE)));
            p.spawn((Label::Controls, text("", 10., Color::WHITE)));
        });
    commands
        .spawn((
            AtlasControls,
            Node {
                position_type: PositionType::Absolute,
                top: px(140),
                left: px(286),
                right: px(300),
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        ))
        .with_children(|p| {
            for (label, action) in [
                ("Fit map to data", Action::FitAtlas),
                ("Draw territory", Action::DrawTerritory),
                ("Export GeoJSON", Action::ExportPolitics),
                ("Explore selected", Action::FlyToSelection),
            ] {
                p.spawn(Node {
                    width: percent(24.),
                    ..default()
                })
                .with_children(|p| button(p, label, action));
            }
        });
    // Visible position indicators make the lower building/faction controls
    // discoverable; scrolling still follows the pointer's side panel.
    for right_side in [false, true] {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(100),
                    bottom: px(104),
                    left: if right_side { Val::Auto } else { px(260) },
                    right: if right_side { px(18) } else { Val::Auto },
                    width: px(3),
                    border_radius: BorderRadius::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.16, 0.23, 0.18)),
                GlobalZIndex(20),
            ))
            .with_children(|p| {
                p.spawn((
                    ScrollThumb(right_side),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        width: px(3),
                        height: px(60),
                        border_radius: BorderRadius::all(px(2)),
                        ..default()
                    },
                    BackgroundColor(ACCENT),
                ));
            });
    }
}

fn update_scrollbars(
    window: Single<&Window>,
    panels: Query<(&SidePanel, &ScrollPosition, &ComputedNode)>,
    mut thumbs: Query<(&ScrollThumb, &mut Node)>,
) {
    let track = (window.height() - 204.).max(1.);
    for (thumb, mut node) in &mut thumbs {
        if let Some((_, scroll, computed)) = panels.iter().find(|(side, _, _)| side.0 == thumb.0) {
            let view = computed.size().y * computed.inverse_scale_factor();
            let content = computed.content_size().y * computed.inverse_scale_factor();
            let extent = (content - view).max(0.);
            let height = (track * view / content.max(1.)).clamp(40., track.max(40.));
            node.height = px(height);
            node.top = px(if extent > 0. {
                (scroll.y / extent).clamp(0., 1.) * (track - height)
            } else {
                0.
            });
            node.display = if extent > 0. {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}
fn button_availability(
    mut commands: Commands,
    runtime: Res<WorldRuntime>,
    net: Option<Res<NetworkStatus>>,
    politics: Res<PoliticalWorld>,
    mut atlas_controls: Query<&mut Node, With<AtlasControls>>,
    buttons: Query<(Entity, &ButtonPolicy, Has<InteractionDisabled>)>,
) {
    for mut node in &mut atlas_controls {
        node.display = if politics.atlas {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (entity, policy, disabled) in &buttons {
        let should_disable = (policy.local_only && runtime.network_authority)
            || (policy.requires_player && net.as_ref().is_some_and(|n| !n.connected));
        if should_disable && !disabled {
            commands.entity(entity).insert(InteractionDisabled);
        } else if !should_disable && disabled {
            commands.entity(entity).remove::<InteractionDisabled>();
        }
    }
}
fn button_style(
    mut buttons: Query<
        (
            &Hovered,
            Has<Pressed>,
            Has<InteractionDisabled>,
            &mut BackgroundColor,
        ),
        With<Button>,
    >,
) {
    for (hover, pressed, disabled, mut color) in &mut buttons {
        color.0 = if disabled {
            Color::srgb(0.055, 0.075, 0.06)
        } else if pressed {
            Color::srgb(0.28, 0.41, 0.25)
        } else if hover.get() {
            Color::srgb(0.17, 0.25, 0.19)
        } else {
            Color::srgb(0.09, 0.14, 0.11)
        };
    }
}
fn scroll_panels(
    window: Single<&Window>,
    scroll: Res<AccumulatedMouseScroll>,
    mut panels: Query<(&SidePanel, &mut ScrollPosition, &ComputedNode)>,
) {
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.y < 88. || cursor.y > window.height() - 92. {
        return;
    }
    let delta = scroll.delta.y
        * if scroll.unit == bevy::input::mouse::MouseScrollUnit::Line {
            32.
        } else {
            1.
        };
    for (panel, mut position, node) in &mut panels {
        if (!panel.0 && cursor.x < 268.) || (panel.0 && cursor.x > window.width() - 284.) {
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
fn update_labels(
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    sandbox: Res<SandboxRuntime>,
    politics: Res<PoliticalWorld>,
    ui: Res<UiState>,
    net: Option<Res<NetworkStatus>>,
    mut texts: Query<(&Label, &mut Text)>,
    time: Res<Time>,
    mut elapsed: Local<f32>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < 0.1 && !ui.is_changed() {
        return;
    }
    *elapsed = 0.;
    let player = sandbox.state.players.get(&sandbox.player_id);
    let weather = runtime.effective_weather();
    let stats = &runtime.stream_stats;
    let active = ui
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
        Label::Location=>ui.location.clone(),
        Label::Coordinates=>if ui.editing_gps && matches!(ui.input_mode,InputMode::Coordinates) {format!("{}▏ [Enter]",ui.gps_text)} else {format!("{:.6}°, {:.6}°",runtime.focus.latitude,runtime.focus.longitude)},
        Label::Clock=>format!("{}\n{:?} · {:.0}× {}{}",runtime.clock.label(),season(&runtime.focus,&runtime.clock),runtime.clock.time_scale,if runtime.clock.paused {"PAUSED"} else {""},if runtime.network_authority {"SERVER"} else {""}),
        Label::Weather=>format!("{:.1}°C   Wind {:.1} m/s\nCloud {:.0}%   Rain {:.1} mm/h\nSun {:.1}° · {} weather",weather.temperature_c,weather.wind_speed_m_s,weather.cloud_cover*100.,weather.precipitation_mm_h,solar_position(&runtime.focus,&runtime.clock).elevation_deg,if !runtime.config.layers.contains(Layer::Weather){"disabled"}else if weather.synthetic {"synthetic"} else {"current live"}),
        Label::Streaming=>format!("CHUNKS {}   QUEUED {}   LOADING {}   CACHE HITS {}   |   STREAM {:.2} ms   SIM {:.2} ms   |   ORIGIN SHIFTS {}",stats.resident,stats.pending,stats.in_flight,stats.cache_hits,runtime.stream_poll_ms,runtime.simulation_ms,runtime.rebases),
        Label::Inventory=>player.map(|p|format!("Player #{}\nWood {} · Stone {} · Seeds {}\nPlanks {} · Blocks {} · Wheat {} · Tools {}",p.id,p.inventory.get("wood").unwrap_or(&0),p.inventory.get("stone").unwrap_or(&0),p.inventory.get("wheat_seed").unwrap_or(&0),p.inventory.get("planks").unwrap_or(&0),p.inventory.get("stone_blocks").unwrap_or(&0),p.inventory.get("wheat").unwrap_or(&0),p.inventory.get("tool").unwrap_or(&0))).unwrap_or_else(||"Waiting for server inventory…".into()),
        Label::Sandbox=>format!("{} plots · {} structures · {} crops\nHome {} · {} ripe crops",sandbox.state.plots.len(),sandbox.state.structures.len(),sandbox.state.farms.len(),player.and_then(|p|p.home).map(|x|x.to_string()).unwrap_or_else(||"not set".into()),sandbox.state.farms.values().filter(|f|f.ready()).count()),
        Label::Toast=>if ui.editing_gps {let prompt=match ui.input_mode {InputMode::Coordinates=>"Latitude, longitude",InputMode::FactionName=>"Faction name",InputMode::InvitePlayer=>"Player ID to invite",InputMode::PromotePlayer=>"Player ID to promote to builder",InputMode::DemotePlayer=>"Player ID to set as member"};format!("{prompt}: {}▏  Enter confirms · Esc cancels",ui.gps_text)} else {sandbox.last_message.clone()},
        Label::Source=>if politics.atlas {match &politics.dataset{None=>"NO POLITICAL DATASET · start with --politics FILE".into(),Some(data)if !data.contains_time(runtime.clock.unix_seconds as i64)=>format!("NO POLITICAL COVERAGE · {}",source_date(Some(runtime.clock.unix_seconds as i64))),_=>"POLITICAL ATLAS · source borders & authored territories".into()}} else if runtime.config.historical_pack.is_some() {format!("ERA PACK · {} · {}",source_date(runtime.config.data_epoch_unix),if resident.chunks.is_empty(){"LOADING"}else if resident.chunks.values().all(|c|c.is_synthetic()){"FICTIONAL EXAMPLE"}else if resident.chunks.values().any(|c|c.sources.iter().any(|s|s.synthetic)){"MIXED SOURCES"}else{"SOURCED GEOGRAPHY"})} else if runtime.config.live_data {"LIVE DATA REQUESTED · source status below".into()} else {"OFFLINE EXAMPLE · synthetic terrain & features".into()},
        Label::Attribution=>{
            if politics.atlas {politics.dataset.as_ref().map(|d|format!("{}\n{} · {}\n{}",d.title,d.attribution,d.license,if politics.hover.is_empty() {"Click a location to select it. Source geometry expresses its own boundary policy."} else {&politics.hover})).unwrap_or_else(||"No political dataset loaded. Start with --politics FILE.".into())}
            else {
                let mut records=std::collections::BTreeSet::new(); for c in resident.chunks.values() {for s in &c.sources {records.insert(format!("{} · {}",s.attribution,s.license));}}
                let live=resident.chunks.values().filter(|c|!c.is_synthetic()).count();let fallback=resident.chunks.values().filter(|c|c.sources.iter().any(|s|s.synthetic)).count();
                let mut value=if runtime.config.historical_pack.is_some() {format!("Frozen source date: {} · simulation date evolves separately",source_date(runtime.config.data_epoch_unix))} else if runtime.config.live_data {format!("{live} chunks include real data · {fallback} include synthetic fallback")} else {"Synthetic demonstration · real GPS reference · authored starter resources".into()};
                if runtime.config.live_data||runtime.config.historical_pack.is_some() {for record in records.iter().take(2) {value.push('\n');value.push_str(record);}}
                if !weather.synthetic {value.push_str("\nWeather by Open-Meteo · CC BY 4.0");}
                if let Some(error)=&runtime.last_error {value.push_str(&format!("\nLast provider warning: {}",error.chars().take(105).collect::<String>()));} value
            }
        },
        Label::Politics=>politics.describe(runtime.focus,&runtime.clock),
        Label::Faction=>active.map(|f|format!("{} · #{}\nYour role: {} · {} members\nShared plots: {}",f.name,f.id,f.members.get(&sandbox.player_id).map(|role|format!("{role:?}")).unwrap_or_else(||"Not a member".into()),f.members.len(),sandbox.state.plots.values().filter(|p|p.faction_owner==Some(f.id)).count())).unwrap_or_else(||"No faction selected. Create one or accept an invitation.".into()),
        Label::Connection=>net.as_ref().map(|n|format!("{} · tick {}",n.message,n.tick)).unwrap_or_else(||"LOCAL SESSION  ·  ENGINE 0.1".into()),
        Label::Layer(layer)=>format!("{}    {layer:?}",if runtime.config.layers.contains(*layer) {"ON "} else {"OFF"}),
        Label::Controls=>if politics.drawing {"Click vertices · Enter closes territory · Backspace undo · Esc cancels".into()} else if politics.atlas {"Teal: borders · Amber: disputed claims · Green: faction-linked\nWheel zoom · Right drag pan · Click select · M returns to world".into()} else if runtime.network_authority {"Arrows move player · Tab follows · WASD / Q,E explore · Right drag look".into()} else {"WASD move · Q / E altitude · Right drag look · Shift faster · M atlas · F5 save".into()},
    };
        if **text != value {
            **text = value;
        }
    }
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
