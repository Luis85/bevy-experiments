//! Interactive F4 explorer. Sampling continues while the display is frozen.
use crate::perf_model::{self, Key, Row};
use crate::perf_runtime::FrameRuntime;
use crate::{DebugPanels, ViewSystems};
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{Pressed, Selectable as UiSelectable, Selected as UiSelected};
use bevy::ui_widgets::{
    Activate, Button as WidgetButton, ButtonPlugin, ScrollArea, ScrollAreaPlugin, ScrollIntoView,
};
use npc_perf::{HistorySelection, PerfHistoryReport};
use npc_perf::{PerfHandle, PerfTreeReport, RuntimeStats, TraceKind, clip};
use std::collections::BTreeSet;

pub const REFRESH_SECS: f32 = 0.5;
const PAGE: usize = 80;
const PANEL: Color = Color::srgb(0.05, 0.06, 0.08);
const TEXT: Color = Color::srgb(0.91, 0.94, 0.91);
const DIM: Color = Color::srgb(0.65, 0.70, 0.75);
const ACCENT: Color = Color::srgb(0.95, 0.80, 0.45);
const OVER: Color = Color::srgb(1.0, 0.58, 0.52);
const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);

/// Keyboard ownership of the performance explorer (not simulation state).
#[derive(Component)]
pub struct PerfControl;
#[derive(Component)]
struct PerfPanel;
#[derive(Component)]
struct Rows;
#[derive(Component)]
struct Summary;
#[derive(Component)]
struct Detail;
#[derive(Component)]
struct DetailBox;
#[derive(Component)]
struct Notice;
#[derive(Component)]
struct RowValue(Key);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Costs,
    Budgets,
    History,
}
#[derive(Component, Clone, Debug, PartialEq, Eq)]
enum Action {
    Pane(Pane),
    Freeze,
    Reset,
    Collapse,
    More,
    Pick(Key),
    Budget(String),
}

