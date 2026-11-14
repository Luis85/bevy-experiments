//! Colony tools are a view of the live simulation. Every edit is a recorded command.
//!
//! In the player session the island bar owns the mode switcher and the clock,
//! and Shape mode has its own palette; this ledger is the Oversee workspace
//! (village, economy, residents). Developer launches keep the ledger's own
//! mode, clock and landscape rows.
mod economy;
mod goods;
mod model;
mod orders;
mod roster;
mod season;
#[cfg(test)]
mod tests;
mod widgets;

pub(crate) use economy::{StockTrends, Trend, scopes, stock_totals, trade_display};
use model::entries;
pub(crate) use model::{Entry, Extra, manage_entries};
/// The words of the world's marks for the player's orders (design-ui 2.7),
/// for `island_atmosphere` and `village_world` to place.
#[allow(unused_imports)]
pub(crate) use orders::{order_pill, order_tag};
pub(crate) use roster::capitalised;
use std::collections::BTreeMap;
use widgets::{button, reconcile, row, set_text};
pub(crate) use widgets::{need_status, reconcile_rows};

use crate::game_ui::icon::Icon;
use crate::game_ui::scale;
use crate::game_ui::tokens::{self as t, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as kit, Reach, Tone};
use crate::game_ui::words;
use crate::island_shell::model::{Column, ColumnEvent, ColumnState, LedgerPage};

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, ScrollArea};
use island_content::village::{self, colony};
use npc_framework::kernel::commands::{CommandLog, SkippedCommands};
use npc_framework::prelude::{
    SimClock, SimCommand, SimConfig, SimControl, SimId, SimIndex, submit,
};
use npc_framework::view_helpers::command_ui::DragBox;
use npc_framework::view_helpers::{Selected, ViewSystems, sim_to_world};

use crate::IslandMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Overview,
    Economy,
    Residents,
    Projects,
    Journal,
    Landscape,
}

impl Tab {
    fn of(page: LedgerPage) -> Self {
        match page {
            LedgerPage::Village => Self::Overview,
            LedgerPage::Economy => Self::Economy,
            LedgerPage::People => Self::Residents,
            LedgerPage::Projects => Self::Projects,
            LedgerPage::Journal => Self::Journal,
        }
    }
    /// The ledger page of a tab; the developer Landscape tab has none.
    fn page(self) -> Option<LedgerPage> {
        Some(match self {
            Self::Overview => LedgerPage::Village,
            Self::Economy => LedgerPage::Economy,
            Self::Residents => LedgerPage::People,
            Self::Projects => LedgerPage::Projects,
            Self::Journal => LedgerPage::Journal,
            Self::Landscape => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Toggle,
    Close,
    Mode(IslandMode),
    Tab(Tab),
    Pause,
    Speed(u32),
    Reserve(u32),
    Gatherings,
    Resident(SimId),
    Home(SimId, SimId),
    Profession(SimId, SimId),
    Settlement(SimId),
    Commission(SimId, SimId),
    CancelHome(SimId),
    Scope(economy::Scope),
    /// Order of the Residents roster.
    Sort(roster::SortKey),
    /// Economy page: the goods table, the professions or the priorities.
    EconomyPage(economy::EconomyPage),
    /// Jobs table: fewer or more of a trade wanted in a village, or the
    /// village decides again (`SetProfessionTarget`).
    Target {
        settlement: SimId,
        profession: String,
        step: economy::TargetStep,
    },
    /// Priorities: one topic of a village at one level
    /// (`SetVillagePriority`).
    Priority {
        settlement: SimId,
        topic: &'static str,
        profession: Option<String>,
        level: village::frontier::oversee::PriorityLevel,
    },
    /// Economy page: select a good for its detail card, or put it back.
    Good(String),
    /// A header of the goods table: sort by that column, again to reverse.
    SortGoods(u8),
    /// Projects page: unfold or fold a section.
    Fold(&'static str),
    /// The resident inspector: show one of its tabs.
    InspectorTab(crate::resident_inspector::Tab),
    /// Bring the camera to a construction site and open its card.
    Site(SimId),
    /// Ask before abandoning a site, then abandon it or keep it.
    CancelSite(SimId),
    ConfirmCancelSite(SimId),
    KeepSite,
    /// Page Down true or Page Up over the open page.
    Page(bool),
    /// Journal page: frame an alert, unfold a day's notes, choose a filter.
    AlertRow(u64),
    AlertFold(u64),
    AlertFilter(crate::island_alerts::model::Filter),
    /// The resident card's Why page: save or reload the behaviour file
    /// (the host does the file work).
    BehaviourFile(crate::BehaviourFile),
    /// Open Jobs, on this village when it names one, and flash this job's
    /// row.
    JobsOf(economy::Scope, Option<String>),
    /// Open People listing only these residents under this head; an empty
    /// list shows everyone again.
    PeopleFilter(String, Vec<SimId>),
    /// The narrow ledger's fold of the good card: open or close it.
    GoodMore,
    /// Take a refused order off "Your orders".
    DismissOrder((u64, u32)),
    /// Priorities: show or hide the jobs that may get their own level.
    JobLevels,
    /// The day's account: show or fold its lines after the first two.
    DayMore,
    /// The season-end card (2.6): open it on this season, close it, or
    /// close it onto the Journal.
    SeasonOpen(u32),
    SeasonClose,
    SeasonJournal,
}
#[derive(Resource)]
struct Ledger {
    /// People: the residents a link chose, and the page's head for them.
    people_filter: Option<(String, Vec<SimId>)>,
    /// The narrow ledger's good card: its fold is open.
    good_more: bool,
    /// Orders this ledger sent that wait or were refused (view state).
    sent: Vec<orders::SentOrder>,
    /// Priorities: the chips of jobs without their own level are open.
    job_levels: bool,
    /// The season whose end card the ledger shows (2.6).
    season_card: Option<u32>,
    /// The tick the season card opened: it closes by itself after a day of
    /// running time (closing phase, defect 3).
    season_opened: Option<u64>,
    /// The day's account is unfolded.
    day_more: bool,
    /// The good whose card was last brought into view.
    scrolled_good: Option<String>,
    /// The last finished season already offered; `None` before the first
    /// look, so a loaded island does not offer an old season.
    season_seen: Option<Option<u32>>,
    /// The foot line last seen and how long it has stood, in real seconds:
    /// it clears after six (2.7); "Your orders" is what stays.
    foot: (String, f32),
    /// A row to bring into view and flash: its key, the seconds it has
    /// flashed, and whether it was found (2.3, "Show baker in Jobs").
    flash: Option<(String, f32, bool)>,
    open: bool,
    tab: Tab,
    /// Settlement shown by the Goods table and the Residents roster; `None`
    /// is the whole island.
    scope: economy::Scope,
    /// The village Jobs and Priorities show, remembered for both this
    /// session; chosen again when it is unset or gone.
    village_scope: Option<SimId>,
    sort: roster::SortKey,
    /// Economy page: which table, and the selected good.
    economy: economy::EconomyPage,
    good: Option<String>,
    /// The goods table's sorted column and whether it runs upwards.
    goods_sort: Option<(u8, bool)>,
    /// Projects page: the sections the player unfolded.
    unfolded: std::collections::BTreeSet<&'static str>,
    /// Village tab: the site whose cancellation waits for a second click.
    confirm_site: Option<SimId>,
    message: String,
    requests: Vec<Action>,
    pending: Option<((u64, u32), String)>,
    last_mode: Option<IslandMode>,
    control_revision: u64,
    rows: BTreeMap<String, Entity>,
    /// Rows kept under the scrolling body: the chosen good's detail card.
    pinned: BTreeMap<String, Entity>,
    elapsed: f32,
    dirty: bool,
    /// The resident and the building card last seen selected: a new one
    /// replaces the ledger with its inspector.
    subject: (Option<SimId>, Option<SimId>),
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            people_filter: None,
            good_more: false,
            sent: Vec::new(),
            job_levels: false,
            season_card: None,
            season_opened: None,
            day_more: false,
            scrolled_good: None,
            season_seen: None,
            foot: (String::new(), 0.0),
            flash: None,
            // Closed by default: a management screen opens on request.
            open: false,
            tab: Tab::Overview,
            scope: None,
            village_scope: None,
            sort: roster::SortKey::default(),
            economy: economy::EconomyPage::Goods,
            good: None,
            goods_sort: None,
            unfolded: std::collections::BTreeSet::new(),
            confirm_site: None,
            message: String::new(),
            requests: Vec::new(),
            pending: None,
            last_mode: None,
            control_revision: 0,
            rows: BTreeMap::new(),
            pinned: BTreeMap::new(),
            elapsed: 1.0,
            dirty: true,
            subject: (None, None),
        }
    }
}
#[derive(Component)]
pub(crate) struct LedgerRoot;
#[derive(Component)]
pub(crate) struct LedgerLauncher;
/// The ledger's fixed foot, under its scrolling body.
#[derive(Component)]
struct LedgerFooter;
#[derive(Component)]
struct LedgerBody;
#[derive(Component)]
struct LedgerStatus;
#[derive(Component)]
struct LedgerMessage;
#[derive(Component, Clone)]
pub(crate) struct LedgerAction(pub(crate) Action);
/// A control inside the Oversee workspace's keyboard focus loop. The alert
/// feed's rows carry it too, so Tab reaches them from the ledger.
#[derive(Component)]
pub(crate) struct LedgerControl;

pub(crate) fn plugin(app: &mut App) {
    crate::game_ui::install(app);
    crate::panel_scroll::install(app);
    app.init_resource::<IslandMode>()
        .init_resource::<Ledger>()
        .init_resource::<economy::StockTrends>()
        .init_resource::<InputFocus>()
        .add_observer(activate)
        .add_observer(acquire_focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(input_active)))
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (keys, pointer)
                .chain()
                .in_set(ViewSystems::OverlayInput)
                .after(crate::island_shell::ShellInput)
                .run_if(not(crate::player_hud::active)),
        )
        .add_systems(
            Update,
            (economy::sample, process, refresh, sync)
                .chain()
                .in_set(ViewSystems::Present),
        );
}

