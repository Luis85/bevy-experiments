//! Shared native game typography and controls. This module owns no simulation state.
use bevy::input_focus::{InputFocus, tab_navigation::TabIndex};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed, prelude::AccessibleLabel};
use bevy::ui_widgets::Button as WidgetButton;

pub const PANEL: Color = Color::srgb(0.05, 0.06, 0.08);
pub const FIELD: Color = Color::srgb(0.025, 0.035, 0.045);
pub const TEXT: Color = Color::srgb(0.93, 0.95, 0.90);
pub const MUTED: Color = Color::srgb(0.69, 0.77, 0.73);
pub const GOLD: Color = Color::srgb(0.95, 0.80, 0.45);
pub const BORDER: Color = Color::srgb(0.32, 0.38, 0.36);
pub const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);
pub const HOVER: Color = Color::srgb(0.27, 0.32, 0.40);
pub const PRIMARY: Color = Color::srgb(0.22, 0.37, 0.28);
pub const PRIMARY_HOVER: Color = Color::srgb(0.29, 0.45, 0.34);
pub const DISABLED: Color = Color::srgb(0.10, 0.12, 0.15);
pub const ERROR: Color = Color::srgb(1.0, 0.66, 0.59);
pub const SUCCESS: Color = Color::srgb(0.66, 0.88, 0.68);

#[derive(Component)]
pub struct GameText;
#[derive(Component)]
pub struct GameButton {
    pub primary: bool,
}
#[derive(Resource)]
pub struct GameFont(pub Handle<Font>);
struct GameUiPlugin;

pub fn install(app: &mut App) {
    if !app.is_plugin_added::<GameUiPlugin>() {
        app.add_plugins(GameUiPlugin);
    }
}
impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::ui_widgets::ButtonPlugin>() {
            app.add_plugins(bevy::ui_widgets::ButtonPlugin);
        }
        app.init_resource::<InputFocus>()
            .add_systems(Startup, load_font)
            .add_systems(Update, (typography, paint));
    }
}
fn load_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        commands.insert_resource(GameFont(
            fonts.add(Font::from_bytes(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/Barlow-Regular.ttf"
                ))
                .to_vec(),
            )),
        ));
    }
}
pub fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        GameText,
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextLayout::default().with_linebreak(bevy::text::LineBreak::WordOrCharacter),
    )
}
pub fn panel(node: Node) -> impl Bundle {
    (node, BackgroundColor(PANEL), BorderColor::all(BORDER))
}

/// Shared scrolling presentation; the caller still owns keyboard/focus scope.
pub fn install_panel_scroll(app: &mut App) {
    crate::panel_scroll::install(app);
}
pub fn attach_panel_scroll(
    world: &mut World,
    root: Entity,
    body: Entity,
    prefix: &str,
    labels: [&str; 2],
) -> [Entity; 2] {
    crate::panel_scroll::attach(world, root, body, prefix, labels)
}

/// Rail and paging controls appear only when the measured body needs scrolling.
pub fn attach_panel_scroll_when_needed(
    world: &mut World,
    root: Entity,
    body: Entity,
    prefix: &str,
    labels: [&str; 2],
) -> [Entity; 2] {
    crate::panel_scroll::attach_when_needed(world, root, body, prefix, labels)
}

/// The caller owns the action component and observer. This only builds a semantic control.
pub fn button(
    world: &mut World,
    parent: Entity,
    label: &str,
    tab: i32,
    primary: bool,
    action: impl Bundle,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            Name::new(format!("Game: {label}")),
            GameButton { primary },
            WidgetButton,
            Hovered::default(),
            TabIndex(tab),
            AccessibleLabel::new(label),
            Node {
                min_height: px(44),
                padding: UiRect::axes(px(14), px(9)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(5)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(if primary { PRIMARY } else { BUTTON }),
            BorderColor::all(Color::NONE),
            action,
        ))
        .id();
    world.spawn((ChildOf(entity), text(label, 16.0, TEXT)));
    entity
}
fn typography(font: Option<Res<GameFont>>, mut labels: Query<&mut TextFont, With<GameText>>) {
    let Some(font) = font else { return };
    let source: bevy::text::FontSource = font.0.clone().into();
    for mut label in &mut labels {
        if label.font != source {
            label.font = source.clone();
        }
    }
}
type ButtonState<'a> = (
    Entity,
    &'a GameButton,
    &'a Hovered,
    Has<Pressed>,
    Has<InteractionDisabled>,
    &'a mut BackgroundColor,
    &'a mut BorderColor,
);
fn paint(
    focus: Res<InputFocus>,
    mut buttons: Query<ButtonState>,
    children: Query<&Children>,
    mut labels: Query<&mut TextColor, With<GameText>>,
) {
    for (entity, style, hovered, pressed, disabled, mut background, mut border) in &mut buttons {
        let color = if disabled {
            DISABLED
        } else if pressed {
            FIELD
        } else if hovered.get() {
            if style.primary { PRIMARY_HOVER } else { HOVER }
        } else if style.primary {
            PRIMARY
        } else {
            BUTTON
        };
        background.set_if_neq(BackgroundColor(color));
        border.set_if_neq(BorderColor::all(if focus.get() == Some(entity) {
            GOLD
        } else {
            Color::NONE
        }));
        if let Ok(children) = children.get(entity) {
            for child in children.iter() {
                if let Ok(mut text) = labels.get_mut(child) {
                    text.set_if_neq(TextColor(if disabled { MUTED } else { TEXT }));
                }
            }
        }
    }
}
