//! Native, read-only session history inside the existing F10 controls.
use super::{Action as ClientAction, Panel};
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::LineBreak;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea};
use island_content::explorer::ai_history::{self, EntryKind, HistoryEntry, Page};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(2);
type ReplySlot = Arc<Mutex<Option<Reply>>>;

struct Pending {
    slot: ReplySlot,
    started: Instant,
}
struct Reply {
    workspace: PathBuf,
    generation: u64,
    result: Result<Snapshot, String>,
}
struct Snapshot {
    entries: Vec<HistoryEntry>,
    selected: Option<String>,
    page: Option<Page>,
    message: String,
    error: bool,
}

pub(super) struct State {
    pub open: bool,
    was_open: bool,
    tab: EntryKind,
    entries: Vec<HistoryEntry>,
    selected: Option<String>,
    log_selection: Option<String>,
    metrics_selection: Option<String>,
    page: Option<Page>,
    offset: u64,
    page_number: usize,
    follow_latest: bool,
    message: String,
    error: bool,
    generation: u64,
    dirty: bool,
    job: Option<Pending>,
    last_refresh: Instant,
    return_focus: Option<Entity>,
    reset_scroll: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            was_open: false,
            tab: EntryKind::Log,
            entries: Vec::new(),
            selected: None,
            log_selection: None,
            metrics_selection: None,
            page: None,
            offset: 0,
            page_number: 1,
            follow_latest: true,
            message: "Loading sessions...".into(),
            error: false,
            generation: 0,
            dirty: true,
            job: None,
            last_refresh: Instant::now(),
            return_focus: None,
            reset_scroll: true,
        }
    }
}
impl State {
    pub fn open(&mut self, return_focus: Entity) {
        self.open = true;
        self.return_focus = Some(return_focus);
        self.invalidate(false);
    }
    pub fn close(&mut self) {
        self.open = false;
        self.generation = self.generation.wrapping_add(1);
    }
    fn invalidate(&mut self, clear: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.dirty = true;
        self.error = false;
        if clear {
            self.page = None;
            self.reset_scroll = true;
        }
    }
    fn records(&self) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.kind == self.tab)
            .collect()
    }
    fn position(&self) -> Option<usize> {
        self.records()
            .iter()
            .position(|entry| Some(&entry.key) == self.selected.as_ref())
    }
    fn select(&mut self, key: Option<String>) {
        self.selected = key;
        match self.tab {
            EntryKind::Log => self.log_selection = self.selected.clone(),
            EntryKind::Metrics => self.metrics_selection = self.selected.clone(),
        }
        self.offset = 0;
        self.page_number = 1;
        self.follow_latest = self.tab == EntryKind::Log;
        self.invalidate(true);
    }
    fn switch_tab(&mut self, tab: EntryKind) {
        if self.tab == tab {
            return;
        }
        let linked = self
            .entries
            .iter()
            .find(|entry| Some(&entry.key) == self.selected.as_ref())
            .and_then(|entry| entry.metrics_key.clone());
        self.tab = tab;
        let key = match tab {
            EntryKind::Log => self.log_selection.clone(),
            EntryKind::Metrics => linked.or_else(|| self.metrics_selection.clone()),
        };
        self.select(key);
    }
    fn enabled(&self, action: Action) -> bool {
        match action {
            Action::Newer => self.position().is_some_and(|position| position > 0),
            Action::Older => self
                .position()
                .is_some_and(|position| position + 1 < self.records().len()),
            Action::Earlier => {
                self.job.is_none()
                    && self
                        .page
                        .as_ref()
                        .is_some_and(|page| page.previous_offset.is_some())
            }
            Action::Later => {
                self.job.is_none()
                    && self
                        .page
                        .as_ref()
                        .is_some_and(|page| page.next_offset.is_some())
            }
            Action::Refresh => self.job.is_none(),
            Action::Latest => self.tab == EntryKind::Log && self.selected.is_some(),
            _ => true,
        }
    }
    fn act(&mut self, action: Action) {
        if !self.enabled(action) {
            return;
        }
        match action {
            Action::Back => self.close(),
            Action::Logs => self.switch_tab(EntryKind::Log),
            Action::Metrics => self.switch_tab(EntryKind::Metrics),
            Action::Refresh => self.invalidate(false),
            Action::Latest => {
                self.follow_latest = !self.follow_latest;
                self.offset = self.page.as_ref().map_or(0, |page| page.offset);
                self.invalidate(self.follow_latest);
            }
            Action::Newer | Action::Older => {
                let records = self.records();
                if let Some(position) = self.position() {
                    let next = if action == Action::Newer {
                        position - 1
                    } else {
                        position + 1
                    };
                    let key = records.get(next).map(|entry| entry.key.clone());
                    self.select(key);
                }
            }
            Action::Earlier | Action::Later => {
                let next = self.page.as_ref().and_then(|page| {
                    if action == Action::Earlier {
                        page.previous_offset
                    } else {
                        page.next_offset
                    }
                });
                if let Some(offset) = next {
                    self.follow_latest = false;
                    self.offset = offset;
                    self.page_number = if action == Action::Earlier {
                        self.page_number.saturating_sub(1).max(1)
                    } else {
                        self.page_number.saturating_add(1)
                    };
                    self.invalidate(true);
                }
            }
        }
    }
    fn accept(&mut self, reply: Reply, workspace: &Path) {
        if reply.generation != self.generation || reply.workspace != workspace {
            return;
        }
        match reply.result {
            Ok(snapshot) => {
                if self.follow_latest
                    && self.tab == EntryKind::Log
                    && self.page.as_ref().map(|page| (&page.text, page.offset))
                        != snapshot.page.as_ref().map(|page| (&page.text, page.offset))
                {
                    self.reset_scroll = true;
                }
                if snapshot.selected != self.selected {
                    self.reset_scroll = true;
                    self.page_number = 1;
                }
                self.entries = snapshot.entries;
                self.selected = snapshot.selected;
                match self.tab {
                    EntryKind::Log => self.log_selection = self.selected.clone(),
                    EntryKind::Metrics => self.metrics_selection = self.selected.clone(),
                }
                self.page = snapshot.page;
                if let Some(page) = &self.page {
                    self.offset = page.offset;
                    self.page_number = (page.offset / ai_history::PAGE_BYTES as u64 + 1) as usize;
                }
                self.message = snapshot.message;
                self.error = snapshot.error;
            }
            Err(error) => {
                self.message = error;
                self.error = true;
            }
        }
    }
}

