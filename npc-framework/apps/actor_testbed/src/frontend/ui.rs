//! Native shell presentation. View drafts are distinct from the saved world.
use super::{Action, FrontendState, NoticeTone, Screen};
use crate::island::session;
use bevy::input_focus::{
    AutoFocus, FocusCause, InputFocus,
    tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation},
};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEdit};
use bevy::ui::{InteractionDisabled, prelude::AccessibleLabel};
use bevy::ui_widgets::{Activate, ScrollArea, TextInput};
use testbed_view::{FrontendUiRoot, IslandMode, game_ui as ui};

#[derive(Resource)]
pub(super) struct NameDraft(String);
impl Default for NameDraft {
    fn default() -> Self {
        Self("Traveller".into())
    }
}
#[derive(Resource, Default)]
struct RenderedRevision(Option<(u64, bool, bool)>);
#[derive(Component)]
pub(super) struct ShellRoot;
#[derive(Component)]
struct MenuLauncher;
#[derive(Component)]
pub(super) struct NameField;
#[derive(Component)]
struct Notice;
#[derive(Component)]
struct BeginButton;
#[derive(Component)]
struct ToolNotice;

pub(super) fn spawn(world: &mut World) {
    world.init_resource::<NameDraft>();
    world.init_resource::<RenderedRevision>();
    refresh(world);
}
pub(super) fn draft_name(world: &World) -> String {
    world.resource::<NameDraft>().0.clone()
}

pub(super) fn sync_name(
    mut draft: ResMut<NameDraft>,
    fields: Query<&EditableText, (With<NameField>, Changed<EditableText>)>,
) {
    for field in &fields {
        draft.0 = field.value().to_string();
    }
}
pub(super) fn keyboard_navigation(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut focus: ResMut<InputFocus>,
    nav: TabNavigation,
    roots: Query<Entity, With<ShellRoot>>,
    flow: Res<FrontendState>,
    mut commands: Commands,
) {
    if matches!(flow.screen, Screen::Playing | Screen::Transition) {
        return;
    }
    let Some(keys) = keys else {
        return;
    };
    if !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    let direction = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        NavAction::Previous
    } else {
        NavAction::Next
    };
    let next = if focus.get().is_some() {
        nav.navigate(&focus, direction)
    } else if let Ok(root) = roots.single() {
        nav.initialize(root, direction)
    } else {
        return;
    };
    if let Ok(entity) = next {
        focus.set(entity, FocusCause::Navigated);
        commands.trigger(bevy::ui_widgets::ScrollIntoView { entity });
    }
}
pub(super) fn activate(
    event: On<Activate>,
    actions: Query<(&Action, Has<InteractionDisabled>)>,
    mut flow: ResMut<FrontendState>,
) {
    if let Ok((action, false)) = actions.get(event.entity) {
        flow.requests.push(*action);
    }
}
fn label(
    world: &mut World,
    parent: Entity,
    text: impl Into<String>,
    size: f32,
    color: Color,
) -> Entity {
    world
        .spawn((ChildOf(parent), ui::text(text, size, color)))
        .id()
}
fn action(
    world: &mut World,
    parent: Entity,
    text: &str,
    tab: i32,
    primary: bool,
    action: Action,
    enabled: bool,
) -> Entity {
    let button = ui::button(world, parent, text, tab, primary, action);
    if !enabled {
        world.entity_mut(button).insert(InteractionDisabled);
    }
    button
}
fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(10),
                row_gap: px(10),
                ..default()
            },
        ))
        .id()
}