/// The player session's bar owns modes and the clock; see the module docs.
fn shell(world: &World) -> bool {
    crate::island_shell::enabled(world)
}

/// The ledger for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Ledger>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let shell = crate::player_hud::enabled(world);
    // Developer launches open the ledger from a button; the player session
    // uses the ledger dock.
    let launcher = world
        .spawn((
            Name::new("Colony ledger launcher"),
            LedgerLauncher,
            crate::island_shell::focus::FocusRegion::Launcher,
            Hovered::default(),
            TabGroup::new(6),
            GlobalZIndex(t::layer::CHROME),
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                bottom: px(12),
                display: Display::None,
                ..default()
            },
        ))
        .id();
    kit::button(
        world,
        launcher,
        kit::ButtonSpec::new("Village and land", kit::Kind::Secondary, Surface::Chrome).key("F11"),
        (LedgerControl, LedgerAction(Action::Toggle)),
    );
    let root = world
        .spawn((
            Name::new("Island colony ledger"),
            LedgerRoot,
            crate::game_ui::sound::PanelSound::default(),
            crate::island_shell::focus::FocusRegion::Ledger,
            TabGroup::new(6),
            GlobalZIndex(t::layer::PANEL),
            kit::panel(Node {
                position_type: PositionType::Absolute,
                top: px(12),
                right: px(t::SCREEN_MARGIN),
                bottom: px(12),
                width: px(t::COLUMN_LEDGER),
                max_width: percent(94),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                display: Display::None,
                ..default()
            }),
        ))
        .id();
    let header = kit::panel_header(
        world,
        root,
        Icon::Ledger,
        if shell { "Ledger" } else { "Island Village" },
    );
    let close = kit::icon_button(
        world,
        header,
        "Close",
        Icon::Close,
        Surface::Paper,
        Reach::Tab(0),
        (LedgerControl, LedgerAction(Action::Close)),
    );
    world
        .entity_mut(close)
        .insert(Tooltip::new("Close", "Close the ledger.").key("Esc"));
    if !shell {
        let modes = row(world, root);
        for (label, mode) in [
            ("Oversee", IslandMode::Inspector),
            ("Live", IslandMode::Character),
            ("Shape", IslandMode::Editor),
        ] {
            button(world, modes, label, Action::Mode(mode));
        }
        let status = kit::paragraph(world, root, "", t::DENSE, Tone::text(Surface::Paper));
        world.entity_mut(status).insert(LedgerStatus);
        let clock = row(world, root);
        button(world, clock, "Pause / resume", Action::Pause);
        for speed in [1, 3, 10] {
            button(world, clock, &format!("{speed}x"), Action::Speed(speed));
        }
    }
    let tabs = kit::tabs(world, root);
    for page in LedgerPage::ALL {
        widgets::tab(
            world,
            tabs,
            page.label(),
            page.icon(),
            Action::Tab(Tab::of(page)),
        );
    }
    if !shell {
        widgets::tab(
            world,
            tabs,
            "Landscape",
            Icon::Shape,
            Action::Tab(Tab::Landscape),
        );
    }
    let body = world
        .spawn((
            ChildOf(root),
            LedgerBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    // A rail, the wheel and the Page keys; no paging buttons, no hint text.
    crate::panel_scroll::attach_rail(world, root, body, "Colony");
    // Under the scrolling rows, always in view: a card of facts about the
    // row chosen above it.
    world.spawn((
        ChildOf(root),
        LedgerFooter,
        Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let message = kit::paragraph(world, root, "", t::CAPTION, Tone::caption(Surface::Paper));
    world.entity_mut(message).insert(LedgerMessage);
    if !shell {
        // Developer launches have no bar, no docks and no tooltips to carry this.
        kit::paragraph(
            world,
            root,
            "F11: ledger · F2: home and farm\nWASD: pan or walk · arrows: pan\nWheel: zoom · Q/E: orbit",
            t::CAPTION,
            Tone::caption(Surface::Paper),
        );
    }
}
fn activate(
    event: On<Activate>,
    actions: Query<(&LedgerAction, Has<InteractionDisabled>)>,
    mut state: ResMut<Ledger>,
) {
    // The resident inspector's rows are ledger actions too, so this does not
    // ask whether the ledger itself is open.
    if let Ok((action, false)) = actions.get(event.entity) {
        state.requests.push(action.0.clone());
    }
}
fn acquire_focus(
    mut event: On<AcquireFocus>,
    controls: Query<
        (),
        (
            With<LedgerControl>,
            Without<crate::island_shell::focus::KeyReach>,
        ),
    >,
    mut focus: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        focus.set(event.focused_entity, FocusCause::Navigated);
    }
}
fn input_active(focus: Option<Res<InputFocus>>, controls: Query<(), With<LedgerControl>>) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| controls.contains(entity))
}
/// F11, Esc and the Page keys, and Tab in developer launches. In the player
/// session the bar's focus ring (`island_shell::focus`) takes Tab first and
/// consumes it. Esc closes the ledger and goes no further this press.
#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    mut state: ResMut<Ledger>,
    mut focus: ResMut<InputFocus>,
    controls: Query<(), With<LedgerControl>>,
    windows: Query<(), With<Window>>,
    roots: Query<Entity, With<LedgerRoot>>,
    nav: TabNavigation,
    experience: Option<Res<crate::PlayerExperience>>,
    mut commands: Commands,
) {
    let Some(mut keys) = keys else { return };
    // In the player session F1 to F5 and Esc open and close the ledger.
    let player = experience.is_some_and(|experience| experience.enabled);
    if focus
        .get()
        .is_some_and(|entity| !controls.contains(entity) && !windows.contains(entity))
    {
        return;
    }
    if !player && keys.just_pressed(KeyCode::F11) {
        state.requests.push(Action::Toggle);
    }
    if !state.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.requests.push(Action::Close);
        keys.reset(KeyCode::Escape);
    }
    for (key, direction) in [(KeyCode::PageUp, -1.0), (KeyCode::PageDown, 1.0)] {
        if keys.just_pressed(key) {
            state.requests.push(Action::Page(direction > 0.0));
            keys.reset(key);
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let dir = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        let next = if focus.get().is_some_and(|entity| controls.contains(entity)) {
            nav.navigate(&focus, dir)
        } else if let Ok(root) = roots.single() {
            nav.initialize(root, dir)
        } else {
            return;
        };
        if let Ok(next) = next {
            focus.set(next, FocusCause::Navigated);
            commands.trigger(bevy::ui_widgets::ScrollIntoView { entity: next });
        }
    }
}

