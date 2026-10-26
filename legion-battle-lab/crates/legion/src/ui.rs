//! Native game screens. This module reads application state and emits commands;
//! simulation, persistence, and navigation between screens belong to `app`.

use std::sync::atomic::Ordering;

use battle_core::{Formation, MAX_UNITS, MapKind, ScenarioConfig, Team, UnitCategory, UnitKind};
use bevy::{
    a11y::AccessibilityNode,
    input::{ButtonState, keyboard::KeyboardInput},
    input_focus::{
        AcquireFocus, FocusCause, FocusedInput, InputFocus,
        tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin},
    },
    picking::{Pickable, hover::Hovered},
    prelude::*,
    ui::{
        InteractionDisabled, Pressed, RelativeCursorPosition, UiSystems, prelude::AccessibleLabel,
    },
    ui_widgets::{Activate, Button},
};

use crate::{
    app::{AppAction, AppSet, ScenarioDraft, Screen, Session},
    crowd::CrowdRenderHealth,
    world::RenderCounts,
};

const INK: Color = Color::srgb(0.063, 0.098, 0.118);
const PANEL: Color = Color::srgb(0.106, 0.157, 0.184);
const CONTROL: Color = Color::srgb(0.165, 0.235, 0.267);
const HOVER: Color = Color::srgb(0.225, 0.310, 0.345);
const TEXT: Color = Color::srgb(0.949, 0.933, 0.894);
const MUTED: Color = Color::srgb(0.725, 0.788, 0.792);
const ACCENT: Color = Color::srgb(0.902, 0.667, 0.392);
const BLUE: Color = Color::srgb(0.333, 0.714, 0.820);
const RED: Color = Color::srgb(0.863, 0.463, 0.353);
const BORDER: Color = Color::srgb(0.254, 0.337, 0.365);

pub struct GameUiPlugin;

/// Camera input should be ignored while a panel owns the cursor.
#[derive(Resource, Default)]
pub struct UiPointerCapture(pub bool);

/// First-load readiness is latched per scenario; normal snapshot uploads must
/// not flash a loading banner on every simulation tick.
#[derive(Resource, Default)]
struct RenderFeedback {
    generation: u64,
    ready_generation: u64,
    pipeline_error: Option<String>,
}

/// Public so integration smoke tests can locate and activate real controls.
#[derive(Component)]
pub struct ActionButton(pub AppAction);

/// Count-step selection is a UI preference; actual changes still use AppAction.
#[derive(Component)]
pub struct CountStepButton {
    pub team: Team,
    pub step: i32,
}

#[derive(Component)]
struct CountAdjustButton {
    team: Team,
    direction: i32,
}

#[derive(Resource)]
struct CountSteps([i32; 2]);

impl Default for CountSteps {
    fn default() -> Self {
        Self([100, 100])
    }
}

#[derive(Component)]
struct UiRoot;

#[derive(Component)]
struct PointerPanel;

#[derive(Component, Clone, Copy)]
enum TextBinding {
    MenuScenario,
    MenuHint,
    BlueCount,
    RedCount,
    BlueDescription,
    RedDescription,
    SetupHint,
    Seed,
    Duration,
    Total,
    Status,
    BlueAlive,
    RedAlive,
    BlueLosses,
    RedLosses,
    Clock,
    BattleState,
    BattleMap,
    PauseAction,
    PauseTitle,
    PauseHint,
    PreparationTitle,
    PreparationHint,
    StartAction,
    QuickAction,
    Diagnostics,
    ResultTitle,
    ResultDetails,
}

#[derive(Component, Clone, Copy)]
enum ConditionalPanel {
    Pause,
    Result,
    Help,
    Diagnostics,
    Preparing,
}

#[derive(Component)]
struct ArmyBar(Team);

#[derive(Clone, Copy)]
enum Tone {
    Primary,
    Normal,
    Quiet,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Screen,
    Help,
    Result,
}

#[derive(Component)]
struct ButtonStyle {
    tone: Tone,
    scope: Scope,
}

struct Fonts {
    regular: Handle<Font>,
    semibold: Handle<Font>,
}

impl Fonts {
    fn load(assets: &AssetServer) -> Self {
        Self {
            regular: assets.load("fonts/Barlow-Regular.ttf"),
            semibold: assets.load("fonts/Barlow-SemiBold.ttf"),
        }
    }
}

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        app.init_resource::<UiPointerCapture>()
            .init_resource::<CountSteps>()
            .init_resource::<RenderFeedback>()
            .add_observer(activate_button)
            .add_observer(activate_count_step)
            .add_observer(navigate_count_step)
            .add_systems(OnEnter(Screen::Menu), menu_screen)
            .add_systems(OnEnter(Screen::Configure), configuration_screen)
            .add_systems(OnEnter(Screen::Battle), battle_screen)
            .add_systems(OnExit(Screen::Menu), remove_screen)
            .add_systems(OnExit(Screen::Configure), remove_screen)
            .add_systems(OnExit(Screen::Battle), remove_screen)
            .add_systems(
                PreUpdate,
                (update_pointer_capture, clear_world_focus)
                    .chain()
                    .after(UiSystems::Focus),
            )
            .add_systems(
                Update,
                (
                    update_render_feedback,
                    update_panels,
                    update_text,
                    update_army_bars,
                    sync_count_steps,
                    update_button_styles,
                )
                    .chain()
                    .in_set(AppSet::Presentation),
            );
    }
}

fn column(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(gap),
        min_width: px(0),
        ..default()
    }
}

fn row(gap: f32) -> Node {
    Node {
        align_items: AlignItems::Center,
        column_gap: px(gap),
        min_width: px(0),
        ..default()
    }
}

fn text_bundle(
    text: impl Into<String>,
    size: f32,
    color: Color,
    font: &Handle<Font>,
) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Pickable::IGNORE,
    )
}

fn label(parent: &mut ChildSpawnerCommands, fonts: &Fonts, text: &str, size: f32, color: Color) {
    parent.spawn(text_bundle(text, size, color, &fonts.regular));
}

fn heading(parent: &mut ChildSpawnerCommands, fonts: &Fonts, text: &str, size: f32, color: Color) {
    parent.spawn(text_bundle(text, size, color, &fonts.semibold));
}

fn bound_text(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    binding: TextBinding,
    size: f32,
    color: Color,
    bold: bool,
) {
    parent.spawn((
        text_bundle(
            "",
            size,
            color,
            if bold {
                &fonts.semibold
            } else {
                &fonts.regular
            },
        ),
        binding,
    ));
}

fn button(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    title: &str,
    action: AppAction,
    tone: Tone,
    scope: Scope,
) -> Entity {
    let action_label = match action {
        AppAction::TogglePause => Some(TextBinding::PauseAction),
        AppAction::StartBattle => Some(TextBinding::StartAction),
        AppAction::QuickBattle => Some(TextBinding::QuickAction),
        _ => None,
    };
    let entity = button_widget(parent, fonts, title, tone, scope, action_label);
    parent
        .commands()
        .entity(entity)
        .insert(ActionButton(action));
    entity
}

