//! The island session shell: one top bar for every way of playing (menu,
//! Live / Oversee / Shape, the village totals, weather, the date and the
//! clock) and, while overseeing, the bottom docks that open the build menu
//! and the ledger's pages.
//!
//! Presentation and input only. Mode changes go through the single control
//! coordinator ([`crate::island_control::request_mode`]), so arrival, deferred
//! detach and foreign-controller refusal keep their existing rules. The clock
//! writes the host's `SimControl` exactly like the host speed keys; Live mode
//! may change speed too, because this is a single-player simulation and a
//! faster clock never changes what one tick does. The totals are read from
//! existing accessors and never stored; nothing here is saved or hashed.
pub(crate) mod coach;
pub(crate) mod dock;
pub(crate) mod focus;
pub(crate) mod model;

use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale::{self, LayoutClass};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, Kind, Part, Reach, StatChip, Tone};
use crate::{IslandMode, PlayerMenuRequest, game_ui as ui, player_actions};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::InputFocus;
use bevy::picking::{Pickable, hover::Hovered};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use focus::FocusRegion;
use island_content::village::{self, VillageState, colony};
use model::{
    ClockAction, ClockModel, LifeState, ModeIntent, ModeTab, SPEEDS, TotalKind, TotalMark,
    TotalsInput,
};
use npc_framework::prelude::{SimClock, SimConfig, SimControl, SimIndex};
use npc_framework::view_helpers::{ViewSystems, command_ui::DragBox};

/// Gap between the bar and the panels below it, and the screen margin.
const MARGIN: f32 = t::SCREEN_MARGIN;
/// Seconds a refused request stays visible.
const NOTICE_SECS: f32 = 6.0;
/// Seconds between two reads of the village totals.
const TOTALS_SECS: f32 = 0.25;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Menu,
    Mode(ModeTab),
    Clock(ClockAction),
    /// Open or close the calendar under the date.
    Calendar,
    /// A village total: open the ledger page that explains it.
    Total(TotalKind),
    /// The button of a host toast that failed: ask the host to try again.
    Retry,
}

/// How a toast from the host reads: a note, something that went well, or
/// something that failed. Never by colour alone: each has its own shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Note,
    Done,
    Failed,
}

/// A toast the host asked for: a save, a failed save, a loaded island.
#[derive(Clone, Debug, PartialEq)]
struct HostToast {
    text: String,
    tone: ToastTone,
    /// Seconds it still stays; `None` stays until the host replaces it.
    left: Option<f32>,
    /// The words on its one button ("Try again"), if it has one.
    button: Option<String>,
}

/// Holds the host's toast in the lane.
#[derive(Component)]
struct HostToastSlot;
#[derive(Component, Clone)]
struct ShellAction(Action);
#[derive(Component)]
pub(crate) struct ShellBar;
/// The toast that carries a refused request or a control message.
#[derive(Component)]
struct ShellNotice;
#[derive(Component)]
struct NoticeText;
#[derive(Component)]
struct PausedPill;
#[derive(Component)]
struct DateText;
#[derive(Component)]
struct TimeText;
#[derive(Component)]
struct DayFill;
#[derive(Component)]
struct SeasonSlot;
#[derive(Component)]
struct WeatherChip;
#[derive(Component)]
struct WeatherSlot;
#[derive(Component)]
struct WeatherLabel;
#[derive(Component)]
struct CalendarButton;
#[derive(Component)]
struct CalendarRoot;
#[derive(Component)]
struct CalendarNow;
#[derive(Component)]
struct CalendarNext;
/// The days ahead: forecast and the merchant boat.
#[derive(Component)]
struct CalendarOutlook;
#[derive(Component)]
struct CalendarOutlookRule;
#[derive(Component)]
struct CalendarGatheringsRule;
#[derive(Component)]
struct CalendarToday;
#[derive(Component)]
struct CalendarGatherings;
#[derive(Component)]
struct CalendarMourning;
/// The totals and the divider before them: Oversee only.
#[derive(Component)]
struct TotalsGroup;
/// The rule between the mode switch and the totals on the bar itself.
#[derive(Component)]
struct TotalsDivider;
/// The second bar row: the totals' place at the Compact width.
#[derive(Component)]
struct TotalsRow;
/// Toasts sit under the bar, however tall it is.
#[derive(Component)]
struct ToastLane;
/// Today's weather inside the date control, where the chip has no room.
#[derive(Component)]
struct DateWeatherSlot;
/// A key cap on a mode segment; hidden at the Standard width.
#[derive(Component)]
struct ModeKey;
/// The word on a mode segment; at the Compact width only the active one shows.
#[derive(Component)]
struct ModeLabel(ModeTab);

/// The bar's own input. The ledger and the character workspace read their
/// keys after it: Tab is taken here first, and a click is seen before a
/// panel shields the world from it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ShellInput;

/// The small panels anchored to a bar or dock control. One at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Popover {
    #[default]
    None,
    Calendar,
    Overlays,
    Places,
}

#[derive(Resource, Default)]
pub(crate) struct ShellState {
    requests: Vec<Action>,
    notice: Option<(String, f32)>,
    /// Notices said while another shows: each gets its turn.
    queued: std::collections::VecDeque<String>,
    host: Option<HostToast>,
    /// What the host's toast shows, so it is rebuilt on change only.
    host_shown: Option<(ToastTone, Option<String>, String)>,
    /// The host toast's button was chosen; the host takes it.
    retry: bool,
    pub(crate) popover: Popover,
    totals: Vec<(TotalKind, StatChip)>,
    totals_elapsed: f32,
    /// What the season and weather slots show, so they rebuild on change only.
    season: Option<Icon>,
    weather: Option<Icon>,
    date_weather: Option<Icon>,
    class: Option<LayoutClass>,
    /// The bar, the totals group, the second row and the group's place on
    /// the bar: the group moves between the two with the layout class.
    totals_home: Option<(Entity, Entity, Entity, usize)>,
    second_row: bool,
    /// Totals on the bar at the last refresh: more than five crowd it.
    totals_shown: usize,
}

pub(crate) fn plugin(app: &mut App) {
    if !colony::enabled(app.world()) {
        return;
    }
    ui::install(app);
    app.init_resource::<ShellState>()
        .init_resource::<dock::Docks>()
        .init_resource::<crate::PlayerExperience>()
        .init_resource::<PlayerMenuRequest>()
        .add_observer(activate)
        .add_observer(dock::activate)
        .add_plugins(coach::plugin)
        .add_systems(Startup, (spawn, dock::spawn))
        // Like a focused ledger control, a focused bar or palette control
        // keeps the keyboard: the camera does not pan under it.
        .configure_sets(Update, ViewSystems::Input.run_if(not(focus::reach_focused)))
        // Esc, one step per press: focus first, then a popover, before the
        // build tool, the right column and the pause menu see the key.
        .add_systems(
            Update,
            (focus::escape, popover_input)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .before(ShellInput)
                .before(crate::resident_inspector::keys)
                .before(crate::island_build::keys)
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (
                focus::release_on_click,
                shield,
                focus::keys,
                keys,
                dock::keys,
                process,
                dock::process,
            )
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .in_set(ShellInput)
                .run_if(not(crate::modal_open)),
        )
        .add_systems(
            Update,
            (refresh, dock::refresh, focus::drop_hidden)
                .chain()
                .in_set(ViewSystems::Present),
        );
}

