//! Read-only native session browser. Disk access stays on one bounded worker.
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::LineBreak;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, ScrollArea};
use testbed_content::explorer::{
    ai_history::{self, EntryKind, History, HistoryEntry, Page},
    home::slug,
};

use crate::{AppSettings, detached::Rx, run, screens::Screen, ui};

const REFRESH: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Metrics,
    Logs,
}
impl Tab {
    fn kind(self) -> EntryKind {
        match self {
            Self::Metrics => EntryKind::Metrics,
            Self::Logs => EntryKind::Log,
        }
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    history: History,
    entry: Option<HistoryEntry>,
    page: Option<Page>,
}

#[derive(Resource)]
struct Browser {
    agent: String,
    workspace: PathBuf,
    origin: Screen,
    tab: Tab,
    key: Option<String>,
    offset: u64,
    generation: u64,
    job: Option<(u64, Rx<Result<Snapshot, String>>)>,
    snapshot: Option<Snapshot>,
    error: Option<String>,
    dirty: bool,
    changed: bool,
    last: Duration,
}

pub struct AgentHistoryPlugin;
#[derive(Resource)]
struct HistoryFont(Handle<Font>);
fn load_history_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
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
impl Plugin for AgentHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_history_font)
            .add_systems(OnEnter(Screen::AgentHistory), spawn)
            .add_systems(OnExit(Screen::AgentHistory), clear_focus)
            .add_systems(
                Update,
                (poll, refresh, scroll_focus)
                    .chain()
                    .run_if(in_state(Screen::AgentHistory)),
            );
    }
}