fn button_widget(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    title: &str,
    tone: Tone,
    scope: Scope,
    action_label: Option<TextBinding>,
) -> Entity {
    parent
        .spawn((
            Button,
            AccessibleLabel::new(title),
            Hovered::default(),
            TabIndex(0),
            Outline::new(px(2), px(2), Color::NONE),
            ButtonStyle { tone, scope },
            Node {
                min_height: px(38),
                min_width: px(38),
                padding: UiRect::axes(px(14), px(8)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(BORDER),
            BackgroundColor(if matches!(tone, Tone::Primary) {
                ACCENT
            } else {
                CONTROL
            }),
        ))
        .with_children(|p| {
            let mut entity = p.spawn(text_bundle(
                title,
                16.0,
                if matches!(tone, Tone::Primary) {
                    INK
                } else {
                    TEXT
                },
                &fonts.semibold,
            ));
            if let Some(binding) = action_label {
                entity.insert(binding);
            }
        })
        .id()
}

fn menu_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let fonts = Fonts::load(&assets);
    commands
        .spawn((
            UiRoot,
            TabGroup::new(0),
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(px(32)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(388),
                    height: percent(100),
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::all(px(28)),
                    border_radius: BorderRadius::all(px(12)),
                    ..column(20.0)
                },
                BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.96)),
                PointerPanel,
                RelativeCursorPosition::default(),
            ))
            .with_children(|panel| {
                panel.spawn(column(10.0)).with_children(|p| {
                    heading(p, &fonts, "LEGION", 54.0, TEXT);
                    heading(p, &fonts, "Battle Lab", 28.0, ACCENT);
                    label(p, &fonts, "Armeen aufstellen.\nSchlachten verstehen.", 22.0, TEXT);
                    p.spawn(Node { height: px(12), ..default() });
                    label(p, &fonts, "Stelle zwei Armeen aus sieben Kategorien auf. Erkunde ihre Schlacht in einer frei beweglichen 3D-Welt.", 17.0, MUTED);
                });
                panel.spawn(column(12.0)).with_children(|p| {
                    button(p, &fonts, "Sofort spielen", AppAction::QuickBattle, Tone::Primary, Scope::Screen);
                    button(p, &fonts, "Szenario konfigurieren", AppAction::Configure, Tone::Normal, Scope::Screen);
                    button(p, &fonts, "Steuerung", AppAction::ToggleHelp, Tone::Quiet, Scope::Screen);
                    button(p, &fonts, "Spiel beenden", AppAction::Quit, Tone::Quiet, Scope::Screen);
                });
                panel.spawn(column(6.0)).with_children(|p| {
                    label(p, &fonts, "Rust + Bevy · Entwicklungsprototyp", 14.0, MUTED);
                    label(p, &fonts, "Szenarien bis 1.000.000 Einheiten. Die CPU berechnet Kampfzonen detailliert und Reserven strategisch; die Darstellung nutzt Detailstufen.", 14.0, MUTED);
                    bound_text(p, &fonts, TextBinding::Status, 14.0, ACCENT, false);
                });
            });
            root.spawn(Node {
                align_self: AlignSelf::FlexEnd,
                max_width: px(390),
                padding: UiRect::all(px(18)),
                ..column(6.0)
            })
            .insert(BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.90)))
            .with_children(|p| {
                bound_text(p, &fonts, TextBinding::MenuScenario, 18.0, TEXT, true);
                bound_text(p, &fonts, TextBinding::MenuHint, 14.0, MUTED, false);
            });
            help_overlay(root, &fonts);
        });
}

fn configuration_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let fonts = Fonts::load(&assets);
    commands
        .spawn((
            UiRoot,
            TabGroup::new(0),
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(16)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..column(0.0)
            },
            BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.97)),
        ))
        .with_children(|root| {
            root.spawn(Node {
                width: percent(100),
                max_width: px(1140),
                ..column(8.0)
            })
            .with_children(|body| {
                body.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..row(20.0)
                })
                .with_children(|p| {
                    p.spawn(column(2.0)).with_children(|p| {
                        heading(p, &fonts, "Dein Schlachtfeld", 30.0, TEXT);
                        label(
                            p,
                            &fonts,
                            "Vorlage wählen, Armeen aufstellen und die Schlacht starten.",
                            15.0,
                            MUTED,
                        );
                    });
                    button(
                        p,
                        &fonts,
                        "Hauptmenü",
                        AppAction::MainMenu,
                        Tone::Quiet,
                        Scope::Screen,
                    );
                });
                body.spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: px(6),
                    ..row(8.0)
                })
                .with_children(|p| {
                    for (index, title) in [
                        "Ausgewogen",
                        "Brückenkampf",
                        "Fernkampf",
                        "Große Schlacht",
                        "1 Million",
                    ]
                    .iter()
                    .enumerate()
                    {
                        button(
                            p,
                            &fonts,
                            title,
                            AppAction::Preset(index),
                            Tone::Normal,
                            Scope::Screen,
                        );
                    }
                });
                body.spawn(column(4.0)).with_children(|p| {
                    p.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: px(6),
                        ..row(12.0)
                    })
                    .with_children(|p| {
                        p.spawn(row(6.0)).with_children(|p| {
                            heading(p, &fonts, "Gelände", 16.0, TEXT);
                            for map in MapKind::ALL {
                                button(
                                    p,
                                    &fonts,
                                    map.label(),
                                    AppAction::SetMap(map),
                                    Tone::Normal,
                                    Scope::Screen,
                                );
                            }
                        });
                        p.spawn(row(6.0)).with_children(|p| {
                            label(p, &fonts, "Startwert", 14.0, MUTED);
                            button(
                                p,
                                &fonts,
                                "−",
                                AppAction::ChangeSeed(-1),
                                Tone::Normal,
                                Scope::Screen,
                            );
                            bound_text(p, &fonts, TextBinding::Seed, 18.0, TEXT, true);
                            button(
                                p,
                                &fonts,
                                "+",
                                AppAction::ChangeSeed(1),
                                Tone::Normal,
                                Scope::Screen,
                            );
                        });
                        p.spawn(row(6.0)).with_children(|p| {
                            label(p, &fonts, "Zeitlimit", 14.0, MUTED);
                            button(
                                p,
                                &fonts,
                                "−30 s",
                                AppAction::ChangeDuration(-30),
                                Tone::Normal,
                                Scope::Screen,
                            );
                            bound_text(p, &fonts, TextBinding::Duration, 18.0, TEXT, true);
                            button(
                                p,
                                &fonts,
                                "+30 s",
                                AppAction::ChangeDuration(30),
                                Tone::Normal,
                                Scope::Screen,
                            );
                        });
                    });
                    bound_text(p, &fonts, TextBinding::SetupHint, 14.0, MUTED, false);
                });
                body.spawn(Node {
                    align_items: AlignItems::Stretch,
                    column_gap: px(16),
                    ..default()
                })
                .with_children(|p| {
                    army_editor(p, &fonts, Team::Blue);
                    army_editor(p, &fonts, Team::Red);
                });
                body.spawn(Node {
                    padding: UiRect::top(px(10)),
                    border: UiRect::top(px(1)),
                    justify_content: JustifyContent::SpaceBetween,
                    ..row(16.0)
                })
                .insert(BorderColor::all(BORDER))
                .with_children(|p| {
                    p.spawn(column(3.0)).with_children(|p| {
                        bound_text(p, &fonts, TextBinding::Total, 24.0, TEXT, true);
                        label(
                            p,
                            &fonts,
                            "Bis zu 1.000.000 insgesamt · 1k = 1.000",
                            14.0,
                            MUTED,
                        );
                    });
                    p.spawn(row(8.0)).with_children(|p| {
                        button(
                            p,
                            &fonts,
                            "Speichern",
                            AppAction::SaveScenario,
                            Tone::Quiet,
                            Scope::Screen,
                        );
                        button(
                            p,
                            &fonts,
                            "Laden",
                            AppAction::LoadScenario,
                            Tone::Quiet,
                            Scope::Screen,
                        );
                        button(
                            p,
                            &fonts,
                            "Schlacht starten",
                            AppAction::StartBattle,
                            Tone::Primary,
                            Scope::Screen,
                        );
                    });
                });
                bound_text(body, &fonts, TextBinding::Status, 14.0, ACCENT, false);
            });
            help_overlay(root, &fonts);
        });
}