/// The bar belongs to the player session; developer launches keep their ledger rows.
pub(crate) fn enabled(world: &World) -> bool {
    world.contains_resource::<ShellState>() && crate::player_hud::enabled(world)
}

/// Whether the session is overseeing: the docks, the alert stack and the
/// right column belong to this state.
pub(crate) fn overseeing(world: &World) -> bool {
    enabled(world)
        && world.get_resource::<IslandMode>() == Some(&IslandMode::Inspector)
        && !crate::player_hud::active(world)
}

/// Top edge (logical px) for panels placed under the bar; 12 without a bar.
pub(crate) fn content_top(world: &mut World) -> f32 {
    if !enabled(world) {
        return 12.0;
    }
    let height = world
        .query_filtered::<&ComputedNode, With<ShellBar>>()
        .iter(world)
        .map(|size| size.size().y * size.inverse_scale_factor())
        .find(|height| *height > 0.0)
        .unwrap_or(t::TOP_BAR);
    let second = world
        .get_resource::<ShellState>()
        .is_some_and(|state| state.second_row);
    height + if second { model::TOTALS_ROW } else { 0.0 } + MARGIN
}

/// Bottom edge (logical px from the window's bottom) for panels placed above
/// the docks while overseeing; the screen margin otherwise.
pub(crate) fn content_bottom(world: &World) -> f32 {
    if overseeing(world) {
        MARGIN + t::DOCK + MARGIN
    } else {
        MARGIN
    }
}

/// The shared clock projection, for any widget that names the time or rate.
pub(crate) fn clock(world: &World) -> ClockModel {
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    let (day, season, _) = village::calendar_at(tick, &config, true);
    let (paused, multiplier) = world
        .get_resource::<SimControl>()
        .map_or((false, 1), |control| (control.paused, control.multiplier));
    ClockModel::new(
        day,
        format!("{season:?}"),
        config.time_of_day_permille(tick),
        config.night_permille,
        paused,
        multiplier,
    )
}

fn life_state(world: &World) -> LifeState {
    let entity = world
        .get_resource::<VillageState>()
        .and_then(|state| world.get_resource::<SimIndex>()?.get(state.played));
    let Some(entity) = entity else {
        return LifeState::Missing;
    };
    match player_actions::control(world) {
        player_actions::ControlState::Foreign { .. } => LifeState::Foreign,
        player_actions::ControlState::Unavailable if crate::player_hud::dead(world) => {
            LifeState::Ended
        }
        player_actions::ControlState::Unavailable => LifeState::Missing,
        _ if world.get::<colony::AwaitingArrival>(entity).is_some() => LifeState::AwaitingArrival,
        _ => LifeState::Playable,
    }
}

fn plain(kind: &str) -> &str {
    kind.strip_prefix("village_").unwrap_or(kind)
}

/// Read the numbers behind the totals from existing accessors. Nothing is
/// estimated: a value the world does not have stays zero and unmarked.
fn totals_input(world: &World) -> TotalsInput {
    if !world.contains_resource::<SimIndex>() || !world.contains_resource::<colony::ColonyState>() {
        return TotalsInput::default();
    }
    let summary = colony::summary(world);
    let stock = crate::colony_panel::stock_totals(world);
    let held = |scope, good: &str| -> u32 {
        stock
            .iter()
            .filter(|((held_scope, kind), _)| *held_scope == scope && plain(kind) == good)
            .map(|(_, units)| *units)
            .sum()
    };
    let trends = world.get_resource::<crate::colony_panel::StockTrends>();
    let trend =
        |wanted: &dyn Fn(&str) -> bool| trends.map(|trends| trends.trend_where(None, wanted));
    let fund = world
        .get_resource::<VillageState>()
        .map_or(0, |state| state.fund);
    let building = crate::island_build::available(world);
    let sites = if building {
        village::civic::sites(world)
    } else {
        Vec::new()
    };
    let mut needed = [0_u32; 2];
    for site in &sites {
        for (kind, wanted) in site.cost.iter() {
            let missing = wanted.saturating_sub(site.delivered.get(kind).copied().unwrap_or(0));
            match plain(kind) {
                "log" => needed[0] += missing,
                "plank" => needed[1] += missing,
                _ => {}
            }
        }
    }
    let in_hand = world
        .get_resource::<crate::island_build::BuildTool>()
        .and_then(|tool| tool.selected.as_deref())
        .and_then(|key| village::civic::definition(world, key))
        .map(|def| {
            def.cost
                .iter()
                .map(|stack| plain(&stack.good).to_owned())
                .collect()
        })
        .unwrap_or_default();
    TotalsInput {
        residents: summary.residents,
        housed: summary.housed,
        beds: summary.beds,
        households: summary.households,
        morale_percent: summary.average_morale / 1000,
        social_events: summary.social_events,
        food: summary.food_units,
        water: summary.water_units,
        logs: held(None, "log"),
        planks: held(None, "plank"),
        fund,
        spendable: if building {
            village::civic::budget(world)
        } else {
            fund
        },
        held: if building {
            village::civic::fund_hold(world)
        } else {
            0
        },
        open_sites: sites.len(),
        needed,
        trends: [
            trend(&|kind| village::catalog::food_restore_in(world, kind).is_some()),
            trend(&|kind| kind == village::catalog::WATER),
            trend(&|kind| plain(kind) == "log"),
            trend(&|kind| plain(kind) == "plank"),
        ],
        villages: crate::colony_panel::scopes(world)
            .into_iter()
            .filter(|(scope, _)| scope.is_some())
            .map(|(scope, name)| (name, held(scope, "log"), held(scope, "plank")))
            .collect(),
        in_hand,
    }
}

