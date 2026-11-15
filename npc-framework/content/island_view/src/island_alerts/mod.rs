//! Village alerts: a short click-to-focus stack while overseeing, the
//! Journal page of the ledger, and a reduced set of fading toasts while
//! living as a character.
//!
//! Every alert has a severity, shown by colour, shape and word. The stack
//! holds at most three rows, critical and warning only; everything else is
//! one click away in the Journal, which folds each day's routine notes into
//! one line and has a filter for emergencies only or every row. The stack
//! collapses to its header. Live toasts fire for emergencies only.
//!
//! Read-only: alerts come from the journal's applied effects. Where the
//! simulation journals an onset (starvation, thirst, an emptied resource
//! patch) that effect is the only source; existing components are scanned
//! only for conditions without one (no home, a village-wide shortage, and
//! every need in a profile or session that journals no onsets). The feed is
//! host presentation scoped to the session epoch; it is never saved or hashed
//! and starts at the present, so a loaded journal's history is not replayed.
pub(crate) mod model;
pub(crate) mod outlook;
#[cfg(test)]
mod tests;

use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale::{self, LayoutClass};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Tone};
use crate::island_shell::model::LedgerPage;
use crate::{IslandMode, game_ui as ui};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use island_content::village::{
    self, ResourcePatch, VillagePlace, Villager, Vitality, catalog, colony,
};
use model::{AlertKind, Feed, Filter, Incoming, Row, Severity};
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::prelude::{Item, SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{Selected, ViewSystems, sim_to_world};
use std::collections::{BTreeMap, BTreeSet};

/// Rows kept in the feed.
const FEED_CAP: usize = 40;
/// Repeats of one alert about one subject fold together within a game day.
const FOLD_TICKS: u64 = 2400;
/// Real seconds a Live toast stays on screen.
const TOAST_SECS: f64 = 9.0;
const TOAST_LIMIT: usize = 3;
/// Rows of the Oversee stack; the rest is "+N more" and the Journal.
pub(crate) const STACK_ROWS: usize = 3;
/// Residents of one village with one reason that fold into one row.
pub(crate) const STACK_FOLD: usize = 3;
/// Real seconds between two reads of the Village outlook, at most.
const OUTLOOK_SECS: f32 = 0.5;
/// Real seconds between two read-only scans for conditions without an effect.
const SCAN_SECS: f32 = 0.5;
/// Ticks at the critical level before a scan reports hunger or thirst, where
/// no onset effect does.
const CRITICAL_TICKS: u32 = 100;

#[derive(Resource)]
pub(crate) struct Alerts {
    pub(crate) feed: Feed,
    cursor: Option<(u64, u32)>,
    epoch: Option<u64>,
    first_tools: BTreeSet<String>,
    /// Patches known to be empty; `None` until the first baseline scan.
    empty_patches: Option<BTreeSet<SimId>>,
    suffering: BTreeSet<(SimId, AlertKind)>,
    /// The tick up to which a condition is already in the feed, by subject
    /// and kind. An onset effect at or before it is the same event again.
    announced: BTreeMap<(u32, AlertKind), u64>,
    /// The needs of this session's world were read once at its start.
    baselined: bool,
    scan_elapsed: f32,
    clock_secs: f64,
    /// Shortages currently reported, so each is announced once per onset.
    short: BTreeSet<&'static str>,
    /// Open sites by label and entrance, so a cancelled one can still be
    /// named and framed after the simulation has removed it.
    sites: BTreeMap<u32, (String, (i32, i32))>,
    /// Whole days of waiting already reported, by site.
    waiting: BTreeMap<u32, u32>,
    requests: Vec<Request>,
    rendered: Option<(u64, u64)>,
    /// Presentation choices; they survive a new world like any view setting.
    pub(crate) filter: Filter,
    pub(crate) collapsed: bool,
    open_days: BTreeSet<u64>,
    /// Bumped when a presentation choice changes what the feed shows.
    view_revision: u64,
    /// The feed's `fresh` count the alert sound last heard.
    fresh_heard: Option<u64>,
    /// The first scan of this world has run: what it found was already
    /// there when the world was opened, and makes no sound.
    scanned: bool,
    /// A need alert's reason by alert serial, read on the scan's beat
    /// (2.4): the short form, the full sentence and since when.
    reasons: BTreeMap<u64, (String, String, u64, Option<String>)>,
    /// A Compact window's stack was opened with Show.
    compact_open: bool,
    /// The folded rows the stack shows: their first alert's serial, the
    /// head of People and the residents (2.4).
    groups: BTreeMap<u64, (String, Vec<SimId>)>,
    /// The Village outlook's lines (2.5); `None` before the first read.
    outlook: Option<Vec<outlook::OutlookLine>>,
    /// The session epoch and game hour of that read, and the real seconds
    /// since: the card is read again when the hour changes, at most twice a
    /// second.
    outlook_read: Option<(u64, u64)>,
    outlook_elapsed: f32,
    /// The lines the card shows now; it is rebuilt only when they change.
    outlook_drawn: Option<Vec<outlook::OutlookLine>>,
}

/// Identity of one feed line across rebuilds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RowKey {
    Alert(u64),
    Fold(u64),
    /// Three or more residents of one village with one reason (2.4).
    Group(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Row(RowKey),
    Filter(Filter),
    Collapse,
    /// "+N more" and the journal line: open the Journal page.
    Journal,
    /// A line of the Village outlook: open what answers it.
    Outlook(usize),
}

impl Default for Alerts {
    fn default() -> Self {
        Self {
            feed: Feed::new(FEED_CAP, FOLD_TICKS),
            cursor: None,
            epoch: None,
            first_tools: BTreeSet::new(),
            empty_patches: None,
            suffering: BTreeSet::new(),
            announced: BTreeMap::new(),
            baselined: false,
            scan_elapsed: SCAN_SECS,
            clock_secs: 0.0,
            short: BTreeSet::new(),
            sites: BTreeMap::new(),
            waiting: BTreeMap::new(),
            requests: Vec::new(),
            rendered: None,
            filter: Filter::default(),
            collapsed: false,
            open_days: BTreeSet::new(),
            view_revision: 0,
            fresh_heard: None,
            scanned: false,
            reasons: BTreeMap::new(),
            compact_open: false,
            groups: BTreeMap::new(),
            outlook: None,
            outlook_read: None,
            outlook_elapsed: OUTLOOK_SECS,
            outlook_drawn: None,
        }
    }
}

#[derive(Component)]
pub(crate) struct FeedRoot;
#[derive(Component)]
struct FeedBody;
#[derive(Component)]
struct FeedRow(RowKey);
#[derive(Component)]
struct FeedTitle;
/// Everything below the header; hidden while the stack is collapsed.
#[derive(Component)]
struct FeedContent;
#[derive(Component)]
struct FeedControl(Request);
#[derive(Component)]
struct CollapseLabel;
/// The header's count of critical true or warning rows.
#[derive(Component)]
struct CountBadge(bool);
#[derive(Component)]
struct CountText(bool);
#[derive(Component)]
struct JournalText;
/// The Village outlook card and the body its lines fill.
#[derive(Component)]
struct OutlookCard;
#[derive(Component)]
struct OutlookBody;
/// The folded header's count of the outlook's urgent lines.
#[derive(Component)]
struct OutlookFoldBadge;
#[derive(Component)]
struct OutlookFoldText;
#[derive(Component)]
struct ToastRoot;
#[derive(Component)]
struct ToastText;

pub(crate) fn plugin(app: &mut App) {
    ui::install(app);
    crate::panel_scroll::install(app);
    app.init_resource::<Alerts>()
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (track, process, refresh, alert_sound)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::colony_panel::refresh),
        );
}

