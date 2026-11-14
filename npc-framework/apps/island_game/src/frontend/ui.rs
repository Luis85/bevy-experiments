//! Native shell presentation: title, new game, pause, settings and the guide,
//! on the island's design system. View drafts are distinct from the saved world.
use super::settings::{Change, HostSettings, SettingsState};
use super::{Action, FrontendState, NoticeTone, Screen, SettingsTab};
use crate::island::session;
use bevy::input_focus::{
    AutoFocus, FocusCause, InputFocus,
    tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation},
};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEdit};
use bevy::ui::{
    BackgroundGradient, ColorStop, Gradient, InteractionDisabled, LinearGradient,
    prelude::AccessibleLabel,
};
use bevy::ui_widgets::{Activate, ScrollArea, SliderValue, TextInput, ValueChange};
use island_view::game_ui::icon::Icon;
use island_view::game_ui::tokens::{self as t, Status, Surface};
use island_view::game_ui::widgets::{
    self as w, ButtonSpec, Kind, Part, Reach, Role, Size, Skin, Tone,
};
use island_view::{FrontendUiRoot, IslandMode, ToastTone, game_ui as ui};

#[derive(Resource)]
pub(super) struct NameDraft(String);
impl Default for NameDraft {
    fn default() -> Self {
        Self("Traveller".into())
    }
}
#[derive(Resource, Default)]
struct RenderedRevision(Option<(u64, bool, bool)>);
/// What the island's toast lane last said for the host, so each notice is
/// said once.
#[derive(Resource, Default)]
struct ToastShown(Option<(String, NoticeTone)>);
#[derive(Component)]
pub(super) struct ShellRoot;
#[derive(Component)]
pub(super) struct NameField;
/// The screen's own line for a problem or an answer, on its surface.
#[derive(Component)]
struct Notice(Surface);
#[derive(Component)]
struct BeginButton;

/// Which setting a slider sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SliderKind {
    Master,
    Ambience,
    Effects,
    Music,
    /// The interface's own sounds.
    Sounds,
    Interface,
    Pan,
}

impl SliderKind {
    /// Range and step, in percent.
    fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Interface => (80.0, 140.0, 10.0),
            Self::Pan => (50.0, 200.0, 25.0),
            _ => (0.0, 100.0, 10.0),
        }
    }
    fn value(self, settings: HostSettings) -> u8 {
        match self {
            Self::Master => settings.master,
            Self::Ambience => settings.ambience,
            Self::Effects => settings.effects,
            Self::Music => settings.music,
            Self::Sounds => settings.interface_sounds,
            Self::Interface => settings.ui_scale,
            Self::Pan => settings.pan_speed,
        }
    }
    fn change(self, value: u8) -> Change {
        match self {
            Self::Master => Change::SetMaster(value),
            Self::Ambience => Change::SetAmbience(value),
            Self::Effects => Change::SetEffects(value),
            Self::Music => Change::SetMusic(value),
            Self::Sounds => Change::SetInterfaceSounds(value),
            Self::Interface => Change::SetUiScale(value),
            Self::Pan => Change::SetPanSpeed(value),
        }
    }
    /// What the slider sets, for its tooltip.
    fn help(self) -> &'static str {
        match self {
            Self::Master => "How loud the whole game is; every other volume is a share of it.",
            Self::Ambience => "Wind, sea, birds and the village around you.",
            Self::Effects => "Tools, footsteps, doors and the work you hear on the island.",
            Self::Music => "The island's music.",
            Self::Sounds => "Clicks, pages, alerts and coins of the interface.",
            Self::Interface => "How large the interface is drawn.",
            Self::Pan => "How fast the keys and the window edge move the camera.",
        }
    }
    fn steps(self) -> (Change, Change) {
        match self {
            Self::Master => (Change::Master(-1), Change::Master(1)),
            Self::Ambience => (Change::Ambience(-1), Change::Ambience(1)),
            Self::Effects => (Change::Effects(-1), Change::Effects(1)),
            Self::Music => (Change::Music(-1), Change::Music(1)),
            Self::Sounds => (Change::InterfaceSounds(-1), Change::InterfaceSounds(1)),
            Self::Interface => (Change::UiScale(-1), Change::UiScale(1)),
            Self::Pan => (Change::PanSpeed(-1), Change::PanSpeed(1)),
        }
    }
}

/// A slider of the Settings screen and the text that shows its value.
#[derive(Component, Clone, Copy)]
pub(super) struct SettingSlider {
    kind: SliderKind,
    text: Entity,
}

pub(super) fn spawn(world: &mut World) {
    world.init_resource::<NameDraft>();
    world.init_resource::<RenderedRevision>();
    world.init_resource::<ToastShown>();
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
/// A settings slider moved: the setting takes its value at once.
pub(super) fn slide(
    change: On<ValueChange<f32>>,
    sliders: Query<&SettingSlider>,
    mut flow: ResMut<FrontendState>,
) {
    if let Ok(slider) = sliders.get(change.source) {
        let value = change.value.round().clamp(0.0, 255.0) as u8;
        flow.requests
            .push(Action::Adjust(slider.kind.change(value)));
    }
}

/// A text button in the screen's Tab order.
#[allow(clippy::too_many_arguments)]
fn button(
    world: &mut World,
    parent: Entity,
    tab: &mut i32,
    text: &str,
    kind: Kind,
    surface: Surface,
    key: Option<&str>,
    action: Action,
) -> Entity {
    let mut spec = ButtonSpec::new(text, kind, surface).reach(Reach::Tab(*tab));
    if let Some(key) = key {
        spec = spec.key(key);
    }
    *tab += 1;
    let entity = w::button(world, parent, spec, action);
    if let Some((title, body)) = menu_tip(action) {
        world
            .entity_mut(entity)
            .insert(island_view::game_ui::tooltip::Tooltip::new(title, body));
    }
    entity
}

/// What a menu button does, for its tooltip: the words on it say what it
/// is, the tooltip what follows.
/// The day within its season, as the top bar's date says it: the summary
/// counts days since the island began.
fn season_day(world: &World, day: u64) -> u64 {
    day.saturating_sub(1) % island_content::village::days_per_season(world) + 1
}

/// "12 minutes ago", "just now".
pub(crate) fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "just now".to_owned(),
        60..120 => "a minute ago".to_owned(),
        120..3600 => format!("{} minutes ago", seconds / 60),
        3600..7200 => "an hour ago".to_owned(),
        _ => format!("{} hours ago", seconds / 3600),
    }
}

/// The title's sentence when the autosave is newer than the save.
pub(crate) fn autosave_sentence(auto_age_secs: u64, own_age_secs: u64) -> String {
    format!(
        "There is an autosave from {}, newer than your save ({}).",
        ago(auto_age_secs),
        ago(own_age_secs)
    )
}

/// The pause menu's line about the two files.
pub(crate) fn saves_line(
    own: Option<std::time::Duration>,
    auto: Option<std::time::Duration>,
    auto_failed: bool,
) -> String {
    let own = own.map_or_else(|| "none yet".to_owned(), |age| ago(age.as_secs()));
    let auto = if auto_failed {
        "the last one failed".to_owned()
    } else {
        auto.map_or_else(|| "none yet".to_owned(), |age| ago(age.as_secs()))
    };
    format!("Your save: {own} \u{b7} Autosave: {auto}")
}

/// What the confirmation says before leaving without a save.
pub(crate) fn confirm_quit_lines(own_age_secs: Option<u64>, failed: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if failed {
        lines.push("The island could not be saved.".to_owned());
    }
    match own_age_secs {
        Some(age) => lines.push(format!(
            "Everything since your last save is lost. Your last save was {}.",
            ago(age)
        )),
        None => lines.push("This island has never been saved. It will be gone.".to_owned()),
    }
    lines
}

/// The card the title shows above its buttons when it cannot simply
/// continue: what happened, what the player can do, and the details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TitleCard {
    pub(crate) title: &'static str,
    pub(crate) lines: Vec<String>,
    pub(crate) details: Option<String>,
    /// "Copy the folder path" is offered (the file is kept there).
    pub(crate) copy_folder: bool,
    pub(crate) status: Status,
}

/// A refusal's technical sentence in the player's words, for the details
/// line; anything not known passes as it is.
pub(crate) fn plain_details(reason: &str) -> String {
    let known = [
        ("different seed", "The save belongs to a different island."),
        (
            "not a local Island Village save",
            "The file is not an island of this game.",
        ),
        (
            "missing its immutable scenario",
            "Part of the save is missing.",
        ),
        (
            "need a new host",
            "The save needs a newer version of the game.",
        ),
    ];
    known
        .iter()
        .find(|(words, _)| reason.contains(words))
        .map_or_else(
            || reason.trim().to_owned(),
            |(_, plain)| (*plain).to_owned(),
        )
}