/// Opens saved history even when a world or its provider is stopped.
pub fn open(world: &mut World, agent: String) {
    open_tab(world, agent, Tab::Metrics);
}
pub fn open_tab(world: &mut World, agent: String, tab: Tab) {
    let workspace = world
        .resource::<AppSettings>()
        .0
        .agents_dir
        .join(slug(&agent));
    let origin = world
        .get_resource::<State<Screen>>()
        .map_or(Screen::ExplorerResume, |s| *s.get());
    let (job, generation, last, previous_origin) = world.get_resource_mut::<Browser>().map_or(
        (None, 0, Duration::ZERO, Screen::ExplorerResume),
        |mut previous| {
            (
                previous.job.take(),
                previous.generation.wrapping_add(1),
                previous.last,
                previous.origin,
            )
        },
    );
    world.insert_resource(Browser {
        agent,
        workspace,
        origin: if origin == Screen::AgentHistory {
            previous_origin
        } else {
            origin
        },
        tab,
        key: None,
        offset: if tab == Tab::Logs { u64::MAX } else { 0 },
        generation,
        job,
        snapshot: None,
        error: None,
        dirty: true,
        changed: true,
        last,
    });
    run::set_screen(world, Screen::AgentHistory);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(AppSettings(
            testbed_content::explorer::settings::Settings::default_for(&std::env::temp_dir()),
        ));
        world.insert_resource(State::new(Screen::ExplorerDetached));
        world.insert_resource(NextState::<Screen>::Unchanged);
        open(&mut world, "Fixture".into());
        world
    }
    fn entry(key: &str, kind: EntryKind, linked: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            key: key.into(),
            kind,
            label: format!("fixture {key}"),
            metrics_key: linked.map(Into::into),
        }
    }
    #[test]
    fn reopening_another_agent_retains_one_pending_reader_and_restores_origin() {
        let mut world = world();
        let (_sender, receiver) = mpsc::channel();
        world.resource_mut::<Browser>().job = Some((0, Rx::new(receiver)));
        world.resource_mut::<Browser>().last = Duration::from_secs(50);
        world.insert_resource(State::new(Screen::AgentHistory));
        open_tab(&mut world, "Other".into(), Tab::Logs);
        let browser = world.resource::<Browser>();
        assert_eq!(browser.generation, 1);
        assert!(
            browser.job.is_some(),
            "reopen must retain ownership of an unfinished reader"
        );
        assert_eq!(browser.last, Duration::from_secs(50));
        assert_eq!(browser.origin, Screen::ExplorerDetached);
        assert_eq!(browser.agent, "Other");
        assert!(browser.snapshot.is_none());
        close(&mut world);
        assert!(matches!(
            world.resource::<NextState<Screen>>(),
            NextState::Pending(Screen::ExplorerDetached)
        ));
    }
    #[test]
    fn linked_metrics_follow_the_selected_run_even_while_the_previous_page_is_cached() {
        let mut world = world();
        let first = entry("log:first", EntryKind::Log, Some("metrics:first"));
        let second = entry("log:second", EntryKind::Log, Some("metrics:second"));
        let mut browser = world.resource_mut::<Browser>();
        browser.tab = Tab::Logs;
        browser.key = Some(second.key.clone());
        browser.snapshot = Some(Snapshot {
            history: History {
                entries: vec![first.clone(), second],
                message: String::new(),
            },
            entry: Some(first),
            page: None,
        });
        let _ = browser;
        act(&mut world, Action::Linked);
        let browser = world.resource::<Browser>();
        assert_eq!(browser.tab, Tab::Metrics);
        assert_eq!(browser.key.as_deref(), Some("metrics:second"));
        assert_eq!(browser.offset, 0);
        assert!(browser.snapshot.is_none());
    }
    #[test]
    fn refresh_preserves_scroll_and_navigation_resets_it_without_touching_a_world() {
        let mut world = world();
        let scroll = world
            .spawn((BodyScroll, ScrollPosition(Vec2::new(0.0, 120.0))))
            .id();
        act(&mut world, Action::Refresh);
        assert_eq!(world.get::<ScrollPosition>(scroll).unwrap().0.y, 120.0);
        act(&mut world, Action::Tab(Tab::Logs));
        assert_eq!(world.get::<ScrollPosition>(scroll).unwrap().0, Vec2::ZERO);
        assert_eq!(world.resource::<Browser>().generation, 2);
    }
    #[test]
    fn manual_reading_freezes_the_current_page_and_rejects_pending_tail_updates() {
        let mut world = world();
        let log = entry("log:fixture", EntryKind::Log, None);
        let mut browser = world.resource_mut::<Browser>();
        browser.tab = Tab::Logs;
        browser.key = Some(log.key.clone());
        browser.offset = u64::MAX;
        browser.snapshot = Some(Snapshot {
            history: History {
                entries: vec![log.clone()],
                message: String::new(),
            },
            entry: Some(log),
            page: Some(Page {
                title: "Fixture".into(),
                text: "tail".into(),
                offset: ai_history::PAGE_BYTES as u64,
                next_offset: None,
                previous_offset: Some(0),
            }),
        });
        pause_follow(&mut browser);
        assert_eq!(browser.offset, ai_history::PAGE_BYTES as u64);
        assert_eq!(browser.generation, 1);
        assert!(browser.changed);
        browser.offset = u64::MAX;
        browser.key = Some("log:other".into());
        pause_follow(&mut browser);
        assert_eq!(
            browser.offset, 0,
            "cached content from another run must not choose the new run's page"
        );
    }
    #[test]
    fn disconnected_old_worker_cannot_publish_an_error_into_the_new_selection() {
        let mut world = world();
        world.insert_resource(Time::<Real>::default());
        for (job_generation, expect_error) in [(0, false), (1, true)] {
            let (sender, receiver) = mpsc::channel();
            drop(sender);
            let mut browser = world.resource_mut::<Browser>();
            browser.generation = 1;
            browser.job = Some((job_generation, Rx::new(receiver)));
            browser.dirty = false;
            browser.changed = false;
            browser.last = Duration::from_secs(100);
            browser.error = None;
            let _ = browser;
            world.run_system_once(poll).unwrap();
            let browser = world.resource::<Browser>();
            assert_eq!(browser.error.is_some(), expect_error);
            assert_eq!(browser.changed, expect_error);
            assert!(browser.job.is_none());
        }
    }
    #[test]
    fn native_screen_renders_switches_tabs_and_returns_with_unique_keyboard_order() {
        let mut app = crate::screens::tests::app(
            std::path::Path::new("unused-fixture"),
            std::path::Path::new("unused-agents"),
        );
        app.add_plugins(AgentHistoryPlugin);
        open(app.world_mut(), "Fixture".into());
        let (_sender, receiver) = mpsc::channel();
        app.world_mut().resource_mut::<Browser>().job = Some((0, Rx::new(receiver)));
        app.update();
        assert_eq!(
            *app.world().resource::<State<Screen>>().get(),
            Screen::AgentHistory
        );
        let mut tabs = app.world_mut().query::<&TabIndex>();
        let mut values: Vec<_> = tabs.iter(app.world()).map(|tab| tab.0).collect();
        let count = values.len();
        values.sort();
        values.dedup();
        assert_eq!(values.len(), count);
        let mut actions = app.world_mut().query::<(Entity, &Action)>();
        let logs = actions
            .iter(app.world())
            .find_map(|(entity, action)| matches!(action, Action::Tab(Tab::Logs)).then_some(entity))
            .unwrap();
        app.world_mut().trigger(Activate { entity: logs });
        app.world_mut().flush();
        app.update();
        assert_eq!(app.world().resource::<Browser>().tab, Tab::Logs);
        close(app.world_mut());
        app.update();
        assert_eq!(*app.world().resource::<State<Screen>>().get(), Screen::Main);
    }
}
pub fn close(world: &mut World) {
    let origin = world
        .get_resource::<Browser>()
        .map_or(Screen::ExplorerResume, |browser| browser.origin);
    run::set_screen(world, origin);
}
fn clear_focus(mut focus: Option<ResMut<InputFocus>>) {
    if let Some(focus) = focus.as_mut() {
        focus.clear();
    }
    // Keep an in-flight worker owned across Back/reopen. Its generation is
    // invalidated on the next open, so rapid navigation cannot start many readers.
}