/// The alert stack for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Alerts>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Village alerts"),
            FeedRoot,
            crate::island_shell::focus::FocusRegion::Alerts,
            GlobalZIndex(t::layer::CHROME),
            Node {
                position_type: PositionType::Absolute,
                top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                left: px(t::SCREEN_MARGIN),
                width: px(t::ATTENTION_COLUMN),
                row_gap: px(4),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let header = world
        .spawn((
            ChildOf(root),
            w::chrome(
                Node {
                    height: px(32),
                    padding: UiRect::new(px(10), px(4), px(0), px(0)),
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    flex_shrink: 0.0,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let title = w::label(
        world,
        header,
        "Alerts",
        t::BODY,
        Tone::text(Surface::Chrome),
    );
    world.entity_mut(title).insert(FeedTitle);
    // Severity is a shape and a count here, a word in every row.
    for (critical, status) in [(true, Status::Crit), (false, Status::Warn)] {
        let badge = w::badge(world, header, Surface::Chrome, status, "0");
        world.entity_mut(badge).insert(CountBadge(critical));
        let text = world
            .get::<Children>(badge)
            .and_then(|children| children.last().copied())
            .expect("badge text");
        world.entity_mut(text).insert(CountText(critical));
    }
    // Folded, the header also counts the outlook's urgent lines (2.5).
    let outlook_badge = w::badge(world, header, Surface::Chrome, Status::Info, "0");
    world.entity_mut(outlook_badge).insert(OutlookFoldBadge);
    let text = world
        .get::<Children>(outlook_badge)
        .and_then(|children| children.last().copied())
        .expect("badge text");
    world.entity_mut(text).insert(OutlookFoldText);
    w::show(world, outlook_badge, false);
    world.spawn((
        ChildOf(header),
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    let collapse = w::button(
        world,
        header,
        ButtonSpec::new("Hide", Kind::Quiet, Surface::Chrome)
            .size(w::Size::Small)
            .reach(Reach::Ring),
        FeedControl(Request::Collapse),
    );
    let label = world
        .get::<Children>(collapse)
        .and_then(|children| children.first().copied())
        .expect("button label");
    world.entity_mut(label).insert(CollapseLabel);
    let content = world
        .spawn((
            ChildOf(root),
            FeedContent,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(content),
        FeedBody,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        },
    ));
    // The Village outlook: now, next, past (2.5). Not drawn before its first
    // read.
    let card = world
        .spawn((
            ChildOf(content),
            OutlookCard,
            w::chrome(
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(2),
                    flex_shrink: 0.0,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    w::label(
        world,
        card,
        "Village outlook",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    world.spawn((
        ChildOf(card),
        OutlookBody,
        Node {
            flex_direction: FlexDirection::Column,
            ..default()
        },
    ));
    // The day's notes live in the Journal; this line leads there.
    let journal = world
        .spawn((
            ChildOf(content),
            w::chrome(
                Node {
                    height: px(t::ROW_ONE_LINE),
                    overflow: Overflow::clip(),
                    flex_shrink: 0.0,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let link = w::control(
        world,
        journal,
        "Open the journal",
        Kind::Quiet,
        Surface::Chrome,
        Reach::Ring,
        Node {
            width: percent(100),
            padding: UiRect::new(px(10), px(6), px(0), px(0)),
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        },
        FeedControl(Request::Journal),
    );
    icon::icon(
        world,
        link,
        Icon::Hint,
        14.0,
        Tone::status(Surface::Chrome, Status::Info),
    );
    let text = w::label(world, link, "", t::DENSE, Tone::secondary(Surface::Chrome));
    world.entity_mut(text).insert((
        JournalText,
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    w::keycap(world, link, Surface::Chrome, LedgerPage::Journal.key_cap());
    world.entity_mut(link).insert(Tooltip::new(
        "Journal",
        "Everything that happened, day by day, with its filters.",
    ));
    let toasts = world
        .spawn((
            Name::new("Village toasts"),
            ToastRoot,
            Pickable::IGNORE,
            GlobalZIndex(26),
            BackgroundColor(ui::PANEL.with_alpha(0.88)),
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(84),
                width: px(340),
                max_width: percent(42),
                padding: UiRect::axes(px(10), px(7)),
                display: Display::None,
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(toasts),
        ToastText,
        Pickable::IGNORE,
        ui::text("", 13.0, ui::TEXT),
    ));
}

fn activate(
    event: On<Activate>,
    rows: Query<(&FeedRow, Has<InteractionDisabled>)>,
    controls: Query<(&FeedControl, Has<InteractionDisabled>)>,
    mut state: ResMut<Alerts>,
) {
    if let Ok((row, false)) = rows.get(event.entity) {
        state.requests.push(Request::Row(row.0));
    }
    if let Ok((control, false)) = controls.get(event.entity) {
        state.requests.push(control.0);
    }
}

/// The ledger's Journal page asks for an alert row or a day's fold line.
pub(crate) fn request_row(world: &mut World, id: u64, fold: bool) {
    if let Some(mut state) = world.get_resource_mut::<Alerts>() {
        state.requests.push(Request::Row(if fold {
            RowKey::Fold(id)
        } else {
            RowKey::Alert(id)
        }));
    }
}

pub(crate) fn request_filter(world: &mut World, filter: Filter) {
    if let Some(mut state) = world.get_resource_mut::<Alerts>() {
        state.requests.push(Request::Filter(filter));
    }
}

/// The Journal page's filter, where alerts are tracked.
pub(crate) fn filter(world: &World) -> Option<Filter> {
    world.get_resource::<Alerts>().map(|state| state.filter)
}

fn name_of(world: &World, id: SimId) -> Option<(String, Option<SimPos>)> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    let position = world.get::<SimPos>(entity).copied();
    let name = world
        .get::<Villager>(entity)
        .map(|person| person.name.clone())
        .or_else(|| {
            world
                .get::<VillagePlace>(entity)
                .map(|place| place.label.clone())
        })?;
    Some((name, position))
}

fn settlement_of(world: &World, entity: Entity) -> Option<String> {
    let id = village::frontier::settlements::member(world, entity)?;
    let settlement = world.get_resource::<SimIndex>()?.get(id)?;
    world
        .get::<village::frontier::Settlement>(settlement)
        .map(|place| place.label.clone())
}

fn kind_name(world: &World, kind: &str) -> String {
    catalog::display_in(world, kind).map_or_else(
        || {
            kind.strip_prefix("village_")
                .unwrap_or(kind)
                .replace('_', " ")
        },
        str::to_owned,
    )
}

fn point(position: Option<SimPos>) -> Option<(i32, i32)> {
    position.map(|position| (position.x, position.z))
}

/// Whether the simulation journals the onset of starvation and thirst and
/// the emptying of a resource patch. It does in the regional living world
/// while the journal records; then those effects are the only source and the
/// scan leaves the three conditions alone, so none is reported twice.
fn onsets_journaled(world: &World) -> bool {
    world
        .get_resource::<Journal>()
        .is_some_and(|journal| journal.enabled)
}

fn depleted_line(world: &World, patch: Entity, kind: &str) -> String {
    let place =
        settlement_of(world, patch).map_or_else(String::new, |label| format!(" near {label}"));
    format!("The {} patch{place} is depleted", kind_name(world, kind))
}

/// An emptied patch as the journal reports it: the alert is about the patch
/// (so repeats fold per site and a click frames it), not about whoever took
/// the last unit.
fn depleted_effect(
    world: &World,
    read: &dyn Fn(&str) -> Option<serde_json::Value>,
    gatherer: Option<(i32, i32)>,
    tick: u64,
) -> Option<Incoming> {
    let patch = read("patch")
        .and_then(|id| id.as_u64())
        .and_then(|id| u32::try_from(id).ok())?;
    let entity = world.get_resource::<SimIndex>()?.get(SimId(patch));
    let kind = entity
        .and_then(|entity| world.get::<ResourcePatch>(entity))
        .map(|patch| patch.kind.clone())
        .or_else(|| read("kind")?.as_str().map(str::to_owned))?;
    Some(Incoming {
        kind: AlertKind::Depleted,
        subject: Some(patch),
        focus: entity
            .and_then(|entity| point(world.get::<SimPos>(entity).copied()))
            .or(gatherer),
        text: entity.map_or_else(
            || format!("The {} patch is depleted", kind_name(world, &kind)),
            |entity| depleted_line(world, entity, &kind),
        ),
        tick,
    })
}

/// A site opened, finished or cancelled. The alert is about the site: its
/// row folds per site and a click frames it, whoever laid the last plank. A
/// cancelled site no longer exists, so its label and place come from what
/// the feed remembered while it was open.
fn project_effect(
    world: &mut World,
    read: &dyn Fn(&str) -> Option<serde_json::Value>,
    tick: u64,
) -> Option<Incoming> {
    let effect = read("type")?.as_str()?.to_owned();
    let site = read(model::project_site_field(&effect)?)
        .and_then(|id| id.as_u64())
        .and_then(|id| u32::try_from(id).ok())?;
    let remembered = world.resource::<Alerts>().sites.get(&site).cloned();
    let live = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(SimId(site)))
        .and_then(|entity| point(world.get::<SimPos>(entity).copied()));
    let with_label = |key: &str| {
        read(key).or_else(|| {
            (key == "label")
                .then(|| remembered.as_ref().map(|(label, _)| label.clone().into()))
                .flatten()
        })
    };
    let (kind, text) = model::classify(&effect, "A resident", &with_label)?;
    let focus = live.or(remembered.as_ref().map(|(_, door)| *door));
    let mut state = world.resource_mut::<Alerts>();
    match kind {
        AlertKind::ProjectStarted => {
            if let (Some(label), Some(door)) = (
                read("label").and_then(|label| label.as_str().map(str::to_owned)),
                focus,
            ) {
                state.sites.insert(site, (label, door));
            }
        }
        _ => {
            state.sites.remove(&site);
            state.waiting.remove(&site);
        }
    }
    Some(Incoming {
        kind,
        subject: Some(site),
        focus,
        text,
        tick,
    })
}

/// Sites that are still short of materials after whole days without a
/// delivery. No journal effect announces a wait, so it is read here, with
/// the labels and entrances the feed keeps for sites that may be cancelled.
fn scan_sites(world: &mut World, tick: u64) {
    if !crate::island_build::available(world) {
        return;
    }
    let open: Vec<_> = crate::island_build::site_rows(world)
        .into_iter()
        .map(|(row, door)| {
            (
                row.id,
                row.label.clone(),
                row.waiting_days,
                row.materials_line(),
                door,
            )
        })
        .collect();
    let mut state = world.resource_mut::<Alerts>();
    let now = state.clock_secs;
    state.waiting.retain(|site, _| {
        open.iter()
            .any(|(id, _, days, _, _)| id == site && *days > 0)
    });
    for (id, label, days, missing, door) in open {
        state.sites.insert(id, (label.clone(), (door.x, door.z)));
        if !model::waiting_due(days, state.waiting.get(&id).copied()) {
            continue;
        }
        state.waiting.insert(id, days);
        state.feed.supersede(
            Incoming {
                kind: AlertKind::SiteWaiting,
                subject: Some(id),
                focus: Some((door.x, door.z)),
                text: model::waiting_text(&label, days, &missing),
                tick,
            },
            now,
        );
    }
}

/// Start at the present for a new world or session.
fn reset(world: &mut World, epoch: u64) {
    let cursor = world
        .get_resource::<Journal>()
        .and_then(|journal| journal.entries.back())
        .map(|entry| (entry.tick, entry.seq));
    let mut state = world.resource_mut::<Alerts>();
    let (filter, collapsed) = (state.filter, state.collapsed);
    *state = Alerts {
        cursor,
        epoch: Some(epoch),
        filter,
        collapsed,
        ..default()
    };
    state.feed.revision = 1;
}

pub(crate) fn track(world: &mut World) {
    if !crate::island_shell::enabled(world) {
        return;
    }
    let epoch = npc_framework::agent_io::api::epoch(world);
    if world.resource::<Alerts>().epoch != Some(epoch) {
        reset(world, epoch);
    }
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let mut state = world.resource_mut::<Alerts>();
        state.clock_secs += f64::from(delta);
        state.scan_elapsed += delta;
    }
    harvest(world);
    if world.resource::<Alerts>().scan_elapsed >= SCAN_SECS {
        world.resource_mut::<Alerts>().scan_elapsed = 0.0;
        scan(world);
        read_reasons(world);
    }
}

/// Each need alert's reason, in the resident's own terms (2.4), read from
/// the simulation's `need_alert` on the scan's beat (twice a second at
/// most), never per frame. Rows rebuild only when a reason changed.
fn read_reasons(world: &mut World) {
    // A resident who has died needs nothing: their need alerts go (the
    // death has its own row; closing phase, defect 1).
    let gone: Vec<u64> = world
        .resource::<Alerts>()
        .feed
        .iter()
        .filter(|alert| model::need_of(alert.kind).is_some())
        .filter(|alert| {
            alert.subject.is_none_or(|subject| {
                world
                    .get_resource::<SimIndex>()
                    .and_then(|index| index.get(SimId(subject)))
                    .is_none_or(|entity| !village::living(world, entity))
            })
        })
        .map(|alert| alert.serial)
        .collect();
    if !gone.is_empty() {
        world
            .resource_mut::<Alerts>()
            .feed
            .remove_where(|alert| gone.contains(&alert.serial));
    }
    let wanted: Vec<(u64, u32, &'static str, u64)> = world
        .resource::<Alerts>()
        .feed
        .iter()
        .filter_map(|alert| {
            let need = model::need_of(alert.kind)?;
            Some((alert.serial, alert.subject?, need, alert.tick))
        })
        .collect();
    let mut reasons = BTreeMap::new();
    for (serial, subject, need, tick) in wanted {
        let Some(entity) = world
            .get_resource::<SimIndex>()
            .and_then(|index| index.get(SimId(subject)))
        else {
            continue;
        };
        let Some((name, _)) = name_of(world, SimId(subject)) else {
            continue;
        };
        let alert = village::view::need_alert(world, entity, need);
        let target = alert.as_ref().and_then(|alert| match &alert.cause {
            village::view::NeedCause::OnTheWay { target, .. } => {
                name_of(world, *target).map(|(name, _)| name)
            }
            _ => None,
        });
        let (short, full) = model::reason_words(
            need,
            alert.as_ref().map(|alert| &alert.cause),
            &name,
            target.as_deref(),
            alert.as_ref().map(|alert| alert.sentence.as_str()),
        );
        if alert.is_none() {
            debug!("no reason for {name}'s {need} alert");
        }
        let since = alert.and_then(|alert| alert.since_tick).unwrap_or(tick);
        let village = settlement_of(world, entity);
        reasons.insert(serial, (short, full, since, village));
    }
    let mut state = world.resource_mut::<Alerts>();
    if state.reasons != reasons {
        state.reasons = reasons;
        state.view_revision = state.view_revision.wrapping_add(1);
    }
}

/// Fold new applied journal effects into the feed. The journal is a bounded
/// ring, so one pass over its unread tail is bounded too.
fn harvest(world: &mut World) {
    let cursor = world.resource::<Alerts>().cursor;
    let Some(journal) = world.get_resource::<Journal>() else {
        return;
    };
    let latest = journal.entries.back().map(|entry| (entry.tick, entry.seq));
    if latest == cursor {
        return;
    }
    // A journal that moved backwards belongs to another timeline.
    let rewound = matches!((latest, cursor), (Some(latest), Some(cursor)) if latest < cursor);
    let mut fresh: Vec<_> = journal
        .entries
        .iter()
        .rev()
        .take_while(|entry| {
            !rewound && cursor.is_none_or(|cursor| (entry.tick, entry.seq) > cursor)
        })
        .filter(|entry| {
            matches!(
                entry.kind,
                JournalKind::Effect { applied: true, .. } | JournalKind::CraftDone { .. }
            )
        })
        .cloned()
        .collect();
    fresh.reverse();
    world.resource_mut::<Alerts>().cursor = latest;
    for entry in fresh {
        let subject = entry
            .unit
            .and_then(|id| name_of(world, id).map(|found| (id, found)));
        let (who, position) = subject.as_ref().map_or_else(
            || {
                // Somebody who was laid to rest no longer resolves: their
                // name is in the simulation's record of the departed.
                let name = entry.unit.and_then(|id| {
                    island_content::village::view::departed(world)
                        .into_iter()
                        .find(|gone| gone.resident == Some(id))
                        .map(|gone| gone.name)
                });
                (name.unwrap_or_else(|| "A resident".to_owned()), None)
            },
            |(_, (name, position))| (name.clone(), *position),
        );
        let incoming = match &entry.kind {
            JournalKind::Effect { effect, .. }
                if effect.get("type").and_then(|value| value.as_str())
                    == Some("VillagePatchDepleted") =>
            {
                let read = |key: &str| effect.get(key).cloned();
                match depleted_effect(world, &read, point(position), entry.tick) {
                    Some(incoming) => incoming,
                    None => continue,
                }
            }
            JournalKind::Effect { effect, .. }
                if effect
                    .get("type")
                    .and_then(|value| value.as_str())
                    .is_some_and(|kind| model::project_site_field(kind).is_some()) =>
            {
                let read = |key: &str| effect.get(key).cloned();
                match project_effect(world, &read, entry.tick) {
                    Some(incoming) => incoming,
                    None => continue,
                }
            }
            JournalKind::Effect { effect, .. } => {
                let Some(kind) = effect.get("type").and_then(|value| value.as_str()) else {
                    continue;
                };
                // A village's name for the notes of the player's orders.
                let village = effect
                    .get("settlement")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|id| u32::try_from(id).ok())
                    .and_then(|id| {
                        let entity = world.get_resource::<SimIndex>()?.get(SimId(id))?;
                        world
                            .get::<village::frontier::Settlement>(entity)
                            .map(|place| place.label.clone())
                    });
                let read = |key: &str| {
                    if key == "settlement_name" {
                        village.clone().map(serde_json::Value::String)
                    } else {
                        effect.get(key).cloned()
                    }
                };
                let Some((alert, text)) = model::classify(kind, &who, &read) else {
                    continue;
                };
                // A death records where it happened; the body may be gone later.
                let recorded = effect.get("pos").and_then(|value| {
                    Some((
                        i32::try_from(value.get(0)?.as_i64()?).ok()?,
                        i32::try_from(value.get(1)?.as_i64()?).ok()?,
                    ))
                });
                Incoming {
                    kind: alert,
                    // All of a day's trades are one row, whoever traded.
                    subject: entry
                        .unit
                        .map(|id| id.0)
                        .filter(|_| alert != AlertKind::Trade),
                    focus: recorded.or(point(position)),
                    text,
                    tick: entry.tick,
                }
            }
            JournalKind::CraftDone {
                recipe, outputs, ..
            } => {
                if model::tool_recipe(recipe).is_none()
                    || !world
                        .resource_mut::<Alerts>()
                        .first_tools
                        .insert(recipe.clone())
                {
                    continue;
                }
                let made = outputs
                    .first()
                    .and_then(|id| world.get_resource::<SimIndex>()?.get(*id))
                    .and_then(|entity| world.get::<Item>(entity))
                    .map(|item| kind_name(world, &item.kind))
                    .or_else(|| model::tool_recipe(recipe).map(str::to_owned))
                    .unwrap_or_else(|| "tool".into());
                Incoming {
                    kind: AlertKind::FirstTool,
                    // One row per kind of tool, whoever made it.
                    subject: None,
                    focus: point(position),
                    text: format!("{who} made the first {made} of this session"),
                    tick: entry.tick,
                }
            }
            _ => continue,
        };
        let mut state = world.resource_mut::<Alerts>();
        // A condition the feed already holds as of this tick (read from the
        // world when the session began) is not announced again by its effect.
        if model::onset(incoming.kind)
            && let Some(subject) = incoming.subject
        {
            let key = (subject, incoming.kind);
            if state
                .announced
                .get(&key)
                .is_some_and(|known| incoming.tick <= *known)
            {
                continue;
            }
            state.announced.insert(key, incoming.tick);
        }
        let now = state.clock_secs;
        state.feed.push(incoming, now);
    }
}

/// Food or clean water below one unit per living resident.
fn shortage_lines(residents: u32, food: u32, water: u32) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    if residents == 0 {
        return found;
    }
    for (good, units, unit) in [("food", food, "portions"), ("clean water", water, "units")] {
        if units < residents {
            found.push((
                good,
                format!("The village is short of {good}: {units} {unit} for {residents} residents"),
            ));
        }
    }
    found
}

fn shortages(world: &World) -> Vec<(&'static str, String)> {
    let summary = colony::summary(world);
    shortage_lines(summary.residents, summary.food_units, summary.water_units)
}

/// Conditions no journal effect announces: residents without a home and
/// village-wide shortages. Hunger, thirst and emptied resource patches are
/// scanned only where the simulation journals no onset for them
/// ([`onsets_journaled`]); where it does, the needs are read once when the
/// session begins, for residents who were already suffering by then.
fn scan(world: &mut World) {
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let journaled = onsets_journaled(world);
    let baseline = journaled && !world.resource::<Alerts>().baselined;
    // Past the onset effect, which fired on the first tick at the level.
    let critical = if journaled { 1 } else { CRITICAL_TICKS };
    let Some(index) = world.get_resource::<SimIndex>() else {
        return;
    };
    let mut suffering = BTreeSet::new();
    let mut empty = BTreeSet::new();
    let mut found = Vec::new();
    let housed: BTreeSet<SimId> = index
        .map
        .values()
        .filter_map(|entity| world.get::<colony::Household>(*entity))
        .flat_map(|home| home.residents.iter().copied())
        .collect();
    for (id, entity) in &index.map {
        if let (Some(person), Some(vital)) = (
            world.get::<Villager>(*entity),
            world.get::<Vitality>(*entity),
        ) && vital.died_at.is_none()
        {
            for (kind, ticks, text) in [
                (AlertKind::Starving, vital.starved_ticks, model::STARVING),
                (
                    AlertKind::Dehydrated,
                    vital.dehydrated_ticks,
                    model::DEHYDRATED,
                ),
            ] {
                if (!journaled || baseline) && ticks >= critical {
                    suffering.insert((*id, kind));
                    found.push((
                        (*id, kind),
                        point(world.get::<SimPos>(*entity).copied()),
                        format!("{} {text}", person.name),
                    ));
                }
            }
            // A visitor who has not arrived yet has no life to house.
            if !housed.contains(id) && world.get::<colony::AwaitingArrival>(*entity).is_none() {
                suffering.insert((*id, AlertKind::Homeless));
                found.push((
                    (*id, AlertKind::Homeless),
                    point(world.get::<SimPos>(*entity).copied()),
                    format!("{} has no home", person.name),
                ));
            }
        }
        if !journaled
            && let Some(patch) = world.get::<ResourcePatch>(*entity)
            && patch.remaining == 0
        {
            empty.insert(*id);
            found.push((
                (*id, AlertKind::Depleted),
                point(world.get::<SimPos>(*entity).copied()),
                depleted_line(world, *entity, &patch.kind),
            ));
        }
    }
    let mut state = world.resource_mut::<Alerts>();
    let now = state.clock_secs;
    let known_empty = state.empty_patches.take();
    state.baselined |= baseline;
    for ((id, kind), focus, text) in found {
        let fresh = if kind == AlertKind::Depleted {
            // The first scan of a session is only the baseline.
            known_empty
                .as_ref()
                .is_some_and(|known| !known.contains(&id))
        } else {
            !state.suffering.contains(&(id, kind))
        };
        if fresh {
            if model::onset(kind) {
                state.announced.insert((id.0, kind), tick);
            }
            state.feed.push(
                Incoming {
                    kind,
                    subject: Some(id.0),
                    focus,
                    text,
                    tick,
                },
                now,
            );
        }
    }
    state.suffering = suffering;
    // Without a scan of the patches there is no baseline to keep.
    state.empty_patches = (!journaled).then_some(empty);

    let short = shortages(world);
    let mut state = world.resource_mut::<Alerts>();
    let now = state.clock_secs;
    let known = std::mem::take(&mut state.short);
    for (good, text) in short {
        if !known.contains(good) {
            state.feed.push(
                Incoming {
                    kind: AlertKind::Shortage,
                    // One row per good; no resident or site has these ids.
                    subject: Some(u32::MAX - u32::from(good != "food")),
                    focus: None,
                    text,
                    tick,
                },
                now,
            );
        }
        state.short.insert(good);
    }
    scan_sites(world, tick);
    // The baseline: alerts found by a world's first scan were there when it
    // was opened (a load, a new game); the sound hears them as already said.
    let mut state = world.resource_mut::<Alerts>();
    if !state.scanned {
        state.scanned = true;
        state.fresh_heard = Some(state.feed.fresh);
    }
}

/// Click-to-focus: move the overview camera to the resident or the site.
/// Header controls and fold lines only change what the feed shows.
fn process(world: &mut World) {
    let mut outlook_opens: Vec<outlook::Opens> = Vec::new();
    let requests = std::mem::take(&mut world.resource_mut::<Alerts>().requests);
    // A line of the outlook opens the page that answers it.
    let requests: Vec<Request> = requests
        .into_iter()
        .filter(|request| {
            let Request::Outlook(index) = request else {
                return true;
            };
            let opens = world
                .resource::<Alerts>()
                .outlook_drawn
                .as_ref()
                .and_then(|lines| lines.get(*index))
                .and_then(|line| line.opens.clone());
            outlook_opens.extend(opens);
            false
        })
        .collect();
    // The resident card, the build menu, a building card or a ledger page.
    let inspecting = crate::resident_inspector::open(world)
        || crate::island_build::column_taken(world)
        || crate::colony_panel::open(world);
    let compact = scale::layout(world).class == LayoutClass::Compact;
    for opens in outlook_opens {
        match opens {
            outlook::Opens::Goods(village, good) => crate::colony_panel::show_economy(
                world,
                village,
                crate::colony_panel::EconomyView::Goods(good),
            ),
            outlook::Opens::Jobs(village) => crate::colony_panel::show_economy(
                world,
                village,
                crate::colony_panel::EconomyView::Jobs,
            ),
            outlook::Opens::Priorities(village) => crate::colony_panel::show_economy(
                world,
                village,
                crate::colony_panel::EconomyView::Priorities,
            ),
            outlook::Opens::Homes => {
                crate::island_build::request_group(world, crate::island_build::model::Group::Homes);
            }
        }
    }
    let mut close_inspector = false;
    let mut open_journal = false;
    let mut open_group = None;
    for request in requests {
        let serial = match request {
            Request::Row(RowKey::Alert(serial)) => serial,
            other => {
                let mut state = world.resource_mut::<Alerts>();
                match other {
                    Request::Row(RowKey::Fold(day)) => {
                        if !state.open_days.remove(&day) {
                            state.open_days.insert(day);
                        }
                    }
                    Request::Filter(filter) => state.filter = filter,
                    // The resident inspector folds the feed to its header:
                    // asking for the alerts back closes that card instead.
                    Request::Collapse if inspecting => {
                        state.collapsed = false;
                        close_inspector = true;
                    }
                    // In a Compact window the stack is folded unless the
                    // player opened it (closing phase, defect 8).
                    Request::Collapse if compact => state.compact_open = !state.compact_open,
                    Request::Collapse => state.collapsed = !state.collapsed,
                    Request::Journal => open_journal = true,
                    Request::Outlook(_) => {}
                    Request::Row(RowKey::Group(serial)) => open_group = Some(serial),
                    Request::Row(RowKey::Alert(_)) => {}
                }
                state.view_revision = state.view_revision.wrapping_add(1);
                continue;
            }
        };
        let Some(alert) = world.resource::<Alerts>().feed.get(serial).cloned() else {
            continue;
        };
        let subject = alert.subject.map(SimId).and_then(|id| {
            let entity = world.get_resource::<SimIndex>()?.get(id)?;
            Some((id, entity))
        });
        // A living resident has moved on since the alert; follow them.
        let current = subject.and_then(|(_, entity)| {
            (world.get::<Villager>(entity).is_some() && village::living(world, entity))
                .then(|| world.get::<SimPos>(entity).copied())
                .flatten()
        });
        let target = current
            .or(alert.focus.map(|(x, z)| SimPos::new(x, z)))
            .or(subject.and_then(|(_, entity)| world.get::<SimPos>(entity).copied()));
        let Some(target) = target else { continue };
        if let Some((id, entity)) = subject
            && world.get::<Villager>(entity).is_some()
        {
            world.get_resource_or_init::<Selected>().0 = Some(id);
        }
        // A site or a building that still stands: frame it and open its card.
        if matches!(
            alert.kind,
            AlertKind::ProjectStarted | AlertKind::ProjectCompleted | AlertKind::SiteWaiting
        ) && let Some((id, _)) = subject
        {
            crate::island_build::focus_place(world, id);
            continue;
        }
        crate::island_camera::focus(world, sim_to_world(target), 16.0);
    }
    if close_inspector {
        crate::colony_panel::close(world);
        crate::resident_inspector::close(world);
        crate::island_build::close_column(world);
    }
    if open_journal {
        crate::colony_panel::open_page(world, LedgerPage::Journal);
    }
    // A folded row: People with only those residents.
    if let Some((head, who)) =
        open_group.and_then(|serial| world.resource::<Alerts>().groups.get(&serial).cloned())
    {
        crate::colony_panel::show_people(world, head, who);
    }
}

/// Severity as the kit's status: critical is a diamond, warning a
/// triangle, a note a square.
fn severity_status(severity: Severity) -> Status {
    match severity {
        Severity::Critical => Status::Crit,
        Severity::Warning => Status::Warn,
        Severity::Info => Status::Info,
    }
}

/// The word that goes with the shape and the colour.
fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "Critical",
        Severity::Warning => "Warning",
        Severity::Info => "Note",
    }
}

/// A title and one line of detail from an alert's sentence: what stands in
/// trailing parentheses is the detail ("planks 3/8 · logs 4/4").
pub(crate) fn title_and_detail(text: &str) -> (&str, Option<&str>) {
    match text
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
    {
        Some((title, detail)) if !title.is_empty() && !detail.is_empty() => (title, Some(detail)),
        _ => (text, None),
    }
}

#[cfg(test)]
fn day_of(world: &World, tick: u64) -> u64 {
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    village::calendar_at(tick, &config, true).0 + 1
}

/// One row of the ledger's Journal page.
pub(crate) struct JournalLine {
    /// The alert's serial, or the day of a fold line.
    pub(crate) id: u64,
    pub(crate) fold: bool,
    pub(crate) day: u64,
    /// The sentence, then one line of detail.
    pub(crate) text: String,
    pub(crate) status: Status,
}

/// The Journal page: the chosen filter, how many rows the feed holds in all
/// and the rows the filter shows, newest first.
pub(crate) fn journal(world: &World) -> (Filter, usize, Vec<JournalLine>) {
    let Some(state) = world.get_resource::<Alerts>() else {
        return (Filter::default(), 0, Vec::new());
    };
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    let day = |tick: u64| village::calendar_at(tick, &config, true).0 + 1;
    let rows = state
        .feed
        .rows(state.filter, &state.open_days, &day)
        .into_iter()
        .map(|row| match row {
            Row::Alert(alert) => {
                let severity = alert.kind.severity();
                let (title, detail) = title_and_detail(&alert.text);
                let mut second = vec![severity_word(severity).to_owned()];
                second.extend(detail.map(str::to_owned));
                if alert.count > 1 {
                    second.push(format!("\u{d7}{}", alert.count));
                }
                JournalLine {
                    id: alert.serial,
                    fold: false,
                    day: day(alert.tick),
                    text: format!("{title}\n{}", second.join(" \u{b7} ")),
                    status: severity_status(severity),
                }
            }
            Row::Fold {
                day,
                label,
                detail,
                open,
                ..
            } => JournalLine {
                id: day,
                fold: true,
                day,
                // The breakdown of a mixed day goes on the second line.
                text: format!(
                    "{label} \u{b7} {}{}",
                    if open { "hide" } else { "show" },
                    detail.map_or_else(String::new, |detail| format!("\n{detail}"))
                ),
                status: Status::Idle,
            },
        })
        .collect();
    (state.filter, state.feed.len(), rows)
}

/// What the stack shows: up to three rows that need attention, newest
/// first, how many more there are, and the counts by severity.
pub(crate) struct Stack {
    /// Serial, status, title, second line and, for a need alert, the
    /// tooltip's title and body.
    rows: Vec<(u64, Status, String, String, Option<(String, String)>)>,
    /// The rows that fold several residents: by serial, the head of People
    /// and the residents.
    groups: BTreeMap<u64, (String, Vec<SimId>)>,
    more: usize,
    critical: usize,
    warning: usize,
    /// "Autumn, day 6 · 36 notes in the journal".
    journal: String,
}

fn stack(world: &World) -> Stack {
    let state = world.resource::<Alerts>();
    let config = world
        .get_resource::<SimConfig>()
        .copied()
        .unwrap_or_default();
    let day = |tick: u64| village::calendar_at(tick, &config, true).0 + 1;
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let today = day(now);
    // The one calendar of the top bar, never the running count.
    let date = crate::game_ui::words::date(world, now);
    let none = BTreeSet::new();
    let (mut rows, mut critical, mut warning, mut notes) = (Vec::new(), 0, 0, 0);
    // Need alerts of one village, one kind and one reason: three or more
    // fold into one row (2.4), keyed by the first of them.
    let mut members: BTreeMap<(String, AlertKind, String), Vec<(u64, SimId)>> = BTreeMap::new();
    for row in state.feed.rows(Filter::Attention, &none, &day) {
        if let Row::Alert(alert) = row
            && let Some((short, _, _, Some(village))) = state.reasons.get(&alert.serial)
            && let Some(subject) = alert.subject
        {
            members
                .entry((village.clone(), alert.kind, short.clone()))
                .or_default()
                .push((alert.serial, SimId(subject)));
        }
    }
    let mut folded: BTreeMap<u64, (u64, usize)> = BTreeMap::new();
    let mut groups = BTreeMap::new();
    for ((village, kind, short), list) in &members {
        if list.len() < STACK_FOLD {
            continue;
        }
        let first = list[0].0;
        for (serial, _) in list {
            folded.insert(*serial, (first, list.len()));
        }
        let head = model::group_title(list.len(), village, *kind);
        groups.insert(
            first,
            (
                head.clone(),
                list.iter().map(|(_, who)| *who).collect::<Vec<_>>(),
                short.clone(),
            ),
        );
    }
    for row in state.feed.rows(Filter::Attention, &none, &day) {
        match row {
            Row::Alert(alert) => {
                let severity = alert.kind.severity();
                if severity == Severity::Critical {
                    critical += 1;
                } else {
                    warning += 1;
                }
                // A member of a folded group: the group's row stands for it.
                if let Some((first, _)) = folded.get(&alert.serial) {
                    if *first == alert.serial
                        && let Some((head, _, short)) = groups.get(first)
                    {
                        let mut second = vec![severity_word(severity).to_owned()];
                        if !short.is_empty() {
                            second.push(short.clone());
                        }
                        rows.push((
                            alert.serial,
                            severity_status(severity),
                            head.clone(),
                            second.join(" \u{b7} "),
                            Some((
                                "Show them".to_owned(),
                                "Opens People with only them.".to_owned(),
                            )),
                        ));
                    }
                    continue;
                }
                let (title, detail) = title_and_detail(&alert.text);
                let mut second = vec![severity_word(severity).to_owned()];
                // A need alert: the resident's own reason; its day goes to
                // the tooltip with the full sentence.
                if let Some((short, full, since, _)) = state.reasons.get(&alert.serial) {
                    if !short.is_empty() {
                        second.push(short.clone());
                    }
                    if alert.count > 1 {
                        second.push(format!("\u{d7}{}", alert.count));
                    }
                    let who = name_of(world, SimId(alert.subject.unwrap_or(0)))
                        .map_or_else(|| title.to_owned(), |(name, _)| name);
                    let tip =
                        model::alert_tip(&who, full, &crate::game_ui::words::moment(world, *since));
                    rows.push((
                        alert.serial,
                        severity_status(severity),
                        title.to_owned(),
                        second.join(" \u{b7} "),
                        Some(tip),
                    ));
                    continue;
                }
                // A waiting site: a short title, the wait and what is missing.
                let title = match model::waiting_parts(title) {
                    Some((label, wait)) => {
                        second.push(wait.to_owned());
                        second.extend(detail.map(str::to_owned));
                        format!("{label} is waiting")
                    }
                    None => {
                        second.extend(detail.map(str::to_owned));
                        second.push(crate::game_ui::words::when(world, alert.tick));
                        title.to_owned()
                    }
                };
                if alert.count > 1 {
                    second.push(format!("\u{d7}{}", alert.count));
                }
                rows.push((
                    alert.serial,
                    severity_status(severity),
                    title,
                    second.join(" \u{b7} "),
                    None,
                ));
            }
            Row::Fold { day, count, .. } if day == today => notes = count,
            Row::Fold { .. } => {}
        }
    }
    let more = rows.len().saturating_sub(STACK_ROWS);
    rows.truncate(STACK_ROWS);
    Stack {
        rows,
        groups: groups
            .into_iter()
            .map(|(serial, (head, who, _))| (serial, (head, who)))
            .collect(),
        more,
        critical,
        warning,
        journal: match notes {
            0 => format!("{date} \u{b7} the journal"),
            1 => format!("{date} \u{b7} 1 note in the journal"),
            notes => format!("{date} \u{b7} {notes} notes in the journal"),
        },
    }
}

/// One row of the stack: a chrome card, at least 44 high, with a severity
/// edge, the shape, a title, one line of detail and a locate marker. A long
/// title wraps and the card grows; it is never cut (its tooltip says it
/// whole as well).
fn row(
    world: &mut World,
    body: Entity,
    key: RowKey,
    status: Status,
    title: &str,
    detail: &str,
) -> Option<Entity> {
    let card = world
        .spawn((
            ChildOf(body),
            w::chrome(
                Node {
                    min_height: px(t::ROW_TWO_LINES),
                    overflow: Overflow::clip(),
                    flex_shrink: 0.0,
                    ..default()
                },
                t::RADIUS_MD,
            ),
        ))
        .id();
    let entity = w::control(
        world,
        card,
        title,
        Kind::Quiet,
        Surface::Chrome,
        Reach::Ring,
        Node {
            width: percent(100),
            min_height: px(t::ROW_TWO_LINES),
            padding: UiRect::new(px(12), px(8), px(5), px(5)),
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        },
        FeedRow(key),
    );
    if matches!(key, RowKey::Alert(_) | RowKey::Group(_)) {
        // The four-pixel severity edge.
        world.spawn((
            ChildOf(entity),
            w::Swatch::Fill,
            Tone::status(Surface::Chrome, status),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                bottom: px(0),
                width: px(4),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ));
        icon::icon(
            world,
            entity,
            Icon::status(status),
            16.0,
            Tone::status(Surface::Chrome, status),
        );
    }
    let column = world
        .spawn((
            ChildOf(entity),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: px(0),
                overflow: Overflow::clip_x(),
                ..default()
            },
        ))
        .id();
    // Wraps: "The village is short of food: 37 portions for 44 residents"
    // is read whole, on two lines when it must (closing re-capture, N5).
    w::paragraph(world, column, title, t::BODY, Tone::text(Surface::Chrome));
    if !detail.is_empty() {
        w::label(
            world,
            column,
            detail,
            t::CAPTION,
            Tone::secondary(Surface::Chrome),
        );
    }
    if matches!(key, RowKey::Alert(_) | RowKey::Group(_)) {
        icon::icon(
            world,
            entity,
            Icon::Locate,
            14.0,
            Tone::secondary(Surface::Chrome),
        );
        // Beside the list: a card below an alert covered the next one.
        world.entity_mut(entity).insert(
            Tooltip::new(
                title,
                "Bring the camera there and select who or what it is about.",
            )
            .beside(),
        );
        return Some(entity);
    }
    None
}

/// One alert sound when new critical or warning alerts arrived since the
/// last frame (several at once sound once). Nothing before a world's first
/// scan has run, nor for what that baseline scan found.
fn alert_sound(world: &mut World) {
    let state = world.resource::<Alerts>();
    let (fresh, heard, scanned) = (state.feed.fresh, state.fresh_heard, state.scanned);
    if !scanned {
        return;
    }
    if alert_due(heard, fresh) {
        crate::game_ui::sound::play(world, crate::game_ui::sound::UiSound::Alert);
    }
    world.resource_mut::<Alerts>().fresh_heard = Some(fresh);
}

/// Whether new alerts arrived since the sound last heard the feed.
pub(crate) fn alert_due(heard: Option<u64>, fresh: u64) -> bool {
    heard.is_some_and(|heard| fresh > heard)
}

fn rebuild(world: &mut World, stack: &Stack) {
    let Some(body) = world
        .query_filtered::<Entity, With<FeedBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    // Focus on a row that is about to go returns to the world.
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    if focused.is_some_and(|entity| world.get::<FeedRow>(entity).is_some()) {
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .clear();
    }
    world.entity_mut(body).despawn_children();
    world.resource_mut::<Alerts>().groups = stack.groups.clone();
    for (serial, status, title, detail, tip) in &stack.rows {
        let key = if stack.groups.contains_key(serial) {
            RowKey::Group(*serial)
        } else {
            RowKey::Alert(*serial)
        };
        let entity = row(world, body, key, *status, title, detail);
        if let (Some(entity), Some((head, text))) = (entity, tip) {
            world
                .entity_mut(entity)
                .insert(Tooltip::new(head, text).beside());
        }
    }
    if stack.more > 0 {
        let more = world
            .spawn((
                ChildOf(body),
                w::chrome(
                    Node {
                        height: px(t::ROW_ONE_LINE),
                        overflow: Overflow::clip(),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    t::RADIUS_MD,
                ),
            ))
            .id();
        let link = w::control(
            world,
            more,
            "More alerts",
            Kind::Quiet,
            Surface::Chrome,
            Reach::Ring,
            Node {
                width: percent(100),
                padding: UiRect::horizontal(px(12)),
                align_items: AlignItems::Center,
                ..default()
            },
            FeedControl(Request::Journal),
        );
        w::label(
            world,
            link,
            format!("+{} more in the journal", stack.more),
            t::DENSE,
            Tone::secondary(Surface::Chrome),
        );
    }
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

/// The folded header's title: "Alerts · Outlook" (its count stands in its
/// own badge beside it, once), "Alerts" when the outlook has nothing urgent.
pub(crate) fn fold_title(outlook: usize) -> String {
    if outlook == 0 {
        "Alerts".to_owned()
    } else {
        "Alerts \u{b7} Outlook".to_owned()
    }
}

/// Whether the stack shows its header only: a card or page is read, the
/// player folded it, or the window is Compact and the player has not opened
/// it there.
pub(crate) fn folded(compact: bool, collapsed: bool, compact_open: bool, inspecting: bool) -> bool {
    inspecting || if compact { !compact_open } else { collapsed }
}

/// Read the Village outlook when the game hour changed, at most twice a
/// second of real time (2.5).
fn read_outlook(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let epoch = npc_framework::agent_io::api::epoch(world);
    let tick = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let day = world
        .get_resource::<SimConfig>()
        .map_or(2400, |config| config.day_length.max(1));
    let hour = tick * 24 / day;
    {
        let mut state = world.resource_mut::<Alerts>();
        state.outlook_elapsed += delta;
        if state.outlook_read == Some((epoch, hour)) || state.outlook_elapsed < OUTLOOK_SECS {
            return;
        }
        state.outlook_elapsed = 0.0;
        state.outlook_read = Some((epoch, hour));
    }
    let villages: Vec<(SimId, String, village::view::Outlook, Option<String>)> =
        village::frontier::settlements::all(world)
            .into_iter()
            .filter_map(|(id, entity)| {
                let name = world
                    .get::<village::frontier::Settlement>(entity)?
                    .label
                    .clone();
                Some((
                    id,
                    name,
                    village::view::outlook(world, id),
                    village::view::housing_line(world, id),
                ))
            })
            .collect();
    let views: Vec<outlook::VillageOutlook> = villages
        .iter()
        .map(|(id, name, outlook, housing)| outlook::VillageOutlook {
            id: *id,
            name,
            outlook,
            housing: housing.as_deref(),
        })
        .collect();
    let fuel = village::roles::roles(world).fuel;
    let lines = outlook::lines(
        &views,
        fuel,
        &|good| kind_name(world, good).to_lowercase(),
        &|good| {
            if good == catalog::WATER {
                Some(outlook::Topic::Water)
            } else if good == fuel {
                Some(outlook::Topic::Fuel)
            } else if catalog::food_restore_in(world, good).is_some() {
                Some(outlook::Topic::Food)
            } else {
                None
            }
        },
    );
    // A low window keeps two lines.
    let mut lines = lines;
    // The window's own height, not the design height: at 1024x600 the
    // interface scales down and the design height reads 720 (final pass, F3).
    let layout = scale::layout(world);
    lines.truncate(outlook::shown(layout.height * layout.scale));
    world.resource_mut::<Alerts>().outlook = Some(lines);
}

/// Fill the outlook card: rows are built again only when the lines changed,
/// so a quiet hour leaves every entity and text as it was.
fn draw_outlook(world: &mut World) {
    let lines = world.resource::<Alerts>().outlook.clone();
    show::<OutlookCard>(world, lines.is_some());
    let Some(lines) = lines else {
        return;
    };
    if world.resource::<Alerts>().outlook_drawn.as_ref() == Some(&lines) {
        return;
    }
    let Some(body) = world
        .query_filtered::<Entity, With<OutlookBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    if focused.is_some_and(|entity| {
        world
            .get::<FeedControl>(entity)
            .is_some_and(|control| matches!(control.0, Request::Outlook(_)))
    }) {
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .clear();
    }
    world.entity_mut(body).despawn_children();
    for (index, line) in lines.iter().enumerate() {
        // A long line wraps and the row grows; it is never cut (final
        // pass, F1: "… waiting for a home." keeps its end).
        let node = Node {
            width: percent(100),
            min_height: px(t::ROW_ONE_LINE),
            padding: UiRect::vertical(px(3)),
            column_gap: px(8),
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
            ..default()
        };
        let entity = if line.opens.is_some() {
            w::control(
                world,
                body,
                &line.text,
                Kind::Quiet,
                Surface::Chrome,
                Reach::Ring,
                node,
                FeedControl(Request::Outlook(index)),
            )
        } else {
            world.spawn((ChildOf(body), node)).id()
        };
        icon::icon(
            world,
            entity,
            Icon::status(line.status),
            14.0,
            Tone::status(Surface::Chrome, line.status),
        );
        let text = w::paragraph(
            world,
            entity,
            &line.text,
            t::DENSE,
            Tone::text(Surface::Chrome),
        );
        world.entity_mut(text).insert(Node {
            flex_grow: 1.0,
            min_width: px(0),
            ..default()
        });
        if line.opens.is_some() {
            w::label(
                world,
                entity,
                "\u{203a}",
                t::DENSE,
                Tone::secondary(Surface::Chrome),
            );
        }
        world
            .entity_mut(entity)
            .insert(Tooltip::new(&line.text, &line.tip).beside());
    }
    world.resource_mut::<Alerts>().outlook_drawn = Some(lines);
}

fn show<M: Component>(world: &mut World, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    for mut node in world.query_filtered::<&mut Node, With<M>>().iter_mut(world) {
        if node.display != display {
            node.display = display;
        }
    }
}

/// What the Journal says when the current filter leaves it without a line.
pub(crate) fn empty_text(filter: Filter, total: usize) -> &'static str {
    match (filter, total) {
        (_, 0) => {
            "Nothing needs your attention yet. Deaths, hunger, cold, shortages and empty resources appear here; each day's routine notes fold into one line."
        }
        (Filter::Critical, _) => "No emergencies. Choose Important or Everything to see the rest.",
        (Filter::Attention, _) => {
            "Nothing important right now. Choose Everything to read the routine notes."
        }
        (Filter::All, _) => "",
    }
}

fn refresh(world: &mut World) {
    let enabled = crate::island_shell::enabled(world);
    let mode = world.get_resource::<IslandMode>().copied();
    let hud = crate::player_hud::active(world);
    let gated = world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0);
    let overseeing = enabled && mode == Some(IslandMode::Inspector) && !hud && !gated;
    let top = crate::island_shell::content_top(world);
    let width = if scale::layout(world).class == LayoutClass::Wide {
        t::ATTENTION_COLUMN
    } else {
        t::ATTENTION_COLUMN_STANDARD
    };
    for mut node in world
        .query_filtered::<&mut Node, With<FeedRoot>>()
        .iter_mut(world)
    {
        let display = if overseeing {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if node.top != px(top) {
            node.top = px(top);
        }
        if node.width != px(width) {
            node.width = px(width);
        }
    }
    if overseeing {
        // The build menu, the inspector, a building card or a ledger page is
        // being read: the stack folds to its header.
        let inspecting = crate::resident_inspector::open(world)
            || crate::island_build::column_taken(world)
            || crate::colony_panel::open(world);
        let (revision, collapsed) = {
            let state = world.resource::<Alerts>();
            (
                (state.feed.revision, state.view_revision),
                // A Compact window shows the header only, until Show.
                folded(
                    scale::layout(world).class == LayoutClass::Compact,
                    state.collapsed,
                    state.compact_open,
                    inspecting,
                ),
            )
        };
        let stack = stack(world);
        if world.resource::<Alerts>().rendered != Some(revision) {
            rebuild(world, &stack);
            world.resource_mut::<Alerts>().rendered = Some(revision);
        }
        let attention = stack.critical + stack.warning;
        let urgent = world
            .resource::<Alerts>()
            .outlook
            .as_deref()
            .map_or(0, outlook::attention);
        let folded = collapsed && urgent > 0;
        let title = if folded {
            fold_title(urgent)
        } else if attention == 0 {
            "No alerts".to_owned()
        } else {
            "Alerts".to_owned()
        };
        set_text::<FeedTitle>(world, &title);
        show::<OutlookFoldBadge>(world, folded);
        set_text::<OutlookFoldText>(world, &urgent.to_string());
        for critical in [true, false] {
            let count = if critical {
                stack.critical
            } else {
                stack.warning
            };
            for (badge, mut node) in world.query::<(&CountBadge, &mut Node)>().iter_mut(world) {
                let display = if badge.0 == critical && count == 0 {
                    Display::None
                } else if badge.0 == critical {
                    Display::Flex
                } else {
                    continue;
                };
                if node.display != display {
                    node.display = display;
                }
            }
            for (badge, mut text) in world.query::<(&CountText, &mut Text)>().iter_mut(world) {
                if badge.0 == critical && text.0 != count.to_string() {
                    text.0 = count.to_string();
                }
            }
        }
        set_text::<CollapseLabel>(world, if collapsed { "Show" } else { "Hide" });
        set_text::<JournalText>(world, &stack.journal);
        show::<FeedContent>(world, !collapsed);
        read_outlook(world);
        draw_outlook(world);
    }
    // Live: a few fading lines, never interactive, never over the toolbar.
    let living = enabled && hud && mode == Some(IslandMode::Character);
    let text = if living {
        let state = world.resource::<Alerts>();
        state
            .feed
            .toasts(state.clock_secs, TOAST_SECS, TOAST_LIMIT)
            .iter()
            .map(|alert| {
                alert.line(&crate::game_ui::words::capital(
                    &crate::game_ui::words::when(world, alert.tick),
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    // Above the toolbar and the host's save notice beside it.
    let bottom = crate::player_hud::toolbar_clearance(world) + 44.0;
    for mut value in world
        .query_filtered::<&mut Text, With<ToastText>>()
        .iter_mut(world)
    {
        if value.0 != text {
            value.0.clone_from(&text);
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<ToastRoot>>()
        .iter_mut(world)
    {
        node.display = if text.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        node.bottom = px(bottom);
    }
}