/// Today's weather where the world has weather; `None` leaves the slot out.
fn weather(world: &World) -> Option<(Icon, String, Vec<String>)> {
    if !world.contains_resource::<village::weather::WeatherState>() {
        return None;
    }
    let name = |weather: village::weather::Weather| {
        let name = weather.name();
        let mut letters = name.chars();
        letters.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(letters).collect()
        })
    };
    let picture = |weather| match weather {
        village::weather::Weather::Clear => Icon::Clear,
        village::weather::Weather::Rain => Icon::Rain,
        village::weather::Weather::Storm => Icon::Storm,
        village::weather::Weather::Snow => Icon::Snow,
    };
    let today = village::weather::today(world);
    let forecast = village::weather::forecast(world)
        .into_iter()
        .enumerate()
        .map(|(index, (_, weather))| {
            if index == 0 {
                format!("Tomorrow: {}", name(weather).to_lowercase())
            } else {
                format!("In {} days: {}", index + 1, name(weather).to_lowercase())
            }
        })
        .collect();
    Some((picture(today), name(today), forecast))
}

fn tip(world: &mut World, entity: Entity, tooltip: Tooltip) {
    if world.get::<Tooltip>(entity) != Some(&tooltip) {
        world.entity_mut(entity).insert(tooltip);
    }
}