/// The title's card for `summary`, with the save's file name and the name
/// its backup will get; `None` when the title can continue as usual.
pub(crate) fn title_card(
    summary: &session::Summary,
    file: &str,
    backup: &str,
) -> Option<TitleCard> {
    use crate::island::Issue;
    if let Some(detail) = &summary.restart {
        return Some(TitleCard {
            title: "The game has to be restarted",
            lines: vec![
                "The new island could not start, and the island you were playing could not be put back.".into(),
                "Your saved island on disk is untouched. Quit, then start the game again and choose Continue.".into(),
            ],
            details: Some(plain_details(detail)),
            copy_folder: false,
            status: Status::Warn,
        });
    }
    Some(match summary.issue.as_ref()? {
        Issue::Older => TitleCard {
            title: "Your island is from an older version",
            lines: vec![
                format!("This version of the game cannot open it. The file is untouched: {file}."),
                format!(
                    "Start a new game to play now. Your old island is then kept beside it as {backup}."
                ),
            ],
            details: None,
            copy_folder: true,
            status: Status::Warn,
        },
        Issue::OtherBuild => TitleCard {
            title: "Your island is from another build of the game",
            lines: vec![
                format!("This build cannot open it. The file is untouched: {file}."),
                format!(
                    "Start a new game to play now. Your old island is then kept beside it as {backup}."
                ),
            ],
            details: None,
            copy_folder: true,
            status: Status::Warn,
        },
        Issue::Newer => TitleCard {
            title: "Your island is from a newer version",
            lines: vec![
                format!("Update the game to continue it. The file is untouched: {file}."),
                format!("Or start a new game now. Your island is then kept beside it as {backup}."),
            ],
            details: None,
            copy_folder: true,
            status: Status::Warn,
        },
        Issue::Unopened(reason) => TitleCard {
            title: "Your island could not be opened",
            lines: vec![format!(
                "The file is untouched: {file}. Starting a new game keeps it beside the new one as {backup}."
            )],
            details: Some(plain_details(reason)),
            copy_folder: true,
            status: Status::Warn,
        },
        Issue::Rules(reason) => TitleCard {
            title: "Your changed island rules could not be read",
            lines: vec![
                "New game uses the island that comes with the game. Your files are untouched."
                    .into(),
            ],
            details: Some(plain_details(reason)),
            copy_folder: true,
            status: Status::Info,
        },
    })
}

fn draw_title_card(world: &mut World, parent: Entity, tab: &mut i32, card: &TitleCard) {
    let surface = Surface::Paper;
    let root = w::card(world, parent, 12.0);
    world.entity_mut(root).insert(Name::new("Title notice"));
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.width = px(420);
        node.margin = UiRect::top(px(10));
    }
    let head = row(world, root, 6.0);
    island_view::game_ui::icon::icon(
        world,
        head,
        island_view::game_ui::icon::Icon::status(card.status),
        16.0,
        Tone::status(surface, card.status),
    );
    w::label(world, head, card.title, t::SUBHEADING, Tone::text(surface));
    for line in &card.lines {
        w::paragraph(world, root, line.clone(), t::BODY, Tone::text(surface));
    }
    if let Some(details) = &card.details {
        w::paragraph(
            world,
            root,
            format!("Details: {details}"),
            t::CAPTION,
            Tone::secondary(surface),
        );
    }
    if card.copy_folder {
        button(
            world,
            root,
            tab,
            "Copy the folder path",
            Kind::Quiet,
            surface,
            None,
            Action::CopyFolder,
        );
    }
}

pub(super) fn menu_tip(action: Action) -> Option<(&'static str, &'static str)> {
    Some(match action {
        Action::Continue => (
            "Continue",
            "Load your island where you left it and play on.",
        ),
        Action::ContinueInspector => (
            "Continue in Oversee",
            "Load your island and watch it from above; join as your character whenever you choose.",
        ),
        Action::New => (
            "New game",
            "Choose a name and how to begin on a new island.",
        ),
        Action::Begin => (
            "Start on the island",
            "Arrive as a traveller and live one life.",
        ),
        Action::BeginInspector => (
            "Oversee the village",
            "Begin from above: guide the villages first.",
        ),
        Action::BeginShape => ("Shape the landscape", "Begin by sculpting the land."),
        Action::Resume => ("Resume", "Back to the island; time runs again."),
        Action::Save => (
            "Save game",
            "Save the island now. The game also keeps an autosave every minute.",
        ),
        Action::Title => (
            "Save and go to title",
            "Save, then leave for the title screen.",
        ),
        Action::Quit => ("Quit", "Close the game. Your island is saved first."),
        Action::ContinueAutosave => (
            "Continue from the autosave",
            "Load the island as the autosave left it; your own save stays as it is until you save.",
        ),
        Action::QuitUnsaved => (
            "Quit without saving",
            "Close the game and keep your last save as it is.",
        ),
        Action::Settings => ("Settings", "Sound, display, controls and accessibility."),
        Action::Controls => ("How to play", "The Guide: keys and how each mode works."),
        Action::Back => ("Back", "Return to the previous screen."),
        Action::CopyFolder => (
            "Copy the folder path",
            "Put the folder that holds your island on the clipboard.",
        ),
        _ => return None,
    })
}
/// The name automation and assistive tools know a control by, where it is
/// not the text on it.
fn rename(world: &mut World, entity: Entity, name: &str) {
    world.entity_mut(entity).insert((
        Name::new(format!("Game: {name}")),
        AccessibleLabel::new(name),
    ));
}
fn row(world: &mut World, parent: Entity, gap: f32) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(gap),
                row_gap: px(gap),
                ..default()
            },
        ))
        .id()
}
fn column(world: &mut World, parent: Entity, gap: f32) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(gap),
                ..default()
            },
        ))
        .id()
}

/// The keys of the island, for the Settings list and the Guide.
const KEYS: [(&str, &str); 11] = [
    (
        "1  2  3",
        "Live as your character, Oversee the villages, Shape the land",
    ),
    ("Space", "Pause or resume time"),
    ("-  =", "Slower or faster: 1\u{d7}, 3\u{d7}, 10\u{d7}"),
    ("Esc", "Close what is open, then the menu"),
    (
        "Tab",
        "Walk the controls; Shift+Tab walks back; Enter or Space activates",
    ),
    ("Alt", "Hold over a control for its longer tooltip"),
    ("Ctrl+S", "Save the island"),
    (
        "W A S D",
        "Walk in Live; pan the camera in Oversee and Shape",
    ),
    (
        "Q  E",
        "Turn the camera (Q and R in Live, where E uses things)",
    ),
    ("Wheel", "Zoom; scroll when over a panel"),
    ("Middle mouse", "Drag to orbit and tilt the camera"),
];

pub(super) const GUIDE_PAGES: [&str; 5] = ["Controls", "Oversee", "Live", "Shape", "The economy"];