fn read_snapshot(
    workspace: &std::path::Path,
    tab: Tab,
    key: Option<&str>,
    offset: u64,
) -> Result<Snapshot, String> {
    let history = ai_history::list(workspace)?;
    let entries: Vec<_> = history
        .entries
        .iter()
        .filter(|entry| entry.kind == tab.kind())
        .collect();
    let entry = key
        .and_then(|key| entries.iter().find(|entry| entry.key == key).copied())
        .or_else(|| entries.first().copied())
        .cloned();
    let offset = if entry
        .as_ref()
        .is_some_and(|entry| Some(entry.key.as_str()) == key)
    {
        offset
    } else if tab == Tab::Logs {
        u64::MAX
    } else {
        0
    };
    let page = entry
        .as_ref()
        .map(|entry| ai_history::read(workspace, &entry.key, offset))
        .transpose()?;
    Ok(Snapshot {
        history,
        entry,
        page,
    })
}

fn poll(time: Res<Time<Real>>, browser: Option<ResMut<Browser>>) {
    let Some(mut browser) = browser else {
        return;
    };
    if let Some((generation, rx)) = &browser.job {
        let generation = *generation;
        match rx.try_recv() {
            Ok(result) => {
                browser.job = None;
                if generation == browser.generation {
                    match result {
                        Ok(snapshot) => {
                            browser.key = snapshot.entry.as_ref().map(|entry| entry.key.clone());
                            if browser.offset != u64::MAX {
                                browser.offset =
                                    snapshot.page.as_ref().map_or(0, |page| page.offset);
                            }
                            browser.snapshot = Some(snapshot);
                            browser.error = None;
                        }
                        Err(error) => browser.error = Some(error),
                    }
                    browser.changed = true;
                }
            }
            Err(TryRecvError::Disconnected) => {
                browser.job = None;
                if generation == browser.generation {
                    browser.error = Some(
                    "Session files could not be read. Check the folders in Settings, then Refresh."
                        .into(),
                );
                    browser.changed = true;
                }
            }
            Err(TryRecvError::Empty) => {
                if time.elapsed().saturating_sub(browser.last) > Duration::from_secs(10)
                    && browser.error.is_none()
                {
                    browser.error=Some("The history folder is taking longer to respond. You can go Back; Refresh will run after the current read finishes.".into());
                    browser.changed = true;
                }
                return;
            }
        }
    }
    if !browser.dirty && time.elapsed().saturating_sub(browser.last) < REFRESH {
        return;
    }
    let (workspace, tab, key, offset) = (
        browser.workspace.clone(),
        browser.tab,
        browser.key.clone(),
        browser.offset,
    );
    let (sender, receiver) = mpsc::channel();
    match std::thread::Builder::new()
        .name("session-history-read".into())
        .spawn(move || {
            let result = read_snapshot(&workspace, tab, key.as_deref(), offset);
            let _ = sender.send(result);
        }) {
        Ok(_) => browser.job = Some((browser.generation, Rx::new(receiver))),
        Err(_) => {
            browser.error = Some("Session history could not start reading. Try Refresh.".into());
            browser.changed = true;
        }
    }
    browser.dirty = false;
    browser.last = time.elapsed();
}

#[derive(Component, Clone, Copy)]
enum Action {
    Tab(Tab),
    Linked,
    Previous,
    Next,
    Earlier,
    Later,
    Latest,
    Refresh,
    Back,
}
#[derive(Component)]
enum Label {
    Selection,
    Body,
    Notice,
    Page,
}
#[derive(Component)]
struct BodyScroll;