#[derive(Component, Clone, Copy)]
pub(super) enum Container {
    Controls,
    Viewer,
    Body,
}
#[derive(Component, Clone, Copy)]
enum Section {
    Record,
    Position,
    Body,
    Status,
}
#[derive(Component)]
struct TextBody;
#[derive(Resource)]
struct HistoryFont(Handle<Font>);

fn load_history_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        // Embed the existing licensed workspace font so log punctuation and
        // Latin text do not depend on the ASCII-only default font or an I/O race.
        let font = Font::from_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/Barlow-Regular.ttf"
            ))
            .to_vec(),
        );
        commands.insert_resource(HistoryFont(fonts.add(font)));
    }
}

fn apply_history_font(font: Option<Res<HistoryFont>>, mut text: Query<(&Section, &mut TextFont)>) {
    let Some(font) = font else {
        return;
    };
    for (section, mut text_font) in &mut text {
        if matches!(section, Section::Body) {
            text_font.font = bevy::text::FontSource::Handle(font.0.clone());
        }
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Back,
    Logs,
    Metrics,
    Refresh,
    Newer,
    Older,
    Earlier,
    Later,
    Latest,
}

pub(super) fn install(app: &mut App) {
    app.add_systems(PreStartup, load_history_font)
        .add_systems(Startup, apply_history_font.after(super::spawn))
        .add_systems(
            Update,
            (poll, paint, scroll_text, paint_text_focus)
                .chain()
                .after(super::refresh)
                .run_if(not(crate::explorer::close_dialog_open)),
        );
}

fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action, tab: i32) {
    parent
        .spawn((
            Name::new(format!("AI sessions {label}")),
            action,
            WidgetButton,
            Hovered::default(),
            TabIndex(tab),
            AccessibleLabel::new(label),
            Node {
                padding: UiRect::axes(px(14), px(8)),
                min_height: px(40),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.20, 0.24, 0.30)),
            BorderColor::all(Color::srgb(0.48, 0.55, 0.65)),
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ))
        .observe(activate);
}
fn text(parent: &mut ChildSpawnerCommands, section: Section, size: f32, color: Color) {
    parent.spawn((
        section,
        Text::new(""),
        TextFont::from_font_size(size),
        TextColor(color),
        TextLayout::linebreak(LineBreak::WordOrCharacter),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
}
pub(super) fn spawn(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Container::Viewer,
            Name::new("AI sessions viewer"),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                width: percent(94),
                max_width: px(960),
                height: percent(92),
                padding: UiRect::all(px(16)),
                row_gap: px(8),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollArea,
            BackgroundColor(Color::srgb(0.08, 0.09, 0.11)),
            BorderColor::all(Color::srgb(0.35, 0.40, 0.48)),
        ))
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: px(12),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("AI sessions"),
                    TextFont::from_font_size(24.0),
                    TextColor(Color::srgb(0.97, 0.95, 0.85)),
                ));
                button(row, "Back to agent controls", Action::Back, 0);
            });
            p.spawn((
                Text::new("Private session logs and accounting for this actor."),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.77, 0.81, 0.87)),
                Node {
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            p.spawn(Node {
                column_gap: px(8),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
                button(row, "Session logs", Action::Logs, 1);
                button(row, "Metrics", Action::Metrics, 2);
                button(row, "Refresh", Action::Refresh, 3);
            });
            text(p, Section::Record, 14.0, Color::srgb(0.95, 0.82, 0.53));
            p.spawn(Node {
                column_gap: px(8),
                row_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
                button(row, "Newer session", Action::Newer, 4);
                button(row, "Older session", Action::Older, 5);
            });
            p.spawn(Node {
                column_gap: px(8),
                row_gap: px(6),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
                button(row, "Earlier text", Action::Earlier, 6);
                button(row, "Later text", Action::Later, 7);
                button(row, "Follow latest", Action::Latest, 8);
                text(row, Section::Position, 13.0, Color::srgb(0.77, 0.81, 0.87));
            });
            p.spawn((
                Container::Body,
            TextBody,
            TabIndex(9),
            AccessibleLabel::new("Session text. Arrow keys and Page Up or Page Down scroll. Home and End jump within this page."),
                ScrollArea,
                Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    min_height: px(96),
                    overflow: Overflow::scroll_y(),
                    padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.04, 0.05, 0.06)),
            BorderColor::all(Color::srgb(0.35,0.40,0.48)),
            ))
            .with_children(|body| {
                text(body, Section::Body, 13.0, Color::srgb(0.88, 0.90, 0.92));
            });
            text(p, Section::Status, 12.0, Color::srgb(0.77, 0.81, 0.87));
        });
}