/// Shield the world from clicks on any panel, and let a world click release
/// keyboard focus. Shape-mode painting follows in `shape_tools`.
pub(crate) fn pointer(world: &mut World) {
    let ui_hovered = world
        .query_filtered::<&Hovered, With<Node>>()
        .iter(world)
        .any(Hovered::get);
    if ui_hovered {
        if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
            mouse.reset_all();
        }
        if let Some(mut scroll) = world.get_resource_mut::<AccumulatedMouseScroll>() {
            scroll.delta = Vec2::ZERO;
        }
        if let Some(mut drag) = world.get_resource_mut::<DragBox>() {
            drag.start = None;
        }
        return;
    }
    let world_click = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|mouse| {
            mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)
        });
    if world_click
        && world
            .resource::<InputFocus>()
            .get()
            .is_some_and(|e| world.get::<LedgerControl>(e).is_some())
    {
        world.resource_mut::<InputFocus>().clear();
    }
}

/// Whether the ledger shows a page of one village: Economy's Jobs or
/// Priorities.
fn village_page(state: &Ledger) -> bool {
    state.tab == Tab::Economy && state.economy != economy::EconomyPage::Goods
}

/// On Jobs or Priorities, keep the remembered village, or choose one when
/// there is none or it is gone: the village under the camera's centre, else
/// the one with most residents.
fn choose_village(world: &mut World) {
    let (wanted, remembered) = {
        let state = world.resource::<Ledger>();
        (village_page(state), state.village_scope)
    };
    if !wanted
        || remembered.is_some_and(|id| {
            village::frontier::settlements::all(world)
                .iter()
                .any(|(candidate, _)| *candidate == id)
        })
    {
        return;
    }
    let camera = world
        .query::<&crate::RtsCamera>()
        .iter(world)
        .next()
        .map(|camera| camera.focus);
    let chosen = economy::pick_village(world, camera);
    let mut state = world.resource_mut::<Ledger>();
    if state.village_scope != chosen {
        state.village_scope = chosen;
        state.dirty = true;
    }
}

fn queue(world: &mut World, command: SimCommand, success: String) {
    let given = words::order_given(words::paused(world)).to_owned();
    queue_saying(world, command, given, success);
}

/// [`queue`] with its own line while the order waits: `given` stands at
/// the ledger's foot until the village answers with `success` or a refusal.
fn queue_saying(
    world: &mut World,
    command: SimCommand,
    given: String,
    success: String,
) -> Option<(u64, u32)> {
    let result = submit(world, None, command);
    let mut state = world.resource_mut::<Ledger>();
    state.dirty = true;
    match result {
        Ok(key) => {
            state.pending = Some((key, success));
            state.message = given;
            Some(key)
        }
        Err(error) => {
            state.message = words::village_refused(&error.to_string());
            None
        }
    }
}

