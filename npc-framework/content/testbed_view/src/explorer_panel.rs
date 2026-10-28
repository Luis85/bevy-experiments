//! The F7 notes editor (agent notes spec G4, §8): the explorer's diary,
//! memory and proposals, read and written as the player. Present only when
//! the app inserted [`ExplorerFiles`] (explorer mode).
//!
//! - **Tabs** Diary, Memory, Proposals (F7 toggles the panel, Ctrl+Tab cycles the
//!   tabs); a scrollable list, newest first: title, id, created and updated
//!   stamps, the body, and for a proposal its status with a button per
//!   status. Below the entries, every file of the book's folder the store
//!   cannot read (no or broken frontmatter, a duplicate id) is named with
//!   the reason; the game never writes such a file, so the dev fixes it by
//!   hand (in an editor or Obsidian) and it shows up as an entry.
//! - **Add** opens a title field and a multi-line body field (Bevy
//!   `TextInput`); **Edit** opens them on the selected entry (click its
//!   title). **Save** calls the store's `add` or `update`, the latter with the
//!   entry's `updated` stamp as it was when editing began, so an entry the
//!   agent (or a hand edit) changed meanwhile is not overwritten: the save is
//!   refused ([`stale_save_note`]), the form keeps the player's text and
//!   shows the entry's current title and body above the fields ("Now in the
//!   file:"), and only then may a second Save replace that version (if the
//!   entry changes again first, the same happens again). **Cancel** drops the
//!   form. A status button sets the proposal's status the same way
//!   ([`STALE_NOTE`] when it changed meanwhile); a status change of the
//!   proposal open in the form is the player's own, so the form follows it.
//! - Everything goes through the one `NotesStores` resource the notes methods
//!   use, as `Writer::Player` (Ruling R1), stamped with the sim tick and the
//!   host's `NotesClock` date. The list is read in the frame the panel opens,
//!   after every write, and then once a second; the store re-reads only
//!   files that changed, so agent writes and hand edits appear within about
//!   a second.
//! - **Input.** While one of the panel's controls has focus
//!   ([`editor_typing`]), the game's input ([`ViewSystems::Input`]: camera,
//!   orders, panel toggles, the app's speed keys) does not run, so typing
//!   never triggers a game key; Tab/Shift+Tab reach fields and Save/Cancel, while Ctrl+Tab
//!   switches books. Escape leaves an open form without discarding its text. While the pointer is over the panel, its clicks and wheel
//!   do not reach the world behind it. Copy, cut and paste use the OS
//!   clipboard ([`crate::field_clipboard`]).
//!
//! Notes are host-side files, never sim state; the panel writes nothing
//! else. The default font (Fira Mono subset) has printable ASCII only.

use std::path::PathBuf;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::AcquireFocus;
use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
use bevy::input_focus::{AutoFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEdit};
use bevy::ui::Pressed;
use bevy::ui::prelude::AccessibleLabel;
use bevy::ui::{Selectable, Selected as UiSelected};
use bevy::ui_widgets::ScrollIntoView;
use bevy::ui_widgets::{
    Activate, Button as WidgetButton, ButtonPlugin, ScrollArea, ScrollAreaPlugin, TextInput,
};
use npc_framework::agent_io::map::{
    Landmark, LandmarkChange, MapEnabled, MapError, NewLandmark, Place,
};
use npc_framework::agent_io::notes::NotesDirs;
use npc_framework::agent_io::notes::{
    Book, NewNote, Note, NoteChange, NotesClock, NotesError, NotesStores, ProposalStatus, Stamp,
    TITLE_MAX, Unreadable, Writer,
};
use npc_framework::prelude::{SimClock, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::ViewSystems;
use npc_framework::view_helpers::command_ui::DragBox;

use crate::panels::TestbedPanels;

/// The agent folder whose notes the panel edits (its `diary/`, `memory/`
/// and `proposals/`, one file per entry).
#[derive(Resource, Clone, Debug)]
pub struct ExplorerFiles {
    pub dir: PathBuf,
}

/// Shown when a save or a status change was refused because the entry
/// changed since the panel read it.
pub const STALE_NOTE: &str = "changed meanwhile - reloaded";
const POLL: Duration = Duration::from_secs(1);
const BOOKS: [Book; 3] = [Book::Diary, Book::Memory, Book::Proposals];
const STATUSES: [ProposalStatus; 4] = [
    ProposalStatus::New,
    ProposalStatus::Liked,
    ProposalStatus::Pursuing,
    ProposalStatus::Rejected,
];
/// The body field's height, in lines.
const BODY_LINES: f32 = 8.0;

const PANEL: Color = Color::srgba(0.05, 0.06, 0.08, 0.88);
const ROW: Color = Color::srgba(0.10, 0.11, 0.14, 0.9);
const FIELD: Color = Color::srgb(0.03, 0.04, 0.05);
const BORDER: Color = Color::srgb(0.35, 0.40, 0.48);
const TEXT: Color = Color::srgb(0.93, 0.95, 0.9);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const NOTE: Color = Color::srgb(0.95, 0.80, 0.45);
const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);
const BUTTON_HOVER: Color = Color::srgb(0.27, 0.32, 0.40);
const BUTTON_PRESSED: Color = Color::srgb(0.14, 0.17, 0.22);
const ACTIVE: Color = Color::srgb(0.42, 0.34, 0.14);
const SELECTED: Color = Color::srgb(0.18, 0.26, 0.36);

/// What a panel button does (one component per button; one observer turns
/// its `Activate` into a request).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelAction {
    /// Show this book.
    Book(Book),
    /// Show the actor's map (when enabled by its world).
    Map,
    /// Select the entry with this id.
    Select(u32),
    /// Open the form for a new entry.
    Add,
    /// Open the form on the selected entry.
    Edit,
    /// Write the form.
    Save,
    /// Drop the form.
    Cancel,
    /// Set this proposal's status.
    Status(u32, ProposalStatus),
}

/// The open form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Form {
    /// `None` for a new entry; else the entry and its `updated` stamp as it
    /// was when editing began (or when a stale save reloaded it).
    pub editing: Option<(u32, Stamp)>,
    /// After a stale save: the entry's title and body now in the file, shown
    /// above the fields, which still hold the player's text.
    pub current: Option<(String, String)>,
}

/// The message after a save refused because entry `id` changed meanwhile.
pub fn stale_save_note(id: u32) -> String {
    format!(
        "#{id} was changed meanwhile - its current text is shown above; Save again to \
         replace it with yours, or Cancel"
    )
}

/// The editor's state.
#[derive(Resource, Debug)]
pub(crate) struct NotesEditor {
    pub map: bool,
    pub landmarks: Vec<Landmark>,
    pub selected_landmark: Option<u32>,
    pub map_form: Option<MapForm>,
    /// The book shown.
    pub book: Book,
    /// Its entries, newest (highest id) first.
    pub notes: Vec<Note>,
    /// The files of its folder that are not entries, with the reason.
    pub unreadable: Vec<Unreadable>,
    /// The selected entry.
    pub selected: Option<u32>,
    /// The form, while adding or editing.
    pub form: Option<Form>,
    /// The last result or error.
    pub message: Option<String>,
    /// Button presses not handled yet.
    requests: Vec<PanelAction>,
    /// The list must be rebuilt.
    rows_dirty: bool,
}

impl Default for NotesEditor {
    fn default() -> Self {
        Self {
            map: false,
            landmarks: Vec::new(),
            selected_landmark: None,
            map_form: None,
            book: Book::Diary,
            notes: Vec::new(),
            unreadable: Vec::new(),
            selected: None,
            form: None,
            message: None,
            requests: Vec::new(),
            rows_dirty: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MapForm {
    editing: Option<(u32, Stamp)>,
    current: Option<Landmark>,
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum MapField {
    Name,
    Kind,
    X,
    Z,
    Note,
}
/// Focusable notes controls, including form actions; game input stays shielded.
#[derive(Component)]
pub struct NotesControl;

/// Notes keyboard input precedes game input; hosts gate this alongside game input.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotesInput;

/// The actor owning this host-side folder, with the world's map switch applied.
pub(crate) fn map_actor(
    files: &ExplorerFiles,
    dirs: Option<&NotesDirs>,
    enabled: Option<&MapEnabled>,
) -> Option<SimId> {
    if enabled.is_some_and(|flag| !flag.0) {
        return None;
    }
    dirs?
        .0
        .iter()
        .find(|(_, dir)| *dir == &files.dir)
        .map(|(id, _)| *id)
}
#[derive(SystemParam)]
struct MapContext<'w, 's> {
    enabled: Option<Res<'w, MapEnabled>>,
    dirs: Option<Res<'w, NotesDirs>>,
    index: Option<Res<'w, SimIndex>>,
    positions: Query<'w, 's, &'static SimPos>,
}
impl MapContext<'_, '_> {
    fn actor(&self, files: &ExplorerFiles) -> Option<SimId> {
        map_actor(files, self.dirs.as_deref(), self.enabled.as_deref())
    }
    fn here(&self, files: &ExplorerFiles) -> Option<[i32; 2]> {
        let entity = self.index.as_deref()?.get(self.actor(files)?)?;
        let pos = self.positions.get(entity).ok()?;
        Some([pos.x, pos.z])
    }
}

#[derive(Resource)]
pub(crate) struct PanelState {
    pub timer: Timer,
    /// The panel was shown last frame.
    was_open: bool,
}

/// The panel box (background, height cap, `Hovered` for the pointer).
#[derive(Component)]
pub(crate) struct ExplorerPanel;
/// The result / error line.
#[derive(Component)]
struct MessageText;
/// The scrollable list; its children are rebuilt.
#[derive(Component)]
struct ListBox;
/// The form box; its children exist while the form is open.
#[derive(Component)]
struct FormBox;
/// A text of the list (title, stamps, body, empty note).
#[derive(Component)]
pub(crate) struct RowText;
/// One of the F7 panel's text fields (see [`editor_typing`]).
#[derive(Component)]
pub struct NoteField;
/// The form's title field.
#[derive(Component)]
pub(crate) struct TitleField;
/// The form's body field.
#[derive(Component)]
pub(crate) struct BodyField;
/// The "Now in the file:" block of the form (shown after a stale save).
#[derive(Component)]
struct CurrentBox;
/// Its text.
#[derive(Component)]
pub(crate) struct CurrentText;

pub(crate) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<ButtonPlugin>() {
        app.add_plugins(ButtonPlugin);
    }
    if !app.is_plugin_added::<ScrollAreaPlugin>() {
        app.add_plugins(ScrollAreaPlugin);
    }
    app.insert_resource(PanelState {
        timer: Timer::new(POLL, TimerMode::Repeating),
        was_open: false,
    })
    .init_resource::<NotesEditor>()
    .init_resource::<NotesStores>()
    .add_plugins(crate::field_clipboard::plugin)
    .add_observer(on_activate)
    .add_observer(acquire_notes_focus)
    .configure_sets(Update, ViewSystems::Input.run_if(not(editor_typing)))
    .configure_sets(
        Update,
        NotesInput
            .before(ViewSystems::Input)
            .run_if(not(crate::explorer_modal::modal_open)),
    )
    .add_systems(Startup, spawn.run_if(resource_exists::<ExplorerFiles>))
    .add_systems(
        Update,
        (
            keys.in_set(NotesInput).before(apply_requests),
            shield_world.before(ViewSystems::Input),
        )
            .run_if(resource_exists::<ExplorerFiles>),
    )
    .add_systems(
        Update,
        (apply_requests, refresh, paint)
            .chain()
            .in_set(ViewSystems::Present)
            .run_if(resource_exists::<ExplorerFiles>),
    );
}

/// Run condition: a notes control owns keyboard focus, including form actions.
pub fn editor_typing(
    focus: Option<Res<InputFocus>>,
    fields: Query<(), With<NotesControl>>,
) -> bool {
    focus
        .and_then(|f| f.get())
        .is_some_and(|e| fields.contains(e))
}

/// A stamp as the list shows it: `t<tick>` and the date when there is one.
pub(crate) fn stamp_text(s: &Stamp) -> String {
    if s.date.is_empty() {
        format!("t{}", s.tick)
    } else {
        format!("t{} {}", s.tick, s.date)
    }
}

fn book_label(book: Book) -> &'static str {
    match book {
        Book::Diary => "Diary",
        Book::Memory => "Memory",
        Book::Proposals => "Proposals",
    }
}

fn empty_label(book: Book) -> &'static str {
    match book {
        Book::Diary => "No diary entries yet",
        Book::Memory => "No memories yet",
        Book::Proposals => "No proposals yet",
    }
}

fn font(px: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(px),
        ..default()
    }
}

