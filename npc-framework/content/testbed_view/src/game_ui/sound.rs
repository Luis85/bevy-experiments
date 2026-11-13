//! The interface's sounds, sent from the shared kit rather than from each
//! screen: a control's activation clicks (a tab turns a page), a hovered
//! control ticks at most once every [`HOVER_GAP`] seconds, a panel marked
//! with [`PanelSound`] opens and closes, and the few moments that are not a
//! control (a refused action, a placed building, a new alert, a trade) call
//! [`play`]. The island's sound plays them ([`crate::island_audio`]); this
//! only asks. Presentation only: nothing here reaches the simulation.
use super::widgets::{Control, Kind};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;

pub use crate::island_audio::{AudioLevels, PlayUiSound, UiSound};

/// The shortest gap between two hover ticks, in seconds.
pub const HOVER_GAP: f32 = 0.08;

/// What a control sounds when activated, where it is not its kind's:
/// `None` keeps it silent (a slider's thumb, a first-run tip's own button).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlSound(pub Option<UiSound>);

/// A panel whose showing and hiding sound Open and Close: the ledger, the
/// workspace, the build tray, a card, a menu. The first state seen is
/// silent, so a panel built hidden or shown at start makes no sound.
#[derive(Component, Default, Debug)]
pub struct PanelSound {
    shown: Option<bool>,
}

#[derive(Resource, Default)]
pub(crate) struct HoverSound {
    /// Seconds since the last hover tick.
    since: f32,
    hovered: Option<Entity>,
}

/// Ask for one interface sound now. Silent where nothing plays sounds.
pub fn play(world: &mut World, sound: UiSound) {
    if world.contains_resource::<Messages<PlayUiSound>>() {
        world.write_message(PlayUiSound(sound));
    }
}

/// The sound of activating a control of `kind`: a tab or a ledger page
/// turns a page, a slider is silent (it is dragged, not pressed), anything
/// else clicks. A [`ControlSound`] on the control wins.
pub fn activation_sound(kind: Kind, own: Option<ControlSound>) -> Option<UiSound> {
    if let Some(ControlSound(sound)) = own {
        return sound;
    }
    match kind {
        Kind::Tab => Some(UiSound::Page),
        Kind::Slider => None,
        _ => Some(UiSound::Click),
    }
}

/// Whether a panel's change of display is worth a sound, and which.
pub fn panel_sound(was: Option<bool>, shown: bool) -> Option<UiSound> {
    match was {
        Some(was) if was != shown => Some(if shown { UiSound::Open } else { UiSound::Close }),
        _ => None,
    }
}

pub(super) fn install(app: &mut App) {
    if !app.world().contains_resource::<Messages<PlayUiSound>>() {
        app.add_message::<PlayUiSound>();
    }
    app.init_resource::<HoverSound>()
        .add_observer(activated)
        .add_systems(Update, (hovers, panels));
}

fn activated(
    event: On<Activate>,
    controls: Query<(&Control, Option<&ControlSound>, Has<InteractionDisabled>)>,
    mut out: MessageWriter<PlayUiSound>,
) {
    let Ok((control, own, disabled)) = controls.get(event.entity) else {
        return;
    };
    // A control that cannot act refuses.
    let sound = if disabled {
        Some(UiSound::Deny)
    } else {
        activation_sound(control.kind, own.copied())
    };
    if let Some(sound) = sound {
        out.write(PlayUiSound(sound));
    }
}

fn hovers(
    time: Option<Res<Time>>,
    mut state: ResMut<HoverSound>,
    controls: Query<(Entity, &Hovered, Option<&ControlSound>), (With<Control>, Changed<Hovered>)>,
    mut out: MessageWriter<PlayUiSound>,
) {
    state.since += time.map_or(0.0, |time| time.delta_secs());
    for (entity, hovered, own) in &controls {
        if !hovered.get() || state.hovered == Some(entity) {
            continue;
        }
        state.hovered = Some(entity);
        // A silent control stays silent on hover too.
        if matches!(own, Some(ControlSound(None))) || state.since < HOVER_GAP {
            continue;
        }
        state.since = 0.0;
        out.write(PlayUiSound(UiSound::Hover));
    }
}