/// Bring the asked-for row into view and flash it for one pulse: a honey
/// outline, taken away again after `t::motion::PULSE`.
fn flash_row(world: &mut World) {
    let Some((key, secs, found)) = world.resource::<Ledger>().flash.clone() else {
        return;
    };
    let Some(row) = world.resource::<Ledger>().rows.get(&key).copied() else {
        // The page is built on the next refresh; give up after a while.
        if secs > 1.0 {
            world.resource_mut::<Ledger>().flash = None;
        } else {
            let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
            world.resource_mut::<Ledger>().flash = Some((key, secs + delta, false));
        }
        return;
    };
    if !found {
        world.trigger(bevy::ui_widgets::ScrollIntoView { entity: row });
        world
            .entity_mut(row)
            .insert(Outline::new(px(2), px(0), t::HONEY_500));
        world.resource_mut::<Ledger>().flash = Some((key, 0.0, true));
        return;
    }
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let secs = secs + delta;
    if secs >= t::motion::PULSE {
        if let Ok(mut entity) = world.get_entity_mut(row) {
            entity.remove::<Outline>();
        }
        world.resource_mut::<Ledger>().flash = None;
    } else {
        world.resource_mut::<Ledger>().flash = Some((key, secs, true));
    }
}

/// How long the ledger's foot line stands before it clears.
const FOOT_SECS: f32 = 6.0;

/// Clear the foot line once it has stood for [`FOOT_SECS`].
fn age_foot(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let mut state = world.resource_mut::<Ledger>();
    if state.foot.0 != state.message {
        state.foot = (state.message.clone(), 0.0);
        return;
    }
    if state.message.is_empty() {
        return;
    }
    state.foot.1 += delta;
    if state.foot.1 >= FOOT_SECS {
        state.message.clear();
        state.foot = (String::new(), 0.0);
        state.dirty = true;
    }
}

/// Open the card of the season that ended last, if one has.
pub(crate) fn open_last_season(world: &mut World) {
    if let Some(season) = last_season(world)
        && world.contains_resource::<Ledger>()
    {
        world
            .resource_mut::<Ledger>()
            .requests
            .push(Action::SeasonOpen(season));
    }
}

/// The last season that has ended, while the island keeps it.
pub(crate) fn last_season(world: &World) -> Option<u32> {
    let held = village::view::season_summaries(world);
    // The newest one held is the season still running.
    held.len().checked_sub(2).map(|index| held[index])
}

/// A season has just ended: its card opens by itself only into an empty
/// right column in Oversee with no tool in hand; otherwise a notice says
/// where it is. Never pauses time (2.6).
fn offer_season(world: &mut World) {
    let ended = last_season(world);
    let seen = world.resource::<Ledger>().season_seen;
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let day = world
        .get_resource::<SimConfig>()
        .map_or(1, |config| config.day_length.max(1));
    let close_card = {
        let mut state = world.resource_mut::<Ledger>();
        state.season_seen = Some(ended);
        if !state.open {
            state.season_card = None;
        }
        if state.season_card.is_none() {
            state.season_opened = None;
        }
        season_card_expired(state.season_card.is_some(), state.season_opened, now, day)
    };
    // A card nobody closed goes by itself after a day of running time: it
    // never keeps a third of the screen for the rest of the season.
    if close_card {
        world.resource_mut::<Ledger>().season_card = None;
        close(world);
    }
    // The first look at this island offers nothing; then only a season
    // that ended since the last look.
    let (Some(seen), Some(season)) = (seen, ended) else {
        return;
    };
    if seen == Some(season) || !shell(world) {
        return;
    }
    let empty = column(world) == Column::Empty
        && *world.resource::<IslandMode>() == IslandMode::Inspector
        && !crate::island_build::palette_open(world);
    if empty {
        world
            .resource_mut::<Ledger>()
            .requests
            .push(Action::SeasonOpen(season));
    } else if let Some(name) =
        village::view::season_summary(world, season).map(|summary| summary.name)
    {
        crate::island_shell::season_toast(world, format!("{name} has ended."));
    }
}

/// Whether an open season card has stood a day of running time.
pub(crate) fn season_card_expired(open: bool, opened: Option<u64>, now: u64, day: u64) -> bool {
    open && opened.is_some_and(|opened| now >= opened.saturating_add(day))
}

/// Orders this ledger sent that time has not applied yet: the Paused pill
/// counts them (with Build's and Shape's own).
pub(crate) fn orders_waiting(world: &World) -> usize {
    world.get_resource::<Ledger>().map_or(0, |state| {
        state
            .sent
            .iter()
            .filter(|order| order.refused.is_none())
            .count()
    })
}

/// A good chosen in the table: its card, under its row, is scrolled into
/// view once (closing phase, defect 11).
fn bring_card_into_view(world: &mut World) {
    let (good, scrolled) = {
        let state = world.resource::<Ledger>();
        (state.good.clone(), state.scrolled_good.clone())
    };
    if good == scrolled {
        return;
    }
    let card = world.resource::<Ledger>().rows.get("stock_detail").copied();
    if let (Some(_), Some(card)) = (&good, card) {
        world.trigger(bevy::ui_widgets::ScrollIntoView { entity: card });
    }
    world.resource_mut::<Ledger>().scrolled_good = good;
}

/// [`queue`] for an order of "Your orders": it is remembered until the
/// village applies it, or refuses it and the player dismisses that.
fn queue_order(
    world: &mut World,
    command: SimCommand,
    success: String,
    village: SimId,
    what: orders::Sent,
) {
    let given = words::order_given(words::paused(world)).to_owned();
    if let Some(key) = queue_saying(world, command, given, success) {
        world.resource_mut::<Ledger>().sent.push(orders::SentOrder {
            command: key,
            village,
            what,
            refused: None,
        });
    }
}