fn button_node() -> Node {
    Node {
        padding: UiRect::axes(px(10), px(3)),
        justify_content: JustifyContent::Center,
        border_radius: BorderRadius::all(px(3)),
        border: UiRect::all(px(1)),
        ..default()
    }
}

/// A labelled panel button doing `action`.
fn button(parent: &mut ChildSpawnerCommands, label: &str, action: PanelAction) {
    // The widget's role must win over AccessibleLabel's generic required node.
    let mut entity = parent.spawn((
        WidgetButton,
        action,
        NotesControl,
        TabIndex(0),
        AccessibleLabel::new(match action {
            PanelAction::Status(id, _) => format!("Mark proposal #{id} {label}"),
            _ => label.to_owned(),
        }),
        button_node(),
        BackgroundColor(BUTTON),
        BorderColor::all(Color::NONE),
        Hovered::default(),
    ));
    if matches!(
        action,
        PanelAction::Book(_) | PanelAction::Map | PanelAction::Select(_) | PanelAction::Status(..)
    ) {
        entity.insert(Selectable);
    }
    entity.with_child((Text::new(label), font(12.0), TextColor(TEXT)));
}

fn spawn(
    mut commands: Commands,
    panels: Res<TestbedPanels>,
    mut editor: ResMut<NotesEditor>,
    files: Res<ExplorerFiles>,
    context: MapContext,
) {
    editor.map = panels.map && context.actor(&files).is_some();
    commands
        .spawn((
            Name::new("explorer notes panel"),
            ExplorerPanel,
            TabGroup::modal(),
            GlobalZIndex(20),
            Hovered::default(),
            BackgroundColor(PANEL),
            Node {
                position_type: PositionType::Absolute,
                top: px(10),
                right: percent(2),
                width: percent(96),
                max_width: px(560),
                max_height: percent(85.0),
                padding: UiRect::all(px(8)),
                row_gap: px(6),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    column_gap: px(6),
                    row_gap: px(4),
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("F7 NOTES"), font(13.0), TextColor(NOTE)));
                    for book in BOOKS {
                        button(row, book_label(book), PanelAction::Book(book));
                    }
                    button(row, "Map", PanelAction::Map);
                    row.spawn((
                        Text::new("Tab: controls  Ctrl+Tab: next book"),
                        font(11.0),
                        TextColor(DIM),
                    ));
                });
            panel
                .spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    button(row, "Add", PanelAction::Add);
                    button(row, "Edit", PanelAction::Edit);
                    row.spawn((MessageText, Text::new(""), font(12.0), TextColor(NOTE)));
                });
            panel.spawn((
                FormBox,
                TabGroup::modal(),
                ScrollArea,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 1.0,
                    min_height: px(0),
                    max_height: px(380),
                    overflow: Overflow::scroll_y(),
                    display: Display::None,
                    ..default()
                },
            ));
            panel.spawn((
                ListBox,
                ScrollArea,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    overflow: Overflow::scroll_y(),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_height: px(0),
                    ..default()
                },
            ));
        });
}

/// A button press becomes a request (handled in [`apply_requests`]).
fn on_activate(ev: On<Activate>, actions: Query<&PanelAction>, mut editor: ResMut<NotesEditor>) {
    if let Ok(action) = actions.get(ev.entity) {
        editor.requests.push(*action);
    }
}

fn acquire_notes_focus(
    mut event: On<AcquireFocus>,
    controls: Query<(), With<NotesControl>>,
    mut focus: ResMut<InputFocus>,
) {
    if controls.contains(event.focused_entity) {
        event.propagate(false);
        focus.set(event.focused_entity, FocusCause::Navigated);
    }
}

#[derive(SystemParam)]
struct KeyboardContext<'w, 's> {
    focus: ResMut<'w, InputFocus>,
    navigation: TabNavigation<'w, 's>,
    roots: Query<'w, 's, Entity, With<ExplorerPanel>>,
    forms: Query<'w, 's, Entity, With<FormBox>>,
    fields: Query<'w, 's, (), With<NoteField>>,
    controls: Query<'w, 's, (), With<NotesControl>>,
    actions: Query<'w, 's, &'static PanelAction>,
}

/// Standard control traversal; Ctrl+Tab keeps fast book switching available.
fn keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut panels: ResMut<TestbedPanels>,
    mut editor: ResMut<NotesEditor>,
    files: Res<ExplorerFiles>,
    context: MapContext,
    mut keyboard: KeyboardContext,
    mut commands: Commands,
) {
    let Some(keys) = keys else { return };
    let typing = keyboard
        .focus
        .get()
        .is_some_and(|entity| keyboard.fields.contains(entity));
    if keys.just_pressed(KeyCode::F7) && !typing {
        panels.explorer = !panels.explorer;
        if !panels.explorer {
            keyboard.focus.clear();
        }
    }
    if !panels.explorer {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        keyboard.focus.clear();
        if editor.form.is_none() && editor.map_form.is_none() {
            panels.explorer = false;
        }
        return;
    }
    if keys.just_pressed(KeyCode::Tab)
        && (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
    {
        let i = BOOKS.iter().position(|b| *b == editor.book).unwrap_or(0);
        let next = if editor.map {
            PanelAction::Book(Book::Diary)
        } else if i + 1 == BOOKS.len() && context.actor(&files).is_some() {
            PanelAction::Map
        } else {
            PanelAction::Book(BOOKS[(i + 1) % BOOKS.len()])
        };
        editor.requests.push(next);
    } else if keys.just_pressed(KeyCode::Tab) {
        let action = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            NavAction::Previous
        } else {
            NavAction::Next
        };
        let form_open = editor.form.is_some() || editor.map_form.is_some();
        let in_form = keyboard.focus.get().is_some_and(|entity| {
            keyboard.fields.contains(entity)
                || keyboard
                    .actions
                    .get(entity)
                    .is_ok_and(|action| matches!(action, PanelAction::Save | PanelAction::Cancel))
        });
        let next = if form_open && !in_form {
            let Ok(form) = keyboard.forms.single() else {
                return;
            };
            keyboard.navigation.initialize(form, action)
        } else if keyboard
            .focus
            .get()
            .is_some_and(|entity| keyboard.controls.contains(entity))
        {
            keyboard.navigation.navigate(&keyboard.focus, action)
        } else if let Ok(root) = keyboard.roots.single() {
            keyboard.navigation.initialize(root, action)
        } else {
            return;
        };
        if let Ok(entity) = next {
            keyboard.focus.set(entity, FocusCause::Navigated);
            commands.trigger(ScrollIntoView { entity });
        }
    }
}

/// While the pointer is over the open panel, its clicks and wheel do not
/// reach the world (the command UI, the camera) behind it.
fn shield_world(
    panels: Res<TestbedPanels>,
    roots: Query<&Hovered, With<ExplorerPanel>>,
    mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    scroll: Option<ResMut<AccumulatedMouseScroll>>,
    drag: Option<ResMut<DragBox>>,
) {
    if !panels.explorer || !roots.iter().any(Hovered::get) {
        return;
    }
    if let Some(mut mouse) = mouse {
        // Clear held buttons too; a swallowed release must not leave an RTS
        // drag or camera pan running behind the panel.
        mouse.reset_all();
    }
    if let Some(mut drag) = drag {
        drag.start = None;
    }
    if let Some(mut scroll) = scroll
        && scroll.delta != Vec2::ZERO
    {
        scroll.delta = Vec2::ZERO;
    }
}