fn scroll_text(
    mut panel: ResMut<Panel>,
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    mut bodies: Query<(Entity, &ComputedNode, &mut ScrollPosition), With<TextBody>>,
) {
    if !panel.open || !panel.history.open {
        return;
    }
    let Some(focused) = focus.and_then(|focus| focus.get()) else {
        return;
    };
    let Ok((_, computed, mut scroll)) = bodies.get_mut(focused) else {
        return;
    };
    let viewport = computed.size().y * computed.inverse_scale_factor;
    let maximum = (computed.content_size().y * computed.inverse_scale_factor - viewport).max(0.0);
    let current = scroll.y.clamp(0.0, maximum);
    let next = if keys.just_pressed(KeyCode::Home) {
        Some(0.0)
    } else if keys.just_pressed(KeyCode::End) {
        Some(maximum)
    } else if keys.just_pressed(KeyCode::PageUp) {
        Some(current - viewport * 0.9)
    } else if keys.just_pressed(KeyCode::PageDown) {
        Some(current + viewport * 0.9)
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(current - 24.0)
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(current + 24.0)
    } else {
        None
    };
    if let Some(next) = next {
        scroll.y = next.clamp(0.0, maximum);
        if panel.history.follow_latest {
            // Manual reading must not be moved by an already-running tail refresh.
            panel.history.follow_latest = false;
            panel.history.invalidate(false);
        }
    }
}