fn panels(
    mut panels: Query<(&Node, &mut PanelSound), Changed<Node>>,
    mut out: MessageWriter<PlayUiSound>,
) {
    for (node, mut panel) in &mut panels {
        let shown = node.display != Display::None;
        if let Some(sound) = panel_sound(panel.shown, shown) {
            out.write(PlayUiSound(sound));
        }
        panel.shown = Some(shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(app: &mut App) -> Vec<UiSound> {
        app.world_mut()
            .resource_mut::<Messages<PlayUiSound>>()
            .drain()
            .map(|message| message.0)
            .collect()
    }

    #[test]
    fn each_kind_of_control_sounds_its_own_way_once() {
        assert_eq!(activation_sound(Kind::Primary, None), Some(UiSound::Click));
        assert_eq!(activation_sound(Kind::Tab, None), Some(UiSound::Page));
        assert_eq!(activation_sound(Kind::Slider, None), None);
        assert_eq!(
            activation_sound(Kind::Secondary, Some(ControlSound(Some(UiSound::Confirm)))),
            Some(UiSound::Confirm)
        );
        assert_eq!(
            activation_sound(Kind::Secondary, Some(ControlSound(None))),
            None
        );
        let mut app = App::new();
        install(&mut app);
        let button = app
            .world_mut()
            .spawn(Control::new(
                Kind::Secondary,
                super::super::tokens::Surface::Paper,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn(Control::new(
                Kind::Tab,
                super::super::tokens::Surface::Paper,
            ))
            .id();
        let off = app
            .world_mut()
            .spawn((
                Control::new(Kind::Secondary, super::super::tokens::Surface::Paper),
                InteractionDisabled,
            ))
            .id();
        for entity in [button, tab, off] {
            app.world_mut().trigger(Activate { entity });
        }
        assert_eq!(
            heard(&mut app),
            [UiSound::Click, UiSound::Page, UiSound::Deny]
        );
    }

    #[test]
    fn a_panel_sounds_when_it_opens_and_closes_but_not_when_first_seen() {
        assert_eq!(panel_sound(None, true), None);
        assert_eq!(panel_sound(Some(false), true), Some(UiSound::Open));
        assert_eq!(panel_sound(Some(true), false), Some(UiSound::Close));
        assert_eq!(panel_sound(Some(true), true), None);
        let mut app = App::new();
        install(&mut app);
        let panel = app
            .world_mut()
            .spawn((Node::default(), PanelSound::default()))
            .id();
        app.update();
        assert!(heard(&mut app).is_empty(), "built shown: silent");
        app.world_mut().get_mut::<Node>(panel).unwrap().display = Display::None;
        app.update();
        app.world_mut().get_mut::<Node>(panel).unwrap().display = Display::Flex;
        app.update();
        assert_eq!(heard(&mut app), [UiSound::Close, UiSound::Open]);
    }

    #[test]
    fn hovering_ticks_at_most_once_per_gap() {
        let mut app = App::new();
        install(&mut app);
        let a = app
            .world_mut()
            .spawn((
                Control::new(Kind::Secondary, super::super::tokens::Surface::Paper),
                Hovered::default(),
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                Control::new(Kind::Secondary, super::super::tokens::Surface::Paper),
                Hovered::default(),
            ))
            .id();
        // Without a clock no time passes: the first hover waits out the gap.
        app.world_mut().resource_mut::<HoverSound>().since = HOVER_GAP;
        app.world_mut().entity_mut(a).insert(Hovered(true));
        app.update();
        app.world_mut().entity_mut(b).insert(Hovered(true));
        app.update();
        assert_eq!(
            heard(&mut app),
            [UiSound::Hover],
            "the second came too soon"
        );
    }
}
