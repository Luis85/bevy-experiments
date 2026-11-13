//! Shared native game typography and controls. This module owns no simulation state.
//!
//! The island interface's design system lives in the submodules: [`tokens`]
//! (colour, type, space, layers, motion), [`widgets`] (the kit), [`icon`]
//! (pictures by concept), [`tooltip`] and [`scale`]. The constants and the
//! `button` below are the older style, kept until each screen adopts the kit.
pub mod gallery;
pub mod icon;
pub mod scale;
pub mod sound;
pub mod tokens;
pub mod tooltip;
pub mod widgets;
pub mod words;

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
/// A control reached with Tab that a click does not focus: the session bar,
/// the docks and the panels beside them. The island shell owns the ring.
#[derive(Component)]
pub struct KeyFocus;
#[derive(Component)]
pub struct GameButton {
    pub primary: bool,
}
#[derive(Resource)]
pub struct GameFont(pub Handle<Font>);
/// The SemiBold face, for text whose type step is SemiBold ([`Strong`]).
#[derive(Resource)]
pub struct GameFontStrong(pub Handle<Font>);
/// Text set in the SemiBold face: a SemiBold step of the type scale, or the
/// label of a selected tab.
#[derive(Component)]
pub struct Strong;

/// The face for a weight, from the two loaded ones.
pub fn face(weight: tokens::Weight, regular: &Handle<Font>, strong: &Handle<Font>) -> Handle<Font> {
    match weight {
        tokens::Weight::Regular => regular.clone(),
        tokens::Weight::SemiBold => strong.clone(),
    }
}
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
        if !app.is_plugin_added::<bevy::ui_widgets::SliderPlugin>() {
            app.add_plugins(bevy::ui_widgets::SliderPlugin);
        }
        sound::install(app);
        app.init_resource::<InputFocus>()
            .init_resource::<bevy::ui::UiScale>()
            .init_resource::<tokens::Theme>()
            .init_resource::<scale::Layout>()
            .init_resource::<tooltip::TooltipDelay>()
            .init_resource::<tooltip::TooltipState>()
            .add_systems(Startup, load_font)
            .add_systems(
                Update,
                (
                    typography,
                    paint,
                    scale::apply,
                    icon::pick_raster,
                    widgets::place_thumbs,
                ),
            )
            // After the screens have refreshed their state for this frame.
            .add_systems(
                Update,
                (widgets::tones, widgets::paint, tooltip::update)
                    .chain()
                    .after(npc_framework::view_helpers::ViewSystems::Present),
            );
    }
}
fn load_font(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        commands.insert_resource(GameFontStrong(
            fonts.add(Font::from_bytes(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/Barlow-SemiBold.ttf"
                ))
                .to_vec(),
            )),
        ));
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
fn typography(
    font: Option<Res<GameFont>>,
    strong: Option<Res<GameFontStrong>>,
    mut labels: Query<(&mut TextFont, Has<Strong>, Option<&ChildOf>), With<GameText>>,
    tabs: Query<&widgets::Control>,
) {
    let Some(font) = font else { return };
    let strong = strong.map_or_else(|| font.0.clone(), |strong| strong.0.clone());
    for (mut label, marked, parent) in &mut labels {
        // A selected tab's label is set strong, like a heading.
        let selected_tab = parent
            .and_then(|parent| tabs.get(parent.parent()).ok())
            .is_some_and(|control| control.kind == widgets::Kind::Tab && control.on);
        let weight = if marked || selected_tab {
            tokens::Weight::SemiBold
        } else {
            tokens::Weight::Regular
        };
        let source: bevy::text::FontSource = face(weight, &font.0, &strong).into();
        if label.font != source {
            label.font = source;
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

#[cfg(test)]
mod weight_tests {
    use super::*;

    #[test]
    fn the_type_scale_resolves_both_weights_to_their_own_face() {
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .add_systems(Startup, load_font)
            .add_systems(Update, typography);
        app.update();
        let regular = app.world().resource::<GameFont>().0.clone();
        let strong = app.world().resource::<GameFontStrong>().0.clone();
        assert_ne!(regular, strong, "two faces are loaded");
        let parent = app.world_mut().spawn(Node::default()).id();
        let heading = {
            let world = app.world_mut();
            widgets::label(
                world,
                parent,
                "Title",
                tokens::HEADING,
                widgets::Tone::text(tokens::Surface::Paper),
            )
        };
        let body = {
            let world = app.world_mut();
            widgets::label(
                world,
                parent,
                "Text",
                tokens::BODY,
                widgets::Tone::text(tokens::Surface::Paper),
            )
        };
        app.update();
        let font = |entity| app.world().get::<TextFont>(entity).unwrap().font.clone();
        assert_eq!(font(heading), bevy::text::FontSource::from(strong));
        assert_eq!(font(body), bevy::text::FontSource::from(regular));
    }
}