fn rebuild(world: &mut World) {
    let old: Vec<_> = world
        .query_filtered::<Entity, With<ShellRoot>>()
        .iter(world)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let screen = world.resource::<FrontendState>().screen;
    if screen == Screen::Playing {
        return;
    }
    let summary = session::summary(world);
    let ready = super::scene_ready(world);
    let root = world
        .spawn((
            FrontendUiRoot,
            ShellRoot,
            Name::new("Island game front end"),
            TabGroup::modal(),
            GlobalZIndex(100),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: if screen == Screen::Title {
                    JustifyContent::FlexStart
                } else {
                    JustifyContent::Center
                },
                align_items: AlignItems::Center,
                padding: UiRect::axes(vw(6), px(24)),
                ..default()
            },
            BackgroundColor(Color::srgba(
                0.02,
                0.03,
                0.035,
                if !ready {
                    1.0
                } else if screen == Screen::Title {
                    0.28
                } else {
                    0.55
                },
            )),
        ))
        .id();
    let panel = world
        .spawn((
            ChildOf(root),
            ui::panel(Node {
                width: px(if screen == Screen::Title { 440 } else { 500 }),
                max_width: percent(100),
                max_height: percent(94),
                flex_direction: FlexDirection::Column,
                row_gap: px(14),
                padding: UiRect::all(px(24)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            }),
        ))
        .id();
    let title = match screen {
        Screen::Title => "Island Village",
        Screen::New => "A new beginning",
        Screen::Pause => "Paused",
        Screen::Controls => "Make yourself at home",
        Screen::Transition => "Saving your island",
        Screen::Playing => unreachable!(),
    };
    label(
        world,
        panel,
        title,
        if screen == Screen::Title { 40.0 } else { 30.0 },
        ui::GOLD,
    );
    if !ready {
        let (text, tone) = super::scene_preparation_message(world);
        label(world, panel, text, 15.0, notice_color(tone));
    }
    let body = world
        .spawn((
            ChildOf(panel),
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    let mut first = None;
    match screen {
        Screen::Title => {
            label(
                world,
                body,
                if testbed_content::village::frontier::enabled(world) {
                    "Follow the coast, discover three villages, and make a home in a changing world."
                } else {
                    "Build a home, tend a garden, and find your place among a living village."
                },
                18.0,
                ui::TEXT,
            );
            if summary.can_continue {
                label(
                    world,
                    body,
                    format!(
                        "{}  ·  {}, day {}",
                        summary.player, summary.season, summary.day
                    ),
                    15.0,
                    ui::MUTED,
                );
            } else {
                label(
                    world,
                    body,
                    "Your story starts with a place to call home.",
                    15.0,
                    ui::MUTED,
                );
            }
            let continue_button = action(
                world,
                body,
                "Continue",
                0,
                true,
                Action::Continue,
                ready && summary.can_continue,
            );
            let new = action(
                world,
                body,
                "New game",
                1,
                !summary.can_continue,
                Action::New,
                ready && summary.can_start,
            );
            if summary.can_continue {
                action(
                    world,
                    body,
                    "Oversee the village",
                    2,
                    false,
                    Action::ContinueInspector,
                    ready,
                );
            }
            first = ready.then_some(if summary.can_continue {
                continue_button
            } else {
                new
            });
            if let Some(issue) = &summary.issue {
                label(world, body, issue, 14.0, ui::ERROR);
            }
        }
        Screen::New => {
            label(
                world,
                body,
                "Choose the name your neighbours will know you by.",
                17.0,
                ui::TEXT,
            );
            label(world, body, "Your name", 14.0, ui::MUTED);
            let mut editable = EditableText {
                allow_newlines: false,
                visible_lines: Some(1.0),
                ..EditableText::new(&world.resource::<NameDraft>().0)
            };
            editable.pending_edits.clear();
            editable.queue_edit(TextEdit::SelectAll);
            let field = world
                .spawn((
                    ChildOf(body),
                    NameField,
                    Name::new("New character name"),
                    AccessibleLabel::new("Your name"),
                    TextInput,
                    editable,
                    TextCursorStyle::default(),
                    ui::GameText,
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(ui::TEXT),
                    TextLayout::no_wrap(),
                    TabIndex(0),
                    Node {
                        width: percent(100),
                        min_width: px(0),
                        min_height: px(48),
                        padding: UiRect::all(px(10)),
                        border: UiRect::all(px(1)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(ui::FIELD),
                    BorderColor::all(ui::BORDER),
                ))
                .id();
            world
                .resource_mut::<InputFocus>()
                .set(field, FocusCause::Navigated);
            label(
                world,
                body,
                format!("World rules: {}", summary.profile),
                14.0,
                ui::MUTED,
            );
            label(
                world,
                body,
                "Arrive as a traveller. Meet your neighbours, gather supplies, and build a real cottage and garden.",
                16.0,
                ui::TEXT,
            );
            label(
                world,
                body,
                "Food, water, rest and warmth matter. A life can end; a successor continues in the same village.",
                14.0,
                ui::MUTED,
            );
            label(
                world,
                body,
                "Or oversee the village first, then join as a character whenever you choose.",
                14.0,
                ui::MUTED,
            );
            if summary.can_continue {
                label(
                    world,
                    body,
                    "Starting fresh keeps a numbered backup of your previous island. Continue keeps your current world.",
                    14.0,
                    ui::GOLD,
                );
            }
        }
        Screen::Pause => {
            label(
                world,
                body,
                format!(
                    "{}  ·  {}, day {}\nYour village is paused.",
                    summary.player, summary.season, summary.day
                ),
                16.0,
                ui::MUTED,
            );
            first = Some(action(world, body, "Resume", 0, true, Action::Resume, true));
            action(world, body, "Save game", 1, false, Action::Save, true);
            action(world, body, "Save and title", 2, false, Action::Title, true);
            label(world, body, "Village tools", 14.0, ui::MUTED);
            let tools = row(world, body);
            action(
                world,
                tools,
                "Inspect village",
                3,
                false,
                Action::Inspect,
                true,
            );
            action(
                world,
                tools,
                "Landscape tools",
                4,
                false,
                Action::Terraform,
                true,
            );
        }
        Screen::Controls => {
            label(
                world,
                body,
                "Start small: meet the village, gather wood, choose a house plot, then make planks at the workshop. Your neighbours keep working while you explore.",
                17.0,
                ui::TEXT,
            );
            for (keys, description) in [
                ("WASD", "Walk around the island"),
                ("Right-click", "Walk to a place"),
                ("E", "Use the nearest available action"),
                (
                    "M / Explore",
                    "Walk to a known place or explore in a direction",
                ),
                ("Wheel / Q / R", "Zoom and orbit while playing a character"),
                (
                    "Bottom toolbar",
                    "Open your pouch, crafting, building, garden, trade and people",
                ),
                ("Esc / Menu", "Pause and save your progress"),
                ("Ctrl+S", "Save your island"),
                ("Wheel / Q / E", "Zoom and orbit in village tools"),
            ] {
                label(
                    world,
                    body,
                    format!("{keys}\n{description}"),
                    15.0,
                    ui::TEXT,
                );
            }
            label(
                world,
                body,
                "Actions use real stock and take real time. If supplies are missing, the interface shows what to gather, make or buy.",
                14.0,
                ui::MUTED,
            );
        }
        Screen::Transition => {
            label(
                world,
                body,
                "Stopping transient movement and saving the whole village. Work and crops keep their progress.",
                17.0,
                ui::TEXT,
            );
        }
        Screen::Playing => unreachable!(),
    }
    if screen != Screen::Transition {
        ui::attach_panel_scroll_when_needed(
            world,
            panel,
            body,
            "Game menu",
            ["More above", "More below"],
        );
    }
    world.spawn((ChildOf(panel), Notice, ui::text("", 14.0, ui::TEXT)));
    let footer = row(world, panel);
    match screen {
        Screen::Title => {
            let help = action(
                world,
                footer,
                "How to play",
                3,
                false,
                Action::Controls,
                true,
            );
            if first.is_none() {
                first = Some(help);
            }
            action(world, footer, "Quit game", 4, false, Action::Quit, true);
        }
        Screen::New => {
            action(world, footer, "Back", 3, false, Action::Back, true);
            let valid = session::name_problem(world, &draft_name(world)).is_none();
            let begin = action(
                world,
                footer,
                "Start on the island",
                1,
                true,
                Action::Begin,
                ready && valid && summary.can_start,
            );
            world.entity_mut(begin).insert(BeginButton);
            let oversee = action(
                world,
                footer,
                "Oversee the village",
                2,
                false,
                Action::BeginInspector,
                ready && valid && summary.can_start,
            );
            world.entity_mut(oversee).insert(BeginButton);
        }
        Screen::Pause => {
            action(
                world,
                footer,
                "How to play",
                5,
                false,
                Action::Controls,
                true,
            );
            action(world, footer, "Quit game", 6, false, Action::Quit, true);
        }
        Screen::Controls => {
            first = Some(action(world, footer, "Back", 0, true, Action::Back, true));
        }
        _ => {}
    }
    if let Some(first) = first {
        world.entity_mut(first).insert(AutoFocus);
    }
}

pub(super) fn refresh(world: &mut World) {
    if !world.contains_resource::<NameDraft>() {
        return;
    }
    let revision = (
        world.resource::<FrontendState>().revision,
        super::scene_ready(world),
        world
            .get_resource::<testbed_view::SceneReadiness>()
            .is_some_and(|scene| scene.error.is_some()),
    );
    if world.resource::<RenderedRevision>().0 != Some(revision) {
        rebuild(world);
        world.resource_mut::<RenderedRevision>().0 = Some(revision);
    }
    let screen = world.resource::<FrontendState>().screen;
    let problem = if screen == Screen::New {
        session::name_problem(world, &draft_name(world))
    } else {
        None
    };
    let notice = problem
        .clone()
        .unwrap_or_else(|| world.resource::<FrontendState>().message.clone());
    let tone = if problem.is_some() {
        NoticeTone::Error
    } else {
        world.resource::<FrontendState>().notice_tone
    };
    for (mut text, mut color, mut node) in world
        .query_filtered::<(&mut Text, &mut TextColor, &mut Node), With<Notice>>()
        .iter_mut(world)
    {
        text.0.clone_from(&notice);
        color.0 = notice_color(tone);
        node.display = if notice.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
    }
    let begin: Vec<_> = world
        .query_filtered::<Entity, With<BeginButton>>()
        .iter(world)
        .collect();
    for entity in begin {
        if problem.is_some() || !session::summary(world).can_start || !super::scene_ready(world) {
            world.entity_mut(entity).insert(InteractionDisabled);
        } else {
            world.entity_mut(entity).remove::<InteractionDisabled>();
        }
    }
    let show_tools = screen == Screen::Playing
        && world.get_resource::<IslandMode>() != Some(&IslandMode::Character);
    let launcher = world
        .query_filtered::<Entity, With<MenuLauncher>>()
        .iter(world)
        .next();
    let launcher = launcher.unwrap_or_else(|| {
        let root = world
            .spawn((
                FrontendUiRoot,
                MenuLauncher,
                GlobalZIndex(90),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(12),
                    left: px(12),
                    max_width: percent(38),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
            ))
            .id();
        action(world, root, "Menu", 0, false, Action::Pause, true);
        world.spawn((
            ChildOf(root),
            ToolNotice,
            ui::text("", 14.0, ui::GOLD),
            BackgroundColor(ui::PANEL),
        ));
        root
    });
    world
        .get_mut::<Node>(launcher)
        .expect("menu launcher")
        .display = if show_tools {
        Display::Flex
    } else {
        Display::None
    };
    let notice = world.resource::<FrontendState>().message.clone();
    let tone = world.resource::<FrontendState>().notice_tone;
    for (mut text, mut color, mut node) in world
        .query_filtered::<(&mut Text, &mut TextColor, &mut Node), With<ToolNotice>>()
        .iter_mut(world)
    {
        text.0.clone_from(&notice);
        color.0 = notice_color(tone);
        node.display = if notice.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
    }
}

fn notice_color(tone: NoticeTone) -> Color {
    match tone {
        NoticeTone::Info => ui::TEXT,
        NoticeTone::Success => ui::SUCCESS,
        NoticeTone::Error => ui::ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::{
        ButtonState, InputPlugin,
        keyboard::{Key, KeyboardInput},
    };
    use bevy::input_focus::{InputDispatchPlugin, InputFocusPlugin};
    use bevy::window::PrimaryWindow;

    #[test]
    fn real_tab_input_moves_once_in_shell_and_is_inactive_while_playing() {
        let mut app = App::new();
        app.add_plugins((InputPlugin, InputFocusPlugin, InputDispatchPlugin))
            .init_resource::<FrontendState>()
            .add_systems(Update, keyboard_navigation);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let root = app.world_mut().spawn((ShellRoot, TabGroup::modal())).id();
        let buttons: Vec<_> = (0..3)
            .map(|index| app.world_mut().spawn((ChildOf(root), TabIndex(index))).id())
            .collect();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(buttons[0], FocusCause::Navigated);
        app.world_mut().write_message(KeyboardInput {
            window,
            key_code: KeyCode::Tab,
            logical_key: Key::Tab,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
        });
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(buttons[1]));
        app.world_mut().write_message(KeyboardInput {
            window,
            key_code: KeyCode::Tab,
            logical_key: Key::Tab,
            state: ButtonState::Released,
            text: None,
            repeat: false,
        });
        app.update();
        app.world_mut().resource_mut::<FrontendState>().screen = Screen::Playing;
        app.world_mut().write_message(KeyboardInput {
            window,
            key_code: KeyCode::Tab,
            logical_key: Key::Tab,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
        });
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(buttons[1]));
    }
}