fn army_editor(parent: &mut ChildSpawnerCommands, fonts: &Fonts, team: Team) {
    let color = team_color(team);
    let (count_binding, description_binding) = match team {
        Team::Blue => (TextBinding::BlueCount, TextBinding::BlueDescription),
        Team::Red => (TextBinding::RedCount, TextBinding::RedDescription),
    };
    parent
        .spawn((
            Node {
                flex_grow: 1.0,
                flex_basis: percent(50),
                padding: UiRect::all(px(12)),
                border_radius: BorderRadius::all(px(10)),
                ..column(6.0)
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..row(12.0)
            })
            .with_children(|p| {
                heading(p, fonts, army_name(team), 24.0, color);
                bound_text(p, fonts, count_binding, 28.0, TEXT, true);
            });
            p.spawn(row(6.0)).with_children(|p| {
                label(p, fonts, "Schritt", 14.0, MUTED);
                let minus = button(
                    p,
                    fonts,
                    "−",
                    AppAction::ChangeCount(team, -100),
                    Tone::Normal,
                    Scope::Screen,
                );
                p.commands().entity(minus).insert(CountAdjustButton {
                    team,
                    direction: -1,
                });
                for (title, step) in [
                    ("1", 1),
                    ("100", 100),
                    ("1k", 1_000),
                    ("10k", 10_000),
                    ("100k", 100_000),
                ] {
                    let entity = button_widget(p, fonts, title, Tone::Normal, Scope::Screen, None);
                    p.commands()
                        .entity(entity)
                        .insert(CountStepButton { team, step });
                }
                let plus = button(
                    p,
                    fonts,
                    "+",
                    AppAction::ChangeCount(team, 100),
                    Tone::Normal,
                    Scope::Screen,
                );
                p.commands()
                    .entity(plus)
                    .insert(CountAdjustButton { team, direction: 1 });
            });
            label(p, fonts, "Kategorie", 14.0, MUTED);
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(6),
                ..row(6.0)
            })
            .with_children(|p| {
                for category in UnitCategory::ALL {
                    button(
                        p,
                        fonts,
                        category.label(),
                        AppAction::SetCategory(team, category),
                        Tone::Normal,
                        Scope::Screen,
                    );
                }
            });
            p.spawn(row(6.0)).with_children(|p| {
                label(p, fonts, "Profil", 14.0, MUTED);
                for kind in UnitKind::ALL {
                    button(
                        p,
                        fonts,
                        profile_label(kind),
                        AppAction::SetKind(team, kind),
                        Tone::Normal,
                        Scope::Screen,
                    );
                }
            });
            bound_text(p, fonts, description_binding, 14.0, MUTED, false);
            p.spawn(row(6.0)).with_children(|p| {
                label(p, fonts, "Formation", 14.0, MUTED);
                for formation in Formation::ALL {
                    button(
                        p,
                        fonts,
                        formation.label(),
                        AppAction::SetFormation(team, formation),
                        Tone::Normal,
                        Scope::Screen,
                    );
                }
            });
        });
}

fn battle_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let fonts = Fonts::load(&assets);
    commands
        .spawn((
            UiRoot,
            TabGroup::new(0),
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                ..column(0.0)
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(100),
                    min_height: px(100),
                    padding: UiRect::axes(px(24), px(14)),
                    justify_content: JustifyContent::SpaceBetween,
                    ..row(24.0)
                },
                BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.95)),
                PointerPanel,
                RelativeCursorPosition::default(),
            ))
            .with_children(|p| {
                army_status(p, &fonts, Team::Blue);
                p.spawn(Node { align_items: AlignItems::Center, ..column(1.0) })
                    .with_children(|p| {
                        bound_text(p, &fonts, TextBinding::BattleMap, 14.0, MUTED, false);
                        bound_text(p, &fonts, TextBinding::Clock, 28.0, TEXT, true);
                        bound_text(p, &fonts, TextBinding::BattleState, 14.0, ACCENT, false);
                    });
                army_status(p, &fonts, Team::Red);
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(20),
                    top: px(120),
                    width: px(294),
                    padding: UiRect::all(px(16)),
                    border_radius: BorderRadius::all(px(8)),
                    display: Display::None,
                    ..column(8.0)
                },
                BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.94)),
                ConditionalPanel::Diagnostics,
                PointerPanel,
                RelativeCursorPosition::default(),
            ))
            .with_children(|p| {
                heading(p, &fonts, "Simulation", 19.0, TEXT);
                bound_text(p, &fonts, TextBinding::Diagnostics, 15.0, MUTED, false);
                label(p, &fonts, "Renderdaten vor Sichtprüfung.\nF3 blendet diese Werte aus.", 14.0, MUTED);
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(114),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
                ConditionalPanel::Pause,
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn((
                    Node { padding: UiRect::axes(px(22), px(10)), border_radius: BorderRadius::all(px(8)), ..row(12.0) },
                    BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.95)),
                    Pickable::IGNORE,
                ))
                .with_children(|p| {
                    bound_text(p, &fonts, TextBinding::PauseTitle, 20.0, ACCENT, true);
                    bound_text(p, &fonts, TextBinding::PauseHint, 15.0, TEXT, false);
                });
            });

            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(114),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
                ConditionalPanel::Preparing,
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn((
                    Node { width: px(380), max_width: percent(85), padding: UiRect::axes(px(22), px(12)), border_radius: BorderRadius::all(px(8)), ..column(4.0) },
                    BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.96)),
                    Pickable::IGNORE,
                ))
                .with_children(|p| {
                    bound_text(p, &fonts, TextBinding::PreparationTitle, 20.0, ACCENT, true);
                    bound_text(p, &fonts, TextBinding::PreparationHint, 15.0, TEXT, false);
                });
            });

            root.spawn((
                Node {
                    width: percent(100),
                    padding: UiRect::axes(px(20), px(12)),
                    ..column(9.0)
                },
                BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.95)),
                PointerPanel,
                RelativeCursorPosition::default(),
            ))
            .with_children(|p| {
                p.spawn(Node { justify_content: JustifyContent::SpaceBetween, flex_wrap: FlexWrap::Wrap, row_gap: px(8), ..row(8.0) })
                    .with_children(|p| {
                        p.spawn(row(6.0)).with_children(|p| {
                            button(p, &fonts, "Pause", AppAction::TogglePause, Tone::Primary, Scope::Screen);
                            button(p, &fonts, "Einzelschritt", AppAction::Step, Tone::Normal, Scope::Screen);
                            for (label, speed) in [("0,5×", 0.5), ("1×", 1.0), ("2×", 2.0), ("4×", 4.0)] {
                                button(p, &fonts, label, AppAction::SetSpeed(speed), Tone::Normal, Scope::Screen);
                            }
                        });
                        p.spawn(row(6.0)).with_children(|p| {
                            button(p, &fonts, "Neustart", AppAction::Restart, Tone::Quiet, Scope::Screen);
                            button(p, &fonts, "Szenario", AppAction::Configure, Tone::Quiet, Scope::Screen);
                            button(p, &fonts, "Menü", AppAction::MainMenu, Tone::Quiet, Scope::Screen);
                            button(p, &fonts, "Hilfe", AppAction::ToggleHelp, Tone::Quiet, Scope::Screen);
                        });
                    });
                p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(12.0) })
                    .with_children(|p| {
                        label(p, &fonts, "Ins Schlachtfeld klicken: WASD · Rechte Maus drehen · Mausrad zoomen · Esc pausiert / setzt fort", 14.0, MUTED);
                        label(p, &fonts, "F3 Diagnose", 14.0, MUTED);
                    });
                bound_text(p, &fonts, TextBinding::Status, 14.0, ACCENT, false);
            });
            result_panel(root, &fonts);
            help_overlay(root, &fonts);
        });
}