/// Reads the book shown; marks the list for a rebuild when it changed.
fn reload(editor: &mut NotesEditor, stores: &mut NotesStores, files: &ExplorerFiles) {
    if editor.map {
        reload_map(editor, stores, files);
        return;
    }
    let book = editor.book;
    let listed = stores
        .store_for(&files.dir)
        .and_then(|store| Ok((store.list(book)?, store.unreadable(book))));
    match listed {
        Ok((mut notes, unreadable)) => {
            notes.reverse();
            if notes != editor.notes {
                editor.notes = notes;
                editor.rows_dirty = true;
            }
            if unreadable != editor.unreadable {
                editor.unreadable = unreadable;
                editor.rows_dirty = true;
            }
        }
        Err(e) => editor.message = Some(format!("notes: {e}")),
    }
}

/// Spawns the form's fields into `form_box`, holding `title` and `body`.
fn open_form(commands: &mut Commands, form_box: Entity, title: &str, body: &str) {
    commands
        .entity(form_box)
        .despawn_children()
        .with_children(|form| {
            form.spawn((
                CurrentBox,
                Node {
                    padding: UiRect::all(px(5)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(3)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(ROW),
                BorderColor::all(NOTE),
            ))
            .with_child((CurrentText, Text::new(""), font(12.0), TextColor(TEXT)));
            form.spawn((Text::new("Title"), font(11.0), TextColor(DIM)));
            form.spawn((
                TitleField,
                AutoFocus,
                field(
                    EditableText {
                        max_characters: Some(TITLE_MAX),
                        ..EditableText::new(title)
                    },
                    TextLayout::no_wrap(),
                    "Title",
                ),
            ));
            form.spawn((Text::new("Body"), font(11.0), TextColor(DIM)));
            form.spawn((
                BodyField,
                field(
                    EditableText {
                        allow_newlines: true,
                        visible_lines: Some(BODY_LINES),
                        ..EditableText::new(body)
                    },
                    TextLayout::default(),
                    "Body",
                ),
            ));
            form.spawn(Node {
                column_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                button(row, "Save", PanelAction::Save);
                button(row, "Cancel", PanelAction::Cancel);
            });
        });
}

fn field(mut text: EditableText, layout: TextLayout, label: &str) -> impl Bundle {
    let multiline = text.allow_newlines;
    if !multiline {
        text.visible_lines = Some(1.0);
    }
    // Keep TextInput before AccessibleLabel so its native role is retained.
    (
        TextInput,
        NoteField,
        NotesControl,
        TabIndex(0),
        AccessibleLabel::new(label),
        text,
        layout,
        font(13.0),
        TextColor(TEXT),
        TextCursorStyle::default(),
        Node {
            width: percent(100),
            min_width: px(0),
            min_height: px(if multiline { 68 } else { 36 }),
            padding: UiRect::all(px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(FIELD),
        BorderColor::all(BORDER),
    )
}

/// The text of a field (empty when it is missing).
fn field_value<F: Component>(fields: &Query<(&EditableText, Has<F>)>) -> String {
    fields
        .iter()
        .find(|(_, is)| *is)
        .map(|(t, _)| t.value().to_string())
        .unwrap_or_default()
}

fn reload_map(editor: &mut NotesEditor, stores: &mut NotesStores, files: &ExplorerFiles) {
    let listed = stores
        .map_for(&files.dir)
        .and_then(|store| Ok((store.list()?, store.unreadable()?)));
    match listed {
        Ok((mut landmarks, unreadable)) => {
            landmarks.reverse();
            if editor.landmarks != landmarks {
                editor.landmarks = landmarks;
                editor.rows_dirty = true;
            }
            if editor.unreadable != unreadable {
                editor.unreadable = unreadable;
                editor.rows_dirty = true;
            }
        }
        Err(error) => editor.message = Some(format!("map: {error}")),
    }
}
fn open_map_form(commands: &mut Commands, form_box: Entity, landmark: Option<&Landmark>) {
    commands
        .entity(form_box)
        .despawn_children()
        .with_children(|form| {
            form.spawn((
                CurrentBox,
                Node {
                    padding: UiRect::all(px(5)),
                    border: UiRect::all(px(1)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(ROW),
                BorderColor::all(NOTE),
            ))
            .with_child((CurrentText, Text::new(""), font(12.0), TextColor(TEXT)));
            form.spawn((
                Text::new("Position: leave both coordinates blank to mark Here when saved."),
                font(11.0),
                TextColor(DIM),
            ));
            let values = [
                (
                    MapField::Name,
                    "Name",
                    landmark.map(|m| m.name.clone()).unwrap_or_default(),
                ),
                (
                    MapField::Kind,
                    "Kind (optional)",
                    landmark.and_then(|m| m.kind.clone()).unwrap_or_default(),
                ),
                (
                    MapField::X,
                    "X (mm, blank with Z = Here)",
                    landmark.map(|m| m.pos[0].to_string()).unwrap_or_default(),
                ),
                (
                    MapField::Z,
                    "Z (mm, blank with X = Here)",
                    landmark.map(|m| m.pos[1].to_string()).unwrap_or_default(),
                ),
                (
                    MapField::Note,
                    "Note",
                    landmark.map(|m| m.note.clone()).unwrap_or_default(),
                ),
            ];
            for (key, label, value) in values {
                form.spawn((Text::new(label), font(11.0), TextColor(DIM)));
                let multiline = key == MapField::Note;
                let mut text = EditableText {
                    allow_newlines: multiline,
                    visible_lines: Some(if multiline { 3.0 } else { 1.0 }),
                    ..EditableText::new(value)
                };
                text.pending_edits.clear();
                text.queue_edit(TextEdit::TextStart(false));
                let mut entity = form.spawn((
                    key,
                    field(
                        text,
                        if multiline {
                            TextLayout::default()
                        } else {
                            TextLayout::no_wrap()
                        },
                        label,
                    ),
                ));
                if key == MapField::Name {
                    entity.insert(AutoFocus);
                }
            }
            form.spawn(Node {
                column_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                button(row, "Save", PanelAction::Save);
                button(row, "Cancel", PanelAction::Cancel);
            });
        });
}
fn map_value(fields: &Query<(&EditableText, &MapField)>, key: MapField) -> String {
    fields
        .iter()
        .find(|(_, field)| **field == key)
        .map(|(text, _)| text.value().to_string())
        .unwrap_or_default()
}
fn map_position(x: &str, z: &str) -> Result<Option<[i32; 2]>, String> {
    match (x.trim().is_empty(), z.trim().is_empty()) {
        (true, true) => Ok(None),
        (true, false) | (false, true) => {
            Err("Enter both X and Z, or leave both blank for Here".into())
        }
        (false, false) => Ok(Some([
            x.trim()
                .parse()
                .map_err(|_| "X must be a whole number in mm")?,
            z.trim()
                .parse()
                .map_err(|_| "Z must be a whole number in mm")?,
        ])),
    }
}
#[allow(clippy::too_many_arguments)]
fn apply_map_request(
    request: PanelAction,
    editor: &mut NotesEditor,
    stores: &mut NotesStores,
    files: &ExplorerFiles,
    at: &Stamp,
    form_box: Option<Entity>,
    fields: &Query<(&EditableText, &MapField)>,
    context: &MapContext,
    commands: &mut Commands,
) -> bool {
    match request {
        PanelAction::Select(id) => editor.selected_landmark = Some(id),
        PanelAction::Add | PanelAction::Edit if editor.map_form.is_some() => {
            editor.message = Some("save or cancel the landmark first".into())
        }
        PanelAction::Add => {
            if let Some(form_box) = form_box {
                open_map_form(commands, form_box, None);
                editor.map_form = Some(MapForm {
                    editing: None,
                    current: None,
                });
                editor.message = None;
            }
        }
        PanelAction::Edit => {
            let landmark = editor
                .selected_landmark
                .and_then(|id| editor.landmarks.iter().find(|m| m.id == id))
                .cloned();
            if let (Some(landmark), Some(form_box)) = (landmark, form_box) {
                open_map_form(commands, form_box, Some(&landmark));
                editor.map_form = Some(MapForm {
                    editing: Some((landmark.id, landmark.updated)),
                    current: None,
                });
                editor.message = None;
            } else {
                editor.message = Some("select a landmark first (click its name)".into());
            }
        }
        PanelAction::Cancel => {
            editor.map_form = None;
            return true;
        }
        PanelAction::Save => {
            let Some(form) = editor.map_form.clone() else {
                return false;
            };
            let name = map_value(fields, MapField::Name);
            let kind = map_value(fields, MapField::Kind);
            let note = map_value(fields, MapField::Note);
            let pos = match map_position(
                &map_value(fields, MapField::X),
                &map_value(fields, MapField::Z),
            ) {
                Ok(pos) => pos,
                Err(error) => {
                    editor.message = Some(error);
                    return false;
                }
            };
            let resolved = match pos.or_else(|| context.here(files)) {
                Some(pos) => pos,
                None => {
                    editor.message = Some("The played actor has no position for Here".into());
                    return false;
                }
            };
            let kind = (!kind.trim().is_empty()).then(|| kind.trim().to_owned());
            let written = stores
                .map_for(&files.dir)
                .and_then(|store| match &form.editing {
                    None => store.add(
                        NewLandmark {
                            name,
                            place: if pos.is_none() {
                                Place::Here
                            } else {
                                Place::Pos(resolved)
                            },
                            kind,
                            note,
                        },
                        resolved,
                        None,
                        at.clone(),
                        Writer::Player,
                    ),
                    Some((id, expected)) => store.update(
                        *id,
                        expected,
                        LandmarkChange {
                            name: Some(name),
                            kind: Some(kind),
                            pos: Some(resolved),
                            note: Some(note),
                        },
                        at.clone(),
                        Writer::Player,
                    ),
                });
            match written {
                Ok(landmark) => {
                    editor.selected_landmark = Some(landmark.id);
                    editor.map_form = None;
                    editor.message = Some(format!(
                        "{} landmark #{}",
                        if form.editing.is_none() {
                            "added"
                        } else {
                            "saved"
                        },
                        landmark.id
                    ));
                    reload_map(editor, stores, files);
                    return true;
                }
                Err(MapError::Stale { id, current }) => {
                    reload_map(editor, stores, files);
                    let now = editor.landmarks.iter().find(|m| m.id == id).cloned();
                    editor.map_form = Some(MapForm {
                        editing: Some((id, current)),
                        current: now,
                    });
                    editor.message = Some(stale_save_note(id));
                }
                Err(error) => editor.message = Some(error.to_string()),
            }
            reload_map(editor, stores, files);
        }
        _ => {}
    }
    false
}
fn spawn_map_rows(list: &mut ChildSpawnerCommands, editor: &NotesEditor) {
    if editor.landmarks.is_empty() {
        list.spawn((
            RowText,
            Text::new("No landmarks yet. Add marks the actor's current position by default."),
            font(12.0),
            TextColor(DIM),
        ));
    }
    for landmark in &editor.landmarks {
        list.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(2),
                padding: UiRect::all(px(5)),
                ..default()
            },
            BackgroundColor(ROW),
        ))
        .with_children(|row| {
            let mut detail = format!(
                "#{}  position {}, {} mm",
                landmark.id, landmark.pos[0], landmark.pos[1]
            );
            if let Some(kind) = &landmark.kind {
                detail.push_str(&format!("  kind: {kind}"));
            }
            if let Some(thing) = landmark.thing {
                detail.push_str(&format!("  thing: #{}", thing.0));
            }
            row.spawn((
                WidgetButton,
                PanelAction::Select(landmark.id),
                NotesControl,
                TabIndex(0),
                Selectable,
                AccessibleLabel::new(format!(
                    "Select landmark #{} {}",
                    landmark.id, landmark.name
                )),
                button_node(),
                Hovered::default(),
                BackgroundColor(Color::NONE),
                BorderColor::all(Color::NONE),
            ))
            .with_child((
                RowText,
                Text::new(&landmark.name),
                font(13.0),
                TextColor(TEXT),
            ));
            row.spawn((RowText, Text::new(detail), font(11.0), TextColor(DIM)));
            row.spawn((
                RowText,
                Text::new(format!(
                    "created {}  updated {}",
                    stamp_text(&landmark.created),
                    stamp_text(&landmark.updated)
                )),
                font(11.0),
                TextColor(DIM),
            ));
            if !landmark.note.is_empty() {
                row.spawn((
                    RowText,
                    Text::new(&landmark.note),
                    font(12.0),
                    TextColor(TEXT),
                ));
            }
        });
    }
    for entry in &editor.unreadable {
        list.spawn((
            RowText,
            Text::new(unreadable_text(entry)),
            font(11.0),
            TextColor(NOTE),
        ));
    }
}

/// Handles the button presses and keys of this frame: switches books,
/// selects, opens and closes the form, writes through the store.
#[allow(clippy::too_many_arguments)]
fn apply_requests(
    mut commands: Commands,
    mut editor: ResMut<NotesEditor>,
    mut stores: ResMut<NotesStores>,
    files: Res<ExplorerFiles>,
    clock: Option<Res<SimClock>>,
    date: Option<Res<NotesClock>>,
    form_box: Query<Entity, With<FormBox>>,
    titles: Query<(&EditableText, Has<TitleField>)>,
    bodies: Query<(&EditableText, Has<BodyField>)>,
    focus: Option<ResMut<InputFocus>>,
    map_fields: Query<(&EditableText, &MapField)>,
    context: MapContext,
) {
    if editor.requests.is_empty() {
        return;
    }
    let at = Stamp {
        tick: clock.map_or(0, |c| c.tick),
        date: date.map_or_else(String::new, |c| (c.0)()),
    };
    let form_box = form_box.single().ok();
    let mut close = false;
    for request in std::mem::take(&mut editor.requests) {
        let book = editor.book;
        if editor.map && !matches!(request, PanelAction::Book(_) | PanelAction::Map) {
            if context.actor(&files).is_none() {
                editor.message = Some("Map is disabled in this world".into());
                continue;
            }
            close = apply_map_request(
                request,
                &mut editor,
                &mut stores,
                &files,
                &at,
                form_box,
                &map_fields,
                &context,
                &mut commands,
            );
            continue;
        }
        match request {
            PanelAction::Map if context.actor(&files).is_none() => {}
            PanelAction::Map if editor.form.is_some() => {
                editor.message = Some("save or cancel the entry first".into());
            }
            PanelAction::Map => {
                editor.map = true;
                editor.rows_dirty = true;
                editor.message = None;
                reload(&mut editor, &mut stores, &files);
            }
            PanelAction::Book(_) if editor.map_form.is_some() => {
                editor.message = Some("save or cancel the landmark first".into());
            }
            PanelAction::Book(b) if editor.form.is_some() && b != book => {
                editor.message = Some("save or cancel the entry first".into());
            }
            PanelAction::Book(b) => {
                if editor.map {
                    editor.map = false;
                    editor.rows_dirty = true;
                    editor.message = None;
                }
                if b != book {
                    editor.book = b;
                    editor.selected = None;
                    editor.notes.clear();
                    editor.rows_dirty = true;
                    editor.message = None;
                }
                reload(&mut editor, &mut stores, &files);
            }
            PanelAction::Select(id) => editor.selected = Some(id),
            PanelAction::Add | PanelAction::Edit if editor.form.is_some() => {
                editor.message = Some("save or cancel the entry first".into());
            }
            PanelAction::Add => {
                if let Some(form_box) = form_box {
                    open_form(&mut commands, form_box, "", "");
                    editor.form = Some(Form {
                        editing: None,
                        current: None,
                    });
                    close = false;
                    editor.message = None;
                }
            }
            PanelAction::Edit => {
                let note = editor
                    .selected
                    .and_then(|id| editor.notes.iter().find(|n| n.id == id))
                    .cloned();
                match (note, form_box) {
                    (Some(note), Some(form_box)) => {
                        open_form(&mut commands, form_box, &note.title, &note.body);
                        close = false;
                        editor.form = Some(Form {
                            editing: Some((note.id, note.updated)),
                            current: None,
                        });
                        editor.message = None;
                    }
                    _ => editor.message = Some("select an entry first (click its title)".into()),
                }
            }
            PanelAction::Cancel => {
                editor.form = None;
                close = true;
            }
            PanelAction::Save => {
                let Some(form) = editor.form.clone() else {
                    continue;
                };
                let title = field_value(&titles);
                let body = field_value(&bodies);
                if title.trim().is_empty() {
                    editor.message = Some("an entry needs a title".into());
                    continue;
                }
                let written = stores
                    .store_for(&files.dir)
                    .and_then(|store| match &form.editing {
                        None => store.add(
                            book,
                            Writer::Player,
                            NewNote {
                                title,
                                body,
                                prompted_by: None,
                            },
                            at.clone(),
                        ),
                        Some((id, expected)) => store.update(
                            book,
                            Writer::Player,
                            *id,
                            expected,
                            NoteChange {
                                title: Some(title),
                                body: Some(body),
                                status: None,
                            },
                            at.clone(),
                        ),
                    });
                match written {
                    Ok(note) => {
                        editor.message = Some(match form.editing {
                            None => format!("added #{}", note.id),
                            Some(_) => format!("saved #{}", note.id),
                        });
                        editor.selected = Some(note.id);
                        editor.form = None;
                        close = true;
                    }
                    Err(NotesError::Stale { id, current }) => {
                        // An informed choice: the version in the file is
                        // shown, and only a second Save (based on it) may
                        // replace it.
                        reload(&mut editor, &mut stores, &files);
                        let now = editor
                            .notes
                            .iter()
                            .find(|n| n.id == id)
                            .map(|n| (n.title.clone(), n.body.clone()));
                        editor.message = Some(stale_save_note(id));
                        editor.form = Some(Form {
                            editing: Some((id, current)),
                            current: now,
                        });
                    }
                    Err(e) => editor.message = Some(e.to_string()),
                }
                reload(&mut editor, &mut stores, &files);
            }
            PanelAction::Status(id, status) => {
                let Some(note) = editor.notes.iter().find(|n| n.id == id).cloned() else {
                    continue;
                };
                if note.status == Some(status) {
                    continue;
                }
                let written = stores.store_for(&files.dir).and_then(|store| {
                    store.update(
                        Book::Proposals,
                        Writer::Player,
                        id,
                        &note.updated,
                        NoteChange {
                            status: Some(status),
                            ..default()
                        },
                        at.clone(),
                    )
                });
                // The player's own status change of the proposal open in the
                // form (which still had the listed version) moves the form
                // along, so its Save is not refused as stale.
                if let (
                    Ok(new),
                    Some(Form {
                        editing: Some((open, stamp)),
                        ..
                    }),
                ) = (&written, editor.form.as_mut())
                    && *open == id
                    && *stamp == note.updated
                {
                    stamp.clone_from(&new.updated);
                }
                editor.message = Some(match written {
                    Ok(_) => format!("#{id} is {}", status.as_str()),
                    Err(NotesError::Stale { .. }) => STALE_NOTE.into(),
                    Err(e) => e.to_string(),
                });
                reload(&mut editor, &mut stores, &files);
            }
        }
    }
    if close && let Some(form_box) = form_box {
        commands.entity(form_box).despawn_children();
        if let Some(mut focus) = focus
            && focus.get().is_some()
        {
            focus.clear();
        }
    }
}

/// Visibility, the 1 s poll, the list rebuild and the message line.
#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    time: Res<Time>,
    panels: Res<TestbedPanels>,
    files: Res<ExplorerFiles>,
    mut state: ResMut<PanelState>,
    mut editor: ResMut<NotesEditor>,
    mut stores: ResMut<NotesStores>,
    mut boxes: Query<
        (&mut Node, Has<ExplorerPanel>, Has<CurrentBox>),
        Or<(With<ExplorerPanel>, With<FormBox>, With<CurrentBox>)>,
    >,
    list: Query<Entity, With<ListBox>>,
    mut texts: Query<(&mut Text, Has<CurrentText>), Or<(With<MessageText>, With<CurrentText>)>>,
    context: MapContext,
    mut focus: Option<ResMut<InputFocus>>,
    form_boxes: Query<Entity, With<FormBox>>,
    controls: Query<(), With<NotesControl>>,
    mut tabs: Query<
        (Entity, &PanelAction, &mut Node, Has<TabIndex>),
        (
            Without<ExplorerPanel>,
            Without<FormBox>,
            Without<CurrentBox>,
        ),
    >,
) {
    // Visibility follows the flag here (not in `keys`), because input is
    // gated while the prompt modal is open and `--panels explorer` must
    // still show the panel then.
    let map_on = context.actor(&files).is_some();
    for (entity, action, mut node, tabbable) in &mut tabs {
        if *action == PanelAction::Map {
            node.display = if map_on { Display::Flex } else { Display::None };
            if map_on && !tabbable {
                commands.entity(entity).insert(TabIndex(0));
            }
            if !map_on && tabbable {
                commands.entity(entity).remove::<TabIndex>();
            }
        }
    }
    if editor.map && !map_on {
        editor.map = false;
        if editor.map_form.take().is_some() {
            for entity in &form_boxes {
                commands.entity(entity).despawn_children();
            }
            if let Some(focus) = focus.as_deref_mut() {
                focus.clear();
            }
        }
        editor.rows_dirty = true;
    }
    let current = if editor.map {
        editor
            .map_form
            .as_ref()
            .and_then(|f| f.current.as_ref())
            .map(|m| {
                (
                    format!(
                        "{}  kind: {}  pos: {}, {}",
                        m.name,
                        m.kind.as_deref().unwrap_or("none"),
                        m.pos[0],
                        m.pos[1]
                    ),
                    m.note.clone(),
                )
            })
    } else {
        editor.form.as_ref().and_then(|f| f.current.clone())
    };
    for (mut node, is_root, is_current) in &mut boxes {
        let shown = if is_root {
            panels.explorer
        } else if is_current {
            current.is_some()
        } else {
            editor.form.is_some() || editor.map_form.is_some()
        };
        let want = if shown { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    let opened = panels.explorer && !state.was_open;
    state.was_open = panels.explorer;
    if !panels.explorer {
        if let Some(focus) = focus.as_deref_mut() {
            // An invisible panel must not keep shielding the world's keyboard.
            if focus.get().is_some_and(|entity| controls.contains(entity)) {
                focus.clear();
            }
        }
        return;
    }
    let due = state.timer.tick(time.delta()).just_finished();
    if opened || due {
        reload(&mut editor, &mut stores, &files);
    }
    let message = editor.message.clone().unwrap_or_default();
    let now = current.map_or_else(String::new, |(title, body)| {
        format!("Now in the file:\n{title}\n{body}")
    });
    for (mut text, is_current) in &mut texts {
        let want = if is_current { &now } else { &message };
        if text.0 != *want {
            text.0.clone_from(want);
        }
    }
    if !editor.rows_dirty {
        return;
    }
    editor.rows_dirty = false;
    let Ok(list) = list.single() else { return };
    let book = editor.book;
    commands
        .entity(list)
        .despawn_children()
        .with_children(|list| {
            if editor.map {
                spawn_map_rows(list, &editor);
                return;
            }
            if editor.notes.is_empty() {
                list.spawn((
                    RowText,
                    Text::new(empty_label(book)),
                    font(12.0),
                    TextColor(DIM),
                ));
            }
            for note in &editor.notes {
                spawn_row(list, book, note);
            }
            for u in &editor.unreadable {
                list.spawn((
                    RowText,
                    Text::new(unreadable_text(u)),
                    font(11.0),
                    TextColor(NOTE),
                ));
            }
        });
}

/// How the list names a file that is not an entry.
pub(crate) fn unreadable_text(u: &Unreadable) -> String {
    format!("Unreadable: {} - {}", u.file, u.reason)
}

/// One entry of the list: its title (a button that selects it), stamps,
/// body and, for a proposal, the status buttons.
fn spawn_row(list: &mut ChildSpawnerCommands, book: Book, note: &Note) {
    let status = note
        .status
        .map(|s| format!(" [{}]", s.as_str()))
        .unwrap_or_default();
    let mut stamps = format!("#{}  created {}", note.id, stamp_text(&note.created));
    if note.updated != note.created {
        stamps.push_str(&format!("  updated {}", stamp_text(&note.updated)));
    }
    if let Some(p) = &note.prompted_by {
        stamps.push_str(&format!("\nprompted by: {p}"));
    }
    list.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            row_gap: px(2),
            padding: UiRect::all(px(5)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(ROW),
    ))
    .with_children(|row| {
        row.spawn((
            WidgetButton,
            PanelAction::Select(note.id),
            NotesControl,
            TabIndex(0),
            Selectable,
            AccessibleLabel::new(format!("Select entry #{} {}", note.id, note.title)),
            Node {
                padding: UiRect::axes(px(3), px(1)),
                border_radius: BorderRadius::all(px(3)),
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
            Hovered::default(),
        ))
        .with_child((
            RowText,
            Text::new(format!("{}{status}", note.title)),
            font(13.0),
            TextColor(TEXT),
        ));
        row.spawn((RowText, Text::new(stamps), font(11.0), TextColor(DIM)));
        if !note.body.is_empty() {
            row.spawn((
                RowText,
                Text::new(note.body.clone()),
                font(12.0),
                TextColor(TEXT),
            ));
        }
        if book == Book::Proposals {
            row.spawn(Node {
                column_gap: px(4),
                ..default()
            })
            .with_children(|buttons| {
                for s in STATUSES {
                    button(buttons, s.as_str(), PanelAction::Status(note.id, s));
                }
            });
        }
    });
}

/// Button colours: pressed, hovered, the active tab / selected entry /
/// current status, else plain.
fn paint(
    mut commands: Commands,
    focus: Res<InputFocus>,
    editor: Res<NotesEditor>,
    mut buttons: Query<(
        Entity,
        &PanelAction,
        &mut BackgroundColor,
        &mut BorderColor,
        Option<&Hovered>,
        Has<Pressed>,
        Has<UiSelected>,
    )>,
    mut fields: Query<(Entity, &mut BorderColor), (With<NoteField>, Without<PanelAction>)>,
) {
    for (entity, action, mut bg, mut border, hovered, pressed, selected) in &mut buttons {
        let (active, plain) = match *action {
            PanelAction::Book(b) => (!editor.map && b == editor.book, BUTTON),
            PanelAction::Map => (editor.map, BUTTON),
            PanelAction::Select(id) => (
                if editor.map {
                    editor.selected_landmark == Some(id)
                } else {
                    editor.selected == Some(id)
                },
                Color::NONE,
            ),
            PanelAction::Status(id, s) => (
                editor
                    .notes
                    .iter()
                    .any(|n| n.id == id && n.status == Some(s)),
                BUTTON,
            ),
            _ => (false, BUTTON),
        };
        let color = if pressed {
            BUTTON_PRESSED
        } else if hovered.is_some_and(Hovered::get) {
            BUTTON_HOVER
        } else if active {
            if matches!(action, PanelAction::Select(_)) {
                SELECTED
            } else {
                ACTIVE
            }
        } else {
            plain
        };
        if bg.0 != color {
            bg.0 = color;
        }
        border.set_if_neq(BorderColor::all(if focus.get() == Some(entity) {
            NOTE
        } else {
            Color::NONE
        }));
        if active != selected {
            if active {
                commands.entity(entity).insert(UiSelected);
            } else {
                commands.entity(entity).remove::<UiSelected>();
            }
        }
    }
    for (entity, mut border) in &mut fields {
        border.set_if_neq(BorderColor::all(if focus.get() == Some(entity) {
            NOTE
        } else {
            BORDER
        }));
    }
}

#[cfg(test)]
mod tests {
    use bevy::input_focus::{FocusCause, InputFocus};
    use bevy::text::{EditableText, TextEdit, TextPlugin};
    use npc_framework::agent_io::notes::{NewNote, NoteChange, NotesClock, NotesStores, Writer};

    use std::path::Path;

    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("npc_notes_editor_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_date() -> String {
        "2026-10-25T12:00:00Z".into()
    }

    /// The panel without a window, open on `dir`. `TextPlugin` applies the
    /// queued `TextEdit`s (in `PostUpdate`), as in the game.
    fn app(dir: &Path) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TextPlugin))
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TestbedPanels {
                explorer: true,
                ..default()
            })
            .insert_resource(ExplorerFiles {
                dir: dir.to_path_buf(),
            })
            .insert_resource(NotesClock(fixed_date))
            .add_plugins(plugin)
            // This headless fixture has no renderer/visibility propagation.
            // Supply the UI visibility that native tab navigation reads.
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
        app
    }

    fn editor(app: &App) -> &NotesEditor {
        app.world().resource::<NotesEditor>()
    }

    /// Activates the button carrying `action`, as a click would.
    fn act(app: &mut App, action: PanelAction) {
        let mut q = app.world_mut().query::<(Entity, &PanelAction)>();
        let entity = q
            .iter(app.world())
            .find(|(_, a)| **a == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("no button for {action:?}"));
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
        app.update();
    }

    fn field<F: Component>(app: &mut App) -> Entity {
        let mut q = app.world_mut().query_filtered::<Entity, With<F>>();
        q.single(app.world()).expect("one field")
    }

    /// Types `text` into the field `F` the way `TextInput` does: queued
    /// `TextEdit::Insert`s, applied by Bevy in `PostUpdate`.
    fn type_into<F: Component>(app: &mut App, text: &str) {
        let f = field::<F>(app);
        let mut editable = app.world_mut().get_mut::<EditableText>(f).unwrap();
        for part in text.split_inclusive(' ') {
            editable.queue_edit(TextEdit::Insert(part.into()));
        }
        app.update();
    }

    fn value<F: Component>(app: &mut App) -> String {
        let f = field::<F>(app);
        app.world()
            .get::<EditableText>(f)
            .unwrap()
            .value()
            .to_string()
    }

    fn list(app: &mut App, dir: &Path, book: Book) -> Vec<Note> {
        app.world_mut()
            .resource_mut::<NotesStores>()
            .store_for(dir)
            .unwrap()
            .list(book)
            .unwrap()
    }

    fn agent_adds(app: &mut App, dir: &Path, book: Book, title: &str) -> Note {
        let at = Stamp {
            tick: 7,
            date: "2026-10-25T11:00:00Z".into(),
        };
        let new = NewNote {
            title: title.into(),
            body: format!("{title} body"),
            prompted_by: None,
        };
        app.world_mut()
            .resource_mut::<NotesStores>()
            .store_for(dir)
            .unwrap()
            .add(book, Writer::Agent, new, at)
            .unwrap()
    }

    fn map_app(dir: &Path) -> App {
        let mut app = app(dir);
        let (id, _) = npc_framework::prelude::spawn_sim(app.world_mut(), SimPos::new(4200, -7300));
        app.insert_resource(NotesDirs(std::collections::BTreeMap::from([(
            id,
            dir.into(),
        )])))
        .insert_resource(MapEnabled(true));
        app.update();
        app
    }
    fn map_field(app: &mut App, key: MapField) -> Entity {
        app.world_mut()
            .query::<(Entity, &MapField)>()
            .iter(app.world())
            .find(|(_, field)| **field == key)
            .unwrap()
            .0
    }
    fn set_map_field(app: &mut App, key: MapField, value: &str) {
        let entity = map_field(app, key);
        let mut text = app.world_mut().get_mut::<EditableText>(entity).unwrap();
        text.editor_mut().set_text(value);
        text.pending_edits.clear();
        app.update();
    }
    fn map_list(app: &mut App, dir: &Path) -> Vec<Landmark> {
        app.world_mut()
            .resource_mut::<NotesStores>()
            .map_for(dir)
            .unwrap()
            .list()
            .unwrap()
    }
    #[test]
    fn f7_map_tab_lists_and_adds_at_here_then_edits_all_fields() {
        let dir = tmp("map-here");
        let mut app = map_app(&dir);
        act(&mut app, PanelAction::Map);
        assert!(editor(&app).map);
        act(&mut app, PanelAction::Add);
        let name = map_field(&mut app, MapField::Name);
        set_map_field(&mut app, MapField::Name, "North gate");
        set_map_field(&mut app, MapField::Kind, "gate");
        set_map_field(&mut app, MapField::Note, "Return with the key");
        assert!(app.world().get::<EditableText>(name).is_some());
        act(&mut app, PanelAction::Save);
        let landmarks = map_list(&mut app, &dir);
        assert_eq!(landmarks.len(), 1);
        assert_eq!(landmarks[0].pos, [4200, -7300]);
        assert_eq!(landmarks[0].name, "North gate");
        assert_eq!(landmarks[0].kind.as_deref(), Some("gate"));
        assert_eq!(landmarks[0].created.date, fixed_date());
        assert!(rows_text(&mut app).contains("North gate"));
        act(&mut app, PanelAction::Edit);
        set_map_field(&mut app, MapField::Name, "Old gate");
        set_map_field(&mut app, MapField::Kind, "");
        set_map_field(&mut app, MapField::X, "12000");
        set_map_field(&mut app, MapField::Z, "-9500");
        set_map_field(&mut app, MapField::Note, "No longer locked");
        act(&mut app, PanelAction::Save);
        let landmarks = map_list(&mut app, &dir);
        assert_eq!(landmarks[0].id, 1);
        assert_eq!(landmarks[0].name, "Old gate");
        assert_eq!(landmarks[0].kind, None);
        assert_eq!(landmarks[0].pos, [12000, -9500]);
        assert_eq!(landmarks[0].note, "No longer locked");
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn f7_map_stale_save_is_reported_and_draft_caret_is_retained() {
        let dir = tmp("map-stale");
        let mut app = map_app(&dir);
        act(&mut app, PanelAction::Map);
        act(&mut app, PanelAction::Add);
        set_map_field(&mut app, MapField::Name, "Well");
        act(&mut app, PanelAction::Save);
        let old = map_list(&mut app, &dir).remove(0);
        act(&mut app, PanelAction::Edit);
        set_map_field(&mut app, MapField::Note, "My pending note");
        let field = map_field(&mut app, MapField::Note);
        app.world_mut()
            .resource_mut::<NotesStores>()
            .map_for(&dir)
            .unwrap()
            .update(
                old.id,
                &old.updated,
                LandmarkChange {
                    note: Some("Agent's latest note".into()),
                    pos: Some([9000, 1000]),
                    ..default()
                },
                Stamp {
                    tick: 9,
                    date: "newdate".into(),
                },
                Writer::Agent,
            )
            .unwrap();
        act(&mut app, PanelAction::Save);
        assert!(
            editor(&app)
                .message
                .as_ref()
                .unwrap()
                .contains("changed meanwhile")
        );
        assert_eq!(map_list(&mut app, &dir)[0].note, "Agent's latest note");
        assert_eq!(map_field(&mut app, MapField::Note), field);
        assert_eq!(
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            "My pending note"
        );
        assert!(
            editor(&app)
                .map_form
                .as_ref()
                .unwrap()
                .current
                .as_ref()
                .is_some_and(|m| m.pos == [9000, 1000])
        );
        act(&mut app, PanelAction::Save);
        assert_eq!(map_list(&mut app, &dir)[0].note, "My pending note");
        assert!(editor(&app).map_form.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn map_fields_block_world_keys_and_cycle_all_five_fields() {
        let dir = tmp("map-typing");
        let mut app = map_app(&dir);
        app.init_resource::<InputRuns>()
            .add_systems(Update, count_input.in_set(ViewSystems::Input));
        act(&mut app, PanelAction::Map);
        act(&mut app, PanelAction::Add);
        let name = map_field(&mut app, MapField::Name);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(name, FocusCause::Navigated);
        let before = app.world().resource::<InputRuns>().0;
        for expected in [MapField::Kind, MapField::X, MapField::Z, MapField::Note] {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::Tab);
            app.update();
            let expected_field = map_field(&mut app, expected);
            assert_eq!(
                app.world().resource::<InputFocus>().get(),
                Some(expected_field)
            );
            assert!(editor(&app).map);
        }
        for action in [PanelAction::Save, PanelAction::Cancel] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            let focused = app.world().resource::<InputFocus>().get().unwrap();
            assert_eq!(app.world().get::<PanelAction>(focused), Some(&action));
            assert!(editor(&app).map);
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(name));
        assert_eq!(app.world().resource::<InputRuns>().0, before);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn map_tab_cycles_only_when_enabled_and_capture_flag_selects_it() {
        let dir = tmp("map-cycle");
        let mut app = map_app(&dir);
        act(&mut app, PanelAction::Book(Book::Proposals));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert!(editor(&app).map);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert!(!editor(&app).map);
        assert_eq!(editor(&app).book, Book::Diary);
        app.insert_resource(MapEnabled(false));
        app.update();
        let entity = app
            .world_mut()
            .query::<(Entity, &PanelAction)>()
            .iter(app.world())
            .find(|(_, action)| **action == PanelAction::Map)
            .unwrap()
            .0;
        assert_eq!(
            app.world().get::<Node>(entity).unwrap().display,
            Display::None
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn capture_flag_initially_opens_the_map_tab() {
        let dir = tmp("map-capture");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TextPlugin))
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TestbedPanels {
                explorer: true,
                map: true,
                ..default()
            })
            .insert_resource(ExplorerFiles { dir: dir.clone() })
            .insert_resource(NotesDirs(std::collections::BTreeMap::from([(
                SimId(1),
                dir.clone(),
            )])))
            .add_plugins(plugin);
        app.update();
        assert!(editor(&app).map);
        assert!(rows_text(&mut app).contains("No landmarks yet"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn panel_release_cancels_marquee_and_clears_held_mouse_buttons() {
        let dir = tmp("drag-release");
        let mut app = app(&dir);
        app.init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(DragBox {
                start: Some(Vec2::new(10.0, 10.0)),
                current: Vec2::new(50.0, 60.0),
            });
        let root = field::<ExplorerPanel>(&mut app);
        app.world_mut().entity_mut(root).insert(Hovered(true));
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.press(MouseButton::Middle);
        mouse.press(MouseButton::Left);
        mouse.release(MouseButton::Left);
        app.update();
        let mouse = app.world().resource::<ButtonInput<MouseButton>>();
        assert!(!mouse.pressed(MouseButton::Middle));
        assert!(!mouse.just_released(MouseButton::Left));
        assert_eq!(app.world().resource::<DragBox>().start, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn editor_adds_a_diary_entry() {
        let dir = tmp("add");
        let mut app = app(&dir);
        assert_eq!(editor(&app).book, Book::Diary);
        act(&mut app, PanelAction::Add);
        assert!(editor(&app).form.is_some());
        type_into::<TitleField>(&mut app, "Found the well");
        type_into::<BodyField>(&mut app, "Water by the north wall.");
        assert_eq!(value::<TitleField>(&mut app), "Found the well");
        act(&mut app, PanelAction::Save);
        let notes = list(&mut app, &dir, Book::Diary);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(notes[0].title, "Found the well");
        assert_eq!(notes[0].body, "Water by the north wall.");
        assert_eq!(notes[0].created.date, fixed_date());
        // The form closed, the list shows it, selected.
        let ed = editor(&app);
        assert!(ed.form.is_none());
        assert_eq!(ed.notes.len(), 1);
        assert_eq!(ed.selected, Some(notes[0].id));
        assert!(
            std::fs::read_to_string(dir.join("diary").join("2026-10-25 Found the well.md"))
                .unwrap()
                .contains("\ntitle: Found the well\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_edits_a_selected_entry() {
        let dir = tmp("edit");
        let mut app = app(&dir);
        let note = agent_adds(&mut app, &dir, Book::Memory, "Key under the mat");
        act(&mut app, PanelAction::Book(Book::Memory));
        act(&mut app, PanelAction::Select(note.id));
        act(&mut app, PanelAction::Edit);
        assert_eq!(value::<TitleField>(&mut app), "Key under the mat");
        type_into::<TitleField>(&mut app, " (blue door)");
        act(&mut app, PanelAction::Save);
        let notes = list(&mut app, &dir, Book::Memory);
        assert_eq!(notes[0].title, "Key under the mat (blue door)");
        assert_ne!(notes[0].updated, note.updated);
        assert!(editor(&app).form.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_sets_a_proposal_status() {
        let dir = tmp("status");
        let mut app = app(&dir);
        let p = agent_adds(&mut app, &dir, Book::Proposals, "Door hints");
        act(&mut app, PanelAction::Book(Book::Proposals));
        assert_eq!(editor(&app).notes.len(), 1);
        act(&mut app, PanelAction::Status(p.id, ProposalStatus::Liked));
        let notes = list(&mut app, &dir, Book::Proposals);
        assert_eq!(notes[0].status, Some(ProposalStatus::Liked));
        assert_eq!(editor(&app).notes[0].status, Some(ProposalStatus::Liked));
        // The status buttons were rebuilt for the new stamp: a second click
        // works too.
        act(
            &mut app,
            PanelAction::Status(p.id, ProposalStatus::Pursuing),
        );
        let notes = list(&mut app, &dir, Book::Proposals);
        assert_eq!(notes[0].status, Some(ProposalStatus::Pursuing));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The agent changes entry `id` behind the editor's back.
    fn agent_updates(app: &mut App, dir: &Path, id: u32, body: &str, tick: u64) -> Note {
        let mut stores = app.world_mut().resource_mut::<NotesStores>();
        let store = stores.store_for(dir).unwrap();
        let now = store
            .list(Book::Diary)
            .unwrap()
            .into_iter()
            .find(|n| n.id == id)
            .unwrap();
        store
            .update(
                Book::Diary,
                Writer::Agent,
                id,
                &now.updated,
                NoteChange {
                    body: Some(body.into()),
                    ..default()
                },
                Stamp {
                    tick,
                    date: fixed_date(),
                },
            )
            .unwrap()
    }

    fn current_text(app: &mut App) -> String {
        let mut q = app.world_mut().query_filtered::<&Text, With<CurrentText>>();
        q.single(app.world()).unwrap().0.clone()
    }

    #[test]
    fn editor_reports_stale_save() {
        let dir = tmp("stale");
        let mut app = app(&dir);
        let note = agent_adds(&mut app, &dir, Book::Diary, "Day 1");
        // Clicking the shown tab reads it again (no wait for the poll).
        act(&mut app, PanelAction::Book(Book::Diary));
        act(&mut app, PanelAction::Select(note.id));
        act(&mut app, PanelAction::Edit);
        assert_eq!(current_text(&mut app), "", "no block before a stale save");
        type_into::<BodyField>(&mut app, " player text");
        let current = agent_updates(&mut app, &dir, note.id, "agent text", 9);
        act(&mut app, PanelAction::Save);
        let notes = list(&mut app, &dir, Book::Diary);
        assert_eq!(notes[0].body, "agent text", "no overwrite");
        let ed = editor(&app);
        let message = ed.message.clone().unwrap();
        assert_eq!(message, stale_save_note(note.id));
        assert!(
            message.starts_with("#1 was changed meanwhile")
                && message.contains("Save again to replace it with yours, or Cancel"),
            "{message}"
        );
        // The agent's version is shown above the fields; the fields keep the
        // player's text; the list is reloaded.
        assert_eq!(ed.notes[0].body, "agent text");
        let form = ed.form.clone().expect("the form stays open");
        assert_eq!(form.editing, Some((note.id, current.updated.clone())));
        assert_eq!(
            form.current,
            Some(("Day 1".to_owned(), "agent text".to_owned()))
        );
        assert_eq!(
            current_text(&mut app),
            "Now in the file:\nDay 1\nagent text"
        );
        assert!(value::<BodyField>(&mut app).ends_with(" player text"));
        // It changes again before the second Save: stale again, the newer
        // version shown, still no overwrite.
        let newer = agent_updates(&mut app, &dir, note.id, "agent text 2", 11);
        act(&mut app, PanelAction::Save);
        assert_eq!(list(&mut app, &dir, Book::Diary)[0].body, "agent text 2");
        assert_eq!(
            current_text(&mut app),
            "Now in the file:\nDay 1\nagent text 2"
        );
        let form = editor(&app).form.clone().unwrap();
        assert_eq!(form.editing, Some((note.id, newer.updated)));
        // Saving with the newest version shown replaces it, knowingly.
        act(&mut app, PanelAction::Save);
        let notes = list(&mut app, &dir, Book::Diary);
        assert!(notes[0].body.ends_with(" player text"), "{:?}", notes[0]);
        assert!(editor(&app).form.is_none());
        assert_eq!(editor(&app).message.as_deref(), Some("saved #1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_status_click_on_the_open_proposal_keeps_the_form_current() {
        let dir = tmp("own-status");
        let mut app = app(&dir);
        let p = agent_adds(&mut app, &dir, Book::Proposals, "Door hints");
        act(&mut app, PanelAction::Book(Book::Proposals));
        act(&mut app, PanelAction::Select(p.id));
        act(&mut app, PanelAction::Edit);
        type_into::<BodyField>(&mut app, " (player)");
        // The player's own status change while the form is open.
        act(&mut app, PanelAction::Status(p.id, ProposalStatus::Liked));
        let liked = list(&mut app, &dir, Book::Proposals)[0].clone();
        assert_eq!(
            editor(&app).form.as_ref().unwrap().editing,
            Some((p.id, liked.updated.clone()))
        );
        act(&mut app, PanelAction::Save);
        let notes = list(&mut app, &dir, Book::Proposals);
        assert_eq!(editor(&app).message.as_deref(), Some("saved #1"));
        assert!(notes[0].body.ends_with(" (player)"), "{:?}", notes[0]);
        assert_eq!(notes[0].status, Some(ProposalStatus::Liked), "status kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_status_click_on_another_proposal_leaves_the_form_alone() {
        let dir = tmp("other-status");
        let mut app = app(&dir);
        let a = agent_adds(&mut app, &dir, Book::Proposals, "Door hints");
        let b = agent_adds(&mut app, &dir, Book::Proposals, "Tool hints");
        act(&mut app, PanelAction::Book(Book::Proposals));
        act(&mut app, PanelAction::Select(a.id));
        act(&mut app, PanelAction::Edit);
        act(
            &mut app,
            PanelAction::Status(b.id, ProposalStatus::Rejected),
        );
        assert_eq!(
            editor(&app).form.as_ref().unwrap().editing,
            Some((a.id, a.updated.clone()))
        );
        act(&mut app, PanelAction::Save);
        assert_eq!(editor(&app).message.as_deref(), Some("saved #1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stand-in for the game's input systems.
    #[derive(Resource, Default)]
    struct InputRuns(u32);

    fn count_input(mut runs: ResMut<InputRuns>) {
        runs.0 += 1;
    }

    #[test]
    fn accessible_names_preserve_native_widget_roles() {
        use bevy::a11y::AccessibilityNode;
        let dir = tmp("accessible-roles");
        let mut app = app(&dir);
        let bare_button = app.world_mut().spawn(WidgetButton).id();
        let bare_field = app.world_mut().spawn(TextInput).id();
        let button_role = app
            .world()
            .get::<AccessibilityNode>(bare_button)
            .unwrap()
            .role();
        let field_role = app
            .world()
            .get::<AccessibilityNode>(bare_field)
            .unwrap()
            .role();
        act(&mut app, PanelAction::Add);
        for (entity, action) in app
            .world_mut()
            .query::<(Entity, &PanelAction)>()
            .iter(app.world())
        {
            assert_eq!(
                app.world().get::<AccessibilityNode>(entity).unwrap().role(),
                button_role,
                "{action:?}"
            );
        }
        for entity in [field::<TitleField>(&mut app), field::<BodyField>(&mut app)] {
            assert_eq!(
                app.world().get::<AccessibilityNode>(entity).unwrap().role(),
                field_role
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keyboard_tab_reaches_panel_actions_and_named_fields() {
        use bevy::ui::prelude::AccessibleLabel;
        let dir = tmp("keyboard-actions");
        let mut app = app(&dir);
        for expected in [
            PanelAction::Book(Book::Diary),
            PanelAction::Book(Book::Memory),
            PanelAction::Book(Book::Proposals),
            PanelAction::Add,
            PanelAction::Edit,
        ] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            let entity = app
                .world()
                .resource::<InputFocus>()
                .get()
                .expect("Tab focuses a control");
            assert_eq!(app.world().get::<PanelAction>(entity), Some(&expected));
            assert!(
                !app.world()
                    .get::<AccessibleLabel>(entity)
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
        act(&mut app, PanelAction::Add);
        let title = field::<TitleField>(&mut app);
        let body = field::<BodyField>(&mut app);
        assert_eq!(
            app.world().get::<AccessibleLabel>(title).unwrap().0,
            "Title"
        );
        assert_eq!(app.world().get::<AccessibleLabel>(body).unwrap().0, "Body");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keyboard_form_traversal_reaches_save_cancel_and_keeps_game_input_blocked() {
        let dir = tmp("keyboard-form");
        let mut app = app(&dir);
        app.init_resource::<InputRuns>()
            .add_systems(Update, count_input.in_set(ViewSystems::Input));
        act(&mut app, PanelAction::Add);
        let title = field::<TitleField>(&mut app);
        let body = field::<BodyField>(&mut app);
        let action_entity = |app: &mut App, action| {
            app.world_mut()
                .query::<(Entity, &PanelAction)>()
                .iter(app.world())
                .find(|(_, candidate)| **candidate == action)
                .unwrap()
                .0
        };
        let save = action_entity(&mut app, PanelAction::Save);
        let cancel = action_entity(&mut app, PanelAction::Cancel);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(title, FocusCause::Navigated);
        app.update();
        let before = app.world().resource::<InputRuns>().0;
        for expected in [body, save, cancel, title] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(expected));
            assert_eq!(app.world().resource::<InputRuns>().0, before);
            assert_eq!(
                *app.world().get::<BorderColor>(expected).unwrap(),
                BorderColor::all(NOTE)
            );
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(cancel));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keyboard_enter_saves_and_cancels_through_the_native_button_observer() {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::{Key, KeyboardInput};
        use bevy::input_focus::FocusedInput;
        let dir = tmp("keyboard-enter");
        let mut app = app(&dir);
        for action in [PanelAction::Save, PanelAction::Cancel] {
            act(&mut app, PanelAction::Add);
            type_into::<TitleField>(&mut app, "Keyboard note");
            type_into::<BodyField>(&mut app, "Preserved body");
            let entity = app
                .world_mut()
                .query::<(Entity, &PanelAction)>()
                .iter(app.world())
                .find(|(_, candidate)| **candidate == action)
                .unwrap()
                .0;
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
            app.world_mut().trigger(FocusedInput::new(
                entity,
                KeyboardInput {
                    key_code: KeyCode::Enter,
                    logical_key: Key::Enter,
                    state: ButtonState::Pressed,
                    text: None,
                    repeat: false,
                    window: Entity::PLACEHOLDER,
                },
                Entity::PLACEHOLDER,
            ));
            app.world_mut().flush();
            app.update();
            assert!(editor(&app).form.is_none());
            assert_eq!(
                list(&mut app, &dir, Book::Diary).len(),
                1,
                "Cancel must not add a second entry"
            );
        }
        assert_eq!(list(&mut app, &dir, Book::Diary)[0].body, "Preserved body");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prompt_modal_and_hidden_panel_do_not_leak_notes_keyboard_focus() {
        let dir = tmp("keyboard-modal");
        let mut app = app(&dir);
        let modal = crate::explorer_modal::PromptModal {
            open: true,
            ..default()
        };
        app.insert_resource(modal);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F7);
        app.update();
        assert!(
            app.world().resource::<TestbedPanels>().explorer,
            "modal shields F7"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<crate::explorer_modal::PromptModal>()
            .open = false;
        act(&mut app, PanelAction::Add);
        let title = field::<TitleField>(&mut app);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(title, FocusCause::Navigated);
        app.world_mut().resource_mut::<TestbedPanels>().explorer = false;
        app.update();
        assert!(app.world().resource::<InputFocus>().get().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn escape_and_header_focus_keep_the_draft_and_tab_returns_to_the_open_form() {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::{Key, KeyboardInput};
        use bevy::input_focus::FocusedInput;
        let dir = tmp("keyboard-draft");
        let mut app = app(&dir);
        act(&mut app, PanelAction::Add);
        type_into::<TitleField>(&mut app, "Pending title");
        type_into::<BodyField>(&mut app, "Pending body");
        let title = field::<TitleField>(&mut app);
        let header = app
            .world_mut()
            .query::<(Entity, &PanelAction)>()
            .iter(app.world())
            .find(|(_, action)| **action == PanelAction::Book(Book::Diary))
            .unwrap()
            .0;
        for focused in [title, header] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(focused, FocusCause::Navigated);
            if focused == title {
                app.world_mut().trigger(FocusedInput::new(
                    title,
                    KeyboardInput {
                        key_code: KeyCode::Escape,
                        logical_key: Key::Escape,
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: Entity::PLACEHOLDER,
                    },
                    Entity::PLACEHOLDER,
                ));
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .press(KeyCode::Escape);
                app.world_mut().flush();
                app.update();
                assert!(app.world().resource::<InputFocus>().get().is_none());
            }
            assert!(editor(&app).form.is_some());
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Tab);
            app.update();
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(title));
            assert_eq!(value::<TitleField>(&mut app), "Pending title");
            assert_eq!(value::<BodyField>(&mut app), "Pending body");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn typing_in_the_editor_blocks_game_keys() {
        let dir = tmp("typing");
        let mut app = app(&dir);
        app.init_resource::<InputRuns>()
            .add_systems(Update, count_input.in_set(ViewSystems::Input));
        app.update();
        let before = app.world().resource::<InputRuns>().0;
        assert!(before > 0, "not typing: game input runs");
        act(&mut app, PanelAction::Add);
        let title = field::<TitleField>(&mut app);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(title, FocusCause::Navigated);
        assert!(app.world_mut().run_system_cached(editor_typing).unwrap());
        // F7 (and every other game key) does nothing while a field has focus.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F7);
        let runs = app.world().resource::<InputRuns>().0;
        app.update();
        app.update();
        assert_eq!(app.world().resource::<InputRuns>().0, runs);
        assert!(app.world().resource::<TestbedPanels>().explorer);
        // Tab moves between the fields instead of cycling the books.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::F7);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        let body = field::<BodyField>(&mut app);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(body));
        assert_eq!(editor(&app).book, Book::Diary);
        // Focus leaves the field: the game's keys work again.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<InputFocus>().clear();
        app.update();
        assert!(app.world().resource::<InputRuns>().0 > runs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_tab_cycles_the_books_and_f7_toggles() {
        let dir = tmp("tab");
        let mut app = app(&dir);
        let press = |app: &mut App, key| {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.clear();
            if key == KeyCode::Tab {
                keys.press(KeyCode::ControlLeft);
            }
            keys.press(key);
            app.update();
        };
        press(&mut app, KeyCode::Tab);
        assert_eq!(editor(&app).book, Book::Memory);
        press(&mut app, KeyCode::Tab);
        assert_eq!(editor(&app).book, Book::Proposals);
        press(&mut app, KeyCode::Tab);
        assert_eq!(editor(&app).book, Book::Diary);
        press(&mut app, KeyCode::F7);
        assert!(!app.world().resource::<TestbedPanels>().explorer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_folder_is_listed_newest_first_with_ids() {
        let dir = tmp("old");
        std::fs::write(
            dir.join("diary.md"),
            "# Diary of Wren\n\n## tick 10 - Woke up\nCold.\n\n## tick 40 - The ruin\nA chest.\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("proposals")).unwrap();
        std::fs::write(
            dir.join("proposals").join("001-door-hints.md"),
            "title: Door hints\nstatus: liked\nwritten: tick 12, 2026-10-24\n\nShow who has the key.\n",
        )
        .unwrap();
        let mut app = app(&dir);
        let ed = editor(&app);
        let titles: Vec<&str> = ed.notes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["The ruin", "Woke up"], "newest first");
        assert!(ed.notes.iter().all(|n| n.id > 0));
        let text = rows_text(&mut app);
        assert!(text.contains("The ruin") && text.contains("t40"), "{text}");
        act(&mut app, PanelAction::Book(Book::Proposals));
        assert_eq!(editor(&app).notes[0].status, Some(ProposalStatus::Liked));
        assert!(rows_text(&mut app).contains("Show who has the key."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_lists_unreadable_files() {
        let dir = tmp("unreadable");
        std::fs::create_dir_all(dir.join("diary")).unwrap();
        std::fs::write(dir.join("diary").join("Typed by hand.md"), "Just text.\n").unwrap();
        std::fs::write(
            dir.join("diary").join("Good.md"),
            "---\nid: 1\ntitle: Good\n---\nFine.\n",
        )
        .unwrap();
        let mut app = app(&dir);
        let ed = editor(&app);
        assert_eq!(ed.notes.len(), 1);
        assert_eq!(ed.unreadable.len(), 1);
        assert_eq!(ed.unreadable[0].file, "diary/Typed by hand.md");
        let text = rows_text(&mut app);
        assert!(
            text.contains("Unreadable: diary/Typed by hand.md - no frontmatter"),
            "{text}"
        );
        assert!(text.contains("Good"), "{text}");
        // Fixed by hand, it becomes an entry within a poll; the row goes.
        std::fs::write(
            dir.join("diary").join("Typed by hand.md"),
            "---\nid: 2\ntitle: Typed by hand\n---\nJust text.\n",
        )
        .unwrap();
        app.world_mut()
            .resource_mut::<PanelState>()
            .timer
            .set_elapsed(POLL);
        app.update();
        assert!(editor(&app).unreadable.is_empty());
        assert_eq!(editor(&app).notes.len(), 2);
        assert!(!rows_text(&mut app).contains("Unreadable"));
        // Each book shows its own.
        std::fs::create_dir_all(dir.join("memory")).unwrap();
        std::fs::write(dir.join("memory").join("Bad.md"), "---\nid: x\n---\n").unwrap();
        act(&mut app, PanelAction::Book(Book::Memory));
        let text = rows_text(&mut app);
        assert!(
            text.contains("Unreadable: memory/Bad.md - id `x`"),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every row text of the list, joined.
    fn rows_text(app: &mut App) -> String {
        let mut q = app.world_mut().query_filtered::<&Text, With<RowText>>();
        q.iter(app.world())
            .map(|t| t.0.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn agent_edits_appear_within_a_poll() {
        let dir = tmp("poll");
        let mut app = app(&dir);
        assert!(editor(&app).notes.is_empty());
        agent_adds(&mut app, &dir, Book::Diary, "Later");
        app.world_mut()
            .resource_mut::<PanelState>()
            .timer
            .set_elapsed(POLL);
        app.update();
        assert_eq!(editor(&app).notes.len(), 1);
        assert!(rows_text(&mut app).contains("Later"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicks_over_the_panel_do_not_reach_the_world() {
        let dir = tmp("click");
        let mut app = app(&dir);
        app.init_resource::<ButtonInput<MouseButton>>();
        #[derive(Resource, Default)]
        struct Clicks(u32);
        app.init_resource::<Clicks>().add_systems(
            Update,
            (|b: Res<ButtonInput<MouseButton>>, mut c: ResMut<Clicks>| {
                if b.just_pressed(MouseButton::Left) {
                    c.0 += 1;
                }
            })
            .in_set(ViewSystems::Input),
        );
        let root = field::<ExplorerPanel>(&mut app);
        app.world_mut().entity_mut(root).insert(Hovered(true));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Clicks>().0, 0);
        app.world_mut().entity_mut(root).insert(Hovered(false));
        let mut b = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        b.release_all();
        b.clear();
        b.press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Clicks>().0, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stamp_text_shows_tick_and_date() {
        let s = Stamp {
            tick: 40,
            date: String::new(),
        };
        assert_eq!(stamp_text(&s), "t40");
        let s = Stamp {
            tick: 7,
            date: "2026-10-25T11:00:00Z".into(),
        };
        assert_eq!(stamp_text(&s), "t7 2026-10-25T11:00:00Z");
    }
}