/// Orders this ledger sent: drop the applied ones, keep a refusal with the
/// village's reason until the player dismisses it.
fn settle_orders(world: &mut World) {
    if world
        .resource::<Ledger>()
        .sent
        .iter()
        .all(|order| order.refused.is_some())
    {
        return;
    }
    let skipped = world.get_resource::<SkippedCommands>();
    let applied = world.get_resource::<CommandLog>();
    let mut sent = world.resource::<Ledger>().sent.clone();
    let before = sent.clone();
    sent.retain_mut(|order| {
        if order.refused.is_some() {
            return true;
        }
        if let Some(error) = skipped.and_then(|log| {
            log.entries
                .iter()
                .rev()
                .find(|entry| (entry.tick, entry.seq) == order.command)
                .map(|entry| entry.error.to_string())
        }) {
            order.refused = Some(error);
            return true;
        }
        !applied.is_some_and(|log| {
            log.entries
                .iter()
                .rev()
                .any(|(tick, seq, _)| (*tick, *seq) == order.command)
        })
    });
    if sent != before {
        let mut state = world.resource_mut::<Ledger>();
        state.sent = sent;
        state.dirty = true;
    }
}
fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Ledger>().requests);
    for request in requests {
        world.resource_mut::<Ledger>().dirty = true;
        match request {
            Action::Toggle => {
                let open = world.resource::<Ledger>().open;
                // Without the palette the ledger is the editor; closing ends it.
                if open && !shell(world) && *world.resource::<IslandMode>() == IslandMode::Editor {
                    switch_mode(world, IslandMode::Inspector);
                }
                column_event(world, ColumnEvent::Last);
            }
            Action::Close => {
                if !shell(world) && *world.resource::<IslandMode>() == IslandMode::Editor {
                    switch_mode(world, IslandMode::Inspector);
                }
                column_event(world, ColumnEvent::Close);
                world.resource_mut::<InputFocus>().clear();
            }
            Action::Page(down) => {
                for (node, mut scroll) in world
                    .query_filtered::<(&ComputedNode, &mut ScrollPosition), With<LedgerBody>>()
                    .iter_mut(world)
                {
                    let visible = node.size().y * node.inverse_scale_factor();
                    let most =
                        (node.content_size().y * node.inverse_scale_factor() - visible).max(0.0);
                    let step = visible * 0.85 * if down { 1.0 } else { -1.0 };
                    scroll.y = (scroll.y + step).clamp(0.0, most);
                }
            }
            Action::AlertRow(serial) => crate::island_alerts::request_row(world, serial, false),
            Action::AlertFold(day) => crate::island_alerts::request_row(world, day, true),
            Action::AlertFilter(filter) => crate::island_alerts::request_filter(world, filter),
            Action::BehaviourFile(what) => crate::behaviour_file::request(world, what),
            Action::Mode(mode) => switch_mode(world, mode),
            Action::Tab(tab) => {
                let mut state = world.resource_mut::<Ledger>();
                state.tab = tab;
                state.open = true;
                state.confirm_site = None;
                reset_scroll(world);
            }
            Action::Site(id) => {
                // A building replaces whoever was inspected: one right column.
                if shell(world) {
                    crate::resident_inspector::close(world);
                }
                crate::island_build::focus_place(world, id);
                if shell(world) {
                    // The card takes the column, also when it was the card
                    // the page had been opened over.
                    column_event(world, ColumnEvent::Selected);
                }
            }
            Action::CancelSite(id) => world.resource_mut::<Ledger>().confirm_site = Some(id),
            Action::KeepSite => world.resource_mut::<Ledger>().confirm_site = None,
            Action::ConfirmCancelSite(site) => {
                world.resource_mut::<Ledger>().confirm_site = None;
                queue(
                    world,
                    SimCommand::content(&village::civic::CancelBuilding { site }),
                    "Site abandoned. Delivered materials went to the nearest store, or lie at the entrance."
                        .into(),
                );
            }
            Action::Scope(scope) => {
                let mut state = world.resource_mut::<Ledger>();
                if village_page(&state) {
                    // Jobs and Priorities never show the whole island.
                    if scope.is_some() {
                        state.village_scope = scope;
                    }
                } else {
                    state.scope = scope;
                }
            }
            Action::Sort(sort) => {
                world.resource_mut::<Ledger>().sort = sort;
                reset_scroll(world);
            }
            Action::EconomyPage(page) => {
                world.resource_mut::<Ledger>().economy = page;
                choose_village(world);
                reset_scroll(world);
            }
            Action::Target {
                settlement,
                profession,
                step,
            } => match economy::target_command(world, settlement, &profession, step) {
                Ok(command) => {
                    let success = economy::target_success(world, &command);
                    let what = orders::Sent::Job {
                        key: profession,
                        display: economy::trade_display(world, command.profession),
                        wanted: command.wanted,
                    };
                    queue_order(
                        world,
                        SimCommand::content(&command),
                        success,
                        settlement,
                        what,
                    );
                }
                Err(reason) => {
                    let mut state = world.resource_mut::<Ledger>();
                    state.message = reason;
                    state.dirty = true;
                }
            },
            Action::Priority {
                settlement,
                topic,
                profession,
                level,
            } => {
                match economy::priority_command(world, settlement, topic, profession.clone(), level)
                {
                    Ok(command) => {
                        let success = economy::priority_success(world, &command);
                        let display = match &profession {
                            Some(_) => format!(
                                "Workplaces: {}",
                                words::noun(
                                    &economy::trade_display(
                                        world,
                                        command.profession.expect("a job's own level")
                                    ),
                                    2
                                )
                            ),
                            None => command.topic.display().to_owned(),
                        };
                        let what = orders::Sent::Priority {
                            topic,
                            profession,
                            display,
                            level,
                        };
                        queue_order(
                            world,
                            SimCommand::content(&command),
                            success,
                            settlement,
                            what,
                        );
                    }
                    Err(reason) => {
                        let mut state = world.resource_mut::<Ledger>();
                        state.message = reason;
                        state.dirty = true;
                    }
                }
            }
            Action::SortGoods(column) => {
                let mut state = world.resource_mut::<Ledger>();
                state.goods_sort = economy::next_sort(state.goods_sort, column);
            }
            Action::Good(kind) => {
                let mut state = world.resource_mut::<Ledger>();
                state.good = (state.good.as_deref() != Some(kind.as_str())).then_some(kind);
            }
            Action::SeasonOpen(season) => {
                open_page(world, LedgerPage::Journal);
                let mut state = world.resource_mut::<Ledger>();
                state.tab = Tab::Journal;
                state.open = true;
                state.season_card = Some(season);
                reset_scroll(world);
                let now = world
                    .get_resource::<SimClock>()
                    .map_or(0, |clock| clock.tick);
                world.resource_mut::<Ledger>().season_opened = Some(now);
            }
            Action::SeasonClose => {
                world.resource_mut::<Ledger>().season_card = None;
                close(world);
            }
            Action::SeasonJournal => {
                world.resource_mut::<Ledger>().season_card = None;
                reset_scroll(world);
            }
            Action::GoodMore => {
                let mut state = world.resource_mut::<Ledger>();
                state.good_more = !state.good_more;
            }
            Action::JobLevels => {
                let mut state = world.resource_mut::<Ledger>();
                state.job_levels = !state.job_levels;
            }
            Action::DayMore => {
                let mut state = world.resource_mut::<Ledger>();
                state.day_more = !state.day_more;
            }
            Action::DismissOrder(key) => world
                .resource_mut::<Ledger>()
                .sent
                .retain(|order| order.command != key),
            Action::JobsOf(village, job) => {
                let mut state = world.resource_mut::<Ledger>();
                state.tab = Tab::Economy;
                state.economy = economy::EconomyPage::Jobs;
                if village.is_some() {
                    state.village_scope = village;
                }
                choose_village(world);
                reset_scroll(world);
                let village = world.resource::<Ledger>().village_scope;
                world.resource_mut::<Ledger>().flash = village
                    .zip(job)
                    .map(|(village, job)| (format!("target_{}_{job}", village.0), 0.0, false));
            }
            Action::PeopleFilter(head, residents) => {
                let mut state = world.resource_mut::<Ledger>();
                if residents.is_empty() {
                    state.people_filter = None;
                } else {
                    state.people_filter = Some((head, residents));
                    state.tab = Tab::Residents;
                    state.open = true;
                    state.confirm_site = None;
                }
                reset_scroll(world);
            }
            Action::Fold(section) => {
                let mut state = world.resource_mut::<Ledger>();
                if !state.unfolded.remove(section) {
                    state.unfolded.insert(section);
                }
            }
            Action::InspectorTab(tab) => crate::resident_inspector::show_tab(world, tab),
            Action::Pause => {
                let mut control = world.resource_mut::<SimControl>();
                if control.is_stopped() {
                    control.paused = false;
                    control.multiplier = control.multiplier.max(1);
                } else {
                    control.paused = true;
                }
            }
            Action::Speed(speed) => {
                world.resource_mut::<SimControl>().multiplier = speed;
            }
            Action::Reserve(value) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "reserve_food".into(),
                    resident: None,
                    house: None,
                    value,
                }),
                format!("Household food reserve set to {value} days."),
            ),
            Action::Gatherings => {
                let value = u32::from(!world.resource::<colony::ColonyState>().gatherings);
                queue(
                    world,
                    SimCommand::content(&colony::ColonyAct {
                        verb: "gatherings".into(),
                        resident: None,
                        house: None,
                        value,
                    }),
                    "Gathering policy updated.".into(),
                );
            }
            // Opens the resident inspector and brings the camera to them.
            Action::Resident(id) => {
                crate::resident_inspector::inspect(world, id);
                crate::resident_inspector::from_roster(world, id);
                if shell(world) {
                    // Their row chosen again, with the People page over their
                    // inspector, is no new selection: hand the column over.
                    column_event(world, ColumnEvent::Selected);
                }
            }
            Action::Profession(resident, workplace) => {
                let name = world
                    .resource::<SimIndex>()
                    .get(resident)
                    .and_then(|entity| world.get::<village::Villager>(entity))
                    .map_or_else(|| "They".to_owned(), |person| person.name.clone());
                let success = format!("{name} will change job when their work is done.");
                let given = if words::paused(world) {
                    format!("{success} Queued: it starts when time runs (Space).")
                } else {
                    success.clone()
                };
                queue_saying(
                    world,
                    SimCommand::content(&village::frontier::labor::AssignProfession {
                        resident,
                        workplace,
                    }),
                    given,
                    success,
                );
            }
            Action::Settlement(id) => {
                if let Some(position) = world
                    .resource::<SimIndex>()
                    .get(id)
                    .and_then(|entity| world.get::<npc_framework::prelude::SimPos>(entity))
                    .copied()
                {
                    crate::island_camera::focus(world, sim_to_world(position), 65.0);
                }
            }
            Action::Home(resident, house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "assign_home".into(),
                    resident: Some(resident),
                    house: Some(house),
                    value: 0,
                }),
                "Home assigned. This resident will rest at the new household.".into(),
            ),
            Action::Commission(resident, house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "commission_home".into(),
                    resident: Some(resident),
                    house: Some(house),
                    value: 0,
                }),
                "Home commissioned. The builder will procure materials and work on site.".into(),
            ),
            Action::CancelHome(house) => queue(
                world,
                SimCommand::content(&colony::ColonyAct {
                    verb: "cancel_home".into(),
                    resident: None,
                    house: Some(house),
                    value: 0,
                }),
                "Housing commission cancelled; unused materials remain with the builder.".into(),
            ),
        }
    }
    if let Some((key, success)) = world.resource::<Ledger>().pending.clone() {
        let failed = world.get_resource::<SkippedCommands>().and_then(|log| {
            log.entries
                .iter()
                .rev()
                .find(|entry| (entry.tick, entry.seq) == key)
                .map(|entry| entry.error.to_string())
        });
        let applied = world.get_resource::<CommandLog>().is_some_and(|log| {
            log.entries
                .iter()
                .rev()
                .any(|(tick, seq, _)| (*tick, *seq) == key)
        });
        if failed.is_some() || applied {
            let mut state = world.resource_mut::<Ledger>();
            state.message = failed.map_or(success, |error| words::village_refused(&error));
            state.pending = None;
            state.dirty = true;
        }
    }
    settle_orders(world);
    age_foot(world);
    flash_row(world);
    offer_season(world);
    sync_mode(world);
    follow_selection(world);
    // Any way onto Jobs or Priorities (the switch, a tab, the shell's
    // column) lands on a village.
    choose_village(world);
    if let Some((revision, message)) = crate::island_control::status(world)
        && revision != world.resource::<Ledger>().control_revision
    {
        let (revision, message) = (revision, message.to_owned());
        // The player session has the bar and its tooltips for this; the
        // ledger's foot carries only the result of a command given here.
        let shell = shell(world);
        let mut state = world.resource_mut::<Ledger>();
        state.control_revision = revision;
        if !shell {
            state.message = message;
        }
        state.dirty = true;
    }
}
/// The ledger's part of the right column, as the pure column model sees it.
fn column_state(world: &World) -> ColumnState {
    let ledger = world.resource::<Ledger>();
    let page = ledger.tab.page();
    ColumnState {
        ledger: page.filter(|_| ledger.open),
        last: page.unwrap_or_default(),
        selected: ledger.subject != (None, None),
    }
}