fn army_status(parent: &mut ChildSpawnerCommands, fonts: &Fonts, team: Team) {
    let (alive_binding, loss_binding) = match team {
        Team::Blue => (TextBinding::BlueAlive, TextBinding::BlueLosses),
        Team::Red => (TextBinding::RedAlive, TextBinding::RedLosses),
    };
    parent
        .spawn(Node {
            width: percent(32),
            max_width: px(370),
            ..column(5.0)
        })
        .with_children(|p| {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..row(8.0)
            })
            .with_children(|p| {
                heading(p, fonts, army_name(team), 20.0, team_color(team));
                bound_text(p, fonts, alive_binding, 26.0, TEXT, true);
            });
            p.spawn((
                Node {
                    width: percent(100),
                    height: px(4),
                    ..default()
                },
                BackgroundColor(CONTROL),
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(team_color(team)),
                    ArmyBar(team),
                ));
            });
            bound_text(p, fonts, loss_binding, 14.0, MUTED, false);
        });
}

fn result_panel(parent: &mut ChildSpawnerCommands, fonts: &Fonts) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: px(426),
                top: percent(25),
                align_self: AlignSelf::Center,
                padding: UiRect::all(px(26)),
                border_radius: BorderRadius::all(px(12)),
                display: Display::None,
                ..column(16.0)
            },
            BackgroundColor(Color::srgba(0.063, 0.098, 0.118, 0.97)),
            ConditionalPanel::Result,
            PointerPanel,
            RelativeCursorPosition::default(),
            ZIndex(10),
        ))
        .with_children(|p| {
            bound_text(p, fonts, TextBinding::ResultTitle, 30.0, ACCENT, true);
            bound_text(p, fonts, TextBinding::ResultDetails, 17.0, TEXT, false);
            button(
                p,
                fonts,
                "Erneut starten",
                AppAction::Restart,
                Tone::Primary,
                Scope::Result,
            );
            button(
                p,
                fonts,
                "Szenario ändern",
                AppAction::Configure,
                Tone::Normal,
                Scope::Result,
            );
            label(
                p,
                fonts,
                "Die Kamera bleibt außerhalb dieses Fensters bedienbar.",
                14.0,
                MUTED,
            );
        });
}

fn help_overlay(parent: &mut ChildSpawnerCommands, fonts: &Fonts) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.055, 0.070, 0.90)),
        ConditionalPanel::Help,
        TabGroup::modal(),
        ZIndex(100),
    ))
    .observe(contain_help_focus)
    .with_children(|p| {
        p.spawn((
            Node {
                width: px(570),
                max_width: percent(92),
                padding: UiRect::all(px(30)),
                border_radius: BorderRadius::all(px(12)),
                ..column(12.0)
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            heading(p, fonts, "Das Schlachtfeld erkunden", 30.0, TEXT);
            for (keys, description) in [
                ("W A S D / Shift", "Nach Klick ins Feld: bewegen / schneller"),
                ("Q / E", "Kamera nach links / rechts drehen"),
                ("Rechte Maustaste", "Gedrückt halten und Kamera drehen"),
                ("Mausrad", "Zum Schlachtfeld hin- und herauszoomen"),
                ("R", "Kamera auf die Ausgangsansicht setzen"),
                ("Leertaste / Esc", "Pause / fortsetzen; Leertaste nach Klick ins Feld"),
                ("F3", "Simulationsdiagnose ein- und ausblenden"),
                ("F1", "Diese Hilfe öffnen oder schließen"),
                ("Tab / Enter", "Schaltflächen wählen und aktivieren"),
                ("Pfeiltasten", "Fokussierte Schrittweite ändern"),
            ] {
                p.spawn(row(16.0)).with_children(|p| {
                    p.spawn(Node { width: px(152), flex_shrink: 0.0, ..default() })
                        .with_children(|p| heading(p, fonts, keys, 16.0, ACCENT));
                    label(p, fonts, description, 16.0, TEXT);
                });
            }
            label(p, fonts, "Im Gefecht pausiert die Hilfe nach dem aktuellen Tick. Esc schließt die Hilfe; nochmals Esc setzt die Schlacht fort.", 15.0, MUTED);
            label(p, fonts, "Die Übersicht stellt Einheiten als Punkte dar. Aus der Nähe kommen Detailmodelle hinzu. F3 zeigt die tatsächlichen Simulations- und Renderzahlen.", 14.0, MUTED);
            label(p, fonts, "Angriffe prüfen Abstand und feste Kategoriehöhe: Boden 0 m, Air Unit +8 m, Space Unit +32 m, Underground Unit −3 m. Die Reichweite muss auch den Höhenabstand überbrücken.", 14.0, MUTED);
            label(p, fonts, "Sea Unit bewegt sich frei über die Karte. Underground Unit hat einen Oberflächenmarker, kämpft aber auf −3 m.", 14.0, MUTED);
            button(p, fonts, "Verstanden", AppAction::ToggleHelp, Tone::Primary, Scope::Help);
        });
    });
}