fn paint_text_focus(
    panel: Res<Panel>,
    focus: Option<Res<InputFocus>>,
    mut bodies: Query<(Entity, &mut BorderColor), With<TextBody>>,
) {
    for (entity, mut border) in &mut bodies {
        *border = BorderColor::all(
            if panel.open
                && panel.history.open
                && focus
                    .as_ref()
                    .is_some_and(|focus| focus.get() == Some(entity))
            {
                Color::srgb(0.95, 0.80, 0.45)
            } else {
                Color::srgb(0.35, 0.40, 0.48)
            },
        );
    }
}

fn activate(
    event: On<Activate>,
    actions: Query<&Action>,
    mut panel: ResMut<Panel>,
    dialog: Option<Res<crate::explorer::CloseDialog>>,
) {
    if !panel.open || !panel.history.open || dialog.is_some_and(|dialog| dialog.open) {
        return;
    }
    if let Ok(action) = actions.get(event.entity) {
        panel.history.act(*action);
    }
}

fn load(
    workspace: &Path,
    tab: EntryKind,
    selected: Option<&str>,
    offset: u64,
) -> Result<Snapshot, String> {
    let history = ai_history::list(workspace)?;
    let requested = history
        .entries
        .iter()
        .find(|entry| entry.kind == tab && Some(entry.key.as_str()) == selected);
    let selected_unavailable = selected.is_some() && requested.is_none();
    let entry = requested.or_else(|| history.entries.iter().find(|entry| entry.kind == tab));
    let key = entry.map(|entry| entry.key.clone());
    let offset = if key.as_deref() == selected {
        offset
    } else if tab == EntryKind::Log {
        ai_history::LATEST_OFFSET
    } else {
        0
    };
    let mut message = history.message;
    if selected_unavailable && key.is_some() {
        message =
            "The selected entry is unavailable. Showing the newest available session in this tab."
                .into();
    }
    let mut error = false;
    let page = if let Some(key) = &key {
        match ai_history::read(workspace, key, offset) {
            Ok(page) => Some(page),
            Err(problem) => {
                message = problem;
                error = true;
                None
            }
        }
    } else {
        None
    };
    Ok(Snapshot {
        entries: history.entries,
        selected: key,
        page,
        message,
        error,
    })
}

fn poll(mut panel: ResMut<Panel>) {
    let reply = panel
        .history
        .job
        .as_ref()
        .and_then(|job| job.slot.try_lock().ok().and_then(|mut slot| slot.take()));
    if let Some(reply) = reply {
        panel.history.job = None;
        let workspace = panel.dir.dir.clone();
        panel.history.accept(reply, &workspace);
    }
    if !panel.open || !panel.history.open || panel.history.job.is_some() {
        return;
    }
    if !panel.history.dirty && panel.history.last_refresh.elapsed() < REFRESH {
        return;
    }
    panel.history.dirty = false;
    panel.history.last_refresh = Instant::now();
    let workspace = panel.dir.dir.clone();
    let generation = panel.history.generation;
    let tab = panel.history.tab;
    let selected = panel.history.selected.clone();
    let offset = if panel.history.tab == EntryKind::Log && panel.history.follow_latest {
        ai_history::LATEST_OFFSET
    } else {
        panel.history.offset
    };
    let slot: ReplySlot = Arc::new(Mutex::new(None));
    let output = slot.clone();
    let spawned = std::thread::Builder::new()
        .name("npc-session-history".into())
        .spawn(move || {
            let result =
                std::panic::catch_unwind(|| load(&workspace, tab, selected.as_deref(), offset))
                    .unwrap_or_else(|_| {
                        Err("Session history could not be read. Try Refresh.".into())
                    });
            if let Ok(mut reply) = output.lock() {
                *reply = Some(Reply {
                    workspace,
                    generation,
                    result,
                });
            }
        });
    match spawned {
        Ok(_) => {
            panel.history.job = Some(Pending {
                slot,
                started: Instant::now(),
            })
        }
        Err(_) => {
            panel.history.message = "Session history could not start loading. Try Refresh.".into();
            panel.history.error = true;
        }
    }
}