/// Apply one event to the right column: the ledger opens, switches page or
/// closes, and closing it also clears the selection it was hiding.
fn column_event(world: &mut World, event: ColumnEvent) {
    let shell = shell(world);
    let before = column_state(world);
    let developer_tab = world.resource::<Ledger>().tab.page().is_none();
    let after = before.step(event);
    {
        let mut state = world.resource_mut::<Ledger>();
        match after.ledger {
            Some(page) if !developer_tab || event != ColumnEvent::Last => {
                if state.tab != Tab::of(page) {
                    state.tab = Tab::of(page);
                    state.confirm_site = None;
                }
                state.open = true;
            }
            // F11 on the developer Landscape tab toggles it as it is.
            Some(_) => state.open = !state.open,
            None => state.open = false,
        }
        state.dirty = true;
    }
    if !shell {
        return;
    }
    if after.ledger.is_some() && before.ledger != after.ledger {
        // A page replaces a building card and the build tray, which would
        // cover its lower part; a resident stays selected underneath.
        crate::island_build::close_column(world);
        reset_scroll(world);
    }
    if matches!(
        event,
        ColumnEvent::Close | ColumnEvent::Page(_) | ColumnEvent::Last
    ) && before.ledger.is_some()
        && after.ledger.is_none()
    {
        // Closing the column empties it: the selection goes with it.
        crate::resident_inspector::close(world);
        crate::island_build::close_card(world);
        world.resource_mut::<Ledger>().subject = (None, None);
    }
}