fn button(p: &mut ChildSpawnerCommands, label: &str, tab: i32, action: Action) -> Entity {
    let id = ui::small_button(p, label, true, tab, action);
    p.commands().entity(id).observe(activate);
    id
}
fn spawn(mut commands: Commands, browser: Option<Res<Browser>>, font: Option<Res<HistoryFont>>) {
    let Some(browser) = browser else {
        return;
    };
    let mut body_font = TextFont::from_font_size(15.0);
    if let Some(font) = font {
        body_font = body_font.with_font(font.0.clone());
    }
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::AgentHistory),
        "AI sessions",
    );
    commands
        .entity(panel)
        .queue(|mut entity: EntityWorldMut| {
            if let Some(mut node) = entity.get_mut::<Node>() {
                node.height = percent(94);
                node.max_width = px(1100);
                node.min_height = px(0);
            }
        })
        .with_children(|p| {
            p.spawn(ui::label(
                format!("{} - saved logs and metrics", browser.agent),
                16.0,
                ui::TEXT,
            ));
            ui::row(p, |r| {
                let metrics = button(r, "Metrics", 0, Action::Tab(Tab::Metrics));
                r.commands().entity(metrics).insert(AutoFocus);
                button(r, "Session logs", 1, Action::Tab(Tab::Logs));
                button(r, "Refresh", 2, Action::Refresh);
                button(r, "Linked metrics", 3, Action::Linked);
            });
            p.spawn((
                Label::Selection,
                ui::label("Loading session history...", 15.0, ui::TITLE),
            ));
            ui::row(p, |r| {
                button(r, "Newer record", 4, Action::Previous);
                button(r, "Older record", 5, Action::Next);
            });
            p.spawn((
                Node {
                    width: percent(100),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_basis: px(0),
                    min_height: px(0),
                    overflow: Overflow::scroll_y(),
                    padding: UiRect::all(px(12)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                ScrollArea,
                TabIndex(6),
                BodyScroll,
                bevy::ui::prelude::AccessibleLabel::new("Session contents"),
                BackgroundColor(ui::FIELD),
                BorderColor::all(ui::PANEL_BORDER),
            ))
            .observe(pause_follow_on_scroll)
            .with_child((
                Label::Body,
                Text::new("Loading..."),
                body_font,
                TextColor(ui::TEXT),
                TextLayout {
                    linebreak: LineBreak::WordOrCharacter,
                    ..default()
                },
                Node {
                    width: percent(100),
                    min_width: px(0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            p.spawn((Label::Page, ui::label("", 13.0, ui::DIM)));
            ui::row(p, |r| {
                button(r, "Earlier", 7, Action::Earlier);
                button(r, "Later", 8, Action::Later);
                button(r, "Follow latest", 9, Action::Latest);
            });
            p.spawn((
                Label::Notice,
                ui::label(
                    "Private gameplay content. History updates while this view is open.",
                    13.0,
                    ui::NOTE,
                ),
            ));
            button(p, "Back", 10, Action::Back);
        });
    commands.queue(move |world: &mut World| {
        let children = world
            .get::<Children>(panel)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for child in children {
            if !world.entity(child).contains::<BodyScroll>()
                && let Some(mut node) = world.get_mut::<Node>(child)
            {
                node.flex_shrink = 0.0;
            }
        }
    });
}

fn activate(event: On<Activate>, actions: Query<&Action>, mut commands: Commands) {
    if let Ok(action) = actions.get(event.entity).copied() {
        commands.queue(move |world: &mut World| act(world, action));
    }
}

type ScrollBodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ComputedNode,
        &'static mut ScrollPosition,
        &'static mut BorderColor,
    ),
    (With<BodyScroll>, Without<ui::MenuButton>),
>;
fn scroll_focus(
    focus: Option<Res<InputFocus>>,
    keys: Res<ButtonInput<KeyCode>>,
    browser: Option<ResMut<Browser>>,
    mut bodies: ScrollBodies,
) {
    let Some(mut browser) = browser else {
        return;
    };
    for (entity, node, mut position, mut border) in &mut bodies {
        let focused = focus
            .as_ref()
            .is_some_and(|focus| focus.get() == Some(entity));
        let color = if focused { ui::NOTE } else { ui::PANEL_BORDER };
        *border = BorderColor::all(color);
        let manual = focused
            && [
                KeyCode::ArrowUp,
                KeyCode::ArrowDown,
                KeyCode::PageUp,
                KeyCode::PageDown,
                KeyCode::Home,
                KeyCode::End,
            ]
            .iter()
            .any(|key| keys.just_pressed(*key));
        if manual {
            pause_follow(&mut browser);
        }
        if browser.tab == Tab::Logs && browser.offset == u64::MAX {
            position.0.y = f32::MAX;
            continue;
        }
        if focused {
            let viewport = node.size().y * node.inverse_scale_factor;
            let max =
                ((node.content_size().y - node.size().y) * node.inverse_scale_factor).max(0.0);
            let delta = if keys.just_pressed(KeyCode::PageDown) {
                viewport * 0.85
            } else if keys.just_pressed(KeyCode::PageUp) {
                -viewport * 0.85
            } else if keys.just_pressed(KeyCode::ArrowDown) {
                32.0
            } else if keys.just_pressed(KeyCode::ArrowUp) {
                -32.0
            } else {
                0.0
            };
            let next = if keys.just_pressed(KeyCode::Home) {
                0.0
            } else if keys.just_pressed(KeyCode::End) {
                max
            } else {
                (position.0.y + delta).clamp(0.0, max)
            };
            if next != position.0.y {
                position.0.y = next;
            }
        }
    }
}

fn pause_follow(browser: &mut Browser) {
    if browser.tab == Tab::Logs && browser.offset == u64::MAX {
        browser.offset = browser
            .snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot
                    .entry
                    .as_ref()
                    .is_some_and(|entry| Some(entry.key.as_str()) == browser.key.as_deref())
            })
            .and_then(|snapshot| snapshot.page.as_ref())
            .map_or(0, |page| page.offset);
        browser.generation = browser.generation.wrapping_add(1);
        browser.changed = true;
    }
}
fn pause_follow_on_scroll(
    _: On<bevy::picking::events::PointerScroll>,
    browser: Option<ResMut<Browser>>,
) {
    if let Some(mut browser) = browser {
        pause_follow(&mut browser);
    }
}
fn act(world: &mut World, action: Action) {
    if matches!(action, Action::Back) {
        close(world);
        return;
    }
    let Some(mut browser) = world.get_resource_mut::<Browser>() else {
        return;
    };
    match action {
        Action::Tab(tab) => {
            if tab == browser.tab {
                return;
            }
            browser.tab = tab;
            browser.key = None;
            browser.offset = if tab == Tab::Logs { u64::MAX } else { 0 };
            browser.snapshot = None;
        }
        Action::Linked => {
            let Some(key) = browser
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .history
                        .entries
                        .iter()
                        .find(|entry| Some(entry.key.as_str()) == browser.key.as_deref())
                })
                .and_then(|entry| entry.metrics_key.clone())
            else {
                return;
            };
            browser.tab = Tab::Metrics;
            browser.key = Some(key);
            browser.offset = 0;
            browser.snapshot = None;
        }
        Action::Previous | Action::Next => {
            let entries: Vec<_> = browser
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .history
                        .entries
                        .iter()
                        .filter(|entry| entry.kind == browser.tab.kind())
                        .collect()
                })
                .unwrap_or_default();
            let index = browser
                .key
                .as_deref()
                .and_then(|key| entries.iter().position(|entry| entry.key == key))
                .unwrap_or(0);
            let next = if matches!(action, Action::Previous) {
                index.saturating_sub(1)
            } else {
                index.saturating_add(1).min(entries.len().saturating_sub(1))
            };
            let key = entries.get(next).map(|entry| entry.key.clone());
            browser.key = key;
            browser.offset = if browser.tab == Tab::Logs {
                u64::MAX
            } else {
                0
            };
        }
        Action::Earlier | Action::Later => {
            let page = browser
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.page.as_ref());
            let offset = page.and_then(|page| {
                if matches!(action, Action::Earlier) {
                    page.previous_offset
                } else {
                    page.next_offset
                }
            });
            let Some(offset) = offset else {
                return;
            };
            browser.offset = offset;
        }
        Action::Latest => browser.offset = u64::MAX,
        Action::Refresh => {}
        Action::Back => unreachable!(),
    }
    browser.generation = browser.generation.wrapping_add(1);
    browser.dirty = true;
    browser.changed = true;
    browser.error = None;
    if !matches!(action, Action::Refresh) {
        let _ = browser;
        let mut scroll = world.query_filtered::<&mut ScrollPosition, With<BodyScroll>>();
        for mut position in scroll.iter_mut(world) {
            position.0 = Vec2::new(
                0.0,
                if matches!(action, Action::Latest) {
                    f32::MAX
                } else {
                    0.0
                },
            );
        }
    }
}