#[derive(Resource)]
struct Explorer {
    pane: Pane,
    frozen: bool,
    expanded: BTreeSet<Key>,
    selected: Option<Key>,
    budget: Option<String>,
    report: Option<PerfTreeReport>,
    rows: Vec<Row>,
    visible: bool,
    dirty: bool,
    elapsed: f32,
    limit: usize,
    requests: Vec<Action>,
    focus_after: Option<Action>,
    history_selection: HistorySelection,
    history_label: Option<String>,
    history: Option<PerfHistoryReport>,
    charts: Vec<Handle<Image>>,
}
impl Default for Explorer {
    fn default() -> Self {
        Self {
            pane: Pane::Costs,
            frozen: false,
            expanded: BTreeSet::new(),
            selected: None,
            budget: None,
            report: None,
            rows: Vec::new(),
            visible: false,
            dirty: true,
            elapsed: 0.0,
            limit: PAGE,
            requests: Vec::new(),
            focus_after: None,
            history_selection: HistorySelection::default(),
            history_label: None,
            history: None,
            charts: Vec::new(),
        }
    }
}
pub(crate) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<ButtonPlugin>() {
        app.add_plugins(ButtonPlugin);
    }
    if !app.is_plugin_added::<ScrollAreaPlugin>() {
        app.add_plugins(ScrollAreaPlugin);
    }
    app.init_resource::<Explorer>()
        .init_resource::<Assets<Image>>()
        .init_resource::<InputFocus>()
        .add_observer(activate)
        .add_observer(acquire_focus)
        .configure_sets(Update, ViewSystems::Input.run_if(not(perf_input_active)))
        .add_systems(Startup, spawn)
        .add_systems(Update, (keyboard, shield).in_set(ViewSystems::OverlayInput))
        .add_systems(
            Update,
            (refresh, rebuild, paint)
                .chain()
                .after(crate::perf_runtime::RuntimeSet)
                .in_set(ViewSystems::Present),
        );
}
pub fn perf_input_active(
    focus: Option<Res<InputFocus>>,
    controls: Query<(), With<PerfControl>>,
) -> bool {
    focus
        .and_then(|f| f.get())
        .is_some_and(|e| controls.contains(e))
}
fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}
fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), font(size), TextColor(color))
}
fn button(p: &mut ChildSpawnerCommands, text: &str, action: Action) -> Entity {
    p.spawn((
        WidgetButton,
        PerfControl,
        TabIndex(0),
        UiSelectable,
        action,
        AccessibleLabel::new(text),
        Hovered::default(),
        Node {
            flex_shrink: 0.0,
            min_height: px(28),
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(10), px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(Color::NONE),
    ))
    .with_child(label(text, 12.0, TEXT))
    .id()
}
fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Name::new("perf panel"),
            PerfPanel,
            TabGroup::modal(),
            GlobalZIndex(25),
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(10),
                left: percent(2),
                width: percent(96),
                max_width: px(920),
                height: percent(86),
                max_height: px(700),
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(Color::srgb(0.35, 0.40, 0.48)),
        ))
        .with_children(|p| {
            p.spawn((Summary, label("", 13.0, TEXT)));
            p.spawn(Node {
                column_gap: px(6),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|r| {
                for (text, action) in [
                    ("Costs", Action::Pane(Pane::Costs)),
                    ("Budgets", Action::Pane(Pane::Budgets)),
                    ("History", Action::Pane(Pane::History)),
                    ("Freeze / Live", Action::Freeze),
                    ("Reset stats", Action::Reset),
                    ("Collapse all", Action::Collapse),
                    ("More rows", Action::More),
                ] {
                    button(r, text, action);
                }
            });
            p.spawn((Notice, label("", 11.0, DIM)));
            p.spawn((
                Rows,
                ScrollArea,
                Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_height: px(0),
                    row_gap: px(2),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
            ));
            p.spawn((
                DetailBox,
                ScrollArea,
                Node {
                    height: px(140),
                    max_height: px(140),
                    min_height: px(100),
                    flex_shrink: 0.0,
                    overflow: Overflow::scroll_y(),
                    padding: UiRect::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.10, 0.13)),
            ))
            .with_child((
                Detail,
                label(
                    "Expand a domain, then select a system to inspect its measured calls.",
                    12.0,
                    TEXT,
                ),
            ));
        });
}
fn activate(event: On<Activate>, actions: Query<&Action>, mut state: ResMut<Explorer>) {
    if let Ok(action) = actions.get(event.entity) {
        state.requests.push(action.clone());
    }
}
fn acquire_focus(
    mut event: On<AcquireFocus>,
    controls: Query<(), With<PerfControl>>,
    mut focus: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        focus.set(event.focused_entity, FocusCause::Navigated);
    }
}
#[derive(SystemParam)]
struct Navigation<'w, 's> {
    focus: ResMut<'w, InputFocus>,
    nav: TabNavigation<'w, 's>,
    controls: Query<'w, 's, (), With<PerfControl>>,
}
fn keyboard(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut panels: ResMut<DebugPanels>,
    mut nav: Navigation,
    mut commands: Commands,
) {
    let Some(keys) = keys else {
        return;
    };
    if !panels.perf || !nav.focus.get().is_some_and(|e| nav.controls.contains(e)) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::F4) {
        panels.perf = false;
        nav.focus.clear();
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        let direction = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        if let Ok(next) = nav.nav.navigate(&nav.focus, direction) {
            nav.focus.set(next, FocusCause::Navigated);
            commands.trigger(ScrollIntoView { entity: next });
        }
    }
}
fn shield(
    panels: Res<DebugPanels>,
    roots: Query<&Hovered, With<PerfPanel>>,
    mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    scroll: Option<ResMut<AccumulatedMouseScroll>>,
    #[cfg(feature = "command")] drag: Option<ResMut<crate::command_ui::DragBox>>,
) {
    if !panels.perf || !roots.iter().any(Hovered::get) {
        return;
    }
    if let Some(mut mouse) = mouse {
        mouse.reset_all();
    }
    if let Some(mut scroll) = scroll {
        scroll.delta = Vec2::ZERO;
    }
    #[cfg(feature = "command")]
    if let Some(mut drag) = drag {
        drag.start = None;
    }
}

