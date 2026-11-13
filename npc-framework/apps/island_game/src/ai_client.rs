//! Native explorer onboarding and live controls for the dedicated AI client.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button as WidgetButton, ButtonPlugin};
use island_content::explorer::ai_client::{self, Provider};
use island_content::explorer::home::AgentDir;
use npc_framework::prelude::SimControl;
use npc_framework::view_helpers::ViewSystems;

#[path = "ai_history.rs"]
mod history;

#[derive(Resource)]
struct Panel {
    history: history::State,
    dir: AgentDir,
    open: bool,
    message: String,
    notice: Option<String>,
    status: ai_client::Status,
    starting: Option<(u32, Instant)>,
    onboarding: bool,
    was_open: bool,
    saved_focus: Option<Entity>,
    details_pause: Option<bool>,
    last_poll: Instant,
    job: Option<Arc<Mutex<Option<ai_client::Status>>>>,
}
#[derive(Component)]
struct PanelRoot;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct Hud;
#[derive(Component, Clone, Copy)]
enum Action {
    Start(Provider),
    Stop,
    Close,
    Details,
    Fresh,
    History,
}

type ControlButtons<'w, 's> = Query<
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
    With<Action>,
>;

pub fn install(app: &mut App, dir: AgentDir, onboarding: bool) {
    // Keep the optional connection-details modal available without making it onboarding.
    crate::explorer::show_prompt_modal(app);
    if let Some(mut modal) = app
        .world_mut()
        .get_resource_mut::<island_view::PromptModal>()
    {
        modal.open = false;
    }
    if !app.is_plugin_added::<ButtonPlugin>() {
        app.add_plugins(ButtonPlugin);
    }
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.insert_resource(Panel {
        history: history::State::default(),
        dir,
        open: onboarding,
        message: "Ready - choose your logged-in CLI. The game supplies prompts and tools.".into(),
        notice: None,
        status: ai_client::Status::default(),
        starting: None,
        onboarding,
        was_open: false,
        saved_focus: None,
        details_pause: None,
        last_poll: Instant::now(),
        job: None,
    })
    .configure_sets(Update, ViewSystems::Input.run_if(not(panel_open)))
    .configure_sets(Update, ViewSystems::OverlayInput.run_if(not(panel_open)))
    .configure_sets(Update, island_view::NotesInput.run_if(not(panel_open)))
    .add_systems(Startup, spawn)
    .add_systems(
        Update,
        restore_details_pause.after(crate::explorer::unpause_on_close),
    )
    .add_systems(
        Update,
        (toggle, poll, refresh)
            .chain()
            .after(ViewSystems::Input)
            .run_if(not(crate::explorer::close_dialog_open)),
    );
    history::install(app);
}
fn panel_open(panel: Res<Panel>) -> bool {
    panel.open
}
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<Panel>,
    mut control: ResMut<SimControl>,
) {
    if panel.open && panel.history.open && keys.just_pressed(KeyCode::Escape) {
        panel.history.close();
        return;
    }
    if keys.just_pressed(KeyCode::F10) || (panel.open && keys.just_pressed(KeyCode::Escape)) {
        panel.open = !panel.open;
        if !panel.open {
            panel.history.close();
        }
        if !panel.open && panel.onboarding {
            control.paused = false;
            panel.onboarding = false;
        }
    }
}
fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action) {
    let mut entity = parent.spawn((
        Name::new(format!("AI client {label}")),
        action,
        WidgetButton,
        Hovered::default(),
        TabIndex(0),
        AccessibleLabel::new(label),
        Node {
            padding: UiRect::axes(px(16), px(9)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.20, 0.24, 0.30)),
        BorderColor::all(Color::srgb(0.48, 0.55, 0.65)),
    ));
    entity
        .with_child((
            Text::new(label),
            TextFont::from_font_size(16.0),
            TextColor(Color::WHITE),
        ))
        .observe(activate);
}
fn spawn(mut commands: Commands, panel: Res<Panel>) {
    commands.spawn((
        Hud,
        Text::new("AI agent: F10 for controls"),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.95, 0.95, 0.90)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(16),
            ..default()
        },
        GlobalZIndex(900),
    ));
    commands.spawn((PanelRoot, Name::new("AI agent controls"), TabGroup::modal(),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100),
            justify_content: JustifyContent::Center, align_items: AlignItems::Center,
            display: if panel.open { Display::Flex } else { Display::None }, ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(1000),
    )).with_children(|root| {
        root.spawn((history::Container::Controls, Node { flex_direction: FlexDirection::Column, width: percent(90), max_width: px(680),
            max_height: percent(90), overflow: Overflow::scroll_y(), padding: UiRect::all(px(24)),
            row_gap: px(18), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
            bevy::ui_widgets::ScrollArea, BackgroundColor(Color::srgb(0.08, 0.09, 0.11)),
            BorderColor::all(Color::srgb(0.35,0.40,0.48)),
        )).with_children(|p| {
            p.spawn((Text::new("Your AI agent"), TextFont::from_font_size(26.0), TextColor(Color::srgb(0.97,0.95,0.85))));
            p.spawn((Text::new(format!("Choose a CLI you have already logged into. {}",ai_client::START_PERMISSION_NOTICE)), TextFont::from_font_size(16.0), TextColor(Color::srgb(0.88,0.90,0.92))));
            p.spawn((StatusText, Text::new(&panel.message), TextFont::from_font_size(15.0), TextColor(Color::srgb(0.95,0.82,0.53))));
            p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(12), row_gap: px(10), ..default() }).with_children(|r| {
                button(r,"Start Codex",Action::Start(Provider::Codex));
                button(r,"Start Claude",Action::Start(Provider::Claude));
                button(r,"Stop agent",Action::Stop);
                button(r,"Connection help",Action::Details);
                button(r,"New AI session",Action::Fresh);
                button(r,"Sessions",Action::History);
            });
            p.spawn((Text::new("First time? Sign in with codex login or claude in your terminal, then return here. F10 opens these controls while you play."), TextFont::from_font_size(14.0), TextColor(Color::srgb(0.77,0.81,0.87))));
            button(p,"Return to world",Action::Close);
        });
        history::spawn(root);
    });
}
fn activate(
    event: On<Activate>,
    actions: Query<&Action>,
    mut panel: ResMut<Panel>,
    mut control: ResMut<SimControl>,
    modal: Option<ResMut<island_view::PromptModal>>,
    dialog: Option<Res<crate::explorer::CloseDialog>>,
) {
    if dialog.is_some_and(|dialog| dialog.open) {
        return;
    }
    if panel.history.open {
        return;
    }
    let Ok(action) = actions.get(event.entity) else {
        return;
    };
    match *action {
        Action::History => panel.history.open(event.entity),
        Action::Close => {
            panel.open = false;
            if panel.onboarding {
                control.paused = false;
                panel.onboarding = false;
            }
        }
        Action::Details => {
            if let Some(mut modal) = modal {
                panel.details_pause = Some(if panel.onboarding {
                    false
                } else {
                    control.paused
                });
                panel.onboarding = false;
                modal.open = true;
                panel.open = false;
            }
        }
        Action::Stop => {
            panel.notice = Some(match ai_client::request_stop(&panel.dir.dir) {
                Ok(()) => "Stopping agent. Your world keeps running.".into(),
                Err(e) => e,
            });
        }
        Action::Start(_) | Action::Fresh => {
            let (provider, fresh) = match *action {
                Action::Start(provider) => (provider, false),
                _ => (
                    if panel.status.provider == "claude" {
                        Provider::Claude
                    } else {
                        Provider::Codex
                    },
                    true,
                ),
            };
            if panel.starting.is_some() || panel.status.is_active() {
                panel.notice = Some(
                    "An AI client is already running. Stop agent before changing providers.".into(),
                );
                return;
            }
            let result = if fresh {
                ai_client::launch_fresh(&panel.dir.dir, provider)
            } else {
                ai_client::launch(&panel.dir.dir, provider)
            };
            match result {
                Ok(pid) => {
                    panel.starting = Some((pid, Instant::now()));
                    panel.notice = None;
                    panel.onboarding = false;
                    panel.message = format!(
                        "Starting {} - checking login and connecting to your world...",
                        provider.name()
                    );
                    control.paused = false;
                }
                Err(e) => panel.notice = Some(e),
            }
        }
    }
}
fn restore_details_pause(
    mut panel: ResMut<Panel>,
    modal: Res<island_view::PromptModal>,
    mut control: ResMut<SimControl>,
) {
    if !modal.open
        && let Some(paused) = panel.details_pause.take()
    {
        control.paused = paused;
    }
}
fn poll(mut panel: ResMut<Panel>) {
    let result = panel
        .job
        .as_ref()
        .and_then(|job| job.lock().ok().and_then(|mut slot| slot.take()));
    if let Some(status) = result {
        if !status.state.is_empty() {
            if panel.starting.is_some_and(|(pid, _)| pid == status.pid) {
                panel.starting = None;
            }
            if panel.starting.is_none() {
                panel.message = status.line();
                panel.status = status;
            }
        }
        panel.job = None;
    }
    if panel
        .starting
        .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(12))
    {
        panel.starting = None;
        panel.notice = Some("Client did not report readiness. Check your CLI login and the agent folder, then try Start again.".into());
    }
    if panel.job.is_none() && panel.last_poll.elapsed() >= Duration::from_secs(1) {
        panel.last_poll = Instant::now();
        let dir = panel.dir.dir.clone();
        let job = Arc::new(Mutex::new(None));
        panel.job = Some(job.clone());
        std::thread::spawn(move || {
            let status = ai_client::read_status(&dir);
            if let Ok(mut result) = job.lock() {
                *result = Some(status);
            }
        });
    }
}
fn refresh(
    mut panel: ResMut<Panel>,
    mut commands: Commands,
    mut roots: Query<&mut Node, With<PanelRoot>>,
    mut text: Query<&mut Text, With<StatusText>>,
    mut focus: Option<ResMut<InputFocus>>,
    mut hud: Query<&mut Text, (With<Hud>, Without<StatusText>)>,
    mut buttons: ControlButtons,
) {
    if panel.open != panel.was_open {
        if let Some(focus) = focus.as_mut() {
            if panel.open {
                // Only restore a control outside this overlay. Startup autofocus
                // or stale overlay focus must not return to a hidden AI button
                // and block other panels' keyboard shortcuts after closing.
                panel.saved_focus = focus.get().filter(|entity| !buttons.contains(*entity));
                if let Some((entity, ..)) = buttons
                    .iter()
                    .find(|(_, action, ..)| matches!(action, Action::Close))
                {
                    focus.set(entity, FocusCause::Navigated);
                }
            } else if let Some(previous) = panel
                .saved_focus
                .take()
                .filter(|entity| !buttons.contains(*entity))
            {
                focus.set(previous, FocusCause::Navigated);
            } else {
                focus.clear();
            }
        }
        panel.was_open = panel.open;
    }
    for mut root in &mut roots {
        root.display = if panel.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut label in &mut text {
        let message = panel.notice.as_ref().map_or_else(
            || panel.message.clone(),
            |notice| format!("{notice}\n\n{}", panel.message),
        );
        if label.0 != message {
            label.0 = message;
        }
    }
    for mut text in &mut hud {
        text.0 = format!(
            "AI agent: {} | F10 controls",
            if panel.status.state.is_empty() {
                "ready"
            } else {
                &panel.status.state
            }
        );
    }
    let active = panel.starting.is_some() || panel.status.is_active();
    for (entity, action, hovered, pressed, mut color, mut border) in &mut buttons {
        let enabled = match action {
            Action::Start(_) => !active,
            Action::Stop => active,
            Action::Fresh => {
                !active && (panel.status.session_ready || panel.status.state == "error")
            }
            _ => true,
        };
        if enabled {
            commands
                .entity(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        } else {
            commands
                .entity(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
        color.0 = if !enabled {
            Color::srgb(0.12, 0.13, 0.15)
        } else if pressed {
            Color::srgb(0.14, 0.17, 0.22)
        } else if hovered.0 {
            Color::srgb(0.27, 0.32, 0.40)
        } else {
            Color::srgb(0.20, 0.24, 0.30)
        };
        *border = BorderColor::all(
            if focus
                .as_ref()
                .is_some_and(|focus| focus.get() == Some(entity))
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

    pub(super) fn app(dir: std::path::PathBuf) -> App {
        let mut app = App::new();
        app.insert_resource(Panel {
            history: history::State::default(),
            dir: AgentDir { dir },
            open: true,
            message: String::new(),
            notice: None,
            status: ai_client::Status::default(),
            starting: None,
            onboarding: true,
            was_open: false,
            saved_focus: None,
            details_pause: None,
            last_poll: Instant::now(),
            job: None,
        })
        .insert_resource(SimControl {
            paused: true,
            ..default()
        });
        app
    }

    fn press(app: &mut App, action: Action) {
        let entity = app.world_mut().spawn(action).observe(activate).id();
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
    }

    #[test]
    fn returning_to_world_needs_no_prompt_or_cli_session_and_unpauses() {
        let mut app = app(std::path::PathBuf::new());
        press(&mut app, Action::Close);
        assert!(!app.world().resource::<Panel>().open);
        assert!(!app.world().resource::<SimControl>().paused);
    }

    #[test]
    fn initial_controls_release_focus_and_hidden_controls_never_take_it() {
        for onboarding in [true, false] {
            let mut app = app(std::path::PathBuf::new());
            {
                let mut panel = app.world_mut().resource_mut::<Panel>();
                panel.open = onboarding;
                panel.onboarding = onboarding;
            }
            app.init_resource::<InputFocus>()
                .add_systems(Startup, spawn)
                .add_systems(Update, refresh);
            app.update();
            if onboarding {
                let focused = app.world().resource::<InputFocus>().get().unwrap();
                assert!(matches!(
                    app.world().get::<Action>(focused),
                    Some(Action::Close)
                ));
                assert!(app.world().resource::<Panel>().saved_focus.is_none());
                press(&mut app, Action::Close);
                app.update();
            }
            assert!(
                app.world().resource::<InputFocus>().get().is_none(),
                "hidden AI buttons must not retain keyboard focus and swallow F7"
            );
        }
    }

    #[test]
    fn reopening_controls_restores_the_notebook_draft_focus_and_pause() {
        let mut app = app(std::path::PathBuf::new());
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.open = false;
            panel.onboarding = false;
        }
        app.init_resource::<InputFocus>()
            .add_systems(Startup, spawn)
            .add_systems(Update, refresh);
        let draft = app
            .world_mut()
            .spawn((
                Node::default(),
                bevy::text::EditableText::new("unsaved F7 draft"),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(draft, FocusCause::Navigated);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(draft));
        app.world_mut().resource_mut::<Panel>().open = true;
        app.update();
        assert_eq!(app.world().resource::<Panel>().saved_focus, Some(draft));
        press(&mut app, Action::Close);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(draft));
        assert!(app.world().resource::<SimControl>().paused);
        assert_eq!(
            app.world()
                .get::<bevy::text::EditableText>(draft)
                .unwrap()
                .value(),
            "unsaved F7 draft"
        );
    }

    #[test]
    fn connection_details_are_optional_and_never_appear_as_onboarding() {
        let mut app = app(std::path::PathBuf::new());
        app.insert_resource(island_view::PromptModal {
            open: false,
            ..default()
        });
        press(&mut app, Action::Details);
        assert!(app.world().resource::<island_view::PromptModal>().open);
        assert!(!app.world().resource::<Panel>().open);
        assert!(app.world().resource::<SimControl>().paused);
    }

    #[test]
    fn save_close_dialog_prevents_agent_controls_from_unpausing_world() {
        let mut app = app(std::path::PathBuf::new());
        app.init_resource::<crate::explorer::CloseDialog>();
        crate::explorer::open_close_dialog(&mut app);
        press(&mut app, Action::Close);
        assert!(app.world().resource::<Panel>().open);
        assert!(app.world().resource::<SimControl>().paused);
    }

    #[test]
    fn returning_from_live_controls_preserves_an_intentionally_paused_world() {
        let mut app = app(std::path::PathBuf::new());
        app.world_mut().resource_mut::<Panel>().onboarding = false;
        press(&mut app, Action::Close);
        assert!(app.world().resource::<SimControl>().paused);
        assert!(!app.world().resource::<Panel>().open);
    }

    #[test]
    fn optional_connection_help_preserves_pause_after_the_legacy_modal_closes() {
        let mut app = app(std::path::PathBuf::new());
        app.world_mut().resource_mut::<Panel>().onboarding = false;
        app.insert_resource(island_view::PromptModal {
            open: false,
            ..default()
        });
        press(&mut app, Action::Details);
        assert_eq!(app.world().resource::<Panel>().details_pause, Some(true));
        app.world_mut()
            .resource_mut::<island_view::PromptModal>()
            .open = false;
        app.world_mut().resource_mut::<SimControl>().paused = false;
        app.add_systems(Update, restore_details_pause);
        app.update();
        assert!(app.world().resource::<SimControl>().paused);
    }

    #[test]
    fn live_client_rejects_provider_switch_without_launching_or_unpausing() {
        let mut app = app(std::path::PathBuf::new());
        app.world_mut().resource_mut::<Panel>().status.state = "running".into();
        press(&mut app, Action::Start(Provider::Claude));
        assert!(
            app.world()
                .resource::<Panel>()
                .notice
                .as_deref()
                .unwrap()
                .contains("Stop agent")
        );
        assert!(app.world().resource::<SimControl>().paused);
    }
}