fn contain_help_focus(
    mut event: On<AcquireFocus>,
    buttons: Query<(Entity, &ButtonStyle)>,
    mut focus: ResMut<InputFocus>,
) {
    // A click on the backdrop or help text must not bubble to the window and
    // clear focus: Tab navigation can only stay in a modal that owns focus.
    event.propagate(false);
    if let Some((entity, _)) = buttons.iter().find(|(_, style)| style.scope == Scope::Help)
        && focus.get() != Some(entity)
    {
        focus.set(entity, FocusCause::Pressed);
    }
}

fn activate_button(
    event: On<Activate>,
    buttons: Query<(
        &ActionButton,
        Option<&CountAdjustButton>,
        Has<InteractionDisabled>,
    )>,
    steps: Res<CountSteps>,
    mut actions: MessageWriter<AppAction>,
) {
    if let Ok((button, adjust, false)) = buttons.get(event.entity) {
        // Resolve the current step at activation, including multiple keyboard
        // events in one frame before the presentation systems have run.
        let action = adjust.map_or_else(
            || button.0.clone(),
            |adjust| {
                AppAction::ChangeCount(adjust.team, adjust.direction * steps.0[adjust.team.index()])
            },
        );
        actions.write(action);
    }
}

fn activate_count_step(
    event: On<Activate>,
    buttons: Query<(&CountStepButton, Has<InteractionDisabled>)>,
    mut steps: ResMut<CountSteps>,
) {
    if let Ok((button, false)) = buttons.get(event.entity) {
        steps.0[button.team.index()] = button.step;
    }
}

fn navigate_count_step(
    mut event: On<FocusedInput<KeyboardInput>>,
    buttons: Query<(Entity, &CountStepButton, Has<InteractionDisabled>)>,
    mut steps: ResMut<CountSteps>,
    mut focus: ResMut<InputFocus>,
) {
    let Ok((_, button, false)) = buttons.get(event.focused_entity) else {
        return;
    };
    if event.input.state != ButtonState::Pressed {
        return;
    }
    const OPTIONS: [i32; 5] = [1, 100, 1_000, 10_000, 100_000];
    let index = OPTIONS
        .iter()
        .position(|step| *step == button.step)
        .unwrap_or(1);
    let next = match event.input.key_code {
        KeyCode::ArrowLeft | KeyCode::ArrowDown => index.saturating_sub(1),
        KeyCode::ArrowRight | KeyCode::ArrowUp => (index + 1).min(OPTIONS.len() - 1),
        KeyCode::Home => 0,
        KeyCode::End => OPTIONS.len() - 1,
        _ => return,
    };
    event.propagate(false);
    let step = OPTIONS[next];
    if let Some((entity, _, _)) = buttons.iter().find(|(_, choice, disabled)| {
        !*disabled && choice.team == button.team && choice.step == step
    }) {
        steps.0[button.team.index()] = step;
        focus.set(entity, FocusCause::Navigated);
    }
}

fn sync_count_steps(
    steps: Res<CountSteps>,
    mut buttons: Query<(&CountAdjustButton, &mut ActionButton)>,
) {
    for (adjust, mut button) in &mut buttons {
        let next =
            AppAction::ChangeCount(adjust.team, adjust.direction * steps.0[adjust.team.index()]);
        if button.0 != next {
            button.0 = next;
        }
    }
}

fn remove_screen(
    mut commands: Commands,
    roots: Query<Entity, With<UiRoot>>,
    mut focus: ResMut<InputFocus>,
) {
    focus.clear();
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn update_pointer_capture(
    screen: Res<State<Screen>>,
    session: Res<Session>,
    panels: Query<(&Node, &RelativeCursorPosition), With<PointerPanel>>,
    mut capture: ResMut<UiPointerCapture>,
) {
    capture.0 = *screen.get() != Screen::Battle
        || session.help
        || panels
            .iter()
            .any(|(node, cursor)| node.display != Display::None && cursor.cursor_over());
}

fn clear_world_focus(
    mouse: Res<ButtonInput<MouseButton>>,
    capture: Res<UiPointerCapture>,
    mut focus: ResMut<InputFocus>,
) {
    if !capture.0
        && (mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right))
    {
        focus.clear();
    }
}