fn refresh(
    mut commands: Commands,
    browser: Option<ResMut<Browser>>,
    mut labels: Query<(&Label, &mut Text)>,
    buttons: Query<(Entity, &Action)>,
    mut tab_labels: Query<(&ChildOf, &mut Text), Without<Label>>,
) {
    let Some(mut browser) = browser else {
        return;
    };
    if !browser.changed {
        return;
    }
    browser.changed = false;
    let snapshot = browser.snapshot.as_ref();
    let entries: Vec<_> = snapshot
        .map(|snapshot| {
            snapshot
                .history
                .entries
                .iter()
                .filter(|entry| entry.kind == browser.tab.kind())
                .collect()
        })
        .unwrap_or_default();
    let index = browser
        .key
        .as_deref()
        .and_then(|key| entries.iter().position(|entry| entry.key == key))
        .unwrap_or(0);
    let selected = entries
        .iter()
        .find(|entry| Some(entry.key.as_str()) == browser.key.as_deref())
        .copied();
    let page = snapshot
        .filter(|snapshot| {
            snapshot.entry.as_ref().is_some_and(|entry| {
                Some(entry.key.as_str()) == browser.key.as_deref()
                    && entry.kind == browser.tab.kind()
            })
        })
        .and_then(|snapshot| snapshot.page.as_ref())
        .filter(|page| {
            page.offset == browser.offset
                || (browser.offset == u64::MAX && page.next_offset.is_none())
        });
    let selection = if snapshot.is_none() && browser.error.is_none() {
        "Loading session history...".into()
    } else {
        selected.map_or_else(
            || match browser.tab {
                Tab::Metrics => "No session metrics yet".into(),
                Tab::Logs => "No session logs yet".into(),
            },
            |entry| format!("{} of {}: {}", index + 1, entries.len(), entry.label),
        )
    };
    let body = page.map_or_else(|| {
        if browser.error.is_none() && (browser.snapshot.is_none() || browser.dirty || browser.job.is_some()) {"Loading session history...".into()}
        else {"Start an AI agent from its controls to create session records. You can return here while it plays or after it stops.".into()}
    },|page|page.text.clone());
    let body = browser.error.as_ref().map_or(body.clone(), |error| {
        format!("{error}\n\nLast loaded content:\n{body}")
    });
    for (label, mut text) in &mut labels {
        let value = match label {
            Label::Selection => selection.clone(),
            Label::Body => body.clone(),
            Label::Page => page
                .map(|page| {
                    format!(
                        "Page {}{}",
                        page.offset / ai_history::PAGE_BYTES as u64 + 1,
                        if page.next_offset.is_some() {
                            " (more pages)"
                        } else {
                            " (end of file)"
                        }
                    )
                })
                .unwrap_or_default(),
            Label::Notice => browser
                .error
                .as_ref()
                .map(|_| "Refresh failed. Details are at the start of the file view.".to_owned())
                .unwrap_or_else(|| {
                    snapshot
                        .map(|snapshot| snapshot.history.message.clone())
                        .filter(|message| !message.is_empty())
                        .unwrap_or_else(|| "Private history | Refreshes every 2 seconds".into())
                }),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    for (entity, action) in &buttons {
        let enabled = match action {
            Action::Previous => index > 0,
            Action::Next => index + 1 < entries.len(),
            Action::Earlier => page.is_some_and(|page| page.previous_offset.is_some()),
            Action::Later => page.is_some_and(|page| page.next_offset.is_some()),
            Action::Latest => browser.tab == Tab::Logs && page.is_some(),
            Action::Linked => {
                browser.tab == Tab::Logs
                    && selected.is_some_and(|entry| entry.metrics_key.is_some())
            }
            _ => true,
        };
        if enabled {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
    for (parent, mut text) in &mut tab_labels {
        if let Ok((_, Action::Latest)) = buttons.get(parent.parent()) {
            let label = if browser.tab == Tab::Logs && browser.offset == u64::MAX {
                "Following latest"
            } else {
                "Follow latest"
            };
            if text.0 != label {
                text.0 = label.into();
            }
        }
        if let Ok((_, Action::Tab(tab))) = buttons.get(parent.parent()) {
            let label = match tab {
                Tab::Metrics => "Metrics",
                Tab::Logs => "Session logs",
            };
            let value = if *tab == browser.tab {
                format!("{label} (selected)")
            } else {
                label.into()
            };
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}