fn refresh(
    time: Option<Res<Time<Real>>>,
    panels: Res<DebugPanels>,
    handle: Option<Res<PerfHandle>>,
    mut state: ResMut<Explorer>,
    focus: Res<InputFocus>,
    actions: Query<&Action>,
) {
    let opened = panels.perf && !state.visible;
    state.visible = panels.perf;
    if opened {
        state.focus_after = Some(Action::Pane(state.pane));
    }
    if !panels.perf {
        return;
    }
    let requests = std::mem::take(&mut state.requests);
    for action in requests {
        match action {
            Action::Pane(pane) => {
                state.pane = pane;
                state.dirty = true;
            }
            Action::Freeze => {
                if !state.frozen
                    && let Some(c) = &handle
                {
                    state.report = Some(c.0.capture_tree());
                }
                state.frozen = !state.frozen;
                state.dirty = true;
            }
            Action::Reset => {
                if let Some(c) = &handle {
                    c.0.reset();
                }
                state.report = None;
                state.frozen = false;
                state.selected = None;
                state.history_selection = HistorySelection::default();
                state.history_label = None;
                state.dirty = true;
            }
            Action::Collapse => {
                state.expanded.clear();
                state.dirty = true;
            }
            Action::More => {
                state.limit = (state.limit + PAGE).min(8192);
                state.dirty = true;
            }
            Action::Budget(name) => {
                state.history_selection = HistorySelection::Budget {
                    mechanic: name.clone(),
                };
                state.history_label = Some(name.clone());
                state.budget = Some(name);
                state.dirty = true;
            }
            Action::Pick(key) => {
                let reference = matches!(key, Key::Reference(..));
                let key = if let Key::Reference(_, child) = key {
                    Key::Trace(child)
                } else {
                    key
                };
                if !state.expanded.insert(key.clone()) {
                    state.expanded.remove(&key);
                }
                state.selected = Some(key.clone());
                state.history_selection = match &key {
                    Key::Domain(domain) => HistorySelection::Domain { domain: *domain },
                    Key::Group(domain, mechanic) => HistorySelection::Group {
                        domain: *domain,
                        mechanic: mechanic.clone(),
                    },
                    Key::Trace(id) => HistorySelection::Trace { id: *id },
                    Key::Reference(_, id) => HistorySelection::Trace { id: *id },
                };
                state.history_label = state
                    .rows
                    .iter()
                    .find(|r| r.key == key)
                    .map(|r| r.full_name.clone());
                if reference {
                    state.focus_after = Some(Action::Pick(key.clone()));
                }
                if let Key::Trace(id) = key
                    && let Some(report) = state.report.clone()
                {
                    perf_model::select_trace(&report, id, &mut state.expanded);
                }
                state.dirty = true;
            }
        }
    }
    state.elapsed += time.as_ref().map_or(0.0, |t| t.delta_secs());
    let refresh = opened || state.dirty || state.elapsed >= REFRESH_SECS;
    if !refresh {
        return;
    }
    if !state.frozen
        && let Some(c) = &handle
    {
        let selected = match state.selected {
            Some(Key::Trace(id)) => Some(id),
            _ => None,
        };
        let report = c.0.tree_report(selected);
        if state
            .report
            .as_ref()
            .is_some_and(|old| old.generation != report.generation)
        {
            state
                .expanded
                .retain(|key| !matches!(key, Key::Trace(_) | Key::Reference(..)));
            if matches!(state.selected, Some(Key::Trace(_))) {
                state.selected = None;
                state.history_selection = HistorySelection::default();
                state.history_label = None;
            }
            state.focus_after = Some(Action::Pane(state.pane));
        }
        state.report = Some(report);
    }
    if state.pane == Pane::History {
        state.history = if state.frozen {
            state
                .report
                .as_ref()
                .and_then(|r| r.history.as_ref())
                .map(|h| h.report(&state.history_selection, state.history_label.as_deref()))
        } else {
            handle.as_ref().map(|c| {
                c.0.history_report(&state.history_selection, state.history_label.as_deref())
            })
        };
    }
    state.elapsed = 0.0;
    let rows = state
        .report
        .as_ref()
        .map(|r| perf_model::rows(r, &state.expanded, state.limit))
        .unwrap_or_default();
    if rows
        .iter()
        .map(|r| &r.key)
        .ne(state.rows.iter().map(|r| &r.key))
    {
        state.dirty = true;
        if state.focus_after.is_none() {
            state.focus_after = focus.get().and_then(|e| actions.get(e).ok()).cloned();
        }
    }
    state.rows = rows;
    if matches!(state.pane, Pane::Budgets | Pane::History) {
        state.dirty = true;
        if state.focus_after.is_none() {
            state.focus_after = focus.get().and_then(|e| actions.get(e).ok()).cloned();
        }
    }
}