#[derive(Clone, Debug, PartialEq, Eq)]
enum GuideLine {
    Text(&'static str),
    Key(&'static str, &'static str),
    /// A mode's heading on the Controls page.
    Heading(&'static str),
    /// A key from the island's one key table (`island_view::guide_controls`).
    Keys(String, &'static str),
}

/// One page of the Guide: a few sentences and the keys that belong to them.
fn guide(page: u8) -> Vec<GuideLine> {
    use GuideLine::{Key, Text};
    match page {
        // The island's one key table: every player key, by mode.
        0 => island_view::guide_controls()
            .into_iter()
            .flat_map(|section| {
                std::iter::once(GuideLine::Heading(section.mode)).chain(
                    section
                        .keys
                        .into_iter()
                        .map(|(keys, what)| GuideLine::Keys(keys, what)),
                )
            })
            .collect(),
        1 => vec![
            Text(
                "Oversee shows the whole island from above. Nothing is done by your hand: residents do the work, and you decide what a village builds, how many do each job, and what it sees to first. The Village outlook on the left says what needs you next.",
            ),
            Key(
                "F1 to F5",
                "The ledger: Village, Economy, People, Projects, Journal",
            ),
            Key("B", "The build tray: choose a building, then a place"),
            Key("O  M", "Overlays, and the places to go to"),
            Key("F", "Follow the selected resident"),
            Text(
                "Click a resident for their card: needs, family and bed, their day, why they chose what they do, home and job. Click a building for its card.",
            ),
            Text(
                "Alerts list what needs you, most urgent first; choose one to go there. While you place a building, the tag at the cursor says whether it can stand there, and why not.",
            ),
        ],
        2 => vec![
            Text(
                "Live is one life on the island. Your goal is top left; urgent needs come before building.",
            ),
            Key(
                "I C B G T N M",
                "Pouch, Craft, Build, Garden, Trade, People, Explore",
            ),
            Key("E", "Use what is in reach"),
            Key("X", "Stop walking or working"),
            Key("Right-click", "Walk to a place"),
            Text(
                "Your needs are bottom left. A triangle means see to it soon, a diamond means now. Food, water, rest and warmth matter: a life can end, and a successor continues in the same village.",
            ),
            Text(
                "Actions use real stock and take real time. A recipe you cannot make says what is missing. Gathering, recipes and garden work can be repeated five or ten times; Stop ends it.",
            ),
            Text(
                "From evening on, your own door offers sleep until morning; the night passes quickly and the morning brings an account of the day. Talk, buy and sell, and give gifts to grow friendships; a friend sells for less and lends a hand. Build improves your homestead one tier at a time.",
            ),
            Text(
                "Materials you order and a builder you hire take island hours: residents fetch, carry and build while you gather, tend the garden or sleep. The Build card says who is on the way, and when nobody has taken an order yet.",
            ),
        ],
        3 => vec![
            Text("Shape is a small editor for the land. Hold the left mouse button and drag."),
            Key("R F T G P", "Raise, Lower, Flatten, Smooth, Paint"),
            Key("[  ]", "Brush size"),
            Key("H", "Hard or soft edge"),
            Key("V", "The next surface: grass, soil, stone, water"),
            Key("Ctrl+Z", "Take the last stroke back; Ctrl+Y lays it again"),
            Key("N  X", "Place a patch of a good, or remove one"),
            Key("L", "Place plots and props; R turns the one in hand"),
            Text(
                "Resource places a patch of a good where you click, as rich as you set. The nearest village works it. Remove takes a patch away; a village keeps its last one of each good. Ctrl+Z brings back what you removed. Export saves the island you shaped as a new island, and New game can start on it.",
            ),
            Text(
                "Residents, homes, workplaces and plots are protected, and cliffs are refused: the outline turns red and dashed and the tag says why. Edits made while paused show when time runs.",
            ),
        ],
        _ => vec![
            Text(
                "Twelve jobs turn what the island gives into food, tools and clothes: grain to flour to bread, logs to planks, ore to ingots to tools, hides to leather, fiber to thread to warm clothes.",
            ),
            Text(
                "Goods and coins move by hand. A resident walks to a store, a stall or a seller, and pays for what they take.",
            ),
            Text(
                "The fund collects a levy and pays residents for the materials of village buildings. A building site waits while the fund cannot pay.",
            ),
            Text(
                "The Economy page of the ledger lists every good with its stock, its price, the days it lasts at a day's use and its change since yesterday, and marks what is low. Choose a good to see its price, the last market, who uses it, who goes without and who makes it. Jobs shows who works where and lets you ask for more or fewer. Priorities says what a village sees to first.",
            ),
        ],
    }
}

/// The three ways to begin, as the new-game screen offers them.
/// The two ways to begin drawn as cards (control name, title, sentence);
/// Shape is the quiet link under them ([`SHAPE_START`]).
const START_CHOICES: [(&str, &str, &str); 2] = [
    (
        "Start on the island",
        "Live",
        "Arrive as a traveller and live one life: meet your neighbours, gather, build a home. Best for a first game.",
    ),
    (
        "Oversee the village",
        "Oversee",
        "Watch the villages from above and guide them: what they build, who does which job. You have no character until you choose Live.",
    ),
];
/// Shape first: (words on the link, its tooltip). Its control name stays
/// "Shape the landscape".
const SHAPE_START: (&str, &str) = (
    "Or shape the land first",
    "Raise and lower the land, place patches of goods, plots and props. The villages carry on while you work.",
);

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
    let title_screen = screen == Screen::Title;
    // The title lets the island show through a bark gradient; every other
    // screen dims it. Until the scene is ready nothing shows through.
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
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(if !ready {
                t::BARK_900
            } else if title_screen {
                Color::NONE
            } else {
                t::SCRIM
            }),
        ))
        .id();
    if ready && title_screen {
        world
            .entity_mut(root)
            .insert(BackgroundGradient(vec![Gradient::Linear(
                LinearGradient::to_right(vec![
                    // 82% to 0% over 560 px, as specified; a stop at the
                    // column's right edge keeps the tagline readable over a
                    // bright island (the plain two-stop ramp fades too soon
                    // in the native capture).
                    ColorStop::px(t::BARK_900.with_alpha(0.82), 0.0),
                    ColorStop::px(t::BARK_900.with_alpha(0.66), 440.0),
                    ColorStop::px(t::BARK_900.with_alpha(0.0), 560.0),
                ]),
            )]));
    }
    let mut tab = 0;
    let mut first = None;
    let surface = if title_screen {
        Surface::Chrome
    } else {
        Surface::Paper
    };
    // The title is a bare column on the gradient; the rest are panels.
    let panel = if title_screen {
        world
            .spawn((
                ChildOf(root),
                Name::new("Title"),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(72),
                    top: px(132),
                    width: px(360),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
            ))
            .id()
    } else {
        let wide = matches!(screen, Screen::Settings | Screen::Controls);
        let width = match screen {
            Screen::New => 840,
            _ if wide => 560,
            _ => 440,
        };
        let mut node = Node {
            position_type: PositionType::Absolute,
            width: px(width),
            max_width: percent(96),
            max_height: percent(92),
            padding: UiRect::all(px(20)),
            row_gap: px(10),
            flex_direction: FlexDirection::Column,
            ..default()
        };
        if screen == Screen::New {
            // In the title's own column.
            node.left = px(72);
            node.top = px(96);
        } else {
            node.top = px(if wide { 64 } else { 128 });
        }
        let panel = world
            .spawn((ChildOf(root), Name::new("Game menu"), w::panel(node)))
            .id();
        world.entity_mut(panel).insert(t::shadow(t::SHADOW_3));
        panel
    };
    let heading = match screen {
        Screen::Title => "Island Village",
        Screen::New => "A new beginning",
        Screen::Pause => "Paused",
        Screen::Controls => "Guide",
        Screen::Settings => "Settings",
        Screen::Transition => "Saving your island",
        Screen::ConfirmQuit => "Quit without saving?",
        Screen::Playing => unreachable!(),
    };
    w::label(
        world,
        panel,
        heading,
        if title_screen { t::DISPLAY } else { t::TITLE },
        Tone::text(surface),
    );
    if !ready {
        let (text, tone) = super::scene_preparation_message(world);
        w::paragraph(
            world,
            panel,
            text,
            t::BODY,
            match tone {
                NoticeTone::Error => Tone::status(surface, Status::Crit),
                _ => Tone::secondary(surface),
            },
        );
    }
    match screen {
        Screen::Title => {
            w::paragraph(
                world,
                panel,
                {
                    "Follow the coast, discover three villages, and make a home in a changing world."
                },
                t::SUBHEADING,
                Tone::secondary(surface),
            );
            let path = crate::island::save_path(world).unwrap_or_default();
            let file = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let older = summary
                .issue
                .as_ref()
                .is_some_and(crate::island::Issue::keeps_forever);
            let backup = crate::island::next_backup(&path, older)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            if let Some(card) = title_card(&summary, &file, &backup) {
                draw_title_card(world, panel, &mut tab, &card);
            }
            if let Some((auto, own)) = summary.autosave_newer {
                let card = w::card(world, panel, 12.0);
                world.entity_mut(card).insert(Name::new("Autosave notice"));
                if let Some(mut node) = world.get_mut::<Node>(card) {
                    node.width = px(420);
                    node.margin = UiRect::top(px(10));
                }
                w::label(
                    world,
                    card,
                    "The game did not close properly.",
                    t::SUBHEADING,
                    Tone::text(Surface::Paper),
                );
                w::paragraph(
                    world,
                    card,
                    autosave_sentence(auto, own),
                    t::BODY,
                    Tone::text(Surface::Paper),
                );
                let pair = row(world, card, 8.0);
                for (text, action) in [
                    ("Continue from the autosave", Action::ContinueAutosave),
                    ("Continue from my save", Action::Continue),
                ] {
                    let entity = button(
                        world,
                        pair,
                        &mut tab,
                        text,
                        Kind::Secondary,
                        Surface::Paper,
                        None,
                        action,
                    );
                    rename(world, entity, text);
                    w::set_enabled(world, entity, ready);
                }
            }
            let buttons = column(world, panel, 8.0);
            if let Some(mut node) = world.get_mut::<Node>(buttons) {
                node.width = px(320);
                node.margin = UiRect::top(px(10));
            }
            // With a usable save Continue leads, with who and when on its
            // second line; without one New game does.
            if summary.can_continue {
                let resume = w::control(
                    world,
                    buttons,
                    "Continue",
                    Kind::Primary,
                    surface,
                    Reach::Tab(tab),
                    Node {
                        height: px(56),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        row_gap: px(2),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                        ..default()
                    },
                    Action::Continue,
                );
                tab += 1;
                for (text, step) in [
                    ("Continue".to_owned(), t::SUBHEADING),
                    (
                        format!(
                            "{} \u{b7} {}, day {}",
                            summary.player,
                            summary.season,
                            season_day(world, summary.day)
                        ),
                        t::CAPTION,
                    ),
                ] {
                    let line = w::label(world, resume, text, step, Tone::text(surface));
                    world.entity_mut(line).insert(Part::Label);
                }
                w::set_enabled(world, resume, ready);
                if let Some((title, body)) = menu_tip(Action::Continue) {
                    world
                        .entity_mut(resume)
                        .insert(island_view::game_ui::tooltip::Tooltip::new(title, body));
                }
                first = ready.then_some(resume);
            }
            // After a failed restore only Settings, How to play and Quit.
            if summary.restart.is_none() {
                let new = button(
                    world,
                    buttons,
                    &mut tab,
                    "New game",
                    if summary.can_continue {
                        Kind::Secondary
                    } else {
                        Kind::Primary
                    },
                    surface,
                    None,
                    Action::New,
                );
                w::set_enabled(world, new, ready && summary.can_start);
                if first.is_none() && ready && summary.can_start {
                    first = Some(new);
                }
            }
            if summary.can_continue {
                let oversee = button(
                    world,
                    buttons,
                    &mut tab,
                    "Continue in Oversee",
                    Kind::Secondary,
                    surface,
                    None,
                    Action::ContinueInspector,
                );
                // Its name stays the script's: "Oversee the village".
                rename(world, oversee, "Oversee the village");
                w::set_enabled(world, oversee, ready);
            }
            let pair = row(world, buttons, 8.0);
            for (text, action) in [
                ("Settings", Action::Settings),
                ("How to play", Action::Controls),
            ] {
                let entity = button(
                    world,
                    pair,
                    &mut tab,
                    text,
                    Kind::Secondary,
                    surface,
                    None,
                    action,
                );
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.flex_grow = 1.0;
                    node.flex_basis = px(0);
                }
                if first.is_none() && action == Action::Controls {
                    first = Some(entity);
                }
            }
            let quit = button(
                world,
                buttons,
                &mut tab,
                "Quit",
                Kind::Quiet,
                surface,
                None,
                Action::Quit,
            );
            rename(world, quit, "Quit game");
            // The credits line, bottom left.
            let credits = w::label(
                world,
                root,
                "Models by Kay Lousberg and Kenney, icons Maki and Temaki, all CC0. Built with Bevy.",
                t::CAPTION,
                Tone::caption(Surface::Chrome),
            );
            world.entity_mut(credits).insert(Node {
                position_type: PositionType::Absolute,
                left: px(72),
                bottom: px(16),
                ..default()
            });
        }
        Screen::New => {
            // Two columns: who and how on the left, which island on the
            // right. Tab walks the name, the worlds, the ways to begin, Back.
            let body = row(world, panel, 20.0);
            if let Some(mut node) = world.get_mut::<Node>(body) {
                node.align_items = AlignItems::FlexStart;
                node.min_height = px(0);
                node.flex_shrink = 1.0;
            }
            let left = column(world, body, 10.0);
            let right = column(world, body, 8.0);
            for side in [left, right] {
                if let Some(mut node) = world.get_mut::<Node>(side) {
                    node.flex_grow = 1.0;
                    node.flex_basis = px(0);
                    node.min_width = px(0);
                    node.min_height = px(0);
                }
            }
            w::label(world, left, "Your name", t::CAPTION, Tone::caption(surface));
            let mut editable = EditableText {
                allow_newlines: false,
                visible_lines: Some(1.0),
                ..EditableText::new(&world.resource::<NameDraft>().0)
            };
            editable.pending_edits.clear();
            editable.queue_edit(TextEdit::SelectAll);
            // The box takes the panel style; the text inside takes its ink.
            let frame = world
                .spawn((
                    ChildOf(left),
                    Skin::Boxed,
                    Tone::new(surface, Role::Raised),
                    Node {
                        height: px(36),
                        padding: UiRect::horizontal(px(10)),
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::NONE),
                ))
                .id();
            let field = world
                .spawn((
                    ChildOf(frame),
                    NameField,
                    Name::new("New character name"),
                    AccessibleLabel::new("Your name"),
                    TextInput,
                    editable,
                    TextCursorStyle::default(),
                    ui::GameText,
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(t::INK_900),
                    Tone::text(surface),
                    TextLayout::no_wrap(),
                    TabIndex(tab),
                    Node {
                        width: percent(100),
                        min_width: px(0),
                        ..default()
                    },
                ))
                .id();
            tab += 1;
            world
                .resource_mut::<InputFocus>()
                .set(field, FocusCause::Navigated);
            // The rules of a name, then the refusal in the same place.
            w::paragraph(
                world,
                left,
                "The name your neighbours will know you by.",
                t::CAPTION,
                Tone::caption(surface),
            );
            let notice = w::paragraph(world, left, "", t::BODY, Tone::secondary(surface));
            world.entity_mut(notice).insert(Notice(surface));
            // The worlds take the Tab places after the name.
            let entries = world
                .resource::<super::worlds::WorldChoice>()
                .entries
                .clone();
            let chosen = world.resource::<super::worlds::WorldChoice>().chosen;
            let world_tab = tab;
            tab += i32::try_from(entries.len()).unwrap_or(0);
            w::label(
                world,
                left,
                "How do you want to begin?",
                t::SUBHEADING,
                Tone::text(surface),
            );
            let valid = session::name_problem(world, &draft_name(world)).is_none();
            for ((name, title, sentence), (picture, action)) in START_CHOICES.into_iter().zip([
                (Icon::Live, Action::Begin),
                (Icon::Oversee, Action::BeginInspector),
            ]) {
                let card = w::card_button(world, left, name, 10.0, Reach::Tab(tab), action);
                tab += 1;
                if let Some(mut node) = world.get_mut::<Node>(card) {
                    node.flex_direction = FlexDirection::Row;
                    node.align_items = AlignItems::Center;
                    node.column_gap = px(12);
                    node.flex_shrink = 0.0;
                }
                let image = island_view::game_ui::icon::icon(
                    world,
                    card,
                    picture,
                    24.0,
                    Tone::text(surface),
                );
                world.entity_mut(image).insert(Part::Label);
                let words = column(world, card, 2.0);
                if let Some(mut node) = world.get_mut::<Node>(words) {
                    node.flex_grow = 1.0;
                    node.flex_basis = px(0);
                    node.min_width = px(0);
                }
                let head = w::label(world, words, title, t::SUBHEADING, Tone::text(surface));
                world.entity_mut(head).insert(Part::Label);
                let line =
                    w::paragraph(world, words, sentence, t::CAPTION, Tone::secondary(surface));
                world.entity_mut(line).insert(Part::Secondary);
                w::set_enabled(world, card, ready && valid && summary.can_start);
                world.entity_mut(card).insert(BeginButton);
            }
            w::paragraph(
                world,
                left,
                "You can switch between Live, Oversee and Shape at any time with 1, 2 and 3.",
                t::CAPTION,
                Tone::caption(surface),
            );
            let shape = button(
                world,
                left,
                &mut tab,
                SHAPE_START.0,
                Kind::Quiet,
                surface,
                None,
                Action::BeginShape,
            );
            rename(world, shape, "Shape the landscape");
            world.entity_mut(shape).insert((
                island_view::game_ui::tooltip::Tooltip::new(SHAPE_START.0, SHAPE_START.1),
                BeginButton,
            ));
            w::set_enabled(world, shape, ready && valid && summary.can_start);
            if summary.has_file {
                w::paragraph(
                    world,
                    left,
                    "Your current island will be kept as a backup. The game keeps the five newest.",
                    t::CAPTION,
                    Tone::caption(surface),
                );
            }
            let foot = row(world, left, 8.0);
            // Back comes last in the Tab order, after the worlds and the
            // ways to begin.
            button(
                world,
                foot,
                &mut tab,
                "Back",
                Kind::Secondary,
                surface,
                Some("Esc"),
                Action::Back,
            );
            // Only the island that comes with the game: nothing to choose.
            if entries.len() > 1 {
                world_list(world, right, surface, &entries, chosen, world_tab, ready);
            }
        }
        Screen::Pause => {
            let subtitle = format!(
                "{} \u{b7} {}, day {}",
                summary.player,
                summary.season,
                season_day(world, summary.day)
            );
            w::paragraph(world, panel, subtitle, t::BODY, Tone::secondary(surface));
            let (own, auto) = crate::island::save_ages(world);
            w::paragraph(
                world,
                panel,
                saves_line(own, auto, crate::island::autosave_failed(world)),
                t::CAPTION,
                Tone::secondary(surface),
            );
            let failed = world.resource::<FrontendState>().quit_failed;
            let buttons = column(world, panel, 8.0);
            first = Some(button(
                world,
                buttons,
                &mut tab,
                "Resume",
                Kind::Primary,
                surface,
                Some("Esc"),
                Action::Resume,
            ));
            button(
                world,
                buttons,
                &mut tab,
                "Save game",
                Kind::Secondary,
                surface,
                Some("Ctrl+S"),
                Action::Save,
            );
            let pair = row(world, buttons, 8.0);
            for (text, action) in [
                ("Settings", Action::Settings),
                ("How to play", Action::Controls),
            ] {
                let entity = button(
                    world,
                    pair,
                    &mut tab,
                    text,
                    Kind::Secondary,
                    surface,
                    None,
                    action,
                );
                if let Some(mut node) = world.get_mut::<Node>(entity) {
                    node.flex_grow = 1.0;
                    node.flex_basis = px(0);
                }
            }
            let notice = w::paragraph(world, panel, "", t::BODY, Tone::secondary(surface));
            world.entity_mut(notice).insert(Notice(surface));
            if failed {
                // Saving on the way out failed: leaving without a save is
                // the way out, after its own question.
                button(
                    world,
                    panel,
                    &mut tab,
                    "Quit without saving",
                    Kind::Primary,
                    surface,
                    None,
                    Action::QuitUnsaved,
                );
            }
            w::divider(world, panel, surface, false);
            // The modes are in the top bar; leaving is all that is left here.
            let leave = column(world, panel, 8.0);
            let title = button(
                world,
                leave,
                &mut tab,
                "Save and go to title",
                Kind::Secondary,
                surface,
                None,
                Action::Title,
            );
            rename(world, title, "Save and title");
            let quit = button(
                world,
                leave,
                &mut tab,
                "Save and quit",
                Kind::Secondary,
                surface,
                None,
                Action::Quit,
            );
            rename(world, quit, "Quit game");
            if !failed {
                let unsaved = button(
                    world,
                    leave,
                    &mut tab,
                    "Quit without saving",
                    Kind::Quiet,
                    surface,
                    None,
                    Action::QuitUnsaved,
                );
                rename(world, unsaved, "Quit without saving");
            }
        }
        Screen::ConfirmQuit => {
            let (own, _) = crate::island::save_ages(world);
            let failed = world.resource::<FrontendState>().quit_failed;
            for line in confirm_quit_lines(own.map(|age| age.as_secs()), failed) {
                w::paragraph(world, panel, line, t::BODY, Tone::text(surface));
            }
            let foot = row(world, panel, 8.0);
            // Back is the safe choice, and the first.
            first = Some(button(
                world,
                foot,
                &mut tab,
                "Back",
                Kind::Primary,
                surface,
                Some("Esc"),
                Action::Back,
            ));
            let leave = button(
                world,
                foot,
                &mut tab,
                "Quit without saving",
                Kind::Secondary,
                surface,
                None,
                Action::ConfirmQuitUnsaved,
            );
            rename(world, leave, "Confirm quit without saving");
        }
        Screen::Settings => {
            let current = world.resource::<FrontendState>().settings_tab;
            let strip = w::tabs(world, panel);
            for choice in SettingsTab::ALL {
                let entity = w::tab(
                    world,
                    strip,
                    choice.label(),
                    None,
                    Reach::Tab(tab),
                    Action::SettingsTab(choice),
                );
                tab += 1;
                w::set_state(world, entity, choice == current, false);
            }
            let body = column(world, panel, 10.0);
            if let Some(mut node) = world.get_mut::<Node>(body) {
                node.min_height = px(280);
            }
            settings_rows(world, body, current, &mut tab);
            let notice = w::paragraph(world, panel, "", t::BODY, Tone::secondary(surface));
            world.entity_mut(notice).insert(Notice(surface));
            let foot = row(world, panel, 8.0);
            first = Some(button(
                world,
                foot,
                &mut tab,
                "Back",
                Kind::Primary,
                surface,
                Some("Esc"),
                Action::Back,
            ));
            w::label(
                world,
                foot,
                "Changes apply at once and are kept beside your island save.",
                t::CAPTION,
                Tone::caption(surface),
            );
        }
        Screen::Controls => {
            let current = world.resource::<FrontendState>().guide_page;
            let strip = w::tabs(world, panel);
            for (index, name) in GUIDE_PAGES.into_iter().enumerate() {
                let entity = w::tab(
                    world,
                    strip,
                    name,
                    None,
                    Reach::Tab(tab),
                    Action::GuidePage(index as u8),
                );
                tab += 1;
                w::set_state(world, entity, index as u8 == current, false);
            }
            let body = world
                .spawn((
                    ChildOf(panel),
                    ScrollArea,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        min_height: px(300),
                        flex_shrink: 1.0,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .id();
            for line in guide(current) {
                match line {
                    GuideLine::Text(text) => {
                        w::paragraph(world, body, text, t::BODY, Tone::text(surface));
                    }
                    GuideLine::Key(keys, what) => key_row(world, body, surface, keys, what),
                    GuideLine::Keys(keys, what) => key_row(world, body, surface, &keys, what),
                    GuideLine::Heading(mode) => {
                        w::label(world, body, mode, t::SUBHEADING, Tone::text(surface));
                    }
                }
            }
            let foot = row(world, panel, 8.0);
            first = Some(button(
                world,
                foot,
                &mut tab,
                "Back",
                Kind::Primary,
                surface,
                Some("Esc"),
                Action::Back,
            ));
        }
        Screen::Transition => {
            w::paragraph(
                world,
                panel,
                "Saving the whole island. Work and crops keep their progress.",
                t::BODY,
                Tone::text(surface),
            );
        }
        Screen::Playing => unreachable!(),
    }
    if title_screen {
        let notice = w::paragraph(world, panel, "", t::BODY, Tone::secondary(surface));
        world.entity_mut(notice).insert(Notice(surface));
    }
    // A settings step rebuilds the screen; keep keyboard focus on its control.
    if let Some(wanted) = world.resource::<FrontendState>().refocus {
        let found = world
            .query_filtered::<(Entity, &Action), Without<InteractionDisabled>>()
            .iter(world)
            .find(|(_, action)| **action == wanted)
            .map(|(entity, _)| entity);
        first = found.or(first);
    }
    if let Some(first) = first {
        world.entity_mut(first).insert(AutoFocus);
    }
}

/// How tall the world list may grow before it scrolls.
pub(super) const WORLD_LIST_HEIGHT: f32 = 380.0;

/// The New-game screen's worlds: the default first, then exported worlds,
/// newest first. A world that cannot be used is shown, disabled, with why.
#[allow(clippy::too_many_arguments)]
fn world_list(
    world: &mut World,
    parent: Entity,
    surface: Surface,
    entries: &[super::worlds::WorldEntry],
    chosen: usize,
    first_tab: i32,
    ready: bool,
) {
    w::label(
        world,
        parent,
        "Which island",
        t::SUBHEADING,
        Tone::text(surface),
    );
    w::paragraph(
        world,
        parent,
        "Worlds you export from Shape appear here, newest first. Your save keeps its own copy of the world.",
        t::CAPTION,
        Tone::caption(surface),
    );
    let list = world
        .spawn((
            ChildOf(parent),
            Name::new("World list"),
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                max_height: px(WORLD_LIST_HEIGHT),
                min_height: px(0),
                flex_shrink: 1.0,
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    for (index, entry) in entries.iter().enumerate() {
        let tab = first_tab + i32::try_from(index).unwrap_or(0);
        let action = Action::ChooseWorld(u8::try_from(index).unwrap_or(u8::MAX));
        let card = w::card_button(world, list, &entry.name, 8.0, Reach::Tab(tab), action);
        rename(world, card, &format!("World {}", entry.name));
        let head = row(world, card, 8.0);
        if let Some(mut node) = world.get_mut::<Node>(head) {
            node.justify_content = JustifyContent::SpaceBetween;
            node.align_items = AlignItems::Baseline;
        }
        let name = w::label(world, head, &entry.name, t::SUBHEADING, Tone::text(surface));
        world.entity_mut(name).insert(Part::Label);
        let when = w::label(world, head, &entry.when, t::CAPTION, Tone::caption(surface));
        world.entity_mut(when).insert(Part::Secondary);
        let tone = if entry.problem.is_some() {
            Tone::status(surface, Status::Crit)
        } else {
            Tone::secondary(surface)
        };
        let line = w::paragraph(world, card, &entry.summary, t::CAPTION, tone);
        world.entity_mut(line).insert(Part::Secondary);
        if let Some(file) = &entry.file {
            let name = w::paragraph(world, card, file, t::CAPTION, Tone::caption(surface));
            world.entity_mut(name).insert(Part::Secondary);
        }
        let usable = entry.problem.is_none();
        w::set_state(world, card, usable && index == chosen, false);
        w::set_enabled(world, card, ready && usable);
        // No tooltip: the row already says what the world is (or why it
        // cannot be used), and a card above or beside a stacked row stood
        // over the next one.
    }
}

/// A key cap (or a few) and what it does, on one line.
fn key_row(world: &mut World, parent: Entity, surface: Surface, keys: &str, what: &str) {
    let line = row(world, parent, 10.0);
    let caps = world
        .spawn((
            ChildOf(line),
            Node {
                width: px(132),
                flex_shrink: 0.0,
                column_gap: px(4),
                ..default()
            },
        ))
        .id();
    w::keycap(world, caps, surface, keys);
    let text = w::paragraph(world, line, what, t::DENSE, Tone::text(surface));
    if let Some(mut node) = world.get_mut::<Node>(text) {
        node.width = Val::Auto;
        node.flex_grow = 1.0;
        node.flex_basis = px(0);
    }
}

/// A setting's name at the left of its row.
fn setting_name(world: &mut World, line: Entity, name: &str) {
    let text = w::label(world, line, name, t::BODY, Tone::text(Surface::Paper));
    world.entity_mut(text).insert(Node {
        width: px(150),
        flex_shrink: 0.0,
        ..default()
    });
}

/// A slider with a step button at each end and its value beside it. The
/// step buttons keep the names automation knows ("Master \u{2212}").
fn slider_row(
    world: &mut World,
    body: Entity,
    tab: &mut i32,
    name: &str,
    short: &str,
    kind: SliderKind,
) {
    let settings = world.resource::<SettingsState>().current;
    let line = row(world, body, 8.0);
    setting_name(world, line, name);
    let (down, up) = kind.steps();
    let step = |world: &mut World, tab: &mut i32, sign: &str, change: Change| {
        let entity = w::button(
            world,
            line,
            ButtonSpec::new(sign, Kind::Secondary, Surface::Paper)
                .size(Size::Small)
                .reach(Reach::Tab(*tab)),
            Action::Adjust(change),
        );
        *tab += 1;
        rename(world, entity, &format!("{short} {sign}"));
        w::set_enabled(world, entity, settings.can(change));
    };
    step(world, tab, "\u{2212}", down);
    let (low, high, by) = kind.range();
    let value = kind.value(settings);
    let slider = w::slider(
        world,
        line,
        name,
        Surface::Paper,
        (low, high),
        by,
        f32::from(value),
        &format!("{value}%"),
        Reach::Tab(*tab),
        (),
    );
    *tab += 1;
    step(world, tab, "+", up);
    // The value stands after the second step button.
    world.entity_mut(slider.value).insert(ChildOf(line));
    world.entity_mut(slider.entity).insert((
        SettingSlider {
            kind,
            text: slider.value,
        },
        island_view::game_ui::tooltip::Tooltip::new(name, kind.help()),
    ));
}

/// A switch and the thing it switches.
fn toggle_row(
    world: &mut World,
    body: Entity,
    tab: &mut i32,
    name: &str,
    on: bool,
    change: Change,
    help: &str,
) {
    let line = row(world, body, 10.0);
    setting_name(world, line, name);
    w::toggle(
        world,
        line,
        name,
        Surface::Paper,
        on,
        Reach::Tab(*tab),
        Action::Adjust(change),
    );
    *tab += 1;
    if !help.is_empty() {
        let text = w::paragraph(world, line, help, t::CAPTION, Tone::caption(Surface::Paper));
        if let Some(mut node) = world.get_mut::<Node>(text) {
            node.width = Val::Auto;
            node.flex_grow = 1.0;
            node.flex_basis = px(0);
        }
    }
}

/// One of a few values, as joined segments.
fn choice_row(
    world: &mut World,
    body: Entity,
    tab: &mut i32,
    name: &str,
    options: &[(&str, Change, bool)],
) {
    let line = row(world, body, 10.0);
    setting_name(world, line, name);
    let group = w::segmented(world, line, Surface::Paper);
    for (text, change, on) in options {
        let entity = w::segment(
            world,
            group,
            text,
            None,
            Some(text),
            None,
            Reach::Tab(*tab),
            Action::Adjust(*change),
        );
        *tab += 1;
        w::set_state(world, entity, *on, false);
    }
}

/// The rows of one Settings tab. Every change applies at once.
fn settings_rows(world: &mut World, body: Entity, current: SettingsTab, tab: &mut i32) {
    let settings = world.resource::<SettingsState>().current;
    let paper = Surface::Paper;
    match current {
        SettingsTab::Sound => {
            slider_row(
                world,
                body,
                tab,
                "Master volume",
                "Master",
                SliderKind::Master,
            );
            slider_row(
                world,
                body,
                tab,
                "Ambience",
                "Ambience",
                SliderKind::Ambience,
            );
            slider_row(world, body, tab, "Effects", "Effects", SliderKind::Effects);
            slider_row(world, body, tab, "Music", "Music", SliderKind::Music);
            slider_row(
                world,
                body,
                tab,
                "Interface sounds",
                "Interface sounds",
                SliderKind::Sounds,
            );
            toggle_row(
                world,
                body,
                tab,
                "Mute",
                settings.muted,
                Change::Mute,
                "A session launched muted stays silent whatever is set here.",
            );
        }
        SettingsTab::Display => {
            choice_row(
                world,
                body,
                tab,
                "Interface size",
                &[
                    ("Auto", Change::SetUiScaleAuto(true), settings.ui_scale_auto),
                    (
                        "Fixed",
                        Change::SetUiScaleAuto(false),
                        !settings.ui_scale_auto,
                    ),
                ],
            );
            slider_row(world, body, tab, "Size", "Interface", SliderKind::Interface);
            w::paragraph(
                world,
                body,
                if settings.ui_scale_auto {
                    "Auto follows the window: 1280\u{d7}720 is 100%, 1920\u{d7}1080 is 150%. The size above multiplies it. This line is drawn at the size you chose."
                } else {
                    "Fixed uses the size above whatever the window. This line is drawn at the size you chose."
                },
                t::BODY,
                Tone::secondary(paper),
            );
            choice_row(
                world,
                body,
                tab,
                "Display",
                &[
                    (
                        "Windowed",
                        Change::SetBorderless(false),
                        !settings.borderless,
                    ),
                    (
                        "Borderless",
                        Change::SetBorderless(true),
                        settings.borderless,
                    ),
                ],
            );
        }
        SettingsTab::Controls => {
            slider_row(world, body, tab, "Camera pan speed", "Pan", SliderKind::Pan);
            toggle_row(
                world,
                body,
                tab,
                "Edge pan",
                settings.edge_pan,
                Change::EdgePan,
                "The camera moves while the pointer rests at a window edge.",
            );
            w::label(world, body, "Keys", t::CAPTION, Tone::caption(paper));
            let list = column(world, body, 3.0);
            for (keys, what) in KEYS {
                key_row(world, list, paper, keys, what);
            }
            w::paragraph(
                world,
                body,
                "Keys cannot be changed yet. Each mode's own keys are in the Guide.",
                t::CAPTION,
                Tone::caption(paper),
            );
        }
        SettingsTab::Accessibility => {
            toggle_row(
                world,
                body,
                tab,
                "Reduce motion",
                settings.reduce_motion,
                Change::ReduceMotion,
                "A calmer screen: no pulses or slides, and trees, water and clouds hold still.",
            );
            choice_row(
                world,
                body,
                tab,
                "Tooltip delay",
                &[
                    (
                        "Short",
                        Change::SetTooltipDelay(0),
                        settings.tooltip_delay == 0,
                    ),
                    (
                        "Normal",
                        Change::SetTooltipDelay(1),
                        settings.tooltip_delay == 1,
                    ),
                    (
                        "Long",
                        Change::SetTooltipDelay(2),
                        settings.tooltip_delay == 2,
                    ),
                ],
            );
            toggle_row(
                world,
                body,
                tab,
                "Show tips",
                settings.show_tips,
                Change::ShowTips,
                "First-run tips and the hint under your goal.",
            );
            let again = row(world, body, 10.0);
            setting_name(world, again, "");
            let entity = w::button(
                world,
                again,
                ButtonSpec::new("Show the first-run tips again", Kind::Secondary, paper)
                    .size(Size::Small)
                    .reach(Reach::Tab(*tab)),
                Action::Adjust(Change::ResetTips),
            );
            *tab += 1;
            w::set_enabled(
                world,
                entity,
                settings.tips_seen != 0 || !settings.show_tips,
            );
            choice_row(
                world,
                body,
                tab,
                "Panel style",
                &[
                    ("Parchment", Change::SetDusk(false), !settings.dusk),
                    ("Dusk", Change::SetDusk(true), settings.dusk),
                ],
            );
        }
    }
}

/// What the island's toast lane says for a notice of the playing screen:
/// tone, words, whether it stays, and whether it offers "Try again".
/// A failed save in the toast's words: the reason, then what to do, as
/// sentences. The key is not written: Try again stands beside it, and the
/// host's own line ("… | Free disk space …") joins them with a bar.
fn failed_save_words(text: &str) -> String {
    let (reason, advice) = text.split_once(" | ").unwrap_or((text, ""));
    let reason = reason
        .strip_prefix("Could not save island: ")
        .or_else(|| reason.strip_prefix("Could not save: "))
        .unwrap_or(reason)
        .trim_end_matches('.');
    let advice = advice
        .split(", then")
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.');
    if advice.is_empty() {
        format!("The island could not be saved: {reason}.")
    } else {
        format!("The island could not be saved: {reason}. {advice}, then try again.")
    }
}

fn toast_of(notice: &HudNotice, saved: Option<Saved>) -> (ToastTone, String, bool, bool) {
    let from_save = saved.is_some_and(|saved| saved.text == notice.text);
    match (notice.tone, from_save) {
        // A failed save stays, with its reason, until the next attempt.
        (NoticeTone::Error, true) => (
            ToastTone::Failed,
            failed_save_words(&notice.text),
            true,
            true,
        ),
        (NoticeTone::Error, false) => (ToastTone::Failed, notice.text.clone(), true, false),
        // A save the player asked for: a tick and two words.
        (_, true) => (
            ToastTone::Done,
            notice
                .text
                .split(" \u{b7} ")
                .next()
                .unwrap_or(&notice.text)
                .to_owned(),
            false,
            false,
        ),
        (NoticeTone::Success, false) => (ToastTone::Done, notice.text.clone(), false, false),
        (NoticeTone::Info, false) => (ToastTone::Note, notice.text.clone(), false, false),
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
            .get_resource::<island_view::SceneReadiness>()
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
    let lines: Vec<(Entity, Surface)> = world
        .query::<(Entity, &Notice)>()
        .iter(world)
        .map(|(entity, notice)| (entity, notice.0))
        .collect();
    for (entity, surface) in lines {
        w::set_text(world, entity, &notice);
        w::show(world, entity, !notice.is_empty());
        let wanted = match tone {
            NoticeTone::Error => Tone::status(surface, Status::Crit),
            NoticeTone::Success => Tone::status(surface, Status::Good),
            NoticeTone::Info => Tone::secondary(surface),
        };
        if world.get::<Tone>(entity) != Some(&wanted) {
            world.entity_mut(entity).insert(wanted);
        }
    }
    let begin: Vec<_> = world
        .query_filtered::<Entity, With<BeginButton>>()
        .iter(world)
        .collect();
    let can_begin =
        problem.is_none() && session::summary(world).can_start && super::scene_ready(world);
    for entity in begin {
        w::set_enabled(world, entity, can_begin);
    }
    // A slider shows the setting as it is, without a rebuild under the thumb.
    let settings = world.resource::<SettingsState>().current;
    let sliders: Vec<(Entity, SettingSlider)> = world
        .query::<(Entity, &SettingSlider)>()
        .iter(world)
        .map(|(entity, slider)| (entity, *slider))
        .collect();
    for (entity, slider) in sliders {
        let value = slider.kind.value(settings);
        if world.get::<SliderValue>(entity).map(|held| held.0) != Some(f32::from(value)) {
            world
                .entity_mut(entity)
                .insert(SliderValue(f32::from(value)));
        }
        w::set_text(world, slider.text, &format!("{value}%"));
    }
    // One notice while playing, on the island's toast lane: a save the
    // player asked for, a failed save, or a problem the host must report.
    let playing = screen == Screen::Playing;
    let overseeing = world.get_resource::<IslandMode>() != Some(&IslandMode::Character);
    let host = world.resource::<FrontendState>();
    let host =
        (overseeing && !host.message.is_empty()).then(|| (host.message.clone(), host.notice_tone));
    let saved = crate::island::save_notice(world);
    let saved = saved.as_ref().map(|saved| Saved {
        text: saved.text.as_str(),
        failed: saved.failed,
        routine: saved.routine,
        age: saved.age.as_secs_f32(),
    });
    let shown = playing
        .then(|| {
            hud_notice(
                host.as_ref().map(|(text, tone)| (text.as_str(), *tone)),
                saved,
            )
        })
        .flatten();
    let key = shown
        .as_ref()
        .map(|notice| (notice.text.clone(), notice.tone));
    if world.resource::<ToastShown>().0 != key {
        let was_problem = world
            .resource::<ToastShown>()
            .0
            .as_ref()
            .is_some_and(|(_, tone)| *tone == NoticeTone::Error);
        match &shown {
            Some(notice) => {
                let (tone, text, lasting, retry) = toast_of(notice, saved);
                island_view::island_toast(world, tone, text, lasting, retry);
                // The toast's button is the save's now, not an export's.
                if let Some(mut export) = world.get_resource_mut::<super::export::ExportState>() {
                    export.retry = false;
                    export.offer = None;
                }
            }
            // A problem that is over takes its lasting toast with it.
            None if was_problem => island_view::clear_island_toast(world),
            None => {}
        }
        world.resource_mut::<ToastShown>().0 = key;
    }
}

/// Seconds a routine notice stays at full strength, then takes to fade out.
const NOTICE_HOLD_SECS: f32 = 3.0;
const NOTICE_FADE_SECS: f32 = 1.5;

#[derive(Clone, Debug, PartialEq)]
struct HudNotice {
    text: String,
    tone: NoticeTone,
    /// 1 opaque, fading to 0.
    alpha: f32,
}

/// The island's latest save as the notice needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Saved<'a> {
    text: &'a str,
    failed: bool,
    /// The minute timer's own save, which nobody asked for.
    routine: bool,
    /// Seconds since it was written.
    age: f32,
}

/// The single notice of the playing screen.
///
/// `host` is the shell's own message, `saved` the island's latest save
/// outcome. A problem stays until it is replaced. A save the player asked
/// for (Ctrl+S, F9, the pause menu) is said once, in one place, and fades.
/// The minute timer's own save says nothing at all: it happens every minute
/// and is not news. A failed save always shows, whoever started it. A
/// success the shell reported was the answer to a menu action and was shown
/// in that menu, so it is not repeated over the island.
fn hud_notice(host: Option<(&str, NoticeTone)>, saved: Option<Saved>) -> Option<HudNotice> {
    let steady = |text: &str, tone| {
        Some(HudNotice {
            text: text.into(),
            tone,
            alpha: 1.0,
        })
    };
    match (host, saved) {
        (Some((text, NoticeTone::Error)), _) => steady(text, NoticeTone::Error),
        (
            _,
            Some(Saved {
                text, failed: true, ..
            }),
        ) => steady(text, NoticeTone::Error),
        (Some((text, NoticeTone::Info)), _) => steady(text, NoticeTone::Info),
        (
            _,
            Some(Saved {
                text,
                routine: false,
                age,
                ..
            }),
        ) => {
            let alpha = (1.0 - (age - NOTICE_HOLD_SECS) / NOTICE_FADE_SECS).clamp(0.0, 1.0);
            (alpha > 0.0).then(|| HudNotice {
                text: text.into(),
                tone: NoticeTone::Info,
                alpha,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary_with(
        issue: Option<crate::island::Issue>,
        restart: Option<&str>,
    ) -> session::Summary {
        session::Summary {
            can_continue: false,
            can_start: restart.is_none(),
            active: false,
            player: "Ada".into(),
            day: 6,
            season: "Autumn".into(),
            issue,
            restart: restart.map(str::to_owned),
            has_file: true,
            autosave_newer: None,
        }
    }

    #[test]
    fn the_older_island_card_names_the_file_and_the_backup() {
        use crate::island::Issue;
        let file = "island.world.json";
        let backup = "island.world.older-1.json";
        let card = title_card(&summary_with(Some(Issue::Older), None), file, backup).unwrap();
        assert_eq!(card.title, "Your island is from an older version");
        assert!(card.lines[0].ends_with("The file is untouched: island.world.json."));
        assert!(card.lines[1].ends_with("kept beside it as island.world.older-1.json."));
        assert!(card.copy_folder && card.details.is_none());
        let refused = Issue::Unopened(
            "This file is not a local Island Village save; choose a different --island-save path."
                .into(),
        );
        let card = title_card(&summary_with(Some(refused), None), file, backup).unwrap();
        assert_eq!(card.title, "Your island could not be opened");
        assert_eq!(
            card.details.as_deref(),
            Some("The file is not an island of this game."),
            "developer words stay out of the details too"
        );
        for issue in [Issue::OtherBuild, Issue::Newer] {
            let card = title_card(&summary_with(Some(issue), None), file, backup).unwrap();
            assert!(card.copy_folder, "{}", card.title);
        }
        let card = title_card(&summary_with(None, Some("disk full")), file, backup).unwrap();
        assert_eq!(card.title, "The game has to be restarted");
        assert_eq!(card.details.as_deref(), Some("disk full"));
        assert_eq!(title_card(&summary_with(None, None), file, backup), None);
        assert_eq!(
            saves_line(None, None, false),
            "Your save: none yet \u{b7} Autosave: none yet"
        );
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(12 * 60), "12 minutes ago");
    }
    use bevy::input::{
        ButtonState, InputPlugin,
        keyboard::{Key, KeyboardInput},
    };
    use bevy::input_focus::{InputDispatchPlugin, InputFocusPlugin};
    use bevy::window::PrimaryWindow;

    const SAVED: &str = "Island saved · Ctrl+S";

    fn asked(age: f32) -> Option<Saved<'static>> {
        Some(Saved {
            text: SAVED,
            failed: false,
            routine: false,
            age,
        })
    }

    #[test]
    fn an_autosave_says_nothing_and_a_requested_or_failed_save_does() {
        let routine = |age| {
            hud_notice(
                None,
                Some(Saved {
                    text: SAVED,
                    failed: false,
                    routine: true,
                    age,
                }),
            )
        };
        // Every minute, at any age, with or without a menu answer pending.
        for age in [0.0, 0.5, NOTICE_HOLD_SECS, 59.0, 600.0] {
            assert_eq!(routine(age), None, "an autosave {age} s ago");
        }
        let menu = Some(("Your island is saved.", NoticeTone::Success));
        assert_eq!(
            hud_notice(
                menu,
                Some(Saved {
                    text: SAVED,
                    failed: false,
                    routine: true,
                    age: 0.0
                })
            ),
            None
        );
        // The same save, asked for, is announced once and fades.
        let shown = hud_notice(None, asked(0.0)).expect("a requested save is announced");
        assert_eq!((shown.text.as_str(), shown.tone), (SAVED, NoticeTone::Info));
        assert_eq!(hud_notice(None, asked(60.0)), None);
        // A failure shows and stays, whether the timer or the player saved.
        for routine in [true, false] {
            let failed = hud_notice(
                None,
                Some(Saved {
                    text: "Could not save island: disk full",
                    failed: true,
                    routine,
                    age: 900.0,
                }),
            )
            .expect("a failed save stays on screen");
            assert_eq!((failed.tone, failed.alpha), (NoticeTone::Error, 1.0));
            assert!(failed.text.contains("disk full"));
        }
    }

    #[test]
    fn a_notice_goes_to_the_toast_lane_with_its_tone_and_staying_power() {
        let notice = |text: &str, tone| HudNotice {
            text: text.into(),
            tone,
            alpha: 1.0,
        };
        let saved = |text: &'static str, failed| Saved {
            text,
            failed,
            routine: false,
            age: 0.5,
        };
        // A requested save: a tick and its first words, gone after a while.
        let words = "Island saved \u{b7} living-world.world.json";
        assert_eq!(
            toast_of(&notice(words, NoticeTone::Info), Some(saved(words, false))),
            (ToastTone::Done, "Island saved".to_owned(), false, false)
        );
        // A failed save says so with its reason, stays and offers a retry.
        let failed = "the disk is full";
        assert_eq!(
            toast_of(
                &notice(failed, NoticeTone::Error),
                Some(saved(failed, true))
            ),
            (
                ToastTone::Failed,
                "The island could not be saved: the disk is full.".to_owned(),
                true,
                true
            )
        );
        let worded = "Could not save: read-only folder";
        assert_eq!(
            toast_of(
                &notice(worded, NoticeTone::Error),
                Some(saved(worded, true))
            )
            .1,
            "The island could not be saved: read-only folder.",
            "the reason is not prefixed twice"
        );
        // The host's own line: reason, then advice, without the key.
        let host = "Could not save island: access denied (os error 5) | Free disk space or restore access, then Ctrl+S to retry";
        assert_eq!(
            toast_of(&notice(host, NoticeTone::Error), Some(saved(host, true))).1,
            "The island could not be saved: access denied (os error 5). Free disk space or restore access, then try again."
        );
        // Another problem stays without a retry; plain news is a note.
        assert_eq!(
            toast_of(&notice("No character", NoticeTone::Error), None),
            (ToastTone::Failed, "No character".to_owned(), true, false)
        );
        assert_eq!(
            toast_of(&notice("Island loaded", NoticeTone::Success), None),
            (ToastTone::Done, "Island loaded".to_owned(), false, false)
        );
        assert_eq!(
            toast_of(&notice("Saving\u{2026}", NoticeTone::Info), None),
            (ToastTone::Note, "Saving\u{2026}".to_owned(), false, false)
        );
    }

    #[test]
    fn one_save_notice_fades_and_problems_stay() {
        let saved = |age| hud_notice(None, asked(age));
        let fresh = saved(0.5).expect("a save is announced");
        assert_eq!(
            (fresh.text.as_str(), fresh.alpha),
            ("Island saved · Ctrl+S", 1.0)
        );
        let fading = saved(NOTICE_HOLD_SECS + NOTICE_FADE_SECS / 2.0).unwrap();
        assert!(fading.alpha > 0.4 && fading.alpha < 0.6, "{}", fading.alpha);
        assert_eq!(saved(NOTICE_HOLD_SECS + NOTICE_FADE_SECS), None);
        assert_eq!(saved(600.0), None, "nothing is left on screen");
        // The pause menu's own "saved" answer is not repeated over the island.
        let menu = Some(("Your island is saved.", NoticeTone::Success));
        assert_eq!(hud_notice(menu, None), None);
        let both = hud_notice(menu, asked(1.0)).unwrap();
        assert_eq!(
            both.text, "Island saved · Ctrl+S",
            "one notice, one wording"
        );
        assert_eq!(hud_notice(menu, asked(60.0)), None);
        // A failed save and a host error never fade.
        let failed = hud_notice(
            None,
            Some(Saved {
                text: "Could not save island: disk full",
                failed: true,
                routine: false,
                age: 900.0,
            }),
        )
        .expect("a failure stays");
        assert_eq!((failed.tone, failed.alpha), (NoticeTone::Error, 1.0));
        let error = hud_notice(
            Some(("Another controller owns your character.", NoticeTone::Error)),
            asked(0.0),
        )
        .unwrap();

        assert_eq!(error.tone, NoticeTone::Error);
        assert_eq!(error.text, "Another controller owns your character.");
        assert_eq!(hud_notice(None, None), None);
    }

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