fn update_panels(
    session: Res<Session>,
    render_feedback: Res<RenderFeedback>,
    mut panels: Query<(&ConditionalPanel, &mut Node)>,
    buttons: Query<(Entity, &ButtonStyle)>,
    mut focus: ResMut<InputFocus>,
    mut help_was_visible: Local<bool>,
) {
    if *help_was_visible != session.help {
        // TabNavigation enters a modal only when focus already belongs to it.
        // Conversely, a closed modal must not retain focus on its hidden button.
        if session.help {
            if let Some((entity, _)) = buttons.iter().find(|(_, style)| style.scope == Scope::Help)
            {
                focus.set(entity, FocusCause::Auto);
            }
        } else {
            focus.clear();
        }
        *help_was_visible = session.help;
    }
    let finished = session
        .battle
        .as_ref()
        .is_some_and(|battle| battle.outcome.is_some());
    let preparing = session.preparing || render_pending(&session, &render_feedback);
    let display_failed = render_feedback.pipeline_error.is_some();
    for (panel, mut node) in &mut panels {
        let visible = match panel {
            ConditionalPanel::Pause => {
                (session.faulted || (session.paused && !finished))
                    && !session.help
                    && !preparing
                    && !display_failed
            }
            ConditionalPanel::Result => finished && !session.help && !preparing && !session.faulted,
            ConditionalPanel::Help => session.help,
            ConditionalPanel::Diagnostics => session.diagnostics && !session.help,
            ConditionalPanel::Preparing => (preparing || display_failed) && !session.help,
        };
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}

fn update_text(
    draft: Res<ScenarioDraft>,
    session: Res<Session>,
    render: Option<Res<RenderCounts>>,
    render_feedback: Res<RenderFeedback>,
    mut labels: Query<(&TextBinding, &mut Text)>,
) {
    let config = &draft.0;
    let battle = session.battle.as_ref();
    let awaiting_render = render_pending(&session, &render_feedback);
    let display_failed = render_feedback.pipeline_error.is_some();
    for (binding, mut text) in &mut labels {
        let value = match binding {
            TextBinding::MenuScenario => {
                if session.preparing { "Szenario wird vorbereitet …".into() }
                else if display_failed { "Darstellung nicht verfügbar".into() }
                else if awaiting_render { "Vorschau wird aufgebaut …".into() }
                else {
                    let preview = battle.map_or(config, |battle| &battle.config);
                    format!("{} · {} gegen {}", preview.map.label(), number(preview.blue.count as u64), number(preview.red.count as u64))
                }
            }
            TextBinding::MenuHint => {
                if session.faulted { "Vorbereitung fehlgeschlagen. Versuche es erneut oder ändere das Szenario.".into() }
                else if display_failed { "Szenario und Menü bleiben bedienbar. Details zum Grafikfehler stehen im Protokoll.".into() }
                else if awaiting_render { "Die Einheiten werden für die neue Vorschau aufbereitet.".into() }
                else if session.preparing && render.as_ref().is_some_and(|r| r.represented_units > 0) { "Die bisherige Welt bleibt während der Szenariovorbereitung sichtbar.".into() }
                else if session.preparing { "Einheiten werden angelegt. Menü und Hilfe sind verfügbar.".into() }
                else { "Vorschau · Die Schlacht beginnt mit „Sofort spielen“.".into() }
            }
            TextBinding::BlueCount => number(config.blue.count as u64),
            TextBinding::RedCount => number(config.red.count as u64),
            TextBinding::BlueDescription => profile_description(config.blue.category, config.blue.kind),
            TextBinding::RedDescription => profile_description(config.red.category, config.red.kind),
            TextBinding::SetupHint => format!("{}   ·   {}", config.map.description(), matchup_hint(config)),
            TextBinding::Seed => number(config.seed),
            TextBinding::Duration => clock(config.time_limit_secs as f32),
            TextBinding::Total => format!("{} Einheiten", number(config.total_units() as u64)),
            TextBinding::Status => {
                if display_failed { "Darstellung nicht verfügbar. Details im Protokoll. Szenario und Menü bleiben bedienbar.".into() }
                else if !session.status.is_empty() { session.status.clone() }
                else if session.faulted { "Simulation angehalten. Starte erneut oder ändere das Szenario.".into() }
                else if session.preparing { "Szenario wird vorbereitet … Menü und Hilfe bleiben verfügbar.".into() }
                else { String::new() }
            }
            TextBinding::BlueAlive => battle.map_or_else(|| "—".into(), |b| number(b.stats.alive[0] as u64)),
            TextBinding::RedAlive => battle.map_or_else(|| "—".into(), |b| number(b.stats.alive[1] as u64)),
            TextBinding::BlueLosses => battle.map_or_else(String::new, |b| format!("{} gefallen · {} gestartet", number(b.stats.casualties[0] as u64), number(b.config.blue.count as u64))),
            TextBinding::RedLosses => battle.map_or_else(String::new, |b| format!("{} gefallen · {} gestartet", number(b.stats.casualties[1] as u64), number(b.config.red.count as u64))),
            TextBinding::Clock => battle.map_or_else(|| "00:00".into(), |b| format!("{} / {}", clock(b.stats.elapsed_secs), clock(b.config.time_limit_secs as f32))),
            TextBinding::BattleState => {
                if session.faulted { "Simulation angehalten · Fehler".into() }
                else if session.preparing { "Szenario wird vorbereitet …".into() }
                else if display_failed { "Grafikfehler · Simulation bleibt steuerbar".into() }
                else if awaiting_render { "Darstellung wird aufgebaut …".into() }
                else if battle.is_some_and(|b| b.outcome.is_some()) { "Schlacht beendet".into() }
                else if session.paused && session.busy { "Pausiert · letzter Tick läuft aus".into() }
                else if session.paused { "Simulation pausiert".into() }
                else { format!("Simulation läuft · Ziel {}×", speed_label(session.speed)) }
            }
            TextBinding::BattleMap => battle.map_or_else(String::new, |b| b.config.map.label().into()),
            TextBinding::PauseAction => if session.paused { "Fortsetzen".into() } else { "Pause".into() },
            TextBinding::PauseTitle => {
                if session.faulted { "Simulationfehler".into() }
                else if session.busy { "Pause angefordert".into() }
                else { "Pausiert".into() }
            }
            TextBinding::PauseHint => {
                if session.faulted { "Neustart oder Szenario wählen".into() }
                else if session.busy { "Aktueller Tick wird noch beendet".into() }
                else { "Esc setzt fort · Kamera frei".into() }
            }
            TextBinding::PreparationTitle => {
                if display_failed { "Darstellung nicht verfügbar".into() }
                else if session.preparing { "Szenario wird vorbereitet …".into() }
                else { "Darstellung wird aufgebaut …".into() }
            },
            TextBinding::PreparationHint => {
                if display_failed { "Szenario oder Menü bleiben verfügbar. Details zum Grafikfehler stehen im Protokoll.".into() }
                else if !session.preparing { "Die Darstellung wird aufbereitet. Die Simulation bleibt steuerbar.".into() }
                else if render.as_ref().is_some_and(|r| r.represented_units > 0) { "Die letzte fertige Welt bleibt sichtbar. Menü und Hilfe sind verfügbar.".into() }
                else { "Einheiten werden angelegt. Menü und Hilfe sind verfügbar.".into() }
            }
            TextBinding::StartAction => if session.preparing { "Vorbereitung läuft …".into() } else { "Schlacht starten".into() },
            TextBinding::QuickAction => if session.preparing { "Vorbereitung läuft …".into() } else if session.faulted { "Erneut versuchen".into() } else { "Sofort spielen".into() },
            TextBinding::Diagnostics => diagnostic_text(&session, render.as_deref()),
            TextBinding::ResultTitle => battle.and_then(|b| b.outcome.as_ref()).map_or_else(String::new, |o| o.label()),
            TextBinding::ResultDetails => battle.map_or_else(String::new, |b| format!(
                "Dauer: {}\nBlaue Armee: {} Überlebende\nRote Armee: {} Überlebende\n\nPasse dein Szenario an oder wiederhole die Schlacht unter denselben Bedingungen.",
                clock(b.stats.elapsed_secs), number(b.stats.alive[0] as u64), number(b.stats.alive[1] as u64),
            )),
        };
        // Changing Text triggers layout/shaping. Leave identical values untouched.
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn update_army_bars(session: Res<Session>, mut bars: Query<(&ArmyBar, &mut Node)>) {
    let Some(battle) = &session.battle else {
        return;
    };
    for (bar, mut node) in &mut bars {
        let initial = match bar.0 {
            Team::Blue => battle.config.blue.count,
            Team::Red => battle.config.red.count,
        };
        let fraction = battle.stats.alive[bar.0.index()] as f32 / initial.max(1) as f32;
        let width = percent((fraction * 100.0).clamp(0.0, 100.0));
        if node.width != width {
            node.width = width;
        }
    }
}

// Explicit ECS accesses keep visual, keyboard and accessibility states in one
// pass; each parameter remains visible to Bevy's scheduler and borrow checks.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_button_styles(
    mut commands: Commands,
    draft: Res<ScenarioDraft>,
    session: Res<Session>,
    steps: Res<CountSteps>,
    mut focus: ResMut<InputFocus>,
    mut buttons: Query<(
        Entity,
        Option<&ActionButton>,
        Option<&CountStepButton>,
        &ButtonStyle,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut Outline,
        &mut TabIndex,
        &Children,
        &AccessibleLabel,
        &mut AccessibilityNode,
    )>,
    mut text_colors: Query<&mut TextColor>,
    text_values: Query<&Text>,
) {
    let finished = session.battle.as_ref().is_some_and(|b| b.outcome.is_some());
    for (
        entity,
        button,
        count_step,
        style,
        hovered,
        pressed,
        was_disabled,
        mut background,
        mut border,
        mut outline,
        mut tab_index,
        children,
        accessible_label,
        mut accessible,
    ) in &mut buttons
    {
        let disabled = (session.help && style.scope != Scope::Help)
            || (!session.help && style.scope == Scope::Help)
            || (style.scope == Scope::Result
                && (!finished || session.preparing || session.faulted))
            || (session.preparing && count_step.is_some())
            || button.is_some_and(|button| disabled_action(&button.0, &draft, &session, finished));
        if disabled != was_disabled {
            if disabled {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
        // InteractionDisabled prevents activation, but native tab navigation
        // filters TabIndex/visibility rather than InteractionDisabled itself.
        // A count-step group is one Tab stop; arrows move its selection/focus.
        let unselected_step =
            count_step.is_some_and(|button| steps.0[button.team.index()] != button.step);
        let next_tab_index = if disabled || unselected_step { -1 } else { 0 };
        if tab_index.0 != next_tab_index {
            tab_index.0 = next_tab_index;
        }
        if disabled && pressed {
            commands.entity(entity).remove::<Pressed>();
        }
        if disabled && focus.get() == Some(entity) {
            focus.clear();
        }
        let selected = count_step.is_some_and(|button| steps.0[button.team.index()] == button.step)
            || button.is_some_and(|button| selected_action(&button.0, &draft, &session));
        // Headless Bevy buttons provide a role, but do not derive accessible
        // names from their Text children. Keep their names and states in sync
        // with both dynamic captions and the team-specific controls.
        let visible_label = children
            .iter()
            .find_map(|child| text_values.get(child).ok())
            .map_or("", |text| text.0.as_str());
        let label = accessible_button_label(button, count_step, visible_label);
        if accessible_label.0 != label {
            commands.entity(entity).insert(AccessibleLabel::new(label));
        }
        if accessible.is_disabled() != disabled {
            if disabled {
                accessible.set_disabled();
            } else {
                accessible.clear_disabled();
            }
        }
        let has_selection = count_step.is_some()
            || button.is_some_and(|button| {
                matches!(
                    button.0,
                    AppAction::SetMap(_)
                        | AppAction::SetCategory(..)
                        | AppAction::SetKind(..)
                        | AppAction::SetFormation(..)
                        | AppAction::SetSpeed(_)
                )
            });
        if has_selection {
            if accessible.toggled() != Some(selected.into()) {
                accessible.set_toggled(selected.into());
            }
        } else if accessible.toggled().is_some() {
            accessible.clear_toggled();
        }
        let hidden = (session.help && style.scope != Scope::Help)
            || (!session.help && style.scope == Scope::Help)
            || (style.scope == Scope::Result
                && (!finished || session.preparing || session.faulted));
        if accessible.is_hidden() != hidden {
            if hidden {
                accessible.set_hidden();
            } else {
                accessible.clear_hidden();
            }
        }
        let focused = focus.get() == Some(entity);
        let outline_color = if focused && !disabled {
            ACCENT
        } else {
            Color::NONE
        };
        if outline.color != outline_color {
            outline.color = outline_color;
        }
        let primary = matches!(style.tone, Tone::Primary);
        let (bg, edge, fg) = if disabled {
            (PANEL, PANEL, Color::srgb(0.42, 0.49, 0.51))
        } else if pressed {
            (ACCENT, ACCENT, INK)
        } else if primary {
            (
                if hovered.get() {
                    Color::srgb(1.0, 0.76, 0.47)
                } else {
                    ACCENT
                },
                if focused { TEXT } else { ACCENT },
                INK,
            )
        } else if selected {
            (Color::srgb(0.22, 0.29, 0.30), ACCENT, TEXT)
        } else if hovered.get() || focused {
            (HOVER, if focused { ACCENT } else { MUTED }, TEXT)
        } else {
            (
                if matches!(style.tone, Tone::Quiet) {
                    PANEL
                } else {
                    CONTROL
                },
                BORDER,
                TEXT,
            )
        };
        if background.0 != bg {
            background.0 = bg;
        }
        let next_border = BorderColor::all(edge);
        if *border != next_border {
            *border = next_border;
        }
        for child in children.iter() {
            if let Ok(mut color) = text_colors.get_mut(child)
                && color.0 != fg
            {
                color.0 = fg;
            }
        }
    }
}

fn accessible_button_label(
    action: Option<&ActionButton>,
    step: Option<&CountStepButton>,
    visible_label: &str,
) -> String {
    if let Some(step) = step {
        return format!(
            "{}: Schrittweite {} Einheiten",
            army_name(step.team),
            number(step.step as u64),
        );
    }
    match action.map(|action| &action.0) {
        Some(AppAction::ChangeCount(team, delta)) => format!(
            "{}: Einheitenzahl um {} {}",
            army_name(*team),
            number(delta.unsigned_abs() as u64),
            if *delta < 0 { "verringern" } else { "erhöhen" },
        ),
        Some(AppAction::SetCategory(team, _)) => {
            format!("{}: Kategorie {visible_label}", army_name(*team))
        }
        Some(AppAction::SetKind(team, _)) => {
            format!("{}: Profil {visible_label}", army_name(*team))
        }
        Some(AppAction::SetFormation(team, _)) => {
            format!("{}: Formation {visible_label}", army_name(*team))
        }
        Some(AppAction::ChangeSeed(delta)) => format!(
            "Startwert {}",
            if *delta < 0 { "verringern" } else { "erhöhen" },
        ),
        Some(AppAction::ChangeDuration(delta)) => format!(
            "Zeitlimit um {} Sekunden {}",
            delta.unsigned_abs(),
            if *delta < 0 { "verringern" } else { "erhöhen" },
        ),
        Some(AppAction::Preset(_)) => format!("Szenariovorlage {visible_label}"),
        Some(AppAction::SetMap(_)) => format!("Gelände {visible_label}"),
        Some(AppAction::SetSpeed(_)) => format!("Simulationsgeschwindigkeit {visible_label}"),
        _ => visible_label.to_owned(),
    }
}

fn selected_action(action: &AppAction, draft: &ScenarioDraft, session: &Session) -> bool {
    match action {
        AppAction::SetMap(map) => draft.0.map == *map,
        AppAction::SetCategory(team, category) => match team {
            Team::Blue => draft.0.blue.category == *category,
            Team::Red => draft.0.red.category == *category,
        },
        AppAction::SetKind(team, kind) => match team {
            Team::Blue => draft.0.blue.kind == *kind,
            Team::Red => draft.0.red.kind == *kind,
        },
        AppAction::SetFormation(team, formation) => match team {
            Team::Blue => draft.0.blue.formation == *formation,
            Team::Red => draft.0.red.formation == *formation,
        },
        AppAction::SetSpeed(speed) => (session.speed - speed).abs() < 0.001,
        _ => false,
    }
}

fn disabled_action(
    action: &AppAction,
    draft: &ScenarioDraft,
    session: &Session,
    finished: bool,
) -> bool {
    if session.preparing
        && matches!(
            action,
            AppAction::QuickBattle
                | AppAction::StartBattle
                | AppAction::Restart
                | AppAction::Preset(_)
                | AppAction::SetMap(_)
                | AppAction::SetCategory(..)
                | AppAction::SetKind(..)
                | AppAction::SetFormation(..)
                | AppAction::ChangeCount(..)
                | AppAction::ChangeSeed(_)
                | AppAction::ChangeDuration(_)
                | AppAction::LoadScenario
                | AppAction::TogglePause
                | AppAction::Step
                | AppAction::SetSpeed(_)
        )
    {
        return true;
    }
    match action {
        AppAction::ChangeCount(team, delta) => {
            let count = match team {
                Team::Blue => draft.0.blue.count,
                Team::Red => draft.0.red.count,
            } as i64;
            let next = count + *delta as i64;
            next < 1 || draft.0.total_units() as i64 + *delta as i64 > MAX_UNITS as i64
        }
        AppAction::ChangeSeed(delta) => {
            (*delta < 0 && draft.0.seed == 0) || (*delta > 0 && draft.0.seed == u64::MAX)
        }
        AppAction::ChangeDuration(delta) => {
            let seconds = draft.0.time_limit_secs as i64 + *delta as i64;
            !(30..=3600).contains(&seconds)
        }
        AppAction::StartBattle => draft.0.validate().is_err(),
        AppAction::Step => {
            !session.paused
                || finished
                || session.busy
                || session.single_step
                || !session.is_ready()
        }
        AppAction::TogglePause | AppAction::SetSpeed(_) => finished || !session.is_ready(),
        _ => false,
    }
}

fn profile_label(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Infantry => "Ausgewogen",
        UnitKind::Heavy => "Schwer",
        UnitKind::Ranger => "Fernkampf",
    }
}

fn profile_description(category: UnitCategory, kind: UnitKind) -> String {
    let definition = category.definition(kind);
    format!(
        "LP {:.0} · Tempo {} m/s · Höhe {} m · Reichweite {} m",
        definition.health,
        decimal(definition.speed),
        decimal(category.elevation()),
        decimal(definition.range),
    )
}

fn decimal(value: f32) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .replace('.', ",")
}

fn matchup_hint(config: &ScenarioConfig) -> String {
    if config.blue.category == UnitCategory::Building
        && config.red.category == UnitCategory::Building
    {
        return "Beide Armeen sind stationär. Außerhalb ihrer Reichweite findet kein Kampf statt."
            .into();
    }
    let height = (config.blue.category.elevation() - config.red.category.elevation()).abs();
    let blue_reaches = config.blue.category.definition(config.blue.kind).range >= height;
    let red_reaches = config.red.category.definition(config.red.kind).range >= height;
    let explanation = match (blue_reaches, red_reaches) {
        (false, false) => "Beide Armeen können sich mit diesen Profilen nicht treffen.",
        (false, true) => "Blau kann Rot mit diesem Profil nicht treffen.",
        (true, false) => "Rot kann Blau mit diesem Profil nicht treffen.",
        (true, true) => "Für Treffer zählen Höhenabstand und Entfernung am Boden.",
    };
    format!("Höhenabstand {} m · {explanation}", decimal(height))
}

fn update_render_feedback(
    session: Res<Session>,
    render: Option<Res<RenderCounts>>,
    health: Option<Res<CrowdRenderHealth>>,
    mut feedback: ResMut<RenderFeedback>,
) {
    if feedback.generation != session.generation {
        feedback.generation = session.generation;
        feedback.pipeline_error = None;
    }
    let (Some(render), Some(health)) = (render, health) else {
        return;
    };
    let health = &health.0;
    if health.generation.load(Ordering::Acquire) != session.generation {
        return;
    }
    let pipeline_error = health
        .shader_error
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if pipeline_error != feedback.pipeline_error {
        if let Some(error) = &pipeline_error {
            // Preserve the complete technical diagnostic without overflowing
            // a game menu or hiding navigation behind an error dialog.
            error!("Darstellung fehlgeschlagen: {error}");
        }
        feedback.pipeline_error = pipeline_error;
    }
    let empty_world = session
        .battle
        .as_ref()
        .is_some_and(|battle| battle.stats.alive == [0, 0] && render.submitted_points == 0);
    if session.is_ready()
        && render.generation == session.generation
        && render.source_tick == health.source_tick.load(Ordering::Acquire)
        && render.submitted_points == health.uploaded_points.load(Ordering::Acquire)
        && feedback.pipeline_error.is_none()
        && (health.pipelines_ready.load(Ordering::Acquire) || empty_world)
    {
        feedback.ready_generation = session.generation;
    }
}

fn render_pending(session: &Session, feedback: &RenderFeedback) -> bool {
    session.is_ready()
        && feedback.ready_generation != session.generation
        && feedback.pipeline_error.is_none()
}

fn diagnostic_text(session: &Session, render: Option<&RenderCounts>) -> String {
    let worker = if session.faulted {
        "Fehler"
    } else if session.preparing {
        "initialisiert"
    } else if session.busy {
        "rechnet"
    } else {
        "bereit"
    };
    let mut text = format!(
        "{:.0} FPS · {:.1} ms / Frame\nCPU-Worker: {worker}",
        session.smoothed_fps, session.last_frame_ms
    );
    if let Some(battle) = &session.battle {
        text.push_str(&format!(
            "\nCPU-Tick: {:.2} ms · Soll: 20 Hz\nSimulation: Tick {}\nDetailliert: {}\nStrategisch: {}\nNachbarprüfungen: {}\nCPU-Daten ≈ {:.1} MiB\nRückstand: {:.2} s\nNicht nachgeholt: {:.2} s\nSnapshot: {:.2} ms",
            battle.stats.step_ms, number(battle.stats.tick), number(battle.stats.full_detail_units as u64),
            number(battle.stats.strategic_units as u64), number(battle.stats.neighbor_checks),
            battle.stats.estimated_state_bytes as f64 / 1_048_576.0,
            session.accumulator, session.dropped_time, session.snapshot_ms,
        ));
    }
    if let Some(render) = render {
        text.push_str(&format!(
            "\nRender-Tick: {}{}\nGPU-Punkte: {}\nDetailmodelle: {}\nNur Punkte: {}\nDraw-Batches: {}\nRenderdaten: {:.1} MiB\nAufbereitung: {:.2} ms",
            number(render.source_tick), if render.packing { " · lädt" } else { "" },
            number(render.submitted_points as u64), number(render.detail_models as u64),
            number(render.point_only_units as u64), render.draw_batches,
            render.snapshot_bytes as f64 / 1_048_576.0, render.packing_ms,
        ));
    }
    text
}

fn army_name(team: Team) -> &'static str {
    match team {
        Team::Blue => "Blaue Armee",
        Team::Red => "Rote Armee",
    }
}

fn team_color(team: Team) -> Color {
    match team {
        Team::Blue => BLUE,
        Team::Red => RED,
    }
}

fn clock(seconds: f32) -> String {
    let seconds = seconds.max(0.0) as u32;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn speed_label(speed: f32) -> String {
    if speed.fract().abs() < 0.01 {
        format!("{speed:.0}")
    } else {
        format!("{speed:.1}").replace('.', ",")
    }
}

fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    grouped
}