fn summary(
    runtime: Option<&RuntimeStats>,
    report: Option<&PerfTreeReport>,
    frozen: bool,
) -> String {
    let mut text = format!(
        "[F4] PERFORMANCE   {}   {}\n",
        if frozen { "FROZEN SNAPSHOT" } else { "LIVE" },
        if cfg!(debug_assertions) {
            "DEV"
        } else {
            "RELEASE"
        }
    );
    if let Some(r) = runtime {
        text.push_str(&format!(
            "Frame interval  {:.1} FPS  {:.2} ms",
            r.frame_fps, r.frame_ms
        ));
        if r.frame_target_us > 0 {
            text.push_str(&format!(
                "   target {:.2} ms ({:.0} FPS)",
                r.frame_target_us as f64 / 1000.0,
                1e6 / r.frame_target_us as f64
            ));
        }
        text.push('\n');
        if let Some(tps) = r.sim_tps {
            text.push_str(&format!(
                "Simulation  {}{tps:.1} ticks/s",
                if r.paused { "PAUSED | observed " } else { "" }
            ));
            if let Some(target) = r.sim_target_tps {
                text.push_str(&format!(" / {target:.1} target"));
            }
            if let Some(hz) = r.sim_hz {
                text.push_str(&format!("   {hz} Hz x{}", r.multiplier.unwrap_or(1)));
            }
            if r.pacing.as_deref() == Some("unpaced") {
                text.push_str("   unpaced");
            } else if r.pacing.is_none() {
                text.push_str("   manual / no driver");
            }
            if let Some(scale) = r.host_time_scale.filter(|s| (*s - 1.0).abs() > 0.0001) {
                text.push_str(&format!("   host x{scale:.2}"));
            }
            if let Some(tick) = report.and_then(|r| r.tick.as_ref()) {
                text.push_str(&format!(
                    "   tick p95 {:.1} us ({} samples)",
                    tick.p95_us, tick.samples
                ));
            }
        } else {
            text.push_str("Simulation  no simulation clock / driver");
        }
        text.push('\n');
    }
    if let Some(r) = report {
        let cost = r.domains.iter().map(|d| d.work.mean_us).sum::<f64>();
        text.push_str(&format!(
            "Measured self  {:.3} ms/{}   {} paths | interval {} | {}-sample window",
            cost / 1000.0,
            r.interval.label(),
            r.nodes.len(),
            r.frames,
            r.window
        ));
        text.push_str("\nCPU-side elapsed includes waits; parallel sums can exceed frame wall time. GPU time not captured.");
    } else {
        text.push_str("Profiler not installed (no PerfHandle). Frame rate remains available.");
    }
    text
}
fn details(state: &Explorer) -> String {
    let Some(report) = &state.report else {
        return "Install NpcPerfPlugin and its logging layer before adding systems.".into();
    };
    if state.pane == Pane::Budgets {
        if let Some(name) = &state.budget
            && let Some(b) = report.budgets.iter().find(|b| &b.mechanic == name)
        {
            return format!(
                "{}: effective limit {} us per {}. Observed inclusive p95: {}.\n{} active samples / {} warmup. Limits are not per call. Dev defaults are x10; configured values are shown.\nA missing measurement is not zero cost. No limit or simulation speed is changed by this panel.",
                b.mechanic,
                b.budget_us,
                report.interval.label(),
                b.observed_p95_us
                    .map_or_else(|| "not observed".into(), |v| format!("{v:.2} us")),
                b.samples,
                report.min_samples
            );
        }
        return format!(
            "Configured effective p95 limits per {}. '-' means not measured; warmup is {} active intervals.\nBudget checks retain their inclusive measurement basis. They are separate from the self-time cost tree.\nThe frame target is diagnostic only. Rendering measurements are CPU-side; GPU timing is not captured.",
            report.interval.label(),
            report.min_samples
        );
    }
    match &state.selected {
        Some(Key::Trace(id))=>{
            let Some(node)=report.nodes.iter().find(|n|n.id==*id) else {return "This path is no longer in the current capture. Select another row.".into();};
            let parent=node.parent.and_then(|p|report.nodes.iter().find(|n|n.id==p)).map_or("thread root",|n|&n.name);
            let mut text=format!("{}\n{} | {} | {:?} | {} calls\nParent: {}\n",node.name,node.domain.label(),node.mechanic,node.kind,node.calls,parent);
            if let Some(d)=report.detail.as_ref().filter(|d|d.id==*id).or_else(||report.details.iter().find(|d|d.id==*id)) {
                text.push_str(&format!("Per call inclusive: mean {:.2} | p50 {:.2} | p95 {:.2} | max {:.2} us\nPer call self: mean {:.2} | p95 {:.2} us. Self per {}: p95 {:.2} us.\n",d.inclusive_per_call.mean_us,d.inclusive_per_call.p50_us,d.inclusive_per_call.p95_us,d.inclusive_per_call.max_us,d.self_per_call.mean_us,d.self_per_call.p95_us,report.interval.label(),d.self_per_interval.p95_us));
            } else {text.push_str("Resume live to calculate percentiles for this selected path.\n");}
            text.push_str("Self excludes measured nested calls; inclusive includes them. CPU-side elapsed time includes waits.");text
        }
        Some(Key::Domain(domain))=>format!("{} owns the measured self time of its classified systems/scopes.\nExpand a mechanic and a system to inspect nested execution paths. Cross-domain calls are references, counted in their owning domain.\nFrame wrappers are excluded. Parallel spans may overlap; these values are not GPU time or thread CPU cycles.",domain.label()),
        Some(Key::Group(domain,name))=>{
            let group=report.groups.iter().find(|g|g.domain==*domain && &g.mechanic==name);
            format!("{} / {}\nSelf p95 per {}: {}. Parent series sum raw samples, not child p95 values.\nBudget status uses the original inclusive measurement; open Budgets to see effective limits and warmup.\nSelect a system for its full path and inclusive/self call timings.",domain.label(),name,report.interval.label(),group.map_or_else(||"not observed".into(),|g|format!("{:.2} us",g.work.p95_us)))
        }
        _=>"Expand a domain, then a mechanic and system. Click a row to select it; Tab/Shift+Tab and Enter/Space work too.\nCosts show recent mean self time per collection interval, including inactive intervals.\nFrame interval includes pacing/waits; parallel CPU spans overlap. GPU time is not measured.".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut state: ResMut<Explorer>,
    panels: Res<DebugPanels>,
    runtime: Res<FrameRuntime>,
    roots: Query<Entity, With<Rows>>,
    mut boxes: Query<&mut Node, With<PerfPanel>>,
    mut detail_boxes: Query<&mut Node, (With<DetailBox>, Without<PerfPanel>)>,
    mut images: ResMut<Assets<Image>>,
    mut texts: Query<
        (&mut Text, Has<Summary>, Has<Notice>, Has<Detail>),
        Or<(With<Summary>, With<Notice>, With<Detail>)>,
    >,
    mut values: Query<
        (&RowValue, &mut Text, &mut TextColor),
        (Without<Summary>, Without<Notice>, Without<Detail>),
    >,
) {
    for mut node in &mut boxes {
        node.display = if panels.perf {
            Display::Flex
        } else {
            Display::None
        };
    }
    if !panels.perf {
        return;
    }
    for mut node in &mut detail_boxes {
        node.display = if state.pane == Pane::History {
            Display::None
        } else {
            Display::Flex
        };
    }
    let info = state
        .report
        .as_ref()
        .and_then(|r| r.runtime.as_ref())
        .or(Some(&runtime.0));
    let title = summary(info, state.report.as_ref(), state.frozen);
    let notice = if state.pane == Pane::History {
        "2 Hz | last 120 s | select a Costs or Budgets row to chart it. Blue = measured; red = peak; dashed gold = target. CPU values are rolling statistics; gaps = unavailable/inactive.".into()
    } else if let Some(r) = &state.report {
        if r.dropped_entries > 0 {
            format!(
                "Capture limit: {} paths, {} entries omitted. Self includes untracked work.",
                r.node_limit, r.dropped_entries
            )
        } else if !r.nodes.iter().any(|n| n.kind == TraceKind::System) {
            "No system spans captured. Enable ECS trace/debug and check span filters; mechanic-only coverage is partial.".into()
        } else if state.pane == Pane::Costs {
            format!(
                "SELF / {} (mean) | SHARE of measured self. Expand > | selected row has full details below.",
                r.interval.label()
            )
        } else {
            format!(
                "MECHANIC | INCLUSIVE p95 us | LIMIT us | STATUS ({} active samples to judge)",
                r.min_samples
            )
        }
    } else {
        "No profiler data available.".into()
    };
    let detail = details(&state);
    for (mut text, is_summary, is_notice, _) in &mut texts {
        let wanted = if is_summary {
            &title
        } else if is_notice {
            &notice
        } else {
            &detail
        };
        if text.0 != *wanted {
            text.0.clone_from(wanted);
        }
    }
    if !state.dirty {
        for (key, mut text, mut color) in &mut values {
            if let Some(row) = state.rows.iter().find(|r| r.key == key.0) {
                text.0 = row_numbers(row);
                color.0 = if row.over { OVER } else { DIM };
            }
        }
        return;
    }
    state.dirty = false;
    if state.pane == Pane::History
        && let Some(history) = state.history.clone()
    {
        for (index, chart) in history.charts.iter().enumerate() {
            let image = crate::perf_charts::image(chart, &history.times_s);
            if let Some(handle) = state.charts.get(index) {
                if let Some(mut asset) = images.get_mut(handle) {
                    *asset = image;
                }
            } else {
                state.charts.push(images.add(image));
            }
        }
    }
    for root in &roots {
        commands
            .entity(root)
            .insert(Node {
                flex_direction: if state.pane == Pane::History {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                flex_wrap: if state.pane == Pane::History {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                },
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                row_gap: px(2),
                column_gap: px(6),
                overflow: Overflow::scroll_y(),
                ..default()
            })
            .despawn_children()
            .with_children(|p| {
                if let Some(report) = &state.report {
                    if state.pane == Pane::History {
                        if let Some(history) = &state.history {
                            if history.times_s.len() < 2 {
                                p.spawn(label(
                                    "Collecting history... charts need two half-second samples.",
                                    12.0,
                                    DIM,
                                ));
                            }
                            for (chart, handle) in history.charts.iter().zip(&state.charts) {
                                let top = crate::perf_charts::ceiling(chart);
                                let legend = chart
                                    .series
                                    .iter()
                                    .map(|s| {
                                        format!(
                                            "{} {}",
                                            s.name,
                                            s.values
                                                .last()
                                                .and_then(|v| *v)
                                                .map_or_else(|| "-".into(), |v| format!("{v:.2}"))
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("   |   ");
                                p.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    flex_shrink: 0.0,
                                    flex_basis: percent(48),
                                    flex_grow: 1.0,
                                    min_width: px(260),
                                    padding: UiRect::all(px(6)),
                                    row_gap: px(3),
                                    ..default()
                                })
                                .with_children(|card| {
                                    card.spawn(label(&chart.title, 12.0, TEXT));
                                    card.spawn(label(
                                        format!(
                                            "{legend} {}   |   vertical scale 0..{top:.1}",
                                            chart.unit
                                        ),
                                        11.0,
                                        DIM,
                                    ));
                                    card.spawn((
                                        ImageNode::new(handle.clone()),
                                        AccessibleLabel::new(format!(
                                            "{}; {}",
                                            chart.title, legend
                                        )),
                                        Node {
                                            width: percent(100),
                                            height: px(100),
                                            flex_shrink: 0.0,
                                            ..default()
                                        },
                                    ));
                                    let first = history.times_s.first().copied().unwrap_or(0.0);
                                    let last = history.times_s.last().copied().unwrap_or(0.0);
                                    card.spawn(label(
                                        format!("Time: {first:.1} - {last:.1} s elapsed (real)"),
                                        10.0,
                                        DIM,
                                    ));
                                });
                            }
                        }
                    } else if state.pane == Pane::Costs {
                        for row in &state.rows {
                            let marker = if row.expandable {
                                if state.expanded.contains(&row.key) {
                                    "v "
                                } else {
                                    "> "
                                }
                            } else {
                                "  "
                            };
                            let id = button(
                                p,
                                &format!("{marker}{}", row.label),
                                Action::Pick(row.key.clone()),
                            );
                            p.commands().entity(id).insert((
                                AccessibleLabel::new(&row.full_name),
                                Node {
                                    width: percent(100),
                                    min_width: px(0),
                                    flex_shrink: 0.0,
                                    column_gap: px(8),
                                    padding: UiRect {
                                        left: px(7.0 + row.depth.min(8) as f32 * 12.0),
                                        right: px(7),
                                        top: px(4),
                                        bottom: px(4),
                                    },
                                    border: UiRect::all(px(1)),
                                    ..default()
                                },
                            ));
                            p.commands().entity(id).with_children(|children| {
                                children.spawn((
                                    RowValue(row.key.clone()),
                                    label(
                                        row_numbers(row),
                                        12.0,
                                        if row.over { OVER } else { DIM },
                                    ),
                                    Node {
                                        margin: UiRect::left(Val::Auto),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                            });
                        }
                    } else {
                        let mut budgets: Vec<_> = report.budgets.iter().collect();
                        budgets.sort_by(|a, b| {
                            b.over_budget
                                .cmp(&a.over_budget)
                                .then(a.mechanic.cmp(&b.mechanic))
                        });
                        for b in budgets.into_iter().take(state.limit) {
                            let status = if b.observed_p95_us.is_none() {
                                "not observed"
                            } else if !b.ready {
                                "warming"
                            } else if b.over_budget {
                                "OVER"
                            } else {
                                "within"
                            };
                            let usage = b.observed_p95_us.map_or_else(
                                || "-".into(),
                                |v| {
                                    if b.budget_us > 0 {
                                        format!("{:.0}%", v / b.budget_us as f64 * 100.0)
                                    } else {
                                        "zero limit".into()
                                    }
                                },
                            );
                            let text = format!(
                                "{:<22} {:>9} / {:>7}   {} {}",
                                clip(&b.mechanic, 22),
                                b.observed_p95_us
                                    .map_or_else(|| "-".into(), |v| format!("{v:.1}")),
                                b.budget_us,
                                usage,
                                status
                            );
                            let id = button(p, &text, Action::Budget(b.mechanic.clone()));
                            p.commands()
                                .entity(id)
                                .insert(BackgroundColor(if b.over_budget {
                                    Color::srgb(0.30, 0.11, 0.12)
                                } else {
                                    BUTTON
                                }));
                        }
                    }
                }
            });
    }
}
fn row_numbers(row: &Row) -> String {
    row.own_us.map_or_else(
        || {
            if matches!(row.key, Key::Reference(..)) {
                "reference".into()
            } else {
                "not observed".into()
            }
        },
        |us| {
            format!(
                "{us:>9.2} us  {:>5.1}%{}",
                row.share,
                if row.over { " !" } else { "" }
            )
        },
    )
}
fn paint(
    mut commands: Commands,
    mut state: ResMut<Explorer>,
    panels: Res<DebugPanels>,
    mut focus: ResMut<InputFocus>,
    mut buttons: Query<(
        Entity,
        &Action,
        &mut BackgroundColor,
        &mut BorderColor,
        &Hovered,
        Has<Pressed>,
        Has<UiSelected>,
    )>,
) {
    if !panels.perf {
        if focus.get().is_some_and(|e| buttons.get(e).is_ok()) {
            focus.clear();
        }
        return;
    }
    let wanted = state.focus_after.take();
    for (entity, action, mut bg, mut border, hovered, pressed, selected) in &mut buttons {
        if wanted.as_ref() == Some(action) {
            focus.set(entity, FocusCause::Navigated);
            commands.trigger(ScrollIntoView { entity });
        }
        let active = match action {
            Action::Pane(p) => state.pane == *p,
            Action::Freeze => state.frozen,
            Action::Pick(key) => state.selected.as_ref() == Some(key),
            Action::Budget(name) => state.budget.as_ref() == Some(name),
            _ => false,
        };
        if active != selected {
            if active {
                commands.entity(entity).insert(UiSelected);
            } else {
                commands.entity(entity).remove::<UiSelected>();
            }
        }
        let over = if let Action::Budget(name) = action {
            state.report.as_ref().is_some_and(|r| {
                r.budgets
                    .iter()
                    .any(|b| b.mechanic == *name && b.over_budget)
            })
        } else {
            false
        };
        let color = if pressed {
            Color::srgb(0.14, 0.17, 0.22)
        } else if hovered.get() {
            Color::srgb(0.27, 0.32, 0.40)
        } else if active {
            Color::srgb(0.36, 0.28, 0.11)
        } else if over {
            Color::srgb(0.30, 0.11, 0.12)
        } else {
            BUTTON
        };
        bg.set_if_neq(BackgroundColor(color));
        border.set_if_neq(BorderColor::all(if focus.get() == Some(entity) {
            ACCENT
        } else {
            Color::NONE
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_distinguishes_frame_sim_rate_pause_and_collection_units() {
        let runtime = RuntimeStats {
            frame_fps: 60.0,
            frame_ms: 16.67,
            sim_tps: Some(20.0),
            sim_target_tps: Some(60.0),
            sim_hz: Some(20),
            multiplier: Some(3),
            frame_target_us: 16667,
            ..default()
        };
        let text = summary(Some(&runtime), None, false);
        assert!(text.contains("60.0 FPS"));
        assert!(text.contains("20.0 ticks/s / 60.0 target"));
        let paused = RuntimeStats {
            paused: true,
            ..runtime
        };
        assert!(summary(Some(&paused), None, true).contains("FROZEN SNAPSHOT"));
        assert!(summary(Some(&paused), None, true).contains("PAUSED"));
        assert!(summary(None, None, false).contains("Profiler not installed"));
    }
    #[test]
    fn costs_are_bounded_and_budget_detail_never_uses_self_as_inclusive_budget() {
        const {
            assert!(REFRESH_SECS >= 0.5);
        }
        let c = npc_perf::PerfCollector::new(16);
        c.set_min_samples(1);
        let mut b = npc_perf::PerfBudgets::default();
        b.set("needs", 1);
        c.set_budgets(b);
        c.record_for_test("npc_needs::decay", 50);
        c.end_frame();
        let report = c.tree_report(None);
        let mut state = Explorer {
            report: Some(report),
            ..default()
        };
        let rows = perf_model::rows(state.report.as_ref().unwrap(), &state.expanded, PAGE);
        assert!(rows.iter().any(|r| r.over));
        assert_eq!(rows.len(), 5);
        state.pane = Pane::Budgets;
        state.budget = Some("needs".into());
        let detail = details(&state);
        assert!(detail.contains("50.00 us"));
        assert!(detail.contains("not per call"));
    }

    fn app() -> (App, npc_perf::PerfCollector) {
        let c = npc_perf::PerfCollector::new(16);
        c.record_for_test("npc_needs::decay", 50);
        c.end_frame();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::text::TextPlugin,
        ))
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(DebugPanels {
            perf: true,
            ..default()
        })
        .insert_resource(PerfHandle(c.clone()))
        .insert_resource(npc_kernel::SimClock { tick: 100 })
        .insert_resource(npc_kernel::SimControl::default())
        .configure_sets(
            Update,
            (
                ViewSystems::OverlayInput,
                ViewSystems::Input,
                ViewSystems::Present,
            )
                .chain(),
        )
        .add_plugins((crate::perf_runtime::plugin, plugin))
        .add_systems(
            PostUpdate,
            |mut nodes: Query<(&Node, &mut InheritedVisibility)>| {
                for (node, mut visible) in &mut nodes {
                    *visible = if node.display == Display::None {
                        InheritedVisibility::HIDDEN
                    } else {
                        InheritedVisibility::VISIBLE
                    };
                }
            },
        );
        app.update();
        (app, c)
    }
    fn press(app: &mut App, action: Action) {
        let entity = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, a)| **a == action)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
        app.update();
    }
    #[test]
    fn actual_widgets_expand_freeze_drill_reset_and_preserve_simulation() {
        let (mut app, c) = app();
        let control = *app.world().resource::<npc_kernel::SimControl>();
        assert_eq!(app.world().resource::<Explorer>().rows.len(), 5);
        press(
            &mut app,
            Action::Pick(Key::Domain(npc_perf::PerfDomain::Simulation)),
        );
        press(
            &mut app,
            Action::Pick(Key::Group(npc_perf::PerfDomain::Simulation, "needs".into())),
        );
        let id = app
            .world()
            .resource::<Explorer>()
            .report
            .as_ref()
            .unwrap()
            .nodes[0]
            .id;
        press(&mut app, Action::Freeze);
        let frozen = app
            .world()
            .resource::<Explorer>()
            .report
            .as_ref()
            .unwrap()
            .clone();
        assert!(!frozen.details.is_empty());
        c.record_for_test("npc_needs::decay", 100);
        c.end_frame();
        app.update();
        assert_eq!(
            *app.world().resource::<Explorer>().report.as_ref().unwrap(),
            frozen
        );
        press(&mut app, Action::Pick(Key::Trace(id)));
        assert!(details(app.world().resource::<Explorer>()).contains("Per call inclusive"));
        press(&mut app, Action::Reset);
        assert!(!app.world().resource::<Explorer>().frozen);
        assert_eq!(c.report().frames, 0);
        assert_eq!(*app.world().resource::<npc_kernel::SimControl>(), control);
        assert_eq!(app.world().resource::<npc_kernel::SimClock>().tick, 100);
    }
    #[test]
    fn history_widgets_follow_selection_freeze_and_reuse_four_image_assets() {
        let (mut app, c) = app();
        c.record_history(10.0, Some(22.0));
        c.record_history(10.5, Some(26.0));
        press(
            &mut app,
            Action::Pick(Key::Domain(npc_perf::PerfDomain::Simulation)),
        );
        press(&mut app, Action::Pane(Pane::History));
        let state = app.world().resource::<Explorer>();
        assert_eq!(state.history.as_ref().unwrap().times_s.len(), 2);
        assert!(
            state.history.as_ref().unwrap().charts[2]
                .title
                .contains("Simulation")
        );
        assert_eq!(state.charts.len(), 4);
        let handles = state.charts.clone();
        press(&mut app, Action::Freeze);
        let frozen = app.world().resource::<Explorer>().history.clone();
        c.record_history(11.0, Some(40.0));
        app.update();
        assert_eq!(app.world().resource::<Explorer>().history, frozen);
        press(&mut app, Action::Freeze);
        assert_eq!(
            app.world()
                .resource::<Explorer>()
                .history
                .as_ref()
                .unwrap()
                .times_s
                .len(),
            3
        );
        assert_eq!(app.world().resource::<Explorer>().charts, handles);
        press(&mut app, Action::Reset);
        assert!(
            app.world()
                .resource::<Explorer>()
                .history
                .as_ref()
                .unwrap()
                .times_s
                .is_empty()
        );
    }
    #[derive(Resource, Default)]
    struct InputRuns(u32);
    #[test]
    fn native_focus_shields_world_keys_and_closed_overlay_releases_them() {
        let (mut app, _) = app();
        app.init_resource::<InputRuns>().add_systems(
            Update,
            (|mut count: ResMut<InputRuns>| count.0 += 1).in_set(ViewSystems::Input),
        );
        app.update();
        let before = app.world().resource::<InputRuns>().0;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert_eq!(app.world().resource::<InputRuns>().0, before);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<DebugPanels>().perf);
        assert!(app.world().resource::<InputFocus>().get().is_none());
        assert!(app.world().resource::<InputRuns>().0 > before);
    }
    #[test]
    fn labels_keep_button_roles_and_unchanged_refresh_keeps_focus() {
        use bevy::a11y::AccessibilityNode;
        let (mut app, _) = app();
        let bare = app.world_mut().spawn(WidgetButton).id();
        let role = app.world().get::<AccessibilityNode>(bare).unwrap().role();
        let focus = app.world().resource::<InputFocus>().get();
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), focus);
        for (entity, _) in app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
        {
            assert_eq!(
                app.world().get::<AccessibilityNode>(entity).unwrap().role(),
                role
            );
            assert!(
                !app.world()
                    .get::<AccessibleLabel>(entity)
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
    }
}