/// A newly selected resident or building replaces the ledger with its
/// inspector: the right column never holds two things.
fn follow_selection(world: &mut World) {
    if !shell(world) {
        return;
    }
    let resident = world
        .get_resource::<Selected>()
        .and_then(|selected| selected.0);
    let card = crate::island_build::card_raw(world);
    let before = world.resource::<Ledger>().subject;
    if before == (resident, card) {
        return;
    }
    let fresh =
        (resident.is_some() && resident != before.0) || (card.is_some() && card != before.1);
    world.resource_mut::<Ledger>().subject = (resident, card);
    if fresh {
        column_event(world, ColumnEvent::Selected);
    } else if (resident, card) == (None, None) {
        column_event(world, ColumnEvent::Deselected);
    }
}

/// Reopen the ledger on its last page, or close it: F11 and the one Ledger
/// tile of the Compact layout.
pub(crate) fn toggle_last(world: &mut World) {
    if world.contains_resource::<Ledger>() {
        column_event(world, ColumnEvent::Last);
    }
}

/// Open a ledger page; the same page again closes it. The dock tiles and
/// F1 to F5 use this.
pub(crate) fn toggle_page(world: &mut World, page: LedgerPage) {
    if world.contains_resource::<Ledger>() {
        column_event(world, ColumnEvent::Page(page));
    }
}

/// Close the ledger and empty the right column: the alert stack asks for
/// its rows back.
pub(crate) fn close(world: &mut World) {
    if open(world) {
        column_event(world, ColumnEvent::Close);
    }
}

/// Open a ledger page and leave it open: a top-bar total asks for its page.
pub(crate) fn open_page(world: &mut World, page: LedgerPage) {
    if world.contains_resource::<Ledger>() && self::page(world) != Some(page) {
        column_event(world, ColumnEvent::Page(page));
    }
}

/// Open People listing only `residents`, under `head`: a folded alert row
/// ("Show them", 2.4).
pub(crate) fn show_people(world: &mut World, head: String, residents: Vec<SimId>) {
    if !world.contains_resource::<Ledger>() {
        return;
    }
    open_page(world, LedgerPage::People);
    world
        .resource_mut::<Ledger>()
        .requests
        .push(Action::PeopleFilter(head, residents));
}

/// Which part of the Economy page another surface asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EconomyView {
    /// Goods, with this good's card open when it names one.
    Goods(Option<String>),
    Jobs,
    Priorities,
}

/// Open Economy on one village (the outlook strip's lines, 2.5): Goods
/// scoped to it, or its Jobs or Priorities.
pub(crate) fn show_economy(world: &mut World, village: SimId, view: EconomyView) {
    if !world.contains_resource::<Ledger>() {
        return;
    }
    open_page(world, LedgerPage::Economy);
    {
        let mut state = world.resource_mut::<Ledger>();
        state.tab = Tab::Economy;
        state.open = true;
        match view {
            EconomyView::Goods(good) => {
                state.economy = economy::EconomyPage::Goods;
                state.scope = Some(village);
                if good.is_some() {
                    state.good = good;
                }
            }
            EconomyView::Jobs => {
                state.economy = economy::EconomyPage::Jobs;
                state.village_scope = Some(village);
            }
            EconomyView::Priorities => {
                state.economy = economy::EconomyPage::Priorities;
                state.village_scope = Some(village);
            }
        }
        state.dirty = true;
    }
    reset_scroll(world);
}

/// The page the ledger shows, while it is open.
pub(crate) fn page(world: &World) -> Option<LedgerPage> {
    let ledger = world.get_resource::<Ledger>()?;
    ledger.tab.page().filter(|_| ledger.open)
}

/// What the right column holds: a ledger page, the inspector of the
/// selection, or nothing.
pub(crate) fn column(world: &World) -> Column {
    if world.contains_resource::<Ledger>() {
        column_state(world).column()
    } else {
        Column::Empty
    }
}

/// Whether the ledger holds the right column.
pub(crate) fn open(world: &World) -> bool {
    matches!(column(world), Column::Ledger(_))
}

fn reset_scroll(world: &mut World) {
    for mut scroll in world
        .query_filtered::<&mut ScrollPosition, With<LedgerBody>>()
        .iter_mut(world)
    {
        scroll.0 = Vec2::ZERO;
    }
}
fn switch_mode(world: &mut World, mode: IslandMode) {
    if let Err(error) = crate::island_control::request_mode(world, mode) {
        world.resource_mut::<Ledger>().message = error;
    }
    sync_mode(world);
}