type Buttons<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Action,
        &'static Hovered,
        Has<Pressed>,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
>;

fn paint(
    mut panel: ResMut<Panel>,
    mut commands: Commands,
    mut containers: Query<(Entity, &mut Node, &Container)>,
    mut labels: Query<(&mut Text, &mut TextColor, &Section)>,
    mut buttons: Buttons,
    controls: Query<(Entity, &ClientAction)>,
    mut focus: Option<ResMut<InputFocus>>,
) {
    let open = panel.open && panel.history.open;
    if open != panel.history.was_open {
        if let Some(focus) = focus.as_mut() {
            if open {
                if let Some((entity, ..)) = buttons
                    .iter()
                    .find(|(_, action, ..)| **action == Action::Back)
                {
                    focus.set(entity, FocusCause::Navigated);
                }
            } else if panel.open {
                let return_focus = panel
                    .history
                    .return_focus
                    .take()
                    .filter(|entity| controls.get(*entity).is_ok());
                if let Some(entity) = return_focus.or_else(|| {
                    controls
                        .iter()
                        .find(|(_, action)| matches!(action, ClientAction::History))
                        .map(|(entity, _)| entity)
                }) {
                    focus.set(entity, FocusCause::Navigated);
                } else {
                    focus.clear();
                }
            }
        }
        panel.history.was_open = open;
    }
    let reset_scroll = panel.history.reset_scroll;
    for (entity, mut node, container) in &mut containers {
        match container {
            Container::Controls => {
                node.display = if panel.open && !open {
                    Display::Flex
                } else {
                    Display::None
                }
            }
            Container::Viewer => node.display = if open { Display::Flex } else { Display::None },
            Container::Body => {
                if reset_scroll {
                    let position =
                        if panel.history.follow_latest && panel.history.tab == EntryKind::Log {
                            // Bevy's layout clamps this to the measured scroll range.
                            ScrollPosition(Vec2::new(0.0, f32::MAX))
                        } else {
                            ScrollPosition::default()
                        };
                    commands.entity(entity).insert(position);
                }
            }
        }
    }
    panel.history.reset_scroll = false;
    let records = panel.history.records();
    let record = records
        .iter()
        .find(|entry| Some(&entry.key) == panel.history.selected.as_ref());
    let title = record.map_or_else(
        || {
            if panel.history.job.is_some() {
                "Loading session history...".into()
            } else if panel.history.error {
                "Session history unavailable".into()
            } else {
                match panel.history.tab {
                    EntryKind::Log => "No session logs yet".into(),
                    EntryKind::Metrics => "No session metrics yet".into(),
                }
            }
        },
        |entry| entry.label.clone(),
    );
    let position = panel.history.position().map_or_else(String::new, |index| {
        format!(
            "Session {} of {} | Page {}",
            index + 1,
            records.len(),
            panel.history.page_number
        )
    });
    let body = if let Some(page) = &panel.history.page {
        format!("{}\n\n{}", page.title, page.text)
    } else if panel.history.job.is_some() {
        "Loading session history...".into()
    } else if panel.history.error {
        panel.history.message.clone()
    } else {
        match panel.history.tab {
            EntryKind::Log => "Start an AI agent to record its conversation here. Earlier sessions stay available after a restart.".into(),
            EntryKind::Metrics => "Session accounting appears when the AI client prepares a conversation. Costs and actual model details can be unavailable when the CLI does not report them.".into(),
        }
    };
    let status = if panel
        .history
        .job
        .as_ref()
        .is_some_and(|job| job.started.elapsed() > Duration::from_secs(15))
    {
        "Still loading. You can return to the world at any time.".into()
    } else if panel.history.job.is_some() {
        "Refreshing...".into()
    } else if !panel.history.message.is_empty() {
        panel.history.message.clone()
    } else {
        format!(
            "Auto-refresh every 2 seconds | Client: {}",
            if panel.status.state.is_empty() {
                "not started"
            } else {
                &panel.status.state
            }
        )
    };
    for (mut text, mut color, section) in &mut labels {
        let value = match section {
            Section::Record => &title,
            Section::Position => &position,
            Section::Body => &body,
            Section::Status => &status,
        };
        if text.0 != *value {
            text.0 = value.clone();
        }
        if matches!(section, Section::Status) {
            color.0 = if panel.history.error {
                Color::srgb(0.96, 0.68, 0.58)
            } else {
                Color::srgb(0.77, 0.81, 0.87)
            };
        }
    }
    for (entity, action, hovered, pressed, mut background, mut border) in &mut buttons {
        let enabled = panel.history.enabled(*action);
        if enabled {
            commands
                .entity(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        } else {
            commands
                .entity(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
        let selected = matches!(
            (*action, panel.history.tab),
            (Action::Logs, EntryKind::Log) | (Action::Metrics, EntryKind::Metrics)
        ) || (*action == Action::Latest
            && panel.history.follow_latest
            && panel.history.tab == EntryKind::Log);
        background.0 = if !enabled {
            Color::srgb(0.12, 0.13, 0.15)
        } else if pressed {
            Color::srgb(0.14, 0.17, 0.22)
        } else if hovered.0 || selected {
            Color::srgb(0.27, 0.32, 0.40)
        } else {
            Color::srgb(0.20, 0.24, 0.30)
        };
        *border = BorderColor::all(
            if focus
                .as_ref()
                .is_some_and(|focus| focus.get() == Some(entity))
                || selected
            {
                Color::srgb(0.95, 0.80, 0.45)
            } else {
                Color::srgb(0.48, 0.55, 0.65)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(key: &str, kind: EntryKind) -> HistoryEntry {
        HistoryEntry {
            key: key.into(),
            label: key.into(),
            kind,
            metrics_key: None,
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            entries: vec![
                entry("log:new", EntryKind::Log),
                entry("log:old", EntryKind::Log),
                entry("metrics:one", EntryKind::Metrics),
            ],
            selected: Some("log:new".into()),
            page: Some(Page {
                title: "New session".into(),
                text: "Readable conversation".into(),
                offset: 0,
                next_offset: Some(ai_history::PAGE_BYTES as u64),
                previous_offset: None,
            }),
            message: String::new(),
            error: false,
        }
    }
    #[test]
    fn navigation_is_scoped_to_tabs_and_disables_boundary_actions() {
        let mut state = State::default();
        let workspace = Path::new("owned");
        state.accept(
            Reply {
                workspace: workspace.into(),
                generation: 0,
                result: Ok(snapshot()),
            },
            workspace,
        );
        assert!(!state.enabled(Action::Newer));
        assert!(state.enabled(Action::Older));
        assert!(!state.enabled(Action::Earlier));
        assert!(state.enabled(Action::Later));
        state.act(Action::Older);
        assert_eq!(state.selected.as_deref(), Some("log:old"));
        assert!(!state.enabled(Action::Older));
        assert!(state.enabled(Action::Newer));
        assert!(state.page.is_none());
        state.act(Action::Metrics);
        assert_eq!(state.tab, EntryKind::Metrics);
        assert_eq!(state.offset, 0);
    }
    #[test]
    fn late_worker_results_cannot_replace_a_new_selection_or_another_workspace() {
        let mut state = State {
            generation: 3,
            message: "current".into(),
            ..Default::default()
        };
        let workspace = Path::new("owned");
        state.accept(
            Reply {
                workspace: workspace.into(),
                generation: 2,
                result: Ok(snapshot()),
            },
            workspace,
        );
        state.accept(
            Reply {
                workspace: PathBuf::from("other"),
                generation: 3,
                result: Ok(snapshot()),
            },
            workspace,
        );
        assert!(state.entries.is_empty());
        assert_eq!(state.message, "current");
    }

    #[test]
    fn linked_metrics_and_live_page_navigation_keep_the_selected_conversation() {
        let mut state = State::default();
        let workspace = Path::new("owned");
        let mut data = snapshot();
        data.entries[0].metrics_key = Some("metrics:one".into());
        state.accept(
            Reply {
                workspace: workspace.into(),
                generation: 0,
                result: Ok(data),
            },
            workspace,
        );
        assert!(state.follow_latest);
        state.act(Action::Later);
        assert!(!state.follow_latest);
        assert_eq!(state.offset, ai_history::PAGE_BYTES as u64);
        state.act(Action::Latest);
        assert!(state.follow_latest);
        state.act(Action::Metrics);
        assert_eq!(state.tab, EntryKind::Metrics);
        assert_eq!(state.selected.as_deref(), Some("metrics:one"));
        assert!(!state.enabled(Action::Latest));
        state.act(Action::Logs);
        assert_eq!(state.selected.as_deref(), Some("log:new"));
        assert!(state.follow_latest);
    }

    fn view_app() -> App {
        let mut app = super::super::tests::app(PathBuf::new());
        app.init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Startup, super::super::spawn)
            .add_systems(
                Update,
                (
                    super::super::toggle,
                    super::super::refresh,
                    paint,
                    scroll_text,
                    paint_text_focus,
                )
                    .chain(),
            );
        app
    }
    fn client_action(app: &mut App, action: ClientAction) -> Entity {
        let entity = app
            .world_mut()
            .query::<(Entity, &ClientAction)>()
            .iter(app.world())
            .find(|(_, candidate)| {
                std::mem::discriminant(*candidate) == std::mem::discriminant(&action)
            })
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
        entity
    }

    #[test]
    fn history_is_native_read_only_and_escape_returns_to_the_agent_controls() {
        let mut app = view_app();
        app.update();
        let session_button = client_action(&mut app, ClientAction::History);
        app.update();
        let panel = app.world().resource::<Panel>();
        assert!(panel.open && panel.history.open);
        assert!(
            app.world()
                .resource::<npc_framework::prelude::SimControl>()
                .paused
        );
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(app.world().get::<Action>(focused), Some(&Action::Back));
        // Hidden controls cannot close the viewer or resume the world.
        client_action(&mut app, ClientAction::Close);
        app.update();
        assert!(app.world().resource::<Panel>().history.open);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let panel = app.world().resource::<Panel>();
        assert!(panel.open && !panel.history.open);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(session_button)
        );
        assert!(
            app.world()
                .resource::<npc_framework::prelude::SimControl>()
                .paused
        );
    }

    #[test]
    fn focused_session_text_scrolls_in_logical_pixels_and_ignores_other_focus() {
        let mut app = view_app();
        app.update();
        client_action(&mut app, ClientAction::History);
        app.update();
        let body = app
            .world_mut()
            .query_filtered::<Entity, With<TextBody>>()
            .single(app.world())
            .unwrap();
        {
            let mut computed = app.world_mut().get_mut::<ComputedNode>(body).unwrap();
            computed.size = Vec2::new(200.0, 200.0);
            computed.content_size = Vec2::new(200.0, 1000.0);
            computed.inverse_scale_factor = 0.5;
        }
        app.world_mut().get_mut::<ScrollPosition>(body).unwrap().y = f32::MAX;
        // Back owns focus initially, so navigation keys do not scroll the text.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, f32::MAX);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(body, FocusCause::Navigated);
        let generation = app.world().resource::<Panel>().history.generation;
        for (key, expected) in [
            (KeyCode::ArrowUp, 376.0),
            (KeyCode::ArrowDown, 400.0),
            (KeyCode::PageUp, 310.0),
            (KeyCode::PageDown, 400.0),
            (KeyCode::Home, 0.0),
            (KeyCode::End, 400.0),
        ] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, expected);
        }
        let panel = app.world().resource::<Panel>();
        assert!(!panel.history.follow_latest);
        assert!(
            panel.history.generation > generation,
            "manual reading rejects any pending follow-latest reply"
        );
        assert_eq!(
            *app.world().get::<BorderColor>(body).unwrap(),
            BorderColor::all(Color::srgb(0.95, 0.80, 0.45))
        );
        assert_eq!(app.world().get::<TabIndex>(body).unwrap().0, 9);
        app.world_mut().resource_mut::<Panel>().history.close();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Home);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(body).unwrap().y, 400.0);
    }

    #[test]
    fn transcript_uses_the_embedded_font_without_rewriting_unicode_content() {
        let mut app = view_app();
        app.init_resource::<Assets<Font>>()
            .add_systems(PreStartup, load_history_font)
            .add_systems(Startup, apply_history_font.after(super::super::spawn));
        app.world_mut().resource_mut::<Panel>().history.page = Some(Page {
            title: "Client session".into(),
            text: "é — preserved Markdown".into(),
            offset: 0,
            next_offset: None,
            previous_offset: None,
        });
        app.update();
        let font = app.world().resource::<HistoryFont>().0.id();
        let mut labels = app.world_mut().query::<(&Section, &Text, &TextFont)>();
        for (section, text, text_font) in labels.iter(app.world()) {
            let uses_history_font = matches!(&text_font.font,bevy::text::FontSource::Handle(handle) if handle.id()==font);
            if matches!(section, Section::Body) {
                assert!(uses_history_font);
                assert!(text.0.contains("é — preserved Markdown"));
            } else {
                assert!(
                    !uses_history_font,
                    "the existing control typography stays unchanged"
                );
            }
        }
    }

    #[test]
    fn history_return_and_f10_close_restore_notebook_focus_and_intentional_pause() {
        for f10 in [false, true] {
            let mut app = view_app();
            {
                let mut panel = app.world_mut().resource_mut::<Panel>();
                panel.open = false;
                panel.onboarding = false;
            }
            let draft = app
                .world_mut()
                .spawn((
                    Node::default(),
                    bevy::text::EditableText::new("unsaved notebook"),
                ))
                .id();
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(draft, FocusCause::Navigated);
            app.update();
            app.world_mut().resource_mut::<Panel>().open = true;
            app.update();
            client_action(&mut app, ClientAction::History);
            app.update();
            if f10 {
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .press(KeyCode::F10);
                app.update();
            } else {
                let back = app
                    .world_mut()
                    .query::<(Entity, &Action)>()
                    .iter(app.world())
                    .find(|(_, action)| **action == Action::Back)
                    .unwrap()
                    .0;
                app.world_mut().trigger(Activate { entity: back });
                app.world_mut().flush();
                app.update();
                client_action(&mut app, ClientAction::Close);
                app.update();
            }
            assert!(!app.world().resource::<Panel>().open);
            assert!(!app.world().resource::<Panel>().history.open);
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(draft));
            assert!(
                app.world()
                    .resource::<npc_framework::prelude::SimControl>()
                    .paused
            );
            assert_eq!(
                app.world()
                    .get::<bevy::text::EditableText>(draft)
                    .unwrap()
                    .value(),
                "unsaved notebook"
            );
        }
    }
}