fn spawn(world: &mut World) {
    let bar = world
        .spawn((
            Name::new("Island session bar"),
            ShellBar,
            FocusRegion::Bar,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    top: px(0),
                    left: px(0),
                    right: px(0),
                    height: px(t::TOP_BAR),
                    padding: UiRect::horizontal(px(MARGIN)),
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                    display: Display::None,
                    ..default()
                },
                0.0,
            ),
        ))
        .id();
    let menu = w::icon_button(
        world,
        bar,
        "Menu",
        Icon::Menu,
        Surface::Chrome,
        Reach::Ring,
        ShellAction(Action::Menu),
    );
    world
        .entity_mut(menu)
        .insert(Tooltip::new("Menu", "Save, settings and the way back to the title.").key("Esc"));
    let modes = w::segmented(world, bar, Surface::Chrome);
    for tab in ModeTab::ALL {
        let segment = w::segment(
            world,
            modes,
            tab.label(),
            Some(tab.icon()),
            Some(tab.label()),
            Some(tab.key_cap()),
            Reach::Ring,
            ShellAction(Action::Mode(tab)),
        );
        let caps: Vec<Entity> = world
            .get::<Children>(segment)
            .map(|children| children.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|child| world.get::<Part>(*child) == Some(&Part::Key))
            .collect();
        for cap in caps {
            world.entity_mut(cap).insert(ModeKey);
        }
        let words: Vec<Entity> = world
            .get::<Children>(segment)
            .map(|children| children.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|child| world.get::<Text>(*child).is_some())
            .collect();
        for word in words {
            world.entity_mut(word).insert(ModeLabel(tab));
        }
    }
    let divider = w::divider(world, bar, Surface::Chrome, true);
    world.entity_mut(divider).insert(TotalsDivider);
    let totals = world
        .spawn((
            ChildOf(bar),
            TotalsGroup,
            // If a narrow window ever runs out of room the totals give way,
            // never the clock at the right.
            Node {
                align_items: AlignItems::Center,
                column_gap: px(2),
                flex_shrink: 1.0,
                min_width: px(0),
                overflow: Overflow::clip_x(),
                ..default()
            },
        ))
        .id();
    let mut chips = Vec::new();
    for kind in TotalKind::ALL {
        let chip = w::stat_chip(
            world,
            totals,
            kind.label(),
            kind.icon(),
            "",
            None,
            Surface::Chrome,
            Reach::Ring,
            ShellAction(Action::Total(kind)),
        );
        // The unit ("/ 84", "fund") sits between the number and the mark.
        let unit = w::label(
            world,
            chip.entity,
            "",
            t::CAPTION,
            Tone::secondary(Surface::Chrome),
        );
        world.entity_mut(unit).insert(Part::Secondary);
        let mut order = world
            .get::<Children>(chip.entity)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        order.retain(|child| *child != unit);
        order.insert(2.min(order.len()), unit);
        world.entity_mut(chip.entity).replace_children(&order);
        chips.push((kind, chip));
    }
    world.resource_mut::<ShellState>().totals = chips;
    // Compact: the totals have a row of their own under the bar.
    let row = world
        .spawn((
            Name::new("Island totals row"),
            TotalsRow,
            FocusRegion::Bar,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    top: px(t::TOP_BAR),
                    left: px(0),
                    right: px(0),
                    height: px(model::TOTALS_ROW),
                    padding: UiRect::horizontal(px(MARGIN)),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                    display: Display::None,
                    ..default()
                },
                0.0,
            ),
        ))
        .id();
    let place = world
        .get::<Children>(bar)
        .and_then(|children| children.iter().position(|child| child == totals))
        .unwrap_or(0);
    world.resource_mut::<ShellState>().totals_home = Some((bar, totals, row, place));
    // The spacer keeps weather, the date and the speed controls on the right.
    world.spawn((
        ChildOf(bar),
        Node {
            flex_grow: 1.0,
            min_width: px(4),
            ..default()
        },
    ));
    let weather = world
        .spawn((
            ChildOf(bar),
            Name::new("Island weather"),
            WeatherChip,
            Hovered::default(),
            w::Skin::Key(Surface::Chrome),
            Node {
                height: px(t::CHIP_CHROME),
                padding: UiRect::horizontal(px(8)),
                column_gap: px(6),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .id();
    world.spawn((
        ChildOf(weather),
        WeatherSlot,
        Pickable::IGNORE,
        Node {
            width: px(16),
            height: px(16),
            ..default()
        },
    ));
    let word = w::label(world, weather, "", t::BODY, Tone::text(Surface::Chrome));
    world.entity_mut(word).insert(WeatherLabel);
    // The date and time are one control that opens the calendar.
    let calendar = w::control(
        world,
        bar,
        "Calendar",
        Kind::Secondary,
        Surface::Chrome,
        Reach::Ring,
        Node {
            height: px(32),
            min_width: px(150),
            padding: UiRect::horizontal(px(10)),
            row_gap: px(3),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_MD)),
            flex_shrink: 0.0,
            ..default()
        },
        (ShellAction(Action::Calendar), CalendarButton),
    );
    let line = world
        .spawn((
            ChildOf(calendar),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(line),
        SeasonSlot,
        Pickable::IGNORE,
        Node {
            width: px(14),
            height: px(14),
            ..default()
        },
    ));
    let date = w::label(world, line, "", t::DENSE, Tone::text(Surface::Chrome));
    world.entity_mut(date).insert((
        DateText,
        Part::Label,
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    world.spawn((
        ChildOf(line),
        DateWeatherSlot,
        Pickable::IGNORE,
        Node {
            width: px(14),
            height: px(14),
            display: Display::None,
            ..default()
        },
    ));
    let time = w::label(world, line, "", t::DENSE, Tone::text(Surface::Chrome));
    world.entity_mut(time).insert((TimeText, Part::Label));
    let track = world
        .spawn((
            ChildOf(calendar),
            Pickable::IGNORE,
            Node {
                height: px(3),
                border_radius: BorderRadius::all(px(2)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(t::BARK_600),
        ))
        .id();
    world.spawn((
        ChildOf(track),
        DayFill,
        Pickable::IGNORE,
        Node {
            width: percent(0),
            height: percent(100),
            ..default()
        },
        BackgroundColor(t::HONEY_500),
    ));
    let speeds = w::segmented(world, bar, Surface::Chrome);
    let pause = w::segment(
        world,
        speeds,
        "Pause",
        Some(Icon::Pause),
        None,
        None,
        Reach::Ring,
        ShellAction(Action::Clock(ClockAction::TogglePause)),
    );
    world.entity_mut(pause).insert(
        Tooltip::new("Pause", "Stop the clock, or let it run again.")
            .key("Space")
            .detail(["Commands given while paused apply when the clock runs again."]),
    );
    for speed in SPEEDS {
        let name = format!("{speed}×");
        let segment = w::segment(
            world,
            speeds,
            &name,
            None,
            Some(&name),
            None,
            Reach::Ring,
            ShellAction(Action::Clock(ClockAction::Speed(speed))),
        );
        if let Some(mut node) = world.get_mut::<Node>(segment) {
            node.padding = UiRect::horizontal(px(7));
        }
        world.entity_mut(segment).insert(
            Tooltip::new(
                format!("Speed {name}"),
                "How fast the clock runs. A faster clock never changes what one moment does.",
            )
            .detail(["Slower and faster: the - and = keys."]),
        );
    }

    // Toasts sit top centre, under the bar: the paused marker, then a notice.
    let lane = world
        .spawn((
            Name::new("Island toasts"),
            ToastLane,
            Pickable::IGNORE,
            GlobalZIndex(t::layer::TOAST),
            Node {
                position_type: PositionType::Absolute,
                top: px(t::TOP_BAR + MARGIN),
                left: px(0),
                right: px(0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
        ))
        .id();
    let paused = w::toast(world, lane, Status::Idle, "Paused", Some(Icon::Pause));
    w::keycap(world, paused, Surface::Chrome, "Space");
    world
        .entity_mut(paused)
        .insert((PausedPill, Pickable::IGNORE));
    w::show(world, paused, false);
    let notice = w::toast(world, lane, Status::Info, "", Some(Icon::Hint));
    world
        .entity_mut(notice)
        .insert((ShellNotice, Pickable::IGNORE));
    let text = world
        .get::<Children>(notice)
        .and_then(|children| children.last().copied())
        .expect("toast text");
    world.entity_mut(text).insert(NoticeText);
    w::show(world, notice, false);
    world.spawn((
        ChildOf(lane),
        Name::new("Island host toast"),
        HostToastSlot,
        focus::FocusRegion::Toast,
        Pickable::IGNORE,
        Node {
            display: Display::None,
            ..default()
        },
    ));

    let popover = world
        .spawn((
            Name::new("Island calendar"),
            CalendarRoot,
            crate::game_ui::sound::PanelSound::default(),
            GlobalZIndex(t::layer::POPOVER),
            w::panel(Node {
                position_type: PositionType::Absolute,
                top: px(t::TOP_BAR + 4.0),
                right: px(MARGIN),
                width: px(336),
                max_width: percent(92),
                row_gap: px(6),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            }),
        ))
        .id();
    w::label(
        world,
        popover,
        "Calendar",
        t::SUBHEADING,
        Tone::text(Surface::Paper),
    );
    let now = w::paragraph(world, popover, "", t::DENSE, Tone::text(Surface::Paper));
    world.entity_mut(now).insert(CalendarNow);
    w::divider(world, popover, Surface::Paper, false);
    let next = w::paragraph(
        world,
        popover,
        "",
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );
    world.entity_mut(next).insert(CalendarNext);
    let rule = w::divider(world, popover, Surface::Paper, false);
    world.entity_mut(rule).insert(CalendarOutlookRule);
    let outlook = w::paragraph(world, popover, "", t::DENSE, Tone::text(Surface::Paper));
    world.entity_mut(outlook).insert(CalendarOutlook);
    // Village gatherings: today's first, in the status colour with a word,
    // then the ones ahead, then who is in mourning.
    let rule = w::divider(world, popover, Surface::Paper, false);
    world.entity_mut(rule).insert(CalendarGatheringsRule);
    let today = w::paragraph(
        world,
        popover,
        "",
        t::DENSE,
        Tone::status(Surface::Paper, crate::game_ui::tokens::Status::Good),
    );
    world.entity_mut(today).insert(CalendarToday);
    let ahead = w::paragraph(world, popover, "", t::DENSE, Tone::text(Surface::Paper));
    world.entity_mut(ahead).insert(CalendarGatherings);
    let mourning = w::paragraph(
        world,
        popover,
        "",
        t::DENSE,
        Tone::secondary(Surface::Paper),
    );
    world.entity_mut(mourning).insert(CalendarMourning);
}

/// The calendar's gatherings and mourning notes from the simulation's own
/// views: `(today, ahead, mourning)` lines.
fn gatherings(world: &World) -> (Vec<String>, Vec<String>, Vec<String>) {
    // The simulation's day count, the one its gatherings and mourning use.
    let today = village::calendar(world).0;
    let clock = |permille: u32| {
        crate::resident_inspector::model::clock_text(i32::try_from(permille).unwrap_or(0))
    };
    let list: Vec<model::GatheringLine> = village::view::gatherings(world)
        .into_iter()
        .map(|gathering| model::GatheringLine {
            name: gathering.name,
            days_away: gathering.next_day.saturating_sub(today),
            hours: format!(
                "{}\u{2013}{}",
                clock(gathering.from_permille),
                clock(gathering.to_permille)
            ),
            active: gathering.active,
            last: gathering.last_attendance,
        })
        .collect();
    let (now, ahead) = model::gathering_lines(&list);
    let mourning = village::view::mourning(world)
        .into_iter()
        .map(|(mourner, whom, until)| {
            let name = world
                .get_resource::<npc_framework::prelude::SimIndex>()
                .and_then(|index| index.get(mourner))
                .and_then(|entity| world.get::<village::Villager>(entity))
                .map_or_else(|| "A resident".to_owned(), |person| person.name.clone());
            model::mourning_line(&name, &whom, until.saturating_sub(today))
        })
        .collect();
    (now, ahead, mourning)
}

/// The days ahead from the simulation's own views: the forecast where the
/// world has weather, the merchant boat where it has one. Nothing invented.
/// The boat's announced orders, as the order rows want them.
pub(crate) fn boat_orders(boat: &village::merchant::BoatView) -> Vec<model::BoatOrderRow> {
    boat.next_order
        .iter()
        .map(|order| {
            (
                order.good.clone(),
                order.units,
                order.unit_price,
                order.delivered,
            )
        })
        .collect()
}

fn outlook(world: &World) -> model::Outlook {
    let mut outlook = model::Outlook {
        growth_percent: 100,
        warmth_loss_percent: 100,
        ..default()
    };
    if world.contains_resource::<village::weather::WeatherState>() {
        let weather = village::view::weather(world);
        outlook.today = weather.today.weather.name().to_owned();
        outlook.ahead = weather
            .forecast
            .iter()
            .map(|day| day.weather.name().to_owned())
            .collect();
        outlook.growth_percent = weather.growth_percent;
        outlook.warmth_loss_percent = weather.warmth_loss_percent;
        outlook.shock = weather.shock.map(|shock| shock.name().to_owned());
    }
    let boat = village::view::boat(world);
    if boat.enabled {
        let (tick, config) = (
            world
                .get_resource::<SimClock>()
                .map_or(0, |clock| clock.tick),
            world
                .get_resource::<SimConfig>()
                .copied()
                .unwrap_or_default(),
        );
        outlook.boat_orders = model::boat_order_lines(&boat_orders(&boat), boat.docked);
        outlook.boat = Some(match boat.leaves_at.filter(|_| boat.docked) {
            Some(leaves) => model::BoatOutlook::In {
                until: crate::resident_inspector::model::clock_text(
                    config.time_of_day_permille(leaves) as i32,
                ),
            },
            None => {
                let today = village::calendar_at(tick, &config, true).0 + 1;
                model::BoatOutlook::Due {
                    days: boat.next_visit_day.saturating_sub(today),
                }
            }
        });
    }
    outlook
}

fn activate(
    event: On<Activate>,
    actions: Query<(&ShellAction, Has<InteractionDisabled>)>,
    mut state: ResMut<ShellState>,
) {
    if let Ok((action, false)) = actions.get(event.entity) {
        state.requests.push(action.0.clone());
    }
}

/// A click on the bar never also selects, orders or paints in the world below.
fn shield(
    bars: Query<(&Hovered, &Node), Or<(With<ShellBar>, With<dock::DockRoot>)>>,
    mut mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    mut scroll: Option<ResMut<AccumulatedMouseScroll>>,
    mut drag: Option<ResMut<DragBox>>,
) {
    if !bars
        .iter()
        .any(|(hovered, node)| node.display != Display::None && hovered.get())
    {
        return;
    }
    if let Some(mouse) = mouse.as_mut() {
        mouse.reset_all();
    }
    if let Some(scroll) = scroll.as_mut() {
        scroll.delta = Vec2::ZERO;
    }
    if let Some(drag) = drag.as_mut() {
        drag.start = None;
    }
}

/// Esc, or a click anywhere but the bars and the popover, closes the open
/// popover. A click on the popover itself never reaches the world below.
fn popover_input(world: &mut World) {
    if world.resource::<ShellState>().popover == Popover::None {
        return;
    }
    let escape = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    if escape {
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset(KeyCode::Escape);
        world.resource_mut::<ShellState>().popover = Popover::None;
        return;
    }
    let clicked = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| {
            mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)
        });
    if !clicked {
        return;
    }
    let over_popover = world
        .query_filtered::<(&Hovered, &Node), Or<(With<CalendarRoot>, With<dock::PopoverRoot>)>>()
        .iter(world)
        .any(|(hovered, node)| node.display != Display::None && hovered.get());
    let over_bar = world
        .query_filtered::<&Hovered, Or<(With<ShellBar>, With<dock::DockRoot>)>>()
        .iter(world)
        .any(Hovered::get);
    if over_popover {
        if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            mouse.reset_all();
        }
    } else if !over_bar {
        world.resource_mut::<ShellState>().popover = Popover::None;
    }
}

/// What the running world says the seasons change, for the calendar.
fn season_facts(world: &World) -> model::SeasonFacts {
    let regional = village::frontier::enabled(world);
    let cold = village::needs_driven(world);
    let winter_growth = village::frontier::spec(world)
        .map(|spec| {
            spec.economy
                .patches
                .iter()
                .filter(|patch| !patch.finite)
                .filter_map(|patch| {
                    let winter = patch.winter_regrowth?;
                    let name = village::catalog::display_in(world, &patch.good)
                        .map_or_else(|| patch.good.replace("village_", ""), str::to_owned);
                    Some((name, winter, patch.regrowth))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut winter_growth = winter_growth;
    winter_growth.sort();
    winter_growth.dedup();
    model::SeasonFacts {
        cold,
        regional,
        winter_growth,
        seasonal_grain: !regional && cold,
        hearth_logs: village::frontier::economy_spec::tuning(world)
            .filter(|_| cold)
            .map(|tuning| (tuning.winter_hearth_lightings.max(1), 2)),
    }
}

/// The calendar behind the clock, from the simulation's own calendar.
pub(crate) fn calendar(world: &World) -> model::CalendarModel {
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    let (day, _, _) = village::calendar_at(tick, &config, true);
    model::CalendarModel::new(day, village::DAYS_PER_SEASON, config.night_permille)
}

/// 1 / 2 / 3 choose the mode; Space, `-` and `=` drive the clock in every mode.
/// Handled keys are consumed so the host's own speed keys do not act twice.
fn keys(world: &mut World) {
    if !enabled(world) {
        return;
    }
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    if focused.is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some()) {
        return;
    }
    // Space activates a focused control; it pauses only from the world itself.
    let control_focused = focused.is_some_and(|entity| world.get::<Window>(entity).is_none());
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    let modifiers = [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ];
    if keys.any_pressed(modifiers) || keys.any_just_pressed(modifiers) {
        return;
    }
    let mut requests = Vec::new();
    let mut consumed = Vec::new();
    for tab in ModeTab::ALL {
        if keys.just_pressed(tab.key()) {
            requests.push(Action::Mode(tab));
            consumed.push(tab.key());
        }
    }
    if keys.just_pressed(KeyCode::Space) && !control_focused {
        requests.push(Action::Clock(ClockAction::TogglePause));
        consumed.push(KeyCode::Space);
    }
    for (key, action) in [
        (KeyCode::Minus, ClockAction::Slower),
        (KeyCode::NumpadSubtract, ClockAction::Slower),
        (KeyCode::Equal, ClockAction::Faster),
        (KeyCode::NumpadAdd, ClockAction::Faster),
    ] {
        if keys.just_pressed(key) {
            requests.push(Action::Clock(action));
            consumed.push(key);
        }
    }
    if requests.is_empty() {
        return;
    }
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    for key in consumed {
        keys.reset(key);
    }
    world.resource_mut::<ShellState>().requests.extend(requests);
}

/// A toast from the host, under the bar in every mode. `lasting` stays until
/// the host replaces or clears it; `retry` adds a "Try again" button whose
/// press [`take_toast_retry`] reports.
pub(crate) fn host_toast(
    world: &mut World,
    tone: ToastTone,
    text: impl Into<String>,
    lasting: bool,
    retry: bool,
) {
    let button = retry.then(|| "Try again".to_owned());
    set_host_toast(world, tone, text.into(), lasting, button);
}

/// A host toast with its own button, such as "Start a new game on it". A
/// toast with a button stays long enough to be reached: [`HOST_BUTTON_STAY`]
/// seconds unless `lasting`.
pub(crate) fn host_toast_with(
    world: &mut World,
    tone: ToastTone,
    text: impl Into<String>,
    lasting: bool,
    button: impl Into<String>,
) {
    set_host_toast(world, tone, text.into(), lasting, Some(button.into()));
}

/// How long a passing toast with a button stays.
pub(crate) const HOST_BUTTON_STAY: f32 = 12.0;

fn set_host_toast(
    world: &mut World,
    tone: ToastTone,
    text: String,
    lasting: bool,
    button: Option<String>,
) {
    // A failure refuses; a success confirms.
    let sound = match tone {
        ToastTone::Failed => Some(crate::game_ui::sound::UiSound::Deny),
        ToastTone::Done => Some(crate::game_ui::sound::UiSound::Confirm),
        ToastTone::Note => None,
    };
    if let Some(sound) = sound {
        crate::game_ui::sound::play(world, sound);
    }
    if let Some(mut state) = world.get_resource_mut::<ShellState>() {
        let stay = if button.is_some() {
            HOST_BUTTON_STAY
        } else {
            t::motion::TOAST_STAY
        };
        state.host = Some(HostToast {
            text,
            tone,
            left: (!lasting).then_some(stay),
            button,
        });
    }
}

pub(crate) fn clear_host_toast(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<ShellState>() {
        state.host = None;
    }
}

/// Whether the host toast's button was chosen since the last call.
pub(crate) fn take_toast_retry(world: &mut World) -> bool {
    world
        .get_resource_mut::<ShellState>()
        .is_some_and(|mut state| std::mem::take(&mut state.retry))
}

/// The shape and the word-free picture of a host toast.
fn toast_look(tone: ToastTone) -> (Status, Icon) {
    match tone {
        ToastTone::Note => (Status::Info, Icon::Hint),
        ToastTone::Done => (Status::Good, Icon::Done),
        ToastTone::Failed => (Status::Crit, Icon::status(Status::Crit)),
    }
}

fn refresh_host_toast(world: &mut World, gated: bool, delta: f32) {
    let wanted = {
        let mut state = world.resource_mut::<ShellState>();
        // A hitch (loading an island takes seconds in one frame) does not
        // eat a toast: it counts at most a tenth of a second a frame.
        if let Some(left) = state.host.as_mut().and_then(|toast| toast.left.as_mut()) {
            *left -= delta.min(0.1);
        }
        if state
            .host
            .as_ref()
            .is_some_and(|toast| toast.left.is_some_and(|left| left <= 0.0))
        {
            state.host = None;
        }
        state
            .host
            .as_ref()
            .map(|toast| (toast.tone, toast.button.clone(), toast.text.clone()))
    };
    let slots: Vec<Entity> = world
        .query_filtered::<Entity, With<HostToastSlot>>()
        .iter(world)
        .collect();
    if world.resource::<ShellState>().host_shown != wanted {
        for slot in &slots {
            world.entity_mut(*slot).despawn_children();
            if let Some((tone, button, text)) = &wanted {
                let (status, picture) = toast_look(*tone);
                let toast = w::toast(world, *slot, status, text, Some(picture));
                if let Some(label) = button {
                    w::button(
                        world,
                        toast,
                        w::ButtonSpec::new(label, Kind::Secondary, Surface::Chrome)
                            .size(w::Size::Small)
                            .reach(Reach::Ring),
                        ShellAction(Action::Retry),
                    );
                }
            }
        }
        world.resource_mut::<ShellState>().host_shown = wanted.clone();
    }
    for slot in slots {
        w::show(world, slot, wanted.is_some() && !gated);
    }
}

/// Show a sentence as a toast under the bar for a few seconds.
pub(crate) fn notice(world: &mut World, text: impl Into<String>) {
    if let Some(mut state) = world.get_resource_mut::<ShellState>() {
        let state = &mut *state;
        model::queue_notice(
            &mut state.notice,
            &mut state.queued,
            text.into(),
            NOTICE_SECS,
        );
    }
}

fn switch(world: &mut World, mode: IslandMode) -> Result<(), String> {
    if mode != IslandMode::Character {
        // Close the character workspace and acknowledge an ended life; the
        // coordinator still decides when native control may detach.
        crate::player_hud::leave_character_view(world);
    }
    crate::island_control::request_mode(world, mode)
}

fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<ShellState>().requests);
    if !enabled(world) {
        return;
    }
    for request in requests {
        match request {
            Action::Menu => world.resource_mut::<PlayerMenuRequest>().0 = true,
            Action::Calendar => {
                let mut state = world.resource_mut::<ShellState>();
                state.popover = if state.popover == Popover::Calendar {
                    Popover::None
                } else {
                    Popover::Calendar
                };
            }
            Action::Total(kind) => crate::colony_panel::open_page(world, kind.page()),
            Action::Retry => world.resource_mut::<ShellState>().retry = true,
            Action::Mode(tab) => {
                let current = *world.resource::<IslandMode>();
                match model::intent(current, tab, life_state(world)) {
                    ModeIntent::Stay => {}
                    ModeIntent::Switch(mode) => {
                        if let Err(error) = switch(world, mode) {
                            notice(world, error);
                        } else {
                            let mut state = world.resource_mut::<ShellState>();
                            state.notice = None;
                            state.queued.clear();
                            state.popover = Popover::None;
                        }
                    }
                    ModeIntent::LifeChoices => crate::player_hud::show_life_choices(world),
                    ModeIntent::Refuse(reason) => notice(world, reason),
                }
            }
            Action::Clock(action) => {
                if let Some(mut control) = world.get_resource_mut::<SimControl>() {
                    let (paused, multiplier) =
                        model::clock_step(control.paused, control.multiplier, action);
                    control.paused = paused;
                    control.multiplier = multiplier;
                }
            }
        }
    }
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

fn show<M: Component>(world: &mut World, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    for mut node in world.query_filtered::<&mut Node, With<M>>().iter_mut(world) {
        if node.display != display {
            node.display = display;
        }
    }
}

/// Replace the icon in a one-icon slot.
fn slot<M: Component>(world: &mut World, picture: Icon, size: f32, tone: Tone) {
    let slots: Vec<Entity> = world
        .query_filtered::<Entity, With<M>>()
        .iter(world)
        .collect();
    for slot in slots {
        world.entity_mut(slot).despawn_children();
        let entity = icon::icon(world, slot, picture, size, tone);
        // Inside a control (the date) the icon follows the control's ink.
        let mut at = slot;
        for _ in 0..3 {
            let Some(parent) = world.get::<ChildOf>(at).map(ChildOf::parent) else {
                break;
            };
            if world.get::<w::Control>(parent).is_some() {
                world.entity_mut(entity).insert(Part::Label);
                break;
            }
            at = parent;
        }
    }
}

/// Move the totals between the bar and the row under it.
fn place_totals(world: &mut World, second: bool) {
    if world.resource::<ShellState>().second_row == second {
        return;
    }
    world.resource_mut::<ShellState>().second_row = second;
    let Some((bar, group, row, place)) = world.resource::<ShellState>().totals_home else {
        return;
    };
    if second {
        world.entity_mut(group).insert(ChildOf(row));
    } else {
        let mut order: Vec<Entity> = world
            .get::<Children>(bar)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        order.retain(|child| *child != group);
        order.insert(place.min(order.len()), group);
        world.entity_mut(bar).replace_children(&order);
    }
}

fn refresh_totals(world: &mut World, shown: bool, class: LayoutClass) {
    let second = model::totals_on_second_row(class, shown);
    place_totals(world, second);
    show::<TotalsGroup>(world, shown);
    show::<TotalsDivider>(world, shown && !second);
    show::<TotalsRow>(world, second);
    let (left, right, drop) = model::toast_lane(
        scale::layout(world).width,
        crate::player_hud::workspace_width(world),
        crate::player_hud::GOAL_EDGE,
    );
    // Shape's resources panel stands right of the rail: the lane (the
    // Paused pill and the toasts) is centred in what is left beside it, so
    // an export's toast never covers the panel's goods.
    let left = if crate::shape_tools::resources::active(world) {
        left.max(t::SCREEN_MARGIN + 76.0 + 8.0 + crate::shape_tools::resources::PANEL_WIDTH + 8.0)
    } else {
        left
    };
    let top = px(content_top(world) + drop);
    for mut node in world
        .query_filtered::<&mut Node, With<ToastLane>>()
        .iter_mut(world)
    {
        if node.top != top {
            node.top = top;
        }
        if node.left != px(left) {
            node.left = px(left);
        }
        if node.right != px(right) {
            node.right = px(right);
        }
    }
    if !shown {
        return;
    }
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let mut state = world.resource_mut::<ShellState>();
        state.totals_elapsed += delta;
        if state.totals_elapsed < TOTALS_SECS && state.class == Some(class) {
            return;
        }
        state.totals_elapsed = 0.0;
    }
    let totals = model::totals(&totals_input(world));
    world.resource_mut::<ShellState>().totals_shown = totals
        .iter()
        .filter(|total| model::total_shown(total, class))
        .count();
    let chips = world.resource::<ShellState>().totals.clone();
    for (kind, chip) in chips {
        let Some(total) = totals.iter().find(|total| total.kind == kind) else {
            continue;
        };
        w::show(world, chip.entity, model::total_shown(total, class));
        w::set_text(world, chip.value, &total.value);
        let unit = world
            .get::<Children>(chip.entity)
            .and_then(|children| children.get(2).copied());
        if let Some(unit) = unit {
            w::set_text(world, unit, total.unit.as_deref().unwrap_or(""));
            // The Standard width keeps the number; the unit is in the tooltip.
            w::show(
                world,
                unit,
                total.unit.is_some() && class == LayoutClass::Wide,
            );
        }
        w::set_mark(
            world,
            chip.mark,
            total.mark.map(|mark| match mark {
                TotalMark::Low => (Icon::Low, Status::Warn),
                TotalMark::Up => (Icon::TrendUp, Status::Good),
                TotalMark::Down => (Icon::TrendDown, Status::Crit),
            }),
            Surface::Chrome,
        );
        w::set_state(world, chip.entity, false, total.in_use);
        tip(
            world,
            chip.entity,
            Tooltip::new(kind.label(), total.sentence.clone())
                .icon(kind.icon())
                .value(total.reading.clone())
                .detail(total.facts.clone()),
        );
    }
}

fn refresh(world: &mut World) {
    let shown = enabled(world);
    show::<ShellBar>(world, shown);
    if !shown {
        show::<HostToastSlot>(world, false);
        show::<ShellNotice>(world, false);
        show::<PausedPill>(world, false);
        show::<CalendarRoot>(world, false);
        return;
    }
    let mode = *world.resource::<IslandMode>();
    let life = life_state(world);
    let bar = model::mode_bar(mode, life);
    let clock = clock(world);
    let calendar = calendar(world);
    let layout = scale::layout(world);
    let wide = layout.class == LayoutClass::Wide;
    let popover = world.resource::<ShellState>().popover;
    let buttons: Vec<_> = world
        .query::<(Entity, &ShellAction)>()
        .iter(world)
        .map(|(entity, action)| (entity, action.0.clone()))
        .collect();
    for (entity, action) in buttons {
        let (on, enabled) = match action {
            Action::Menu | Action::Total(_) | Action::Retry => continue,
            Action::Mode(tab) => {
                let button = bar.iter().find(|button| button.tab == tab).expect("mode");
                tip(
                    world,
                    entity,
                    Tooltip::new(tab.label(), model::mode_tooltip(mode, tab, life))
                        .icon(tab.icon())
                        .key(tab.key_cap()),
                );
                (button.active, button.enabled)
            }
            Action::Clock(ClockAction::TogglePause) => (clock.stopped, true),
            Action::Clock(ClockAction::Speed(speed)) => (clock.active_speed() == Some(speed), true),
            Action::Clock(_) => (false, true),
            Action::Calendar => (popover == Popover::Calendar, true),
        };
        w::set_state(world, entity, on, false);
        w::set_enabled(world, entity, enabled);
    }
    // Key caps leave the mode switch at the Standard width; they stay in the tooltips.
    show::<ModeKey>(world, wide);
    // Compact, or a Standard bar crowded by a building in hand: icons, with
    // the word on the active segment only; the weather moves into the date.
    let overseeing_now = mode == IslandMode::Inspector && !crate::player_hud::active(world);
    let (fold_weather, fold_modes) = model::bar_crowding(
        layout.class,
        if overseeing_now {
            world.resource::<ShellState>().totals_shown
        } else {
            0
        },
    );
    let compact = fold_weather;
    for (label, mut node) in world.query::<(&ModeLabel, &mut Node)>().iter_mut(world) {
        let display = if !fold_modes || ModeTab::of(mode) == label.0 {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    let overseeing = mode == IslandMode::Inspector && !crate::player_hud::active(world);
    refresh_totals(world, overseeing, layout.class);
    world.resource_mut::<ShellState>().class = Some(layout.class);

    set_text::<DateText>(world, &calendar.date_text());
    set_text::<TimeText>(world, &clock.time_text());
    let fill = percent(clock.day_fraction() * 100.0);
    for mut node in world
        .query_filtered::<&mut Node, With<DayFill>>()
        .iter_mut(world)
    {
        if node.width != fill {
            node.width = fill;
        }
    }
    let season = Icon::of_season(&clock.season);
    if world.resource::<ShellState>().season != season
        && let Some(picture) = season
    {
        slot::<SeasonSlot>(world, picture, 14.0, Tone::text(Surface::Chrome));
        world.resource_mut::<ShellState>().season = season;
    }
    let dates: Vec<Entity> = world
        .query_filtered::<Entity, With<CalendarButton>>()
        .iter(world)
        .collect();
    for entity in dates {
        tip(
            world,
            entity,
            Tooltip::new("Calendar", calendar.next_line())
                .value(format!("{} · {}", calendar.season_line(), clock.phase))
                .detail([
                    calendar.light_line(),
                    format!("Day {} on the island", clock.day),
                ]),
        );
    }
    // Weather: shown only where the world has weather; never invented.
    let today = weather(world);
    // Compact has no room for the chip; the calendar still has the season.
    show::<WeatherChip>(world, today.is_some() && !compact);
    // There the weather folds into the date: its icon beside the time, its
    // name in the calendar's tooltip.
    show::<DateWeatherSlot>(world, today.is_some() && compact);
    if let Some((picture, name, _)) = &today {
        if compact && world.resource::<ShellState>().date_weather != Some(*picture) {
            slot::<DateWeatherSlot>(world, *picture, 14.0, Tone::text(Surface::Chrome));
            world.resource_mut::<ShellState>().date_weather = Some(*picture);
        }
        if compact {
            let dates: Vec<Entity> = world
                .query_filtered::<Entity, With<CalendarButton>>()
                .iter(world)
                .collect();
            for entity in dates {
                tip(
                    world,
                    entity,
                    Tooltip::new("Calendar", calendar.next_line())
                        .value(format!("{} \u{b7} {}", calendar.season_line(), clock.phase))
                        .detail([
                            calendar.light_line(),
                            format!("Day {} on the island", clock.day),
                            format!("Weather today: {name}"),
                        ]),
                );
            }
        }
    }
    if let Some((picture, name, forecast)) = today {
        if world.resource::<ShellState>().weather != Some(picture) {
            slot::<WeatherSlot>(world, picture, 16.0, Tone::text(Surface::Chrome));
            world.resource_mut::<ShellState>().weather = Some(picture);
        }
        set_text::<WeatherLabel>(world, &name);
        // The Standard width keeps the icon; the word is in the tooltip.
        show::<WeatherLabel>(world, wide);
        let chips: Vec<Entity> = world
            .query_filtered::<Entity, With<WeatherChip>>()
            .iter(world)
            .collect();
        for entity in chips {
            tip(
                world,
                entity,
                Tooltip::new("Weather", "Today's weather on the island.")
                    .icon(picture)
                    .value(name.clone())
                    .detail(forecast.clone()),
            );
        }
    }
    refresh_calendar(world);

    // The paused state is always shown twice: the lit segment and this pill.
    let gated = world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0);
    // The calendar opens under the date and, in a narrow window, reaches the
    // middle: the pill gives way to it (the bar's own Pause is lit meanwhile).
    let calendar_open = world.resource::<ShellState>().popover == Popover::Calendar;
    show::<PausedPill>(world, clock.stopped && !gated && !calendar_open);
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    refresh_host_toast(world, gated, delta);
    let transient = {
        let mut state = world.resource_mut::<ShellState>();
        let state = &mut *state;
        model::advance_notice(&mut state.notice, &mut state.queued, delta, NOTICE_SECS);
        state.notice.as_ref().map(|(text, _)| text.clone())
    };
    // A refusal, or the reason a mode is unavailable for as long as it holds.
    let text = transient
        .or_else(|| model::mode_reason(mode, life).map(str::to_owned))
        .unwrap_or_default();
    set_text::<NoticeText>(world, &text);
    show::<ShellNotice>(world, !text.is_empty() && !gated);
}

/// Show the calendar under the date, right-aligned with it, while open.
fn refresh_calendar(world: &mut World) {
    let open = world.resource::<ShellState>().popover == Popover::Calendar
        && !world
            .get_resource::<crate::FrontendGate>()
            .is_some_and(|gate| gate.0);
    if open {
        let calendar = calendar(world);
        let (now, next) = model::calendar_text(&calendar, &season_facts(world));
        set_text::<CalendarNow>(world, &now);
        set_text::<CalendarNext>(world, &next);
        let ahead = model::outlook_lines(&outlook(world));
        set_text::<CalendarOutlook>(world, &ahead.join("\n"));
        show::<CalendarOutlook>(world, !ahead.is_empty());
        show::<CalendarOutlookRule>(world, !ahead.is_empty());
        let (now, mut later, mourning) = gatherings(world);
        if now.is_empty() && later.is_empty() && mourning.is_empty() {
            later.push(model::NO_GATHERINGS.to_owned());
        }
        set_text::<CalendarToday>(world, &now.join("\n"));
        set_text::<CalendarGatherings>(world, &later.join("\n"));
        set_text::<CalendarMourning>(world, &mourning.join("\n"));
        show::<CalendarToday>(world, !now.is_empty());
        show::<CalendarGatherings>(world, !later.is_empty());
        show::<CalendarMourning>(world, !mourning.is_empty());
        show::<CalendarGatheringsRule>(
            world,
            !(now.is_empty() && later.is_empty() && mourning.is_empty()),
        );
    }
    let top = content_top(world) - MARGIN + 4.0;
    let width = scale::layout(world).width;
    // Right edge of the date control, in logical pixels.
    let edge = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<CalendarButton>>()
        .iter(world)
        .next()
        .map(|(size, transform)| {
            (transform.translation.x + size.size().x * 0.5) * size.inverse_scale_factor()
        });
    let right = match edge {
        Some(edge) if edge > 0.0 => (width - edge).max(MARGIN),
        _ => MARGIN,
    };
    for mut node in world
        .query_filtered::<&mut Node, With<CalendarRoot>>()
        .iter_mut(world)
    {
        let display = if open { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if node.top != px(top) {
            node.top = px(top);
        }
        if node.right != px(right) {
            node.right = px(right);
        }
    }
}

#[cfg(test)]
mod tests;