/// Presentation follows the desired mode; controller receipts live in island_control.
fn sync_mode(world: &mut World) {
    let mode = *world.resource::<IslandMode>();
    if world.resource::<Ledger>().last_mode == Some(mode) {
        return;
    }
    let shell = shell(world);
    if mode == IslandMode::Character {
        crate::island_camera::character(world);
        world.resource_mut::<Ledger>().open = false;
    } else {
        let previous = world.resource::<Ledger>().last_mode;
        // Oversee and Shape share the overview camera; keep its framing
        // when the player only swaps tools.
        if previous.is_none() || previous == Some(IslandMode::Character) || !shell {
            crate::island_camera::overview(world, previous.is_none());
        }
        let mut state = world.resource_mut::<Ledger>();
        if shell {
            // Shape has its own palette; the ledger is an Oversee reference
            // book that opens on request and closes with the mode.
            if mode != IslandMode::Inspector {
                state.open = false;
            }
            if state.tab == Tab::Landscape {
                state.tab = Tab::Overview;
            }
        } else {
            state.open = true;
            state.tab = if mode == IslandMode::Editor {
                Tab::Landscape
            } else {
                Tab::Overview
            };
        }
    }
    if let Some(mut follow) = world.get_resource_mut::<crate::village_world::FollowHero>()
        && follow.enabled != (mode == IslandMode::Character)
    {
        follow.toggle();
    }
    world.resource_mut::<InputFocus>().clear();
    let mut state = world.resource_mut::<Ledger>();
    state.last_mode = Some(mode);
    state.dirty = true;
}

pub(crate) fn refresh(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let mut state = world.resource_mut::<Ledger>();
        state.elapsed += delta;
        if !state.dirty && state.elapsed < 0.25 {
            return;
        }
        state.dirty = false;
        state.elapsed = 0.0;
    }
    let open = world.resource::<Ledger>().open;
    let shell = shell(world);
    let top = crate::island_shell::content_top(world);
    let bottom = crate::island_shell::content_bottom(world);
    let class = scale::layout(world).class;
    let width =
        crate::island_shell::model::column_width(Column::Ledger(LedgerPage::default()), class);
    let overseeing = !shell || crate::island_shell::overseeing(world);
    let pictures = if crate::island_shell::model::ledger_tab_icons(class) {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in world
        .query_filtered::<&mut Node, With<widgets::TabIcon>>()
        .iter_mut(world)
    {
        if node.display != pictures {
            node.display = pictures;
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<LedgerRoot>>()
        .iter_mut(world)
    {
        // Each field written only when it differs, so layout stays idle.
        let display = if open && overseeing {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if node.width != px(width) {
            node.width = px(width);
        }
        // The right column: under the bar, above the docks.
        if shell && (node.top != px(top) || node.bottom != px(bottom)) {
            node.top = px(top);
            node.bottom = px(bottom);
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<LedgerLauncher>>()
        .iter_mut(world)
    {
        // The player session opens the ledger from its dock.
        let display = if open || shell {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    let clock = world.resource::<SimClock>();
    let config = world.resource::<SimConfig>();
    let (day, season, _) = village::calendar_at(clock.tick, config, true);
    let control = world.resource::<SimControl>();
    let run_status = if control.is_stopped() {
        "World paused".into()
    } else {
        format!("Village running · {}x speed", control.multiplier)
    };
    let status = format!(
        "Day {} · {:?} · {}\n{}",
        day + 1,
        season,
        if config.is_night(clock.tick) {
            "Night"
        } else {
            "Daylight"
        },
        run_status
    );
    set_text::<LedgerStatus>(world, &status);
    let message = world.resource::<Ledger>().message.clone();
    set_text::<LedgerMessage>(world, &message);
    for mut node in world
        .query_filtered::<&mut Node, With<LedgerMessage>>()
        .iter_mut(world)
    {
        let display = if message.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    if open {
        // The market figures: read again only when the day or the market
        // moved, or another good or scope is chosen.
        let (tab, scope, good, village) = {
            let state = world.resource::<Ledger>();
            (
                state.tab,
                state.scope,
                state.good.clone(),
                village_page(state).then_some(state.village_scope).flatten(),
            )
        };
        if tab == Tab::Economy {
            goods::refresh_cache(world, scope, good.as_deref(), village);
        }
        let rows = entries(world);
        reconcile(world, rows);
        bring_card_into_view(world);
    }
    if !open
        && world
            .resource::<InputFocus>()
            .get()
            .is_some_and(|entity| world.get::<LedgerAction>(entity).is_some())
    {
        world.resource_mut::<InputFocus>().clear();
    }
}
/// Light the tab, filter, sort and row that are selected. The kit paints.
fn sync(world: &mut World) {
    let (tab, scope, sort, open, page, good) = {
        let state = world.resource::<Ledger>();
        (
            state.tab,
            if village_page(state) {
                state.village_scope
            } else {
                state.scope
            },
            state.sort,
            state.open,
            state.economy,
            state.good.clone(),
        )
    };
    if !open {
        return;
    }
    let mode = *world.resource::<IslandMode>();
    let reserve = world
        .get_resource::<colony::ColonyState>()
        .map(|colony| colony.food_reserve_days);
    let inspected = world
        .get_resource::<Selected>()
        .and_then(|selected| selected.0);
    let card = crate::island_build::card_raw(world);
    let filter = crate::island_alerts::filter(world);
    let controls: Vec<(Entity, Action)> = world
        .query_filtered::<(Entity, &LedgerAction), With<LedgerControl>>()
        .iter(world)
        .map(|(entity, action)| (entity, action.0.clone()))
        .collect();
    // Each village's profession table, read once for all its buttons.
    let mut tables: BTreeMap<
        npc_framework::prelude::SimId,
        Vec<village::frontier::oversee::ProfessionRowView>,
    > = BTreeMap::new();
    for (entity, action) in controls {
        let selected = match action {
            Action::Mode(value) => value == mode,
            Action::Tab(value) => value == tab,
            Action::Scope(value) => value == scope,
            Action::Sort(value) => value == sort,
            Action::EconomyPage(value) => value == page,
            Action::Good(ref value) => good.as_deref() == Some(value.as_str()),
            Action::Resident(value) => inspected == Some(value),
            Action::Reserve(value) => reserve == Some(value),
            Action::Site(value) => card == Some(value),
            Action::AlertFilter(value) => filter == Some(value),
            Action::Priority {
                settlement,
                topic,
                ref profession,
                level,
            } => economy::priority_level(world, settlement, topic, profession.as_deref()) == level,
            _ => false,
        };
        kit::set_state(world, entity, selected, false);
        // A step the simulation would refuse is dimmed; its tooltip says why.
        if let Action::Target {
            settlement,
            ref profession,
            step,
        } = action
        {
            let rows = tables
                .entry(settlement)
                .or_insert_with(|| village::frontier::oversee::profession_table(world, settlement));
            let allowed = rows
                .iter()
                .find(|row| row.key == *profession)
                .is_some_and(|row| economy::target_from_row(world, settlement, row, step).is_ok());
            kit::set_enabled(world, entity, allowed);
        }
    }
}
